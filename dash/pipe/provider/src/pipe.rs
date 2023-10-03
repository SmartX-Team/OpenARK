use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

use anyhow::{anyhow, bail, Result};
use clap::{ArgAction, Parser};
use futures::{Future, StreamExt};
use nats::{Client, ServerAddr, Subscriber, ToServerAddrs};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use tokio::{spawn, task::yield_now};
use tracing::warn;

use crate::{
    function::Function,
    message::PipeMessages,
    storage::{StorageSet, StorageType},
    PipeMessage,
};

#[derive(Clone, Debug, Parser, Serialize, Deserialize)]
pub struct PipeArgs<F>
where
    F: Function,
{
    #[arg(long, env = "NATS_ADDRS", value_name = "ADDR")]
    addrs: Vec<String>,

    #[arg(long, env = "PIPE_BATCH_SIZE", value_name = "BATCH_SIZE")]
    #[serde(default)]
    batch_size: Option<usize>,

    #[command(flatten)]
    function_args: <F as Function>::Args,

    #[arg(long, env = "PIPE_BATCH_SIZE", value_name = "NUM")]
    #[serde(default)]
    max_tasks: Option<usize>,

    #[arg(long, env = "PIPE_PERSISTENCE", action = ArgAction::SetTrue)]
    #[serde(default)]
    persistence: Option<bool>,

    #[arg(long, env = "PIPE_REPLY", action = ArgAction::SetTrue)]
    #[serde(default)]
    reply: Option<bool>,

    #[command(flatten)]
    storage: crate::storage::StorageArgs,

    #[arg(long, env = "PIPE_STREAM_IN", value_name = "NAME")]
    #[serde(default)]
    stream_in: Option<String>,

    #[arg(long, env = "PIPE_STREAM_OUT", value_name = "NAME")]
    #[serde(default)]
    stream_out: Option<String>,
}

impl<F> PipeArgs<F>
where
    F: Function,
{
    pub fn from_env() -> Self {
        Self::parse()
    }

    pub fn with_batch_size(mut self, batch_size: usize) -> Self {
        self.batch_size = Some(batch_size);
        self
    }

    pub fn with_function_args(mut self, function_args: <F as Function>::Args) -> Self {
        self.function_args = function_args;
        self
    }

    pub fn with_persistence(mut self, persistence: bool) -> Self {
        self.persistence = Some(persistence);
        self
    }

    pub fn with_reply(mut self, reply: bool) -> Self {
        self.reply = Some(reply);
        self
    }

    pub fn with_stream_in(mut self, stream_in: String) -> Self {
        self.stream_in = Some(stream_in);
        self
    }

    pub fn with_stream_out(mut self, stream_out: String) -> Self {
        self.stream_out = Some(stream_out);
        self
    }
}

impl<F> PipeArgs<F>
where
    F: Function,
{
    fn parse_addrs(&self) -> Result<Vec<ServerAddr>> {
        let addrs = self
            .addrs
            .iter()
            .flat_map(|addr| {
                addr.to_server_addrs()
                    .map_err(|error| anyhow!("failed to parse NATS address: {error}"))
            })
            .flatten()
            .collect::<Vec<_>>();
        if addrs.is_empty() {
            bail!("failed to parse NATS address: no available addresses");
        } else {
            Ok(addrs)
        }
    }

    async fn init_context(&self) -> Result<Context<F>> {
        let client = ::nats::connect(self.parse_addrs()?)
            .await
            .map_err(|error| anyhow!("failed to init NATS client: {error}"))?;

        let storage = Arc::new({
            let default_output = match self.persistence {
                Some(true) => StorageType::PERSISTENT,
                Some(false) | None => StorageType::TEMPORARY,
            };
            StorageSet::try_new(&self.storage, &client, default_output).await?
        });

        Ok(Context {
            function: <F as Function>::try_new(&self.function_args)
                .await
                .map(Into::into)
                .map_err(|error| anyhow!("failed to init function: {error}"))?,
            reader: ReadContext {
                batch_size: self.batch_size,
                storage: storage.clone(),
                stream_input: match &self.stream_in {
                    Some(stream) => client
                        .subscribe(stream.clone())
                        .await
                        .map(Some)
                        .map_err(|error| anyhow!("failed to init NATS input stream: {error}"))?,
                    None => None,
                },
            },
            writer: WriteContext {
                atomic_session: AtomicSession::new(
                    self.batch_size.unwrap_or(1) * self.max_tasks.unwrap_or(8),
                ),
                client,
                storage,
                stream_output: self.stream_out.clone(),
            },
        })
    }

    pub fn loop_forever(&self) {
        ::ark_core::tracer::init_once();

        if let Err(error) = ::tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("failed to init tokio runtime")
            .block_on(self.loop_forever_async())
        {
            panic!("{error}")
        }
    }

    async fn loop_forever_async(&self) -> Result<()> {
        let mut ctx = self.init_context().await?;

        loop {
            // yield per every loop
            yield_now().await;

            if let Err(error) = tick_async(&mut ctx).await {
                warn!("{error}")
            }
        }
    }
}

