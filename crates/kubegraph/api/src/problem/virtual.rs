use anyhow::Result;
use async_trait::async_trait;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{connector::NetworkConnectorDB, graph::GraphScope};

use super::ProblemSpec;

#[async_trait]
pub trait NetworkVirtualProblem {
    async fn inspect(
        &self,
        connector_db: &dyn NetworkConnectorDB,
        scope: GraphScope,
    ) -> Result<VirtualProblem>;
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct VirtualProblem {
    #[serde(flatten)]
    pub scope: GraphScope,
    #[serde(default)]
    pub spec: ProblemSpec,
}
