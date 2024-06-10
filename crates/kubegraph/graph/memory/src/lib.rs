use std::{collections::BTreeMap, sync::Arc};

use anyhow::Result;
use async_trait::async_trait;
use kubegraph_api::{
    frame::LazyFrame,
    graph::{Graph, GraphData, GraphFilter, GraphScope},
};
use tokio::sync::RwLock;
use tracing::{info, instrument, Level};

#[derive(Clone, Default)]
pub struct NetworkGraphDB {
    map: Arc<RwLock<BTreeMap<GraphScope, Graph<GraphData<LazyFrame>>>>>,
}

#[async_trait]
impl ::kubegraph_api::graph::NetworkGraphDB for NetworkGraphDB {
    #[instrument(level = Level::INFO, skip(self))]
    async fn get(&self, scope: &GraphScope) -> Result<Option<Graph<GraphData<LazyFrame>>>> {
        Ok(self.map.read().await.get(scope).cloned())
    }

    #[instrument(level = Level::INFO, skip(self, graph))]
    async fn insert(&self, graph: Graph<GraphData<LazyFrame>>) -> Result<()> {
        let mut map = self.map.write().await;
        map.insert(graph.scope.clone(), graph);
        Ok(())
    }

    #[instrument(level = Level::INFO, skip(self))]
    async fn list(&self, filter: &GraphFilter) -> Result<Vec<Graph<GraphData<LazyFrame>>>> {
        Ok(self
            .map
            .read()
            .await
            .iter()
            .filter(|&(key, _)| filter.contains(key))
            .map(|(_, value)| value.clone())
            .collect())
    }

    #[instrument(level = Level::INFO, skip(self))]
    async fn close(&self) -> Result<()> {
        info!("Closing in-memory db...");
        Ok(())
    }
}
