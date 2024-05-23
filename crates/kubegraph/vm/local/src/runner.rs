use anyhow::Result;
use async_trait::async_trait;
use futures::{stream::FuturesUnordered, TryStreamExt};
use kubegraph_api::{
    frame::LazyFrame,
    graph::{GraphData, GraphMetadataStandard, ScopedNetworkGraphDB},
    problem::ProblemSpec,
};
use tracing::{instrument, Level};

#[derive(Clone)]
pub struct NetworkRunner {
    #[cfg(feature = "runner-simulator")]
    simulator: ::kubegraph_runner_simulator::NetworkRunner,
}

impl NetworkRunner {
    pub async fn try_default() -> Result<Self> {
        Ok(Self {
            #[cfg(feature = "runner-simulator")]
            simulator: ::kubegraph_runner_simulator::NetworkRunner::default(),
        })
    }
}

#[async_trait]
impl ::kubegraph_api::runner::NetworkRunner<GraphData<LazyFrame>> for NetworkRunner {
    #[instrument(level = Level::INFO, skip(self, graph_db, graph, problem))]
    async fn execute(
        &self,
        graph_db: &dyn ScopedNetworkGraphDB,
        graph: GraphData<LazyFrame>,
        problem: &ProblemSpec<GraphMetadataStandard>,
    ) -> Result<()> {
        let tasks = vec![
            #[cfg(feature = "runner-simulator")]
            self.simulator.execute(graph_db, graph.clone(), problem),
        ];

        FuturesUnordered::from_iter(tasks).try_collect().await
    }
}
