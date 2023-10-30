use std::sync::Arc;

use anyhow::{anyhow, bail, Result};
use ark_core_k8s::data::Name;
use async_trait::async_trait;
use bytes::Bytes;
use clap::Parser;
use rdkafka::{
    consumer::{Consumer, StreamConsumer},
    producer::{FutureProducer, FutureRecord},
    util::Timeout,
    ClientConfig, Message,
};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use tracing::debug;

use crate::message::PipeMessage;

pub struct Messenger {
    config: ClientConfig,
}

impl Messenger {
    pub async fn try_new(args: &MessengerNatsArgs) -> Result<Self> {
        debug!("Initializing Messenger IO - Kafka");

        let mut config = ClientConfig::new();
        config
            .set("bootstrap.servers", args.kafka_hosts.join(","))
            .set("enable.auto.commit", "true");
        Ok(Self { config })
    }
}

#[async_trait]
impl<Value> super::Messenger<Value> for Messenger {
    fn messenger_type(&self) -> super::MessengerType {
        super::MessengerType::Kafka
    }

    async fn publish(&self, topic: Name) -> Result<Arc<dyn super::Publisher>> {
        Ok(Arc::new(Publisher {
            client: self.config.create()?,
            topic: topic.into(),
        }))
    }

    async fn subscribe(&self, topic: Name) -> Result<Box<dyn super::Subscriber<Value>>>
    where
        Value: Send + Default + DeserializeOwned,
    {
        let consumer: StreamConsumer = self.config.create()?;
        consumer
            .subscribe(&[&topic])
            .map_err(|error| anyhow!("failed to subscribe Kafka topic: {error}"))?;
        Ok(Box::new(consumer))
    }
}

pub struct Publisher {
    client: FutureProducer,
    topic: String,
}

#[async_trait]
impl super::Publisher for Publisher {
    async fn reply_one(&self, _data: Bytes, _reply: String) -> Result<()> {
        bail!("cannot reply with Kafka")
    }

    async fn request_one(&self, _data: Bytes) -> Result<Bytes> {
        bail!("cannot request with Kafka")
    }

    async fn send_one(&self, data: Bytes) -> Result<()> {
        self.client
            .send(
                FutureRecord::<[u8], [u8]>::to(&self.topic).payload(&*data),
                Timeout::Never,
            )
            .await
            .map(|_| ())
            .map_err(|(error, _)| anyhow!("failed to publish data to Kafka: {error}"))
    }
}

pub type Subscriber = StreamConsumer;

#[async_trait]
impl<Value> super::Subscriber<Value> for Subscriber
where
    Self: Send + Sync,
    Value: Send + Default + DeserializeOwned,
{
    async fn read_one(&mut self) -> Result<Option<PipeMessage<Value, ()>>> {
        self.recv()
            .await
            .map_err(|error| anyhow!("failed to subscribe Kafka input: {error}"))
            .and_then(|message| message.payload().unwrap_or_default().try_into().map(Some))
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, Parser)]
pub struct MessengerNatsArgs {
    #[arg(long, env = "KAFKA_HOSTS", value_name = "ADDR")]
    kafka_hosts: Vec<String>,
}