async fn tick_async<F>(ctx: &mut Context<F>) -> Result<()>
where
    F: Function,
{
    match ctx.reader.read_inputs().await? {
        Some(inputs) => match ctx.function.tick(inputs).await? {
            PipeMessages::None => Ok(()),
            outputs => {
                let mut writer = ctx.writer.clone();
                spawn(async move { writer.write_outputs(outputs).await });
                ctx.writer.atomic_session.wait().await;
                Ok(())
            }
        },
        None => Ok(()),
    }
}

struct Context<F> {
    function: F,
    reader: ReadContext,
    writer: WriteContext,
}

struct ReadContext {
    batch_size: Option<usize>,
    storage: Arc<StorageSet>,
    stream_input: Option<Subscriber>,
}

impl ReadContext {
    async fn read_inputs<Value>(&mut self) -> Result<Option<PipeMessages<Value>>>
    where
        Value: DeserializeOwned,
    {
        match self.read_message_batch().await? {
            Some(inputs) => inputs
                .load_payloads(&self.storage)
                .await
                .map(Some)
                .map_err(|error| anyhow!("failed to read NATS input: {error}")),
            None => Ok(None),
        }
    }

    async fn read_message_batch<Value>(&mut self) -> Result<Option<PipeMessages<Value, ()>>>
    where
        Value: DeserializeOwned,
    {
        match &self.stream_input {
            Some(_) => match self.batch_size {
                Some(batch_size) => {
                    let mut inputs = vec![];
                    for _ in 0..batch_size {
                        match self.read_message_once().await? {
                            Some(input) => inputs.push(input),
                            None => break,
                        }
                    }

                    if inputs.is_empty() {
                        Ok(None)
                    } else {
                        Ok(Some(PipeMessages::Batch(inputs)))
                    }
                }
                None => match self.read_message_once().await? {
                    Some(input) => Ok(Some(PipeMessages::Single(input))),
                    None => Ok(None),
                },
            },
            None => Ok(Some(PipeMessages::None)),
        }
    }

    async fn read_message_once<Value>(&mut self) -> Result<Option<PipeMessage<Value, ()>>>
    where
        Value: DeserializeOwned,
    {
        match &mut self.stream_input {
            Some(stream) => stream
                .next()
                .await
                .map(TryInto::try_into)
                .transpose()
                .map_err(|error| anyhow!("failed to subscribe NATS input: {error}")),
            None => Ok(None),
        }
    }
}

#[derive(Clone)]
struct WriteContext {
    atomic_session: AtomicSession,
    client: Client,
    storage: Arc<StorageSet>,
    stream_output: Option<String>,
}

impl WriteContext {
    async fn write_outputs<Value>(&mut self, messages: PipeMessages<Value>) -> Result<()>
    where
        Value: Serialize,
    {
        match &self.stream_output {
            Some(stream) => {
                self.atomic_session
                    .alloc(async {
                        for output in messages.dump_payloads(&self.storage).await?.into_vec() {
                            let output = output
                                .to_json_bytes()
                                .map_err(|error| anyhow!("failed to parse NATS output: {error}"))?;
                            self.client
                                .publish(stream.clone(), output)
                                .await
                                .map_err(|error| {
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
