use std::sync::Arc;

use anyhow::Result;
use ark_core_k8s::data::Name;
use async_trait::async_trait;
use clap::Parser;
use derivative::Derivative;
use schemars::JsonSchema;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use tracing::{instrument, Level};

use crate::{
    message::{Codec, DynValue, PipeMessage},
    messengers::{init_messenger, Messenger, MessengerArgs, Publisher, Subscriber},
    storage::{MetadataStorageArgs, MetadataStorageType, StorageArgs, StorageSet},
};

#[derive(Derivative)]
#[derivative(Debug)]
pub struct PipeClient<Value = DynValue> {
    encoder: Codec,

    #[derivative(Debug = "ignore")]
    messenger: Box<dyn Messenger<Value>>,

    #[derivative(Debug = "ignore")]
    storage: Arc<StorageSet>,
}

impl PipeClient {
    #[instrument(level = Level::INFO)]
    pub async fn try_default_dynamic() -> ::anyhow::Result<Self> {
        Self::try_default().await
    }

    #[instrument(level = Level::INFO, skip(args))]
    pub async fn try_new_dynamic(args: &PipeClientArgs) -> ::anyhow::Result<Self> {
        Self::try_new(args).await
    }
}

impl<Value> PipeClient<Value> {
    #[instrument(level = Level::INFO)]
    pub async fn try_default() -> ::anyhow::Result<Self>
    where
        Value: JsonSchema,
    {
        let args = PipeClientArgs::try_parse()?;
        Self::try_new(&args).await
    }

    #[instrument(level = Level::INFO, skip(args))]
    pub async fn try_new(args: &PipeClientArgs) -> ::anyhow::Result<Self>
    where
        Value: JsonSchema,
    {
        let PipeClientArgs {
            extra:
                PipeClientExtraArgs {
                    encoder,
                    default_metadata_type,
                },
            messenger,
            storage,
        } = args;

        Ok(Self {
            encoder: encoder.unwrap_or_default(),
            messenger: init_messenger(messenger).await?,
            storage: Arc::new(
                StorageSet::try_new(
                    storage,
                    None,
                    None,
                    MetadataStorageArgs::<Value>::new(*default_metadata_type),
                )
                .await?,
            ),
        })
    }

    #[instrument(level = Level::INFO, skip(self))]
    pub async fn publish(&self, topic: Name) -> Result<PipePublisher> {
        let inner = self.messenger.publish(topic).await?;

        Ok(PipePublisher {
            encoder: self.encoder,
            topic: inner.topic().clone(),
            inner,
            storage: self.storage.clone(),
        })
    }

    #[instrument(level = Level::INFO, skip(self))]
    pub async fn subscribe(&self, topic: Name) -> Result<PipeSubscriber<Value>>
    where
        Value: Send + DeserializeOwned,
    {
        let inner = self.messenger.subscribe(topic).await?;

        Ok(PipeSubscriber {
            topic: inner.topic().clone(),
            inner,
            storage: self.storage.clone(),
        })
    }

    #[instrument(level = Level::INFO, skip(self, data))]
    pub async fn call<ValueOut>(
        &self,
        topic: Name,
        data: PipeMessage,
    ) -> Result<PipeMessage<ValueOut>>
    where
        ValueOut: Send + DeserializeOwned,
    {
        self.publish(topic).await?.request_one(data).await
    }

    #[instrument(level = Level::INFO, skip(self))]
    pub async fn read(&self, topic: Name) -> Result<Option<PipeMessage<Value>>>
    where
        Value: Send + DeserializeOwned,
    {
        self.subscribe(topic).await?.read_one().await
    }

    pub const fn storage(&self) -> &Arc<StorageSet> {
        &self.storage
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, Parser)]
pub struct PipeClientArgs {
    #[command(flatten)]
    pub extra: PipeClientExtraArgs,

    #[command(flatten)]
    pub messenger: MessengerArgs,

