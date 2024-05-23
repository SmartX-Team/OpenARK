use anyhow::Result;
use async_trait::async_trait;
use kubegraph_api::{
    frame::LazyFrame,
    graph::{Graph, GraphMetadataRaw, GraphMetadataStandard},
    problem::VirtualProblem,
    resource::NetworkResourceCollectionDB,
};
use tracing::{instrument, Level};

#[derive(Clone)]
pub struct NetworkAnalyzer {
    #[cfg(feature = "analyzer-llm")]
    llm: ::kubegraph_analyzer_llm::NetworkAnalyzer,
}

impl NetworkAnalyzer {
    pub async fn try_default() -> Result<Self> {
        Ok(Self {
            #[cfg(feature = "analyzer-llm")]
            llm: ::kubegraph_analyzer_llm::NetworkAnalyzer::try_default().await?,
        })
    }
}

#[async_trait]
impl ::kubegraph_api::analyzer::NetworkAnalyzer for NetworkAnalyzer {
    #[instrument(level = Level::INFO, skip(self, resource_db))]
    async fn inspect(
        &self,
        resource_db: &dyn NetworkResourceCollectionDB,
    ) -> Result<Vec<VirtualProblem>> {
        #[cfg(feature = "analyzer-llm")]
        {
            self.llm.inspect(resource_db).await
        }
    }

    #[instrument(level = Level::INFO, skip(self, problem, graph))]
    async fn pin_graph_raw(
        &self,
        problem: &VirtualProblem,
        graph: Graph<LazyFrame, GraphMetadataRaw>,
    ) -> Result<Graph<LazyFrame, GraphMetadataStandard>> {
        #[cfg(feature = "analyzer-llm")]
        {
            self.llm.pin_graph_raw(problem, graph).await
        }
    }
}
