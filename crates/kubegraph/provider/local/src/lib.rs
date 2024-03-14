use anyhow::{anyhow, Result};
use async_trait::async_trait;
use clap::Parser;
use kubegraph_api::graph::{NetworkEdgeKey, NetworkGraphRow, NetworkValue};
use serde::{Deserialize, Serialize};
use sled::{Batch, Config, Db};
use tracing::{info, instrument, Level};

#[derive(Clone, Debug, Serialize, Deserialize, Parser)]
pub struct NetworkGraphProviderArgs {
    #[arg(
        long,
        env = "KUBEGRAPH_DB_PATH",
        value_name = "PATH",
        default_value_t = NetworkGraphProviderArgs::default_db_path(),
    )]
    #[serde(default = "NetworkGraphProviderArgs::default_db_path")]
    db_path: String,
}

impl NetworkGraphProviderArgs {
    pub fn default_db_path() -> String {
        "default.sled".into()
    }
}

#[derive(Clone)]
pub struct NetworkGraphProvider {
    db: Db,
}

impl NetworkGraphProvider {
    #[instrument(level = Level::INFO)]
    pub async fn try_default() -> Result<Self> {
        let args = NetworkGraphProviderArgs::try_parse()?;
        Self::try_new(&args).await
    }

    #[instrument(level = Level::INFO, skip(args))]
    pub async fn try_new(args: &NetworkGraphProviderArgs) -> Result<Self> {
        info!("Loading graph...");

        let NetworkGraphProviderArgs { db_path } = args;

        Ok(Self {
            db: Config::default()
                .path(db_path)
                .open()
                .map_err(|error| anyhow!("failed to open db: {error}"))?,
        })
    }
}

#[async_trait]
impl ::kubegraph_api::provider::NetworkGraphProvider for NetworkGraphProvider {
    #[instrument(level = Level::INFO, skip_all)]
    async fn add_edges(
        &self,
        edges: impl Send + IntoIterator<Item = (NetworkEdgeKey, NetworkValue)>,
    ) -> Result<()> {
        let mut batch = Batch::default();

        edges
            .into_iter()
            .filter_map(|(key, value)| {
                let key = ::serde_json::to_vec(&key).ok()?;
                let value = ::serde_json::to_vec(&value).ok()?;
                Some((key, value))
            })
            .for_each(|(key, value)| {
                batch.insert(key, value);
            });

        self.db
            .apply_batch(batch)
            .map_err(|error| anyhow!("failed to write edges: {error}"))
    }

    #[instrument(level = Level::INFO, skip_all)]
    async fn get_edges(&self) -> Vec<NetworkGraphRow> {
        self.db
            .iter()
            .filter_map(|result| result.ok())
            .filter_map(|(key, value)| {
                let key = ::serde_json::from_slice(&key).ok()?;
                let value = ::serde_json::from_slice(&value).ok()?;
                Some((key, value))
            })
            .map(|(key, value)| NetworkGraphRow { key, value })
            .collect()
    }

    async fn close(self) -> Result<()> {
        info!("Closing graph...");

        self.db
            .flush_async()
            .await
            .map(|_| ())
            .map_err(|error| anyhow!("failed to flush db: {error}"))
    }
}
