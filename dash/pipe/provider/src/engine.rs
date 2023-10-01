use std::future::Future;

use anyhow::{anyhow, bail, Result};
use clap::{ArgAction, Parser};
use futures::{StreamExt, TryFutureExt};
use log::warn;
use nats::ToServerAddrs;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use tokio::task::yield_now;

use crate::{
    message::PipeMessages,
    storage::{StorageSet, StorageType},
};

#[derive(Clone, Debug, Parser, Serialize, Deserialize)]
pub struct PipeEngine {
    #[arg(long, env = "NATS_ADDRS", value_name = "ADDR")]
    addrs: Vec<String>,

    #[arg(long, env = "PIPE_BATCH_SIZE", value_name = "BATCH_SIZE")]
    #[serde(default)]
    batch_size: Option<usize>,

    #[arg(long, env = "PIPE_PERSISTENCE", action=ArgAction::SetTrue)]
    #[serde(default)]
    persistence: Option<bool>,

    #[arg(long, env = "PIPE_REPLY", action=ArgAction::SetTrue)]
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

impl PipeEngine {
    pub fn from_env() -> Self {
        Self::parse()
    }

    pub fn with_batch_size(mut self, batch_size: usize) -> Self {
        self.batch_size = Some(batch_size);
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

impl PipeEngine {
    pub fn loop_forever<F, Fut, Input, Output>(&self, tick: F)
    where
        F: Fn(PipeMessages<Input>) -> Fut,
        Fut: Future<Output = Result<PipeMessages<Output>>>,
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
        F: Fn(PipeMessages<Input>) -> Fut,
        Fut: Future<Output = Result<PipeMessages<Output>>>,
        Input: DeserializeOwned,
        Output: Serialize,
    {
        // init client
        let client = {
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
            }

            match ::nats::connect(addrs).await {
                Ok(client) => client,
                Err(error) => bail!("failed to init NATS client: {error}"),
            }
        };

        // init streams
        let mut input_stream = match &self.stream_in {
            Some(stream) => match client.subscribe(stream.clone()).await {
                Ok(stream) => Some(stream),
                Err(error) => bail!("failed to init NATS input stream: {error}"),
            },
            None => None,
        };

        // init storages
        let storage = {
            let default_output = match self.persistence {
                Some(true) => StorageType::LakeHouse,
                Some(false) | None => StorageType::Nats,
            };
            StorageSet::try_new(&self.storage, &client, "myobjbucket", default_output).await?
        };

        'main: loop {
            // yield per every loop
            yield_now().await;

            let inputs = match &mut input_stream {
                // TODO: to be implemented
                Some(stream) => match self.batch_size {
                    Some(batch_size) => {
                        let mut inputs = vec![];
                        for _ in 0..batch_size {
                            match stream.next().await.map(TryInto::try_into).transpose() {
                                Ok(Some(input)) => {
                                    inputs.push(input);
                                }
                                Ok(None) => break,
                                Err(error) => {
                                    warn!("failed to parse NATS batch input: {error}");
                                    continue 'main;
                                }
                            }
                        }

                        if inputs.is_empty() {
                            continue 'main;
                        } else {
                            PipeMessages::Batch(inputs)
                        }
                    }
                    None => match stream.next().await.map(TryInto::try_into).transpose() {
                        Ok(Some(input)) => PipeMessages::Single(input),
                        Ok(None) => continue 'main,
                        Err(error) => {
                            warn!("failed to parse NATS input: {error}");
                            continue 'main;
                        }
                    },
                },
                None => PipeMessages::None,
            };

            let inputs = match inputs.load_payloads(&storage).await {
                Ok(inputs) => inputs,
                Err(error) => {
                    warn!("failed to get NATS payloads: {error}");
                    continue 'main;
                }
            };

            let outputs = match tick(inputs)
                .and_then(|inputs| inputs.dump_payloads(&storage))
                .await
            {
                Ok(PipeMessages::None) => continue 'main,
                Ok(outputs) => outputs,
                Err(error) => {
                    warn!("{error}");
                    continue 'main;
                }
            };

            if let Some(output_stream) = &self.stream_out {
                for output in outputs.into_vec() {
                    match output.try_into() {
                        Ok(output) => {
                            if let Err(error) = client.publish(output_stream.clone(), output).await
                            {
                                warn!("failed to send NATS output: {error}");
                            }
                        }
                        Err(error) => {
                            warn!("failed to parse NATS output: {error}");
                        }
                    }
                }
            }
        }
    }
}
