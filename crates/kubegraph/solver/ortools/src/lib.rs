#[cfg(feature = "polars")]
extern crate polars as pl;

#[cfg(feature = "polars")]
mod polars;

use anyhow::Result;
use kubegraph_api::{
    frame::LazyFrame,
    graph::Graph,
    solver::{MaxFlowProblem, MinCostProblem},
};

#[derive(Default)]
pub struct Solver {}

impl ::kubegraph_api::solver::LocalSolver<Graph<LazyFrame>, String> for Solver {
    type Output = Graph<LazyFrame>;

    fn step_max_flow(
        &self,
        graph: Graph<LazyFrame>,
        problem: MaxFlowProblem<String>,
    ) -> Result<Self::Output> {
        match graph {
            #[cfg(feature = "polars")]
            Graph {
                edges: LazyFrame::Polars(edges),
                nodes: LazyFrame::Polars(nodes),
            } => self
                .step_max_flow(Graph { edges, nodes }, problem)
                .map(Into::into),
        }
    }

    fn step_min_cost(
        &self,
        graph: Graph<LazyFrame>,
        problem: MinCostProblem<String>,
    ) -> Result<Self::Output> {
        match graph {
            #[cfg(feature = "polars")]
            Graph {
                edges: LazyFrame::Polars(edges),
                nodes: LazyFrame::Polars(nodes),
            } => self
                .step_min_cost(Graph { edges, nodes }, problem)
                .map(Into::into),
        }
    }
}
