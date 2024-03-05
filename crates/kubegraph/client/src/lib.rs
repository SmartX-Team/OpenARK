use std::{sync::Arc, time::Duration};

use anyhow::Result;
use clap::Parser;
use dash_pipe_provider::{storage::lakehouse::StorageContext, PipeClient, PipeClientArgs};
use kubegraph_api::{
    graph::{
        NetworkEdgeKey, NetworkGraph, NetworkNode, NetworkNodeKey, NetworkValue,
        NetworkValueBuilder,
    },
    model,
    row::NetworkGraphRow,
};
use serde::{Deserialize, Serialize};
use tracing::{instrument, Level};

#[derive(Clone, Debug, Serialize, Deserialize, Parser)]
pub struct NetworkGraphClientArgs {
    #[command(flatten)]
    pipe: PipeClientArgs,

    #[arg(long, env = "PIPE_INTERVAL_MS", value_name = "MILLISECONDS", default_value_t = NetworkGraphClientArgs::default_interval_ms(),)]
    #[serde(default = "NetworkGraphClientArgs::default_interval_ms")]
    interval_ms: u64,
}

impl NetworkGraphClientArgs {
    const fn default_interval_ms() -> u64 {
        5
    }
}

#[derive(Clone)]
pub struct NetworkGraphClient {
    pipe: Arc<PipeClient>,
    storage: StorageContext,
}

impl NetworkGraphClient {
    #[instrument(level = Level::INFO)]
    pub async fn try_default() -> Result<Self> {
        let args = NetworkGraphClientArgs::try_parse()?;
        Self::try_new(&args).await
    }

    #[instrument(level = Level::INFO, skip(args))]
    pub async fn try_new(args: &NetworkGraphClientArgs) -> Result<Self> {
        let pipe = PipeClient::try_new(&args.pipe).await?;
        let storage = pipe
            .storage()
            .create_lakehouse::<NetworkGraphRow>(
                &model::data()?,
                Some(Duration::from_millis(args.interval_ms)),
            )
            .await?;

        Ok(Self {
            pipe: Arc::new(pipe),
            storage,
        })
    }

    #[instrument(level = Level::INFO, skip_all)]
    pub async fn add_edges(
        &self,
        edges: impl IntoIterator<Item = (NetworkEdgeKey, NetworkValueBuilder)>,
    ) -> Result<()> {
        todo!()
    }

    #[instrument(level = Level::INFO, skip_all)]
    pub async fn get_node(&self, key: &NetworkNodeKey) -> Result<Option<NetworkNode>> {
        todo!()
    }

    #[instrument(level = Level::INFO, skip_all)]
    pub async fn get_edge(&self, key: &NetworkEdgeKey) -> Result<Option<NetworkValue>> {
        todo!()
    }

    #[instrument(level = Level::INFO, skip_all)]
    pub async fn to_json(&self) -> Result<NetworkGraph> {
        todo!()
    }
}
