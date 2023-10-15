use std::{path::PathBuf, sync::Arc};

use anyhow::{anyhow, bail, Result};
use async_nats::{Client, ServerAddr, ToServerAddrs};
use async_trait::async_trait;
use bytes::Bytes;
use clap::{ArgAction, Parser};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use tokio_stream::StreamExt;
use tracing::debug;

use crate::message::{Name, PipeMessage};

pub struct Messenger {
    client: Arc<Client>,
}

impl Messenger {
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

    async fn publish(&self, topic: Name) -> Result<Arc<dyn super::Publisher>> {
        Ok(Arc::new(Publisher {
            client: self.client.clone(),
            subject: topic.into(),
        }))
    }

    async fn subscribe(&self, topic: Name) -> Result<Box<dyn super::Subscriber<Value>>>
    where
        Value: Send + Default + DeserializeOwned,
    {
        Ok(Box::new(self.client.subscribe(topic.into()).await?))
    }

    async fn subscribe_queued(
        &self,
        topic: Name,
        queue_group: Name,
    ) -> Result<Box<dyn super::Subscriber<Value>>>
    where
        Value: Send + Default + DeserializeOwned,
    {
        Ok(Box::new(
            self.client
                .queue_subscribe(topic.into(), queue_group.into())
                .await?,
        ))
    }
}

pub struct Publisher {
    client: Arc<Client>,
    subject: String,
}

#[async_trait]
impl super::Publisher for Publisher {
    async fn send_one(&self, data: Bytes) -> Result<()> {
        self.client
            .publish(self.subject.clone(), data)
            .await
            .map_err(|error| anyhow!("failed to publish data to NATS: {error}"))
    }
}

pub type Subscriber = ::async_nats::Subscriber;

#[async_trait]
impl<Value> super::Subscriber<Value> for Subscriber
where
    Self: Send + Sync,
    Value: Send + Default + DeserializeOwned,
{
    async fn read_one(&mut self) -> Result<Option<PipeMessage<Value, ()>>> {
        self.next()
            .await
            .map(|message| message.payload.try_into())
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
