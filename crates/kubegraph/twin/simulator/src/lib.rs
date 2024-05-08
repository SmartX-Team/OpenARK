#[cfg(feature = "polars")]
extern crate polars as pl;

#[cfg(feature = "polars")]
mod polars;

use anyhow::{bail, Result};
use kubegraph_api::{frame::LazyFrame, graph::Graph, solver::Problem};

#[derive(Default)]
pub struct Twin {}

impl ::kubegraph_api::twin::LocalTwin<Graph<LazyFrame>, String> for Twin {
    type Output = LazyFrame;

    fn execute(&self, graph: Graph<LazyFrame>, problem: &Problem<String>) -> Result<Self::Output> {
        match graph {
            Graph {
                edges: LazyFrame::Empty,
                nodes: _,
            }
            | Graph {
                edges: _,
                nodes: LazyFrame::Empty,
            } => bail!("cannot execute simulator twin with empty graph"),

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
