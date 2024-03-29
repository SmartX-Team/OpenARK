use std::sync::Arc;

use anyhow::{anyhow, bail, Result};
use ark_core_k8s::data::Name;
use async_trait::async_trait;
use bytes::Bytes;
use clap::Parser;
use rdkafka::{
    consumer::{Consumer, StreamConsumer},
    producer::{FutureProducer, FutureRecord, Producer},
    util::Timeout,
    ClientConfig, Message,
};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use tracing::{debug, instrument, Level};

use crate::message::PipeMessage;

pub struct Messenger {
    config: ClientConfig,
}

impl Messenger {
    pub fn try_new(args: &MessengerKafkaArgs) -> Result<Self> {
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

    #[instrument(level = Level::INFO, skip_all, err(Display))]
    async fn publish(&self, topic: Name) -> Result<Arc<dyn super::Publisher>> {
        Ok(Arc::new(Publisher {
            client: self.config.create()?,
            topic,
        }))
    }

    #[instrument(level = Level::INFO, skip_all, err(Display))]
    async fn subscribe(&self, topic: Name) -> Result<Box<dyn super::Subscriber<Value>>>
    where
        Value: Send + DeserializeOwned,
    {
        let consumer: StreamConsumer = self.config.create()?;
        consumer
            .subscribe(&[&topic])
            .map_err(|error| anyhow!("failed to subscribe Kafka topic: {error}"))?;
        Ok(Box::new(Subscriber { consumer, topic }))
    }
}

pub struct Publisher {
    client: FutureProducer,
    topic: Name,
}

#[async_trait]
impl super::Publisher for Publisher {
    fn topic(&self) -> &Name {
        &self.topic
    }

    #[instrument(
        level = Level::INFO,
        skip_all,
        fields(
            data.len = %_data.len(),
            data.model = %self.topic.as_str(),
        ),
        err(Display),
    )]
    async fn reply_one(&self, _data: Bytes, _inbox: String) -> Result<()> {
        bail!("cannot reply with Kafka")
    }

    #[instrument(
        level = Level::INFO,
        skip_all,
        fields(
            data.len = %_data.len(),
            data.model = %self.topic.as_str(),
        ),
        err(Display),
    )]
    async fn request_one(&self, _data: Bytes) -> Result<Bytes> {
        bail!("cannot request with Kafka")
    }

    #[instrument(
        level = Level::INFO,
        skip_all,
        fields(
            data.len = %data.len(),
            data.model = %self.topic.as_str(),
        ),
        err(Display),
    )]
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

    #[instrument(
        level = Level::INFO,
        skip_all,
        fields(
            data.len = %1usize,
            data.model = %self.topic.as_str(),
        ),
        err(Display),
    )]
    async fn flush(&self) -> Result<()> {
        self.client
            .flush(Timeout::Never)
            .map_err(|error| anyhow!("failed to terminate Kafka publisher: {error}"))
    }
}

pub struct Subscriber {
    consumer: StreamConsumer,
    topic: Name,
}

#[async_trait]
impl<Value> super::Subscriber<Value> for Subscriber
where
    Self: Send + Sync,
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
        self.consumer
            .recv()
            .await
            .map_err(|error| anyhow!("failed to subscribe Kafka input: {error}"))
            .and_then(|message| {
                message
                    .payload()
                    .unwrap_or_default()
                    .try_into()
                    .map(|input: PipeMessage<Value>| Some(input.drop_reply()))
            })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, Parser)]
pub struct MessengerKafkaArgs {
    #[arg(long, env = "KAFKA_HOSTS", value_name = "ADDR")]
    kafka_hosts: Vec<String>,
}
