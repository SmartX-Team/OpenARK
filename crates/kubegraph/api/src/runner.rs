use anyhow::Result;
use async_trait::async_trait;

use crate::{graph::ScopedNetworkGraphDB, problem::ProblemSpec};

#[async_trait]
pub trait NetworkRunner<G> {
    async fn execute(
        &self,
        graph_db: &dyn ScopedNetworkGraphDB,
        graph: G,
        problem: &ProblemSpec,
    ) -> Result<()>;
}
