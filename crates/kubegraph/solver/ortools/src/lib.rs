#[cfg(feature = "df-polars")]
extern crate polars as pl;

#[cfg(feature = "df-polars")]
mod polars;

use anyhow::{bail, Result};
use async_trait::async_trait;
use kubegraph_api::{
    frame::LazyFrame,
    graph::{GraphData, GraphMetadataPinned},
    problem::ProblemSpec,
};
use tracing::{instrument, Level};

#[derive(Clone, Debug, Default)]
pub struct NetworkSolver {}

#[async_trait]
impl ::kubegraph_api::solver::NetworkSolver<GraphData<LazyFrame>> for NetworkSolver {
    type Output = GraphData<LazyFrame>;

    #[instrument(level = Level::INFO, skip(self, graph, problem))]
    async fn solve(
        &self,
        graph: GraphData<LazyFrame>,
        problem: &ProblemSpec<GraphMetadataPinned>,
    ) -> Result<Self::Output> {
        match graph {
            GraphData {
                edges: _,
                nodes: LazyFrame::Empty,
            } => bail!("cannot execute local solver with empty graph"),
            GraphData {
                edges: LazyFrame::Empty,
                nodes: _,
            } => Ok(graph),

            #[cfg(feature = "df-polars")]
            GraphData {
                edges: LazyFrame::Polars(edges),
                nodes: LazyFrame::Polars(nodes),
            } => self
                .solve(GraphData { edges, nodes }, problem)
                .await
                .map(Into::into),
        }
    }
}
