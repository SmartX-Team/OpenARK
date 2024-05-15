#[cfg(feature = "df-polars")]
extern crate polars as pl;

#[cfg(feature = "df-polars")]
mod polars;

use anyhow::{bail, Result};
use kubegraph_api::{frame::LazyFrame, graph::Graph, problem::ProblemSpec};

#[derive(Default)]
pub struct Twin {}

impl ::kubegraph_api::twin::LocalTwin<Graph<LazyFrame>> for Twin {
    type Output = LazyFrame;

    fn execute(&self, graph: Graph<LazyFrame>, problem: &ProblemSpec) -> Result<Self::Output> {
        match graph {
            Graph {
                edges: LazyFrame::Empty,
                nodes: _,
            }
            | Graph {
                edges: _,
                nodes: LazyFrame::Empty,
            } => bail!("cannot execute simulator twin with empty graph"),

            #[cfg(feature = "df-polars")]
            Graph {
                edges: LazyFrame::Polars(edges),
                nodes: LazyFrame::Polars(nodes),
            } => self
                .execute(Graph { edges, nodes }, problem)
                .map(Into::into),
        }
    }
}
