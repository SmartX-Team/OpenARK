use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use anyhow::{anyhow, bail, Result};
use ark_core_k8s::data::Name;
use async_trait::async_trait;
use bytes::Bytes;
use clap::Parser;
use futures::{Stream, StreamExt};
use r2r::{std_msgs::msg::String as StringMessage, QosProfile};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use tokio::{
    sync::{mpsc, oneshot},
    task::JoinHandle,
};
use tracing::{debug, instrument, Level};

use crate::message::PipeMessage;

pub struct Messenger {
    _handle: Arc<MessengerHandle>,
    channel: mpsc::Sender<MessengerRequest>,
}

impl Messenger {
    pub fn try_new(args: &MessengerRos2Args) -> Result<Self> {
        debug!("Initializing Messenger IO - ROS2");

        let ctx = ::r2r::Context::create()
            .map_err(|error| anyhow!("failed to init ROS2 context: {error}"))?;
        let mut node = ::r2r::Node::create(ctx, "testnode", "default")
            .map_err(|error| anyhow!("failed to create a ROS2 node: {error}"))?;

        // parse data
        let qos_profile = default_qos_profile();
        let spin_interval = if args.ros2_spin_interval > 0 {
            Some(Duration::from_millis(args.ros2_spin_interval))
        } else {
            None
        };

        // create channel pipe
        let (channel_tx, mut channel_rx) = mpsc::channel(1);

        // spin node
        let handle = ::tokio::task::spawn(async move {
            loop {
                let instant = Instant::now();
                if let Ok(request) = channel_rx.try_recv() {
                    match request {
                        MessengerRequest::CreatePublisher { topic, reply } => {
                            let topic_name = parse_topic_name(&topic);

                            let value = node
                                .create_publisher::<StringMessage>(&topic_name, qos_profile.clone())
                                .map(|inner| Publisher {
                                    _handle: None,
                                    inner,
                                    topic,
                                })
                                .map_err(Into::into);
                            reply.send(value).ok();
                        }
                        MessengerRequest::CreateSubscriber { topic, reply } => {
                            let topic_name = parse_topic_name(&topic);

                            let value = node
                                .subscribe::<StringMessage>(&topic_name, qos_profile.clone())
                                .map(Box::new)
                                .map(|inner| Subscriber {
                                    _handle: None,
                                    inner,
                                    topic,
                                })
                                .map_err(Into::into);
                            reply.send(value).ok();
                        }
                    }
                }

                node.spin_once(Duration::from_millis(100));
                if let Some(spin_interval) = spin_interval {
                    let elapsed = instant.elapsed();
                    if elapsed < spin_interval {
                        ::tokio::time::sleep(spin_interval - elapsed).await;
                    } else {
                        ::tokio::task::yield_now().await;
                    }
                } else {
                    ::tokio::task::yield_now().await;
                }
            }
        });

        // prefetch protocols
        Ok(Self {
            _handle: Arc::new(MessengerHandle {
                inner: Some(handle),
            }),
            channel: channel_tx,
        })
    }
}

#[async_trait]
impl<Value> super::Messenger<Value> for Messenger {
    fn messenger_type(&self) -> super::MessengerType {
        super::MessengerType::Ros2
    }

    #[instrument(level = Level::INFO, skip_all, err(Display))]
    async fn publish(&self, topic: Name) -> Result<Arc<dyn super::Publisher>> {
        let (tx, rx) = oneshot::channel();
        let request = MessengerRequest::CreatePublisher { topic, reply: tx };
        self.channel.send(request).await?;

        let mut value = rx.await??;
        value._handle = Some(self._handle.clone());
        Ok(Arc::new(value))
    }

    #[instrument(level = Level::INFO, skip_all, err(Display))]
    async fn subscribe(&self, topic: Name) -> Result<Box<dyn super::Subscriber<Value>>>
    where
        Value: Send + DeserializeOwned,
    {
        let (tx, rx) = oneshot::channel();
        let request = MessengerRequest::CreateSubscriber { topic, reply: tx };
        self.channel.send(request).await?;

        let mut value = rx.await??;
        value._handle = Some(self._handle.clone());
        Ok(Box::new(value))
    }
}

struct MessengerHandle {
    inner: Option<JoinHandle<()>>,
}

impl Drop for MessengerHandle {
    fn drop(&mut self) {
        if let Some(handle) = self.inner.take() {
            handle.abort()
        }
    }
}

enum MessengerRequest {
    CreatePublisher {
        topic: Name,
        reply: oneshot::Sender<Result<Publisher>>,
    },
    CreateSubscriber {
        topic: Name,
        reply: oneshot::Sender<Result<Subscriber>>,
    },
}

pub struct Publisher {
    _handle: Option<Arc<MessengerHandle>>,
    inner: ::r2r::Publisher<StringMessage>,
    topic: Name,
}

// TODO: Proof it! (Send is ok, Sync is ???)
unsafe impl Sync for Publisher {}

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
        bail!("cannot reply with Ros2")
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
        bail!("cannot request with Ros2")
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
        self.inner
            .publish(&StringMessage {
                data: String::from_utf8(data.into())?,
            })
            .map_err(|error| anyhow!("failed to publish data to Ros2: {error}"))
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
        Ok(())
    }
}

pub struct Subscriber {
    _handle: Option<Arc<MessengerHandle>>,
    inner: Box<dyn Unpin + Send + Sync + Stream<Item = StringMessage>>,
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
        self.inner
            .next()
            .await
            .map(|message| {
                message
                    .data
                    .as_str()
                    .try_into()
                    .map(|input: PipeMessage<Value>| input.drop_reply())
                    .map_err(|error| anyhow!("failed to subscribe Ros2 input: {error}"))
            })
            .transpose()
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, Parser)]
pub struct MessengerRos2Args {
    // TODO: collect QOS profile info
    #[arg(long, env = "ROS2_SPIN_INTERVAL", value_name = "MILLISECONDS", default_value_t = MessengerRos2Args::default_spin_interval(),)]
    #[serde(default = "MessengerRos2Args::default_spin_interval")]
    ros2_spin_interval: u64,
}

impl MessengerRos2Args {
    const fn default_spin_interval() -> u64 {
        1 // in milliseconds
    }
}

fn default_qos_profile() -> QosProfile {
    QosProfile::default()
}

fn parse_topic_name(topic: &Name) -> String {
    format!(
        "/{topic}",
        topic = topic.replace(".", "/").replace("-", "_"),
    )
}
