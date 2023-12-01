use std::{
    collections::HashMap,
    fmt,
    process::exit,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

use anyhow::{anyhow, Result};
use ark_core_k8s::data::Name;
use clap::{ArgAction, Args, Parser};
use derivative::Derivative;
use futures::Future;
use opentelemetry::global;
use schemars::JsonSchema;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use strum::{Display, EnumString};
use tokio::{
    select, spawn,
    sync::mpsc::{self, Receiver, Sender},
    task::{yield_now, JoinHandle},
    time::sleep,
};
use tracing::{debug, error, info, instrument, warn, Level};

use crate::{
    function::{
        Function, FunctionBuilder, FunctionContext, OwnedFunctionBuilder, OwnedFunctionBuilderArgs,
        RemoteFunction,
    },
    message::{Codec, PipeMessage, PipeMessages, PipePayload},
    messengers::{init_messenger, MessengerArgs, Publisher, PublisherExt, Subscriber},
    storage::{DummyStorageArgs, MetadataStorageArgs, MetadataStorageType, StorageIO, StorageSet},
};

#[derive(Derivative, Serialize, Deserialize, Parser)]
#[derivative(
    Clone(bound = "
        <F as FunctionBuilder>::Args: Clone,
        S: Clone,
    "),
    Debug(bound = "
        <F as FunctionBuilder>::Args: fmt::Debug,
        S: fmt::Debug,
    ")
)]
#[serde(bound = "
    <F as FunctionBuilder>::Args: Serialize + DeserializeOwned,
    S: Serialize + DeserializeOwned,
")]
pub struct PipeArgs<F, S = crate::storage::StorageArgs>
where
    F: FunctionBuilder,
    S: Args,
{
    #[arg(long, env = "PIPE_BATCH_SIZE", value_name = "BATCH_SIZE")]
    #[serde(default)]
    batch_size: Option<usize>,

    #[arg(long, env = "PIPE_BATCH_TIMEOUT", value_name = "MILLISECONDS")]
    #[serde(default)]
    batch_timeout_ms: Option<u64>,

    /// Init the function and stop it immediately.
    #[arg(long, env = "PIPE_BOOTSTRAP", action = ArgAction::SetTrue)]
    #[serde(default)]
    bootstrap: bool,

    #[arg(long, env = "PIPE_DEFAULT_MODEL_IN", value_name = "POLICY")]
    #[serde(default)]
    default_model_in: Option<DefaultModelIn>,

    #[arg(long, env = "PIPE_ENCODER", value_name = "CODEC")]
    #[serde(default)]
    encoder: Option<Codec>,

    #[command(flatten)]
    function_args: <F as FunctionBuilder>::Args,

    #[arg(long, env = "PIPE_IGNORE_SIGINT", action = ArgAction::SetTrue)]
    #[serde(default)]
    ignore_sigint: bool,

    #[arg(long, env = "RUST_LOG")]
    #[serde(default)]
    log_level: Option<String>,

    #[arg(long, env = "PIPE_MAX_TASKS", value_name = "NUM")]
    #[serde(default)]
    max_tasks: Option<usize>,

    #[command(flatten)]
    messenger_args: MessengerArgs,

    #[arg(long, env = "PIPE_MODEL_IN", value_name = "NAME")]
    #[serde(default)]
    model_in: Option<Name>,

    #[arg(long, env = "PIPE_MODEL_OUT", value_name = "NAME")]
    #[serde(default)]
    model_out: Option<Name>,

    #[arg(long, env = "PIPE_QUEUE_GROUP", action = ArgAction::SetTrue)]
    #[serde(default)]
    queue_group: bool,

    #[command(flatten)]
    storage: S,
}

impl<F, S> PipeArgs<OwnedFunctionBuilder<F>, S>
where
    F: Send + Sync + RemoteFunction,
    <F as RemoteFunction>::Input: fmt::Debug + DeserializeOwned + JsonSchema,
    <F as RemoteFunction>::Output: fmt::Debug + Serialize + JsonSchema,
    S: Args,
{
    pub fn with_function(function: F) -> Result<Self> {
        Ok(Self {
            function_args: OwnedFunctionBuilderArgs::new(function),
            ..Self::try_parse()?
        })
    }
}

impl<F, S> PipeArgs<F, S>
where
    F: FunctionBuilder,
    S: Args,
{
    pub fn from_env() -> Self
    where
        <F as FunctionBuilder>::Args: Args,
    {
        Self::parse()
    }

    pub fn default_max_tasks(&self) -> usize {
        self.max_tasks.unwrap_or(8)
    }

    pub fn with_batch_size(mut self, batch_size: usize) -> Self {
        self.batch_size = Some(batch_size);
        self
    }

    pub fn with_bootstrap(mut self, bootstrap: bool) -> Self {
        self.bootstrap = bootstrap;
        self
    }

    pub fn with_default_model_in(mut self, default_model_in: DefaultModelIn) -> Self {
        self.default_model_in = Some(default_model_in);
        self
    }

    pub fn with_function_args(mut self, function_args: <F as FunctionBuilder>::Args) -> Self {
        self.function_args = function_args;
        self
    }

    pub fn with_ignore_sigint(mut self, ignore_sigint: bool) -> Self {
        self.ignore_sigint = ignore_sigint;
        self
    }

    pub fn with_model_in(mut self, model_in: Option<Name>) -> Self {
        self.model_in = model_in;
        self
    }

    pub fn with_model_out(mut self, model_out: Option<Name>) -> Self {
        self.model_out = model_out;
        self
    }

    pub fn with_storage(mut self, storage: S) -> Self {
        self.storage = storage;
        self
    }
}

impl<F> PipeArgs<F, DummyStorageArgs>
where
    F: FunctionBuilder,
{
    pub const fn with_dummy_storage(self) -> Self {
        self
    }
}

impl<F> PipeArgs<F>
where
    F: FunctionBuilder,
{
    #[instrument(level = Level::INFO, skip_all, err(Display))]
    async fn init_context(&self) -> Result<Context<F>> {
        let messenger = init_messenger(&self.messenger_args).await?;

        debug!("Initializing Task Context");
        let mut function_context = FunctionContext::new(messenger.messenger_type());
        if !self.ignore_sigint {
            function_context.clone().trap_on_sigint()?;
        }

        // Do not load payloads on writer mode
        if self.model_in.is_none() {
            function_context.disable_load();
        }

        // Force read-only mode on self processing
        if self.model_in.as_ref().map(|model| model.storage())
            == self.model_out.as_ref().map(|model| model.storage())
            || self.model_out.is_none()
        {
            function_context.disable_store();
            function_context.disable_store_metadata();
        }

        debug!("Initializing Storage IO");
        let max_tasks = self.batch_size.unwrap_or(1) * self.default_max_tasks();
        let storage = Arc::new({
            let default_metadata_type = MetadataStorageType::default();

            StorageIO {
                input: Arc::new({
                    let default_metadata =
                        MetadataStorageArgs::<<F as Function>::Input>::new(default_metadata_type);
                    let model = self.model_in.as_ref().or_else(|| {
                        match self.default_model_in.unwrap_or_default() {
                            DefaultModelIn::ModelOut => self.model_out.as_ref(),
                            DefaultModelIn::Skip => None,
                        }
                    });
                    StorageSet::try_new(
                        &self.storage,
                        &mut function_context,
                        model,
                        default_metadata,
                    )
                    .await?
                }),
                output: Arc::new({
                    let default_metadata =
                        MetadataStorageArgs::<<F as Function>::Output>::new(default_metadata_type);
                    let model = self.model_out.as_ref();

                    StorageSet::try_new(
                        &self.storage,
                        &mut function_context,
                        model,
                        default_metadata,
                    )
                    .await?
                }),
            }
        });

        debug!("Initializing Task");

        #[instrument(level = Level::INFO, skip_all, err(Display))]
        async fn init_function<F, Fut>(f: impl Future<Output = Result<F>>) -> Result<F>
        where
            Fut: Future<Output = Result<F>>,
        {
            f.await
        }

        let function = self.init_function(&mut function_context, &storage).await?;

        debug!("Initializing Reader");
        let reader = match self.model_in.as_ref() {
            Some(model) => {
                let (tx, rx) = mpsc::channel(max_tasks);

                Some(ReadContext {
                    _job: ReadSession {
                        function_context: function_context.clone(),
                        model_out: self.model_out.clone(),
                        storage: storage.input.clone(),
                        stream: if self.queue_group {
                            messenger
                                .subscribe_queued(model.clone(), model.clone())
                                .await
                        } else {
                            messenger.subscribe(model.clone()).await
                        }
                        .map_err(|error| anyhow!("failed to init input stream: {error}"))?,
                        tx: tx.into(),
                    }
                    .loop_forever()
                    .await,
                    rx,
                })
            }
            None => None,
        };

        debug!("Initializing Writer");
        let writer = WriteContext {
            atomic_session: AtomicSession::new(max_tasks),
            encoder: self.encoder.unwrap_or_default(),
            function_context: function_context.clone(),
            storage: storage.output.clone(),
            stream: match self.model_out.as_ref() {
                Some(model) => Some(messenger.publish(model.clone()).await?),
                None => None,
            },
        };

        Ok(Context {
            batch_size: self.batch_size,
            batch_timeout: self.batch_timeout_ms.map(Duration::from_millis),
            function,
            function_context,
            reader,
            writer,
            storage,
        })
    }

    #[instrument(level = Level::INFO, skip_all, err(Display))]
    async fn init_function(
        &self,
        ctx: &mut FunctionContext,
        storage: &Arc<StorageIO>,
    ) -> Result<F> {
        <F as FunctionBuilder>::try_new(&self.function_args, ctx, storage)
            .await
            .map(Into::into)
            .map_err(|error| anyhow!("failed to init function: {error}"))
    }

    pub fn loop_forever(&self) {
        let runtime = ::tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("failed to init tokio runtime");

        match runtime.block_on(self.loop_forever_async()) {
            Ok(()) => {
                info!("Terminated.");
                global::shutdown_tracer_provider();
            }
            Err(error) => {
                error!("{error}");
                global::shutdown_tracer_provider();
                exit(1)
            }
        }
    }

    pub async fn loop_forever_async(&self) -> Result<()> {
        match &self.log_level {
            Some(level) => ::ark_core::tracer::init_once_with(level),
            None => ::ark_core::tracer::init_once(),
        }

        let mut ctx = self.init_context().await?;
        info!("Initialized!");

        if self.bootstrap {
            Ok(())
        } else {
            loop {
                // yield per every loop
                yield_now().await;

                if ctx.function_context.is_terminating()
                    && !ctx.function_context.is_disabled_store_metadata()
                {
                    break ctx.storage.flush().await;
                }

                let response = tick_async(&mut ctx).await;
                if ctx.function_context.is_terminating() {
                    if ctx.function_context.is_disabled_store_metadata() {
                        break response;
                    } else {
                        let flush_response = ctx.storage.flush().await;
                        break response.and(flush_response);
                    }
                } else if let Err(error) = response {
                    warn!("{error}");
                }
            }
        }
    }
}

#[instrument(name = "tick", level = Level::INFO, skip(ctx), err(Display))]
async fn tick_async<F>(ctx: &mut Context<F>) -> Result<()>
where
    F: Function,
{
    #[instrument(name = "read", level = Level::INFO, skip_all, err(Display))]
    async fn recv_one<Value>(
        function_context: &FunctionContext,
        reader: &mut ReadContext<Value>,
    ) -> Result<Option<PipeMessage<Value>>>
    where
        Value: Default,
    {
        loop {
            select! {
                input = reader.rx.recv() => break Ok(input),
                () = sleep(Duration::from_millis(100)) => if function_context.is_terminating() {
                    break Ok(None)
                },
            }
        }
    }

    let inputs = match &mut ctx.reader {
        Some(reader) => {
            let input = match recv_one(&ctx.function_context, reader).await? {
                Some(input) => input,
                None => return Ok(()),
            };
            match ctx.batch_size {
                Some(batch_size) => {
                    let timer = ctx.batch_timeout.map(Timer::new);

                    let mut inputs = vec![input];
                    for _ in 1..batch_size {
                        if timer
                            .as_ref()
                            .map(|timer| timer.is_outdated())
                            .unwrap_or_default()
                        {
                            break;
                        } else {
                            inputs.push(match recv_one(&ctx.function_context, reader).await? {
                                Some(input) => input,
                                None => return Ok(()),
                            })
                        }
                    }
                    PipeMessages::Batch(inputs)
                }
                None => PipeMessages::Single(input),
            }
        }
        None => PipeMessages::None,
    };

    let input_payloads = inputs.get_payloads_ref();

    #[instrument(level = Level::INFO, skip(function), err(Display))]
    async fn call_function<F>(
        function: &mut F,
        inputs: PipeMessages<<F as Function>::Input>,
    ) -> Result<PipeMessages<<F as Function>::Output>>
    where
        F: Function,
    {
        function.tick(inputs).await
    }

    match call_function(&mut ctx.function, inputs).await? {
        PipeMessages::None => Ok(()),
        outputs => {
            let mut writer = ctx.writer.clone();
            spawn(async move { writer.write_outputs(&input_payloads, outputs).await });
            ctx.writer.atomic_session.wait().await;
            Ok(())
        }
    }
}

#[derive(Copy, Clone, Debug, Display, EnumString, Default, Serialize, Deserialize)]
pub enum DefaultModelIn {
    ModelOut,
    #[default]
    Skip,
}

struct Timer {
    timeout: Duration,
    timestamp: Instant,
}

impl Timer {
    fn new(timeout: Duration) -> Self {
        Self {
            timeout,
            timestamp: Instant::now(),
        }
    }

    fn is_outdated(&self) -> bool {
        self.timestamp.elapsed() >= self.timeout
    }
}

struct Context<F>
where
    F: Function,
{
    batch_size: Option<usize>,
    batch_timeout: Option<Duration>,
    function: F,
    function_context: FunctionContext,
    reader: Option<ReadContext<<F as Function>::Input>>,
    storage: Arc<StorageIO>,
    writer: WriteContext,
}

struct ReadContext<Value>
where
    Value: Default,
{
    _job: JoinHandle<()>,
    rx: Receiver<PipeMessage<Value>>,
}

struct ReadSession<Value>
where
    Value: Default,
{
    function_context: FunctionContext,
    model_out: Option<Name>,
    storage: Arc<StorageSet>,
    stream: Box<dyn Subscriber<Value>>,
    tx: Arc<Sender<PipeMessage<Value>>>,
}

impl<Value> ReadSession<Value>
where
    Value: 'static + Send + Sync + Default + DeserializeOwned,
{
    async fn loop_forever(mut self) -> JoinHandle<()> {
        spawn(async move {
            loop {
                match self.read_input_one().await {
                    Ok(()) => yield_now().await,
                    Err(error) => {
                        warn!("failed to read inputs: {error}");
                        yield_now().await;
                    }
                }
            }
        })
    }

    #[instrument(level = Level::INFO, skip_all, err(Display))]
    async fn read_input_one(&mut self) -> Result<()> {
        #[instrument(level = Level::INFO, skip_all, err(Display))]
        async fn send_one<Value>(
            tx: &Sender<PipeMessage<Value>>,
            input: PipeMessage<Value>,
        ) -> Result<()>
        where
            Value: Default,
        {
            tx.send(input)
                .await
                .map_err(|error| anyhow!("failed to send input: {error}"))
        }

        match self
            .stream
            .read_one()
            .await?
            .map(|input| input.with_reply_target(&self.model_out))
        {
            Some(input) => {
                if self.function_context.is_disabled_load() || input.payloads.is_empty() {
                    send_one(&self.tx, input.drop_payloads()).await
                } else {
                    let storage = self.storage.clone();
                    let tx = self.tx.clone();
                    spawn(async move {
                        let input = input
                            .load_payloads(&storage)
                            .await
                            .map_err(|error| anyhow!("failed to read input: {error}"))?;
                        send_one(&tx, input).await
                    });
                    Ok(())
                }
            }
            None => Ok(()),
        }
    }
}

#[derive(Clone)]
struct WriteContext {
    atomic_session: AtomicSession,
    encoder: Codec,
    function_context: FunctionContext,
    storage: Arc<StorageSet>,
    stream: Option<Arc<dyn Publisher>>,
}

impl WriteContext {
    #[instrument(level = Level::INFO, skip_all)]
    async fn write_outputs<Value>(
        &mut self,
        input_payloads: &HashMap<String, PipePayload<()>>,
        messages: PipeMessages<Value>,
    ) where
        Value: Send + Sync + Default + Serialize + JsonSchema,
    {
        if let Err(error) = self.try_write_outputs(input_payloads, messages).await {
            error!("{error}");
        }
    }

    #[instrument(level = Level::INFO, skip_all, err(Display))]
    async fn try_write_outputs<Value>(
        &mut self,
        input_payloads: &HashMap<String, PipePayload<()>>,
        messages: PipeMessages<Value>,
    ) -> Result<()>
    where
        Value: Send + Sync + Default + Serialize + JsonSchema,
    {
        match self.stream.as_ref() {
            Some(stream) => {
                self.atomic_session
                    .alloc(async {
                        let outputs = if self.function_context.is_disabled_store() {
                            messages.drop_payloads()
                        } else {
                            messages
                                .dump_payloads(&self.storage, input_payloads)
                                .await?
                        }
                        .into_vec();

                        for output in outputs {
                            if !self.function_context.is_disabled_store_metadata() {
                                if let Err(error) = self
                                    .storage
                                    .get_default_metadata()
                                    .put_metadata(&[&output])
                                    .await
                                {
                                    warn!("{error}");
                                }
                            }

                            let data = output
                                .to_bytes(self.encoder)
                                .map_err(|error| anyhow!("failed to parse output: {error}"))?;
                            stream.reply_or_send_one(data, output.reply).await?
                        }
                        Ok(())
                    })
                    .await
            }
            None => Ok(()),
        }
    }
}

#[derive(Clone)]
struct AtomicSession {
    max_tasks: usize,
    num_tasks: Arc<AtomicUsize>,
}

impl AtomicSession {
    fn new(max_tasks: usize) -> Self {
        Self {
            max_tasks,
            num_tasks: Default::default(),
        }
    }

    #[instrument(level = Level::INFO, skip_all)]
    async fn alloc<F>(&self, task: F) -> <F as Future>::Output
    where
        F: Future,
    {
        self.num_tasks.fetch_add(1, Ordering::SeqCst);
        let result = task.await;
        self.num_tasks.fetch_sub(1, Ordering::SeqCst);
        result
    }

    #[instrument(name = "submit", level = Level::INFO, skip_all)]
    async fn wait(&self) {
        while self.num_tasks.load(Ordering::SeqCst) >= self.max_tasks {
            yield_now().await
        }
    }
}
