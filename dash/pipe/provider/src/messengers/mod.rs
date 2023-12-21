#[cfg(feature = "kafka")]
mod kafka;
#[cfg(feature = "nats")]
mod nats;

use std::sync::Arc;

use anyhow::{anyhow, Result};
use ark_core_k8s::data::Name;
use async_trait::async_trait;
use bytes::Bytes;
use clap::Parser;
use schemars::JsonSchema;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use strum::{Display, EnumString};
use tracing::{debug, instrument, Level};

use crate::message::{PipeMessage, PipeReply};

#[instrument(level = Level::INFO, skip_all, err(Display))]
pub async fn init_messenger<Value>(args: &MessengerArgs) -> Result<Box<dyn Messenger<Value>>> {
    debug!("Initializing Messenger IO");

    Ok(match args.default_messenger {
        #[cfg(feature = "kafka")]
        MessengerType::Kafka => Box::new(self::kafka::Messenger::try_new(&args.kafka)?),
        #[cfg(feature = "nats")]
        MessengerType::Nats => Box::new(self::nats::Messenger::try_new(&args.nats).await?),
    })
}

#[async_trait]
pub trait Messenger<Value = ::serde_json::Value>
where
    Self: Send + Sync,
{
    fn messenger_type(&self) -> MessengerType;

    async fn publish(&self, namespace: String, topic: Name) -> Result<Arc<dyn Publisher>>;

    async fn subscribe(&self, namespace: String, topic: Name) -> Result<Box<dyn Subscriber<Value>>>
    where
        Value: Send + DeserializeOwned;

    #[instrument(level = Level::INFO, skip_all, err(Display))]
    async fn subscribe_queued(
        &self,
        namespace: String,
        topic: Name,
        _queue_group: Name,
    ) -> Result<Box<dyn Subscriber<Value>>>
    where
        Value: Send + DeserializeOwned,
    {
        self.subscribe(namespace, topic).await
    }
}

#[async_trait]
impl<T, Value> Messenger<Value> for &T
where
    T: ?Sized + Messenger<Value>,
{
    fn messenger_type(&self) -> MessengerType {
        <T as Messenger<Value>>::messenger_type(*self)
    }

    #[instrument(level = Level::INFO, skip_all, err(Display))]
    async fn publish(&self, namespace: String, topic: Name) -> Result<Arc<dyn Publisher>> {
        <T as Messenger<Value>>::publish(*self, namespace, topic).await
    }

    #[instrument(level = Level::INFO, skip_all, err(Display))]
    async fn subscribe(&self, namespace: String, topic: Name) -> Result<Box<dyn Subscriber<Value>>>
    where
        Value: Send + DeserializeOwned,
    {
        <T as Messenger<Value>>::subscribe(*self, namespace, topic).await
    }

    #[instrument(level = Level::INFO, skip_all, err(Display))]
    async fn subscribe_queued(
        &self,
        namespace: String,
        topic: Name,
        queue_group: Name,
    ) -> Result<Box<dyn Subscriber<Value>>>
    where
        Value: Send + DeserializeOwned,
    {
        <T as Messenger<Value>>::subscribe_queued(*self, namespace, topic, queue_group).await
    }
}

#[async_trait]
impl<Value> Messenger<Value> for Box<dyn Messenger<Value>> {
    fn messenger_type(&self) -> MessengerType {
        self.as_ref().messenger_type()
    }

    #[instrument(level = Level::INFO, skip_all, err(Display))]
    async fn publish(&self, namespace: String, topic: Name) -> Result<Arc<dyn Publisher>> {
        self.as_ref().publish(namespace, topic).await
    }

    #[instrument(level = Level::INFO, skip_all, err(Display))]
    async fn subscribe(&self, namespace: String, topic: Name) -> Result<Box<dyn Subscriber<Value>>>
    where
        Value: Send + DeserializeOwned,
    {
        self.as_ref().subscribe(namespace, topic).await
    }

    #[instrument(level = Level::INFO, skip_all, err(Display))]
    async fn subscribe_queued(
        &self,
        namespace: String,
        topic: Name,
        queue_group: Name,
    ) -> Result<Box<dyn Subscriber<Value>>>
    where
        Value: Send + DeserializeOwned,
    {
        self.as_ref()
            .subscribe_queued(namespace, topic, queue_group)
            .await
    }
}

#[async_trait]
pub trait Publisher
where
    Self: Send + Sync,
{
    fn topic(&self) -> &Name;

    async fn reply_one(&self, data: Bytes, inbox: String) -> Result<()>;

    async fn request_one(&self, data: Bytes) -> Result<Bytes>;

    async fn send_one(&self, data: Bytes) -> Result<()>;
}

#[async_trait]
pub trait PublisherExt
where
    Self: Send + Sync,
{
    async fn reply_or_send_one(&self, data: Bytes, reply: Option<PipeReply>) -> Result<()>;
}

#[async_trait]
impl PublisherExt for Arc<dyn Publisher> {
    #[instrument(level = Level::INFO, skip_all, err(Display))]
    async fn reply_or_send_one(&self, data: Bytes, reply: Option<PipeReply>) -> Result<()> {
        match reply {
            Some(PipeReply { inbox, target }) if Some(self.topic()) == target.as_ref() => self
                .reply_one(data, inbox)
                .await
                .map_err(|error| anyhow!("failed to reply output: {error}")),
            Some(_) | None => self
                .send_one(data)
                .await
                .map_err(|error| anyhow!("failed to send output: {error}")),
        }
    }
}

#[async_trait]
pub trait Subscriber<Value>
where
    Self: Send,
    Value: Send + DeserializeOwned,
{
    async fn read_one(&mut self) -> Result<Option<PipeMessage<Value, ()>>>;
}

#[derive(
    Copy,
    Clone,
    Debug,
    Display,
    EnumString,
    Default,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    JsonSchema,
)]
pub enum MessengerType {
    #[cfg(feature = "kafka")]
    #[cfg_attr(all(not(feature = "nats"), feature = "kafka"), default)]
    Kafka,

    #[cfg(feature = "nats")]
    #[cfg_attr(feature = "nats", default)]
    Nats,
}

#[derive(Clone, Debug, Serialize, Deserialize, Parser)]
pub struct MessengerArgs {
    #[arg(long, env = "PIPE_DEFAULT_MESSENGER", value_name = "TYPE", default_value_t = Default::default())]
    default_messenger: MessengerType,

    #[cfg(feature = "kafka")]
    #[command(flatten)]
    kafka: self::kafka::MessengerNatsArgs,

    #[cfg(feature = "nats")]
    #[command(flatten)]
    nats: self::nats::MessengerNatsArgs,
}
