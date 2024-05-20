use anyhow::Result;
use async_trait::async_trait;
use kubegraph_api::{
    connector::NetworkConnectorDB,
    frame::LazyFrame,
    graph::{Graph, GraphMetadataRaw, GraphMetadataStandard, GraphScope},
    problem::{r#virtual::VirtualProblem, ProblemSpec},
};

#[derive(Clone)]
pub struct NetworkVirtualProblem {}

impl NetworkVirtualProblem {
    pub async fn try_default() -> Result<Self> {
        Ok(Self {})
    }
}

#[async_trait]
impl ::kubegraph_api::problem::r#virtual::NetworkVirtualProblem for NetworkVirtualProblem {
    async fn inspect(
        &self,
        _connector_db: &dyn NetworkConnectorDB,
        scope: GraphScope,
    ) -> Result<VirtualProblem> {
        // TODO: to be implemented
        #[cfg(feature = "vp-identity")]
        {
            Ok(VirtualProblem {
                scope,
                spec: ProblemSpec::default(),
            })
        }
    }

    async fn pin_graph_raw(
        &self,
        graph: Graph<LazyFrame, GraphMetadataRaw>,
    ) -> Result<Graph<LazyFrame, GraphMetadataStandard>> {
        // TODO: to be implemented
        #[cfg(feature = "vp-identity")]
        {
            todo!()
        }
    }
}
