use anyhow::Result;
use async_trait::async_trait;
use kubegraph_api::{
    frame::LazyFrame,
    graph::{Graph, GraphData, GraphFilter, GraphScope, IntoGraph},
};
use tracing::{info, instrument, Level};

#[derive(Clone)]
pub struct NetworkGraphDB {
    #[cfg(feature = "graph-local")]
    local: ::kubegraph_graph_local::NetworkGraphDB,
    #[cfg(feature = "graph-memory")]
    memory: ::kubegraph_graph_memory::NetworkGraphDB,
}

impl NetworkGraphDB {
    pub async fn try_default() -> Result<Self> {
        Ok(Self {
            #[cfg(feature = "graph-local")]
            local: ::kubegraph_graph_local::NetworkGraphDB::try_default().await?,
            #[cfg(feature = "graph-memory")]
            memory: ::kubegraph_graph_memory::NetworkGraphDB::default(),
        })
    }

    fn get_default_db(&self) -> &impl ::kubegraph_api::graph::NetworkGraphDB {
        #[cfg(feature = "graph-local")]
        {
            &self.local
        }
        #[cfg(feature = "graph-memory")]
        {
            &self.memory
        }
    }
}

#[async_trait]
impl ::kubegraph_api::graph::NetworkGraphDB for NetworkGraphDB {
    #[instrument(level = Level::INFO, skip(self))]
    async fn get(&self, scope: &GraphScope) -> Result<Option<Graph<LazyFrame>>> {
        self.get_default_db().get(scope).await
    }

    #[instrument(level = Level::INFO, skip(self, graph))]
    async fn insert(&self, graph: Graph<LazyFrame>) -> Result<()> {
        self.get_default_db().insert(graph).await
    }

    #[instrument(level = Level::INFO, skip(self))]
    async fn list(&self, filter: &GraphFilter) -> Result<Vec<Graph<LazyFrame>>> {
        self.get_default_db().list(filter).await
    }

    #[instrument(level = Level::INFO, skip(self))]
    async fn close(&self) -> Result<()> {
        info!("Closing network graph...");

        #[cfg(feature = "graph-local")]
        self.local.close().await?;

        #[cfg(feature = "graph-memory")]
        self.memory.close().await?;

        Ok(())
    }
}

#[derive(Clone, Default)]
pub struct GraphContext {
    pub(crate) graph: GraphData<Option<LazyFrame>>,
}

impl IntoGraph<LazyFrame> for GraphContext {
    fn try_into_graph(self) -> Result<GraphData<LazyFrame>> {
        let GraphData { edges, nodes } = self.graph;
        Ok(GraphData {
            edges: edges.unwrap_or_default(),
            nodes: nodes.unwrap_or_default(),
        })
    }
}
