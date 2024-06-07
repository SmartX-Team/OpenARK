use anyhow::Result;
use async_trait::async_trait;

use crate::{
    frame::LazyFrame,
    function::NetworkFunctionCrd,
    graph::{Graph, GraphData, GraphEdges},
    problem::VirtualProblem,
};

#[async_trait]
pub trait NetworkDependencySolver {
    async fn build_pipeline(
        &self,
        problem: &VirtualProblem,
        spec: NetworkDependencySolverSpec,
    ) -> Result<NetworkDependencyPipeline<GraphData<LazyFrame>>>;
}

pub struct NetworkDependencySolverSpec {
    pub functions: Vec<NetworkFunctionCrd>,
    pub graphs: Vec<Graph<LazyFrame>>,
}

pub struct NetworkDependencyPipeline<G> {
    pub graph: G,
    pub static_edges: Option<GraphEdges<LazyFrame>>,
}
