use std::{sync::Arc, time::Duration};

use anyhow::{anyhow, Result};
use clap::Parser;
use dash_pipe_api::storage::StorageS3Args;
use infinitree::{crypto::UsernamePassword, fields::VersionedMap, Infinitree};
use infinitree_backends::{Credentials, Region, S3};
use kubegraph_api::{
    graph::{NetworkEdgeKey, NetworkValue},
    model,
};
use serde::{Deserialize, Serialize};
use tokio::{
    spawn,
    sync::{Mutex, RwLock},
    task::JoinHandle,
    time::sleep,
};
use tracing::{error, info, instrument, Level};

#[derive(Clone, Debug, Serialize, Deserialize, Parser)]
pub struct NetworkGraphClientArgs {
    #[arg(
        long,
        env = "PIPE_NETWORK_COMMIT_INTERVAL_MS",
        value_name = "MILLISECONDS",
        default_value_t = NetworkGraphClientArgs::default_commit_interval_ms(),
    )]
    #[serde(default = "NetworkGraphClientArgs::default_commit_interval_ms")]
    commit_interval_ms: u64,

    #[command(flatten)]
    storage: StorageS3Args,
}

impl NetworkGraphClientArgs {
    pub fn default_commit_interval_ms() -> u64 {
        300_000 // 5 minutes
    }
}

#[derive(Clone)]
pub struct NetworkGraphClient {
    job_commit: Arc<Mutex<Option<JoinHandle<()>>>>,
    tree: Arc<RwLock<Infinitree<VersionedMap<NetworkEdgeKey, NetworkValue>>>>,
}

impl NetworkGraphClient {
    #[instrument(level = Level::INFO)]
    pub async fn try_default() -> Result<Self> {
        let args = NetworkGraphClientArgs::try_parse()?;
        Self::try_new(&args).await
    }

    #[instrument(level = Level::INFO, skip(args))]
    pub async fn try_new(args: &NetworkGraphClientArgs) -> Result<Self> {
        info!("Loading graph...");

        let NetworkGraphClientArgs {
            commit_interval_ms,
            storage:
                StorageS3Args {
                    access_key,
                    region,
                    s3_endpoint,
                    secret_key,
                },
        } = args;

        let credentials = Credentials::new(access_key, secret_key);
        let region = Region::Custom {
            region: region.clone(),
            endpoint: s3_endpoint.to_string(),
        };

        let backend = S3::with_credentials(region, model::data()?.as_str(), credentials)
            .map_err(|error| anyhow!("failed to init s3 bucket: {error}"))?;

        let key = || {
            UsernamePassword::with_credentials(access_key.to_string(), secret_key.to_string())
                .map_err(|error| anyhow!("failed to load tree key: {error}"))
        };
        let tree = Arc::new(RwLock::new(
            Infinitree::open(backend.clone(), key()?)
                .or_else(|_| Infinitree::empty(backend, key()?))
                .map_err(|error| anyhow!("failed to load tree: {error}"))?,
        ));

        Ok(Self {
            job_commit: Arc::new(Mutex::new(Some({
                let commit_interval = Duration::from_millis(*commit_interval_ms);
                let tree = tree.clone();

                spawn(async move {
                    loop {
                        sleep(commit_interval).await;
                        if let Err(error) = tree.write().await.commit(None) {
                            error!("failed to auto-commit tree: {error}");
                        }
                    }
                })
            }))),
            tree,
        })
    }

    #[instrument(level = Level::INFO, skip_all)]
    pub async fn add_edges(&self, edges: impl IntoIterator<Item = (NetworkEdgeKey, NetworkValue)>) {
        let tree = self.tree.read().await;
        edges.into_iter().for_each(|(key, value)| {
            tree.index().insert(key, value);
        });
    }

    pub async fn close(self) -> Result<()> {
        info!("Closing graph...");

        if let Some(job) = self.job_commit.lock().await.take() {
            job.abort();
        }

        self.tree
            .write()
            .await
            .commit(None)
            .map(|_| ())
            .map_err(|error| anyhow!("failed to save tree: {error}"))
    }
}
