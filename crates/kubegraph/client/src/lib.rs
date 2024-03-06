#![recursion_limit = "256"]

use std::sync::Arc;

use anyhow::Result;
use clap::Parser;
use dash_pipe_provider::{
    lancedb::table::AddDataMode,
    storage::{lancedb::StorageContext, MetadataStorage},
    PipeClient, PipeClientArgs, PipeMessage,
};
use kubegraph_api::{
    graph::{NetworkEdgeKey, NetworkGraphRow, NetworkValue},
    model,
};
use serde::{Deserialize, Serialize};
use tracing::{instrument, Level};

#[derive(Clone, Debug, Serialize, Deserialize, Parser)]
pub struct NetworkGraphClientArgs {
    #[command(flatten)]
    pipe: PipeClientArgs,
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
            .create_lancedb_dynamic(AddDataMode::Append, &model::data()?)
            .await?;

        Ok(Self {
            pipe: Arc::new(pipe),
            storage,
        })
    }

    #[instrument(level = Level::INFO, skip_all)]
    pub async fn add_edges(
        &self,
        edges: impl IntoIterator<Item = (NetworkEdgeKey, NetworkValue)>,
    ) -> Result<()> {
        let values: Vec<_> = edges
            .into_iter()
            .map(|(key, value)| NetworkGraphRow { key, value })
            .map(PipeMessage::new)
            .collect();
        if values.is_empty() {
            return Ok(());
        }

        let values: Vec<_> = values.iter().collect();
        self.storage.put_metadata(&values).await
    }
}
