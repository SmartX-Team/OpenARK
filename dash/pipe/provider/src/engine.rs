use std::{future::Future, sync::Arc};

use anyhow::{anyhow, bail, Result};
use clap::{ArgAction, Args, Parser};
use futures::StreamExt;
use log::warn;
use nats::{Client, ServerAddr, Subscriber, ToServerAddrs};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use tokio::task::yield_now;

use crate::{
    message::PipeMessages,
    storage::{StorageSet, StorageType},
    PipeMessage,
};

#[derive(Clone, Debug, Parser, Serialize, Deserialize)]
pub struct PipeEngine<FunctionArgs = EmptyArgs>
where
    FunctionArgs: Args,
{
    #[arg(long, env = "NATS_ADDRS", value_name = "ADDR")]
    addrs: Vec<String>,

    #[arg(long, env = "PIPE_BATCH_SIZE", value_name = "BATCH_SIZE")]
    #[serde(default)]
    batch_size: Option<usize>,

    #[command(flatten)]
    function_args: FunctionArgs,

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

impl<FunctionArgs> PipeEngine<FunctionArgs>
where
    FunctionArgs: Args,
{
    pub fn from_env() -> Self {
        Self::parse()
    }

    pub fn with_batch_size(mut self, batch_size: usize) -> Self {
        self.batch_size = Some(batch_size);
        self
    }

    pub fn with_function_args(mut self, function_args: FunctionArgs) -> Self {
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

impl<FunctionArgs> PipeEngine<FunctionArgs>
where
    FunctionArgs: Clone + Args,
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

    async fn init_context(&self) -> Result<Context<FunctionArgs>> {
        let client = ::nats::connect(self.parse_addrs()?)
            .await
            .map_err(|error| anyhow!("failed to init NATS client: {error}"))?;

        Ok(Context {
            batch_size: self.batch_size,
            function_args: self.function_args.clone().into(),
            storage: {
                let default_output = match self.persistence {
                    Some(true) => StorageType::LakeHouse,
                    Some(false) | None => StorageType::Nats,
                };
                StorageSet::try_new(&self.storage, &client, default_output).await?
            },
            stream_input: match &self.stream_in {
                Some(stream) => client
                    .subscribe(stream.clone())
                    .await
                    .map(Some)
                    .map_err(|error| anyhow!("failed to init NATS input stream: {error}"))?,
                None => None,
            },
            stream_output: self.stream_out.clone(),
            client,
        })
    }

    pub fn loop_forever<F, Fut, Input, Output>(&self, tick: F)
    where
        F: Fn(Arc<FunctionArgs>, PipeMessages<Input>) -> Fut,
        Fut: Future<Output = Result<Option<PipeMessages<Output>>>>,
        Input: DeserializeOwned,
        Output: Serialize,
    {
        ::ark_core::logger::init_once();

        if let Err(error) = ::tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("failed to init tokio runtime")
            .block_on(self.loop_forever_async(tick))
        {
            panic!("{error}")
        }
    }

    async fn loop_forever_async<F, Fut, Input, Output>(&self, tick: F) -> Result<()>
    where
        F: Fn(Arc<FunctionArgs>, PipeMessages<Input>) -> Fut,
        Fut: Future<Output = Result<Option<PipeMessages<Output>>>>,
        Input: DeserializeOwned,
        Output: Serialize,
    {
        let mut ctx = self.init_context().await?;

        loop {
            // yield per every loop
            yield_now().await;

            if let Err(error) = self.tick_async(&mut ctx, &tick).await {
                warn!("{error}")
            }
        }
    }

    async fn tick_async<F, Fut, Input, Output>(
        &self,
        ctx: &mut Context<FunctionArgs>,
        f: F,
    ) -> Result<()>
    where
        F: Fn(Arc<FunctionArgs>, PipeMessages<Input>) -> Fut,
        Fut: Future<Output = Result<Option<PipeMessages<Output>>>>,
        Input: DeserializeOwned,
        Output: Serialize,
    {
        match ctx.read_inputs().await? {
            Some(inputs) => match f(ctx.function_args.clone(), inputs).await? {
                Some(outputs) => ctx.write_outputs(outputs).await,
                None => Ok(()),
            },
            None => Ok(()),
        }
    }
}

struct Context<FunctionArgs> {
    batch_size: Option<usize>,
    client: Client,
    function_args: Arc<FunctionArgs>,
    storage: StorageSet,
    stream_input: Option<Subscriber>,
    stream_output: Option<String>,
}

impl<FunctionArgs> Context<FunctionArgs> {
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
            None => Ok(None),
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

    async fn write_outputs<Value>(&mut self, messages: PipeMessages<Value>) -> Result<()>
    where
        Value: Serialize,
    {
        match &self.stream_output {
            Some(stream) => {
                for output in messages.dump_payloads(&self.storage).await?.into_vec() {
                    let output = output
                        .to_json_bytes()
                        .map_err(|error| anyhow!("failed to parse NATS output: {error}"))?;
                    self.client
                        .publish(stream.clone(), output)
                        .await
                        .map_err(|error| anyhow!("failed to publish NATS output: {error}"))?
                }
                Ok(())
            }
            None => Ok(()),
        }
    }
}

#[derive(Clone, Debug, Parser, Serialize, Deserialize)]
pub struct EmptyArgs {}
