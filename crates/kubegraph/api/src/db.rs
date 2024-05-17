use anyhow::Result;
use async_trait::async_trait;
use tracing::{instrument, Level};

use crate::{
    frame::{IntoLazyFrame, LazyFrame},
    graph::{Graph, NetworkEntry, NetworkEntryKeyFilter, NetworkEntryMap},
};

#[async_trait]
pub trait NetworkGraphDB
where
    Self: Send + Sync,
{
    async fn add_entries(
        &self,
        entries: impl Send + IntoIterator<Item = NetworkEntry>,
    ) -> Result<()>;

    async fn get_namespaces(&self) -> Vec<String>;

    async fn get_entries(&self, filter: Option<&NetworkEntryKeyFilter>) -> NetworkEntryMap;

    #[instrument(level = Level::INFO, skip(self))]
    async fn get_graph(&self, filter: Option<&NetworkEntryKeyFilter>) -> Result<Graph<LazyFrame>> {
        let NetworkEntryMap { edges, nodes } = self.get_entries(filter).await;
        Ok(Graph {
            edges: edges.try_into_lazy_frame()?,
            nodes: nodes.try_into_lazy_frame()?,
        })
    }

    async fn close(self) -> Result<()>;
}
