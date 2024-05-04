#[cfg(feature = "polars")]
extern crate polars as pl;

#[cfg(feature = "polars")]
mod polars;

use anyhow::Result;
use kubegraph_api::{frame::LazyFrame, graph::Graph, solver::Problem};

#[derive(Default)]
pub struct Twin {}

impl ::kubegraph_api::twin::LocalTwin<Graph<LazyFrame>, String> for Twin {
    type Output = Graph<LazyFrame>;

    fn execute(&self, graph: Graph<LazyFrame>, problem: &Problem<String>) -> Result<Self::Output> {
        match graph {
            #[cfg(feature = "polars")]
            Graph {
                edges: LazyFrame::Polars(edges),
                nodes: LazyFrame::Polars(nodes),
            } => self
                .execute(Graph { edges, nodes }, problem)
                .map(Into::into),
        }
    }
}
