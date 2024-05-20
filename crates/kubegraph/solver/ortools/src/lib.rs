#[cfg(feature = "df-polars")]
extern crate polars as pl;

#[cfg(feature = "df-polars")]
mod polars;

use anyhow::{bail, Result};
use async_trait::async_trait;
use kubegraph_api::{
    frame::LazyFrame,
    graph::{GraphData, GraphMetadataStandard},
    problem::ProblemSpec,
};

#[derive(Clone, Debug, Default)]
pub struct NetworkSolver {}

#[async_trait]
impl ::kubegraph_api::solver::NetworkSolver<GraphData<LazyFrame>> for NetworkSolver {
    type Output = GraphData<LazyFrame>;

    async fn solve(
        &self,
        graph: GraphData<LazyFrame>,
        problem: &ProblemSpec<GraphMetadataStandard>,
    ) -> Result<Self::Output> {
        match graph {
            GraphData {
                edges: LazyFrame::Empty,
                nodes: _,
            }
            | GraphData {
                edges: _,
                nodes: LazyFrame::Empty,
            } => bail!("cannot execute local solver with empty graph"),

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
