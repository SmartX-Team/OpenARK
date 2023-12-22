use std::{path::PathBuf, sync::Arc};

use anyhow::{anyhow, bail, Result};
use ark_core_k8s::data::Name;
use async_nats::{Client, ServerAddr, ToServerAddrs};
use async_trait::async_trait;
use bytes::Bytes;
use clap::{ArgAction, Parser};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use tokio_stream::StreamExt;
use tracing::{debug, instrument, Level};

use crate::message::PipeMessage;

pub struct Messenger {
    client: Arc<Client>,
}

impl Messenger {
    #[instrument(level = Level::INFO, err(Display))]
    pub async fn try_new(args: &MessengerNatsArgs) -> Result<Self> {
        debug!("Initializing Messenger IO - Nats");

        fn parse_addrs(args: &MessengerNatsArgs) -> Result<Vec<ServerAddr>> {
            let addrs = args
                .nats_addrs
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

        #[instrument(level = Level::INFO, skip_all, err(Display))]
        async fn parse_password(args: &MessengerNatsArgs) -> Result<Option<String>> {
            match args.nats_password_path.as_ref() {
                Some(path) => ::tokio::fs::read_to_string(path)
                    .await
                    .map(|password| password.split('\n').next().unwrap().trim().to_string())
                    .map(Some)
                    .map_err(|error| anyhow!("failed to get NATS token: {error}")),
                None => Ok(None),
            }
        }

        let mut config =
            ::async_nats::ConnectOptions::default().require_tls(args.nats_tls_required);
        if let Some(user) = args.nats_account.as_ref() {
            if let Some(pass) = parse_password(args).await? {
                config = config.user_and_password(user.clone(), pass);
            }
        }
        config
            .connect(parse_addrs(args)?)
            .await
            .map(Into::into)
            .map(|client| Self { client })
            .map_err(|error| anyhow!("failed to init NATS client: {error}"))
    }
}

#[async_trait]
impl<Value> super::Messenger<Value> for Messenger {
    fn messenger_type(&self) -> super::MessengerType {
        super::MessengerType::Nats
    }

    #[instrument(level = Level::INFO, skip_all, err(Display))]
    async fn publish(&self, namespace: String, topic: Name) -> Result<Arc<dyn super::Publisher>> {
        Ok(Arc::new(Publisher {
            client: self.client.clone(),
            namespace,
            topic,
        }))
    }

    #[instrument(level = Level::INFO, skip_all, err(Display))]
    async fn subscribe(
        &self,
        namespace: String,
        topic: Name,
    ) -> Result<Box<dyn super::Subscriber<Value>>>
    where
        Value: Send + DeserializeOwned,
    {
        Ok(Box::new(self.client.subscribe(topic.clone()).await.map(
            |inner| Subscriber {
                inner,
                namespace,
                topic,
            },
        )?))
    }

    #[instrument(level = Level::INFO, skip_all, err(Display))]
    async fn subscribe_queued(
        &self,
        namespace: String,
        topic: Name,
        queue_group: Name,
    ) -> Result<Box<dyn super::Subscriber<Value>>>
    where
        Value: Send + DeserializeOwned,
    {
        Ok(Box::new(
            self.client
                .queue_subscribe(topic.clone(), queue_group.into())
                .await
                .map(|inner| Subscriber {
                    inner,
                    namespace,
                    topic,
                })?,
        ))
    }
}

pub struct Publisher {
    client: Arc<Client>,
    namespace: String,
    topic: Name,
}

#[async_trait]
impl super::Publisher for Publisher {
    fn topic(&self) -> &Name {
        &self.topic
    }

    #[instrument(level = Level::INFO, skip(self, data), fields(data.len = %data.len(), data.name = %self.topic.as_str(), data.namespace = %self.namespace), err(Display))]
    async fn reply_one(&self, data: Bytes, inbox: String) -> Result<()> {
        self.client
            .publish(inbox, data)
            .await
            .map_err(|error| anyhow!("failed to reply data to NATS: {error}"))
    }

    #[instrument(level = Level::INFO, skip(self, data), fields(data.len = %data.len(), data.name = %self.topic.as_str(), data.namespace = %self.namespace), err(Display))]
    async fn request_one(&self, data: Bytes) -> Result<Bytes> {
        self.client
            .request(&self.topic, data)
            .await
            .map(|message| message.payload)
            .map_err(|error| anyhow!("failed to request data to NATS: {error}"))
    }

    #[instrument(level = Level::INFO, skip(self, data), fields(data.len = %data.len(), data.name = %self.topic.as_str(), data.namespace = %self.namespace), err(Display))]
    async fn send_one(&self, data: Bytes) -> Result<()> {
        self.client
            .publish(&self.topic, data)
            .await
            .map_err(|error| anyhow!("failed to publish data to NATS: {error}"))
    }

    #[instrument(level = Level::INFO, skip(self), fields(data.name = %self.topic.as_str(), data.namespace = %self.namespace), err(Display))]
    async fn flush(&self) -> Result<()> {
        self.client
            .flush()
            .await
            .map_err(|error| anyhow!("failed to terminate NATS publisher: {error}"))
    }
}

pub struct Subscriber {
    inner: ::async_nats::Subscriber,
    namespace: String,
    topic: Name,
}

#[async_trait]
impl<Value> super::Subscriber<Value> for Subscriber
where
    Self: Send + Sync,
    Value: Send + DeserializeOwned,
{
    #[instrument(level = Level::INFO, skip_all, fields(data.len = 1usize, data.name = %self.topic.as_str(), data.namespace = %self.namespace), err(Display))]
    async fn read_one(&mut self) -> Result<Option<PipeMessage<Value, ()>>> {
        self.inner
            .next()
            .await
            .map(|message| {
                message
                    .payload
                    .try_into()
                    .map(|input: PipeMessage<_, _>| match message.reply {
                        Some(inbox) => input.with_reply_inbox(inbox.to_string()),
                        None => input.drop_reply(),
                    })
            })
            .transpose()
            .map_err(|error| anyhow!("failed to subscribe NATS input: {error}"))
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, Parser)]
pub struct MessengerNatsArgs {
    #[arg(long, env = "NATS_ACCOUNT", value_name = "NAME")]
    nats_account: Option<String>,

    #[arg(long, env = "NATS_ADDRS", value_name = "ADDR")]
    nats_addrs: Vec<String>,

    #[arg(long, env = "NATS_PASSWORD_PATH", value_name = "PATH")]
    nats_password_path: Option<PathBuf>,

    #[arg(long, env = "NATS_TLS_REQUIRED", action = ArgAction::SetTrue)]
    nats_tls_required: bool,
}
