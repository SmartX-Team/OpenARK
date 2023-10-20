#[cfg(feature = "kafka")]
mod kafka;
#[cfg(feature = "nats")]
mod nats;

use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use bytes::Bytes;
use clap::Parser;
use schemars::JsonSchema;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use strum::{Display, EnumString};
use tracing::debug;

use crate::message::{Name, PipeMessage};

pub async fn init_messenger<Value>(
    args: &MessengerArgs,
) -> Result<Box<dyn Send + Sync + Messenger<Value>>> {
    debug!("Initializing Messenger IO");

    Ok(match args.default_messenger {
        #[cfg(feature = "kafka")]
        MessengerType::Kafka => Box::new(self::kafka::Messenger::try_new(&args.kafka).await?),
        #[cfg(feature = "nats")]
        MessengerType::Nats => Box::new(self::nats::Messenger::try_new(&args.nats).await?),
    })
}

#[async_trait]
pub trait Messenger<Value> {
    fn messenger_type(&self) -> MessengerType;

    async fn publish(&self, topic: Name) -> Result<Arc<dyn Publisher>>;

    async fn subscribe(&self, topic: Name) -> Result<Box<dyn Subscriber<Value>>>
    where
        Value: Send + Default + DeserializeOwned;

    async fn subscribe_queued(
        &self,
        topic: Name,
        _queue_group: Name,
    ) -> Result<Box<dyn Subscriber<Value>>>
    where
        Value: Send + Default + DeserializeOwned,
    {
        self.subscribe(topic).await
    }
}

#[async_trait]
pub trait Publisher
where
    Self: Send + Sync,
{
    async fn reply_one(&self, data: Bytes, reply: String) -> Result<()>;

    async fn send_one(&self, data: Bytes) -> Result<()>;
}

#[async_trait]
pub trait Subscriber<Value>
where
    Self: Send,
    Value: Send + Default + DeserializeOwned,
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
