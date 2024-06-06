use anyhow::Result;
use async_trait::async_trait;

use crate::{graph::GraphMetadataPinned, problem::ProblemSpec};

#[async_trait]
pub trait NetworkSolver<G> {
    type Output;

    async fn solve(
        &self,
        graph: G,
        problem: &ProblemSpec<GraphMetadataPinned>,
    ) -> Result<Self::Output>;
}
