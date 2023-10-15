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

pub struct MessengerIO {
    default: MessengerType,
    #[cfg(feature = "nats")]
    nats: self::nats::Messenger,
}

impl MessengerIO {
    pub async fn try_new(args: &MessengerArgs) -> Result<Self> {
        debug!("Initializing Messenger IO");
        Ok(Self {
            default: args.default_messenger,
            nats: self::nats::Messenger::try_new(&args.nats).await?,
        })
    }

    pub fn get<Value>(
        &mut self,
        messenger_type: MessengerType,
    ) -> &mut (dyn Send + Sync + Messenger<Value>) {
        match messenger_type {
            #[cfg(feature = "nats")]
            MessengerType::Nats => &mut self.nats,
        }
    }

    pub fn get_default<Value>(&mut self) -> &mut (dyn Send + Sync + Messenger<Value>) {
        self.get(self.default)
    }
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
        queue_group: Name,
    ) -> Result<Box<dyn Subscriber<Value>>>
    where
        Value: Send + Default + DeserializeOwned;
}

#[async_trait]
pub trait Publisher
where
    Self: Send + Sync,
{
    async fn send_one(&self, data: Bytes) -> Result<()>;
}

#[async_trait]
pub trait Subscriber<Value>
where
    Self: Send + Sync,
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
    #[cfg(feature = "nats")]
    Nats,
}

impl MessengerType {
    #[cfg(feature = "nats")]
    pub const DEFAULT: Self = Self::Nats;
}

#[derive(Clone, Debug, Serialize, Deserialize, Parser)]
pub struct MessengerArgs {
    #[arg(long, env = "PIPE_DEFAULT_MESSENGER", value_name = "TYPE", default_value_t = MessengerType::DEFAULT)]
    default_messenger: MessengerType,

    #[cfg(feature = "nats")]
    #[command(flatten)]
    nats: self::nats::MessengerNatsArgs,
}
