use anyhow::Result;
use async_trait::async_trait;

use crate::{graph::GraphMetadataStandard, problem::ProblemSpec};

#[async_trait]
pub trait NetworkSolver<G> {
    type Output;

    async fn solve(
        &self,
        graph: G,
        problem: &ProblemSpec<GraphMetadataStandard>,
    ) -> Result<Self::Output>;
}
