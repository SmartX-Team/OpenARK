use anyhow::Result;
use async_trait::async_trait;
use kubegraph_api::{
    connector::NetworkConnectorDB,
    graph::GraphScope,
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
}
