#[cfg(feature = "df-polars")]
extern crate polars as pl;

#[cfg(feature = "df-polars")]
mod polars;

use anyhow::{bail, Result};
use async_trait::async_trait;
use kubegraph_api::{
    frame::LazyFrame,
    graph::{GraphData, ScopedNetworkGraphDB},
    problem::ProblemSpec,
};

#[derive(Clone, Debug, Default)]
pub struct NetworkRunner {}

#[async_trait]
impl ::kubegraph_api::runner::NetworkRunner<GraphData<LazyFrame>> for NetworkRunner {
    async fn execute(
        &self,
        graph_db: &dyn ScopedNetworkGraphDB,
        graph: GraphData<LazyFrame>,
        problem: &ProblemSpec,
    ) -> Result<()> {
        match graph {
            GraphData {
                edges: LazyFrame::Empty,
                nodes: _,
            }
            | GraphData {
                edges: _,
                nodes: LazyFrame::Empty,
            } => bail!("cannot execute simulator runner with empty graph"),

            #[cfg(feature = "df-polars")]
            GraphData {
                edges: LazyFrame::Polars(edges),
                nodes: LazyFrame::Polars(nodes),
            } => self
                .execute(graph_db, GraphData { edges, nodes }, problem)
                .await
                .map(Into::into),
        }
    }
}