    #[command(flatten)]
    pub storage: StorageArgs,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, Parser)]
pub struct PipeClientExtraArgs {
    #[arg(
        long,
        env = "PIPE_DEFAULT_METADATA_TYPE",
        value_name = "TYPE",
        default_value_t = MetadataStorageType::default(),
    )]
    #[serde(default)]
    pub default_metadata_type: MetadataStorageType,

    #[arg(long, env = "PIPE_ENCODER", value_name = "CODEC")]
    #[serde(default)]
    pub encoder: Option<Codec>,
}

#[derive(Clone)]
pub struct PipePublisher {
    encoder: Codec,
    inner: Arc<dyn Publisher>,
    topic: Name,
    storage: Arc<StorageSet>,
}

#[async_trait]
impl<Value, ValueOut> Publisher<PipeMessage<Value>, PipeMessage<ValueOut>> for PipePublisher
where
    Value: Send + Serialize,
    ValueOut: Send + DeserializeOwned,
{
    fn topic(&self) -> &Name {
        &self.topic
    }

    #[instrument(
        level = Level::INFO,
        skip_all,
        fields(
            message.len = %1,
            message.model = %self.topic.as_str(),
        ),
        err(Display),
    )]
    async fn reply_one(&self, message: PipeMessage<Value>, inbox: String) -> Result<()>
    where
        Value: 'async_trait,
    {
        let message = message
            .dump_payloads(&self.storage, Some(&self.topic), None)
            .await?;
        let data = message.to_bytes(self.encoder)?;
        self.inner.reply_one(data, inbox).await
    }

    #[instrument(
        level = Level::INFO,
        skip_all,
        fields(
            message.len = %1,
            message.model = %self.topic.as_str(),
        ),
        err(Display),
    )]
    async fn request_one(&self, message: PipeMessage<Value>) -> Result<PipeMessage<ValueOut>>
    where
        Value: 'async_trait,
        ValueOut: 'async_trait,
    {
        let message_req = message
            .dump_payloads(&self.storage, Some(&self.topic), None)
            .await?;
        let data_req = message_req.to_bytes(self.encoder)?;

        let data_res = self.inner.request_one(data_req).await?;
        let message_res: PipeMessage<ValueOut> = data_res.try_into()?;
        message_res.load_payloads(&self.storage).await
    }

    #[instrument(
        level = Level::INFO,
        skip_all,
        fields(
            message.len = %1,
            message.model = %self.topic.as_str(),
        ),
        err(Display),
    )]
    async fn send_one(&self, message: PipeMessage<Value>) -> Result<()>
    where
        Value: 'async_trait,
    {
        let message = message
            .dump_payloads(&self.storage, Some(&self.topic), None)
            .await?;
        let data = message.to_bytes(self.encoder)?;
        self.inner.send_one(data).await
    }

    #[instrument(
        level = Level::INFO,
        skip_all,
        fields(
            message.len = %1usize,
            message.model = %self.topic.as_str(),
        ),
        err(Display),
    )]
    async fn flush(&self) -> Result<()> {
        self.inner.flush().await
    }
}

pub struct PipeSubscriber<Value> {
    inner: Box<dyn Subscriber<Value>>,
    topic: Name,
    storage: Arc<StorageSet>,
}

#[async_trait]
impl<Value> Subscriber<Value> for PipeSubscriber<Value>
where
    Value: Send + DeserializeOwned,
{
    fn topic(&self) -> &Name {
        &self.topic
    }

    #[instrument(
        level = Level::INFO,
        skip_all,
        fields(
            data.len = %1usize,
            data.model = %self.topic.as_str(),
        ),
        err(Display),
    )]
    async fn read_one(&mut self) -> Result<Option<PipeMessage<Value>>> {
        match self.inner.read_one().await? {
            Some(msg) => msg.load_payloads(&self.storage).await.map(Some),
            None => Ok(None),
        }
    }
}
