use std::{
    collections::HashMap,
    process::exit,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

use anyhow::{anyhow, Result};
use clap::{ArgAction, Parser};
use futures::Future;
use schemars::JsonSchema;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use strum::{Display, EnumString};
use tokio::{
    select, spawn,
    sync::mpsc::{self, Receiver, Sender},
    task::{yield_now, JoinHandle},
    time::sleep,
};
use tracing::{debug, error, info, warn};

use crate::{
    function::{Function, FunctionContext},
    message::{Name, PipeMessages},
    messengers::{MessengerArgs, MessengerIO, Publisher, Subscriber},
    storage::{MetadataStorageArgs, MetadataStorageType, StorageIO, StorageSet},
    PipeMessage, PipePayload,
};

#[derive(Clone, Debug, Serialize, Deserialize, Parser)]
pub struct PipeArgs<F>
where
    F: Function,
{
    #[arg(long, env = "PIPE_BATCH_SIZE", value_name = "BATCH_SIZE")]
    #[serde(default)]
    batch_size: Option<usize>,

    #[arg(long, env = "PIPE_BATCH_TIMEOUT", value_name = "MILLISECONDS")]
    #[serde(default)]
    batch_timeout_ms: Option<u64>,

    #[arg(long, env = "PIPE_DEFAULT_MODEL_IN", value_name = "POLICY")]
    #[serde(default)]
    default_model_in: Option<DefaultModelIn>,

    #[command(flatten)]
    function_args: <F as Function>::Args,

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

    #[arg(long, env = "PIPE_REPLY", action = ArgAction::SetTrue)]
    #[serde(default)]
    reply: Option<bool>,

    #[command(flatten)]
    storage: crate::storage::StorageArgs,
}

impl<F> PipeArgs<F>
where
    F: Function,
{
    pub fn from_env() -> Self {
        Self::parse()
    }

    pub fn default_max_tasks(&self) -> usize {
        self.max_tasks.unwrap_or(8)
    }

    pub fn with_batch_size(mut self, batch_size: usize) -> Self {
        self.batch_size = Some(batch_size);
        self
    }

    pub fn with_default_model_in(mut self, default_model_in: DefaultModelIn) -> Self {
        self.default_model_in = Some(default_model_in);
        self
    }

    pub fn with_function_args(mut self, function_args: <F as Function>::Args) -> Self {
        self.function_args = function_args;
        self
    }

    pub fn with_model_in(mut self, model_in: Name) -> Self {
        self.model_in = Some(model_in);
        self
    }

    pub fn with_model_out(mut self, model_out: Name) -> Self {
        self.model_out = Some(model_out);
        self
    }

    pub fn with_reply(mut self, reply: bool) -> Self {
        self.reply = Some(reply);
        self
    }
}

impl<F> PipeArgs<F>
where
    F: Function,
{
    async fn init_context(&self) -> Result<Context<F>> {
        let mut messenger = MessengerIO::try_new(&self.messenger_args).await?;
        let messenger = messenger.get_default();

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
                    StorageSet::try_new(&self.storage, model, default_metadata).await?
                }),
                output: Arc::new({
                    let default_metadata =
                        MetadataStorageArgs::<<F as Function>::Output>::new(default_metadata_type);
                    let model = self.model_out.as_ref();

                    StorageSet::try_new(&self.storage, model, default_metadata).await?
                }),
            }
        });

        debug!("Initializing Function Context");
        let mut function_context = FunctionContext::default();
        function_context.clone().trap_on_sigint()?;

        debug!("Initializing Function");
        let function =
            <F as Function>::try_new(&self.function_args, &mut function_context, &storage)
                .await
                .map(Into::into)
                .map_err(|error| anyhow!("failed to init function: {error}"))?;

        debug!("Initializing Reader");
        let reader = match self.model_in.as_ref() {
            Some(model) => {
                let (tx, rx) = mpsc::channel(max_tasks);

                Some(ReadContext {
                    _job: ReadSession {
                        function_context: function_context.clone(),
                        storage: storage.input.clone(),
                        stream: if self.queue_group {
                            messenger
                                .subscribe_queued(model.clone(), model.clone())
                                .await
                        } else {
                            messenger.subscribe(model.clone()).await
                        }
                        .map_err(|error| anyhow!("failed to init NATS input stream: {error}"))?,
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

    pub fn loop_forever(&self) {
        ::ark_core::tracer::init_once();

        match ::tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("failed to init tokio runtime")
            .block_on(self.loop_forever_async())
        {
            Ok(()) => {
                info!("Terminated.");
            }
            Err(error) => {
                error!("{error}");
                exit(1)
            }
        }
    }

    async fn loop_forever_async(&self) -> Result<()> {
        let mut ctx = self.init_context().await?;
        info!("Initialized!");

        loop {
            // yield per every loop
            yield_now().await;

            if ctx.function_context.is_terminating() {
                break ctx.storage.flush(&ctx.function_context).await;
            }

            let response = tick_async(&mut ctx).await;
            if ctx.function_context.is_terminating() {
                let flush_response = ctx.storage.flush(&ctx.function_context).await;
                break response.and(flush_response);
            } else if let Err(error) = response {
                warn!("{error}");
            }
        }
    }
}

async fn tick_async<F>(ctx: &mut Context<F>) -> Result<()>
where
    F: Function,
{
    async fn recv_one<Value>(
        function_context: &FunctionContext,
        reader: &mut ReadContext<Value>,
    ) -> Result<Option<PipeMessage<Value>>>
    where
        Value: Default,
    {
        loop {
            select! {
                input = reader
                .rx
                .recv() => break Ok(input),
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

    match ctx.function.tick(inputs).await? {
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
                        error!("failed to read inputs: {error}");
                        break;
                    }
                }
            }
        })
    }

    async fn read_input_one(&mut self) -> Result<()> {
        async fn send_one<Value>(
            tx: &Sender<PipeMessage<Value>>,
            input: PipeMessage<Value>,
        ) -> Result<()>
        where
            Value: Default,
        {
            tx.send(input)
                .await
                .map_err(|error| anyhow!("failed to send NATS input: {error}"))
        }

        match self.stream.read_one().await? {
            Some(input) => {
                if self.function_context.is_disabled_load() || input.payloads.is_empty() {
                    send_one(&self.tx, input.load_payloads_as_empty()).await
                } else {
                    let storage = self.storage.clone();
                    let tx = self.tx.clone();
                    spawn(async move {
                        let input = input
                            .load_payloads(&storage)
                            .await
                            .map_err(|error| anyhow!("failed to read NATS input: {error}"))?;
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
    function_context: FunctionContext,
    storage: Arc<StorageSet>,
    stream: Option<Arc<dyn Publisher>>,
}

impl WriteContext {
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
                        let outputs = messages
                            .dump_payloads(&self.storage, input_payloads)
                            .await?
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

                            let output = output
                                .to_json_bytes()
                                .map_err(|error| anyhow!("failed to parse NATS output: {error}"))?;
                            stream.send_one(output).await.map_err(|error| {
                                anyhow!("failed to publish NATS output: {error}")
                            })?;
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

    async fn alloc<F>(&self, task: F) -> <F as Future>::Output
    where
        F: Future,
    {
        self.num_tasks.fetch_add(1, Ordering::SeqCst);
        let result = task.await;
        self.num_tasks.fetch_sub(1, Ordering::SeqCst);
        result
    }

    async fn wait(&self) {
        while self.num_tasks.load(Ordering::SeqCst) >= self.max_tasks {
            yield_now().await
        }
    }
}
