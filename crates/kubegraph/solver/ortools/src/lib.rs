#[cfg(feature = "df-polars")]
extern crate polars as pl;

#[cfg(feature = "df-polars")]
mod polars;

use anyhow::{bail, Result};
use kubegraph_api::{frame::LazyFrame, graph::Graph, solver::Problem};

#[derive(Default)]
pub struct Solver {}

impl ::kubegraph_api::solver::LocalSolver<Graph<LazyFrame>, String> for Solver {
    type Output = Graph<LazyFrame>;

    fn step(&self, graph: Graph<LazyFrame>, problem: Problem<String>) -> Result<Self::Output> {
        match graph {
            Graph {
                edges: LazyFrame::Empty,
                nodes: _,
            }
            | Graph {
                edges: _,
                nodes: LazyFrame::Empty,
            } => bail!("cannot execute local solver with empty graph"),

            #[cfg(feature = "df-polars")]
            Graph {
                edges: LazyFrame::Polars(edges),
                nodes: LazyFrame::Polars(nodes),
            } => self.step(Graph { edges, nodes }, problem).map(Into::into),
        }
    }
}
