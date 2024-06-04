use std::collections::BTreeMap;

use anyhow::Result;
use async_trait::async_trait;

use crate::{
    analyzer::{NetworkAnalyzer, VirtualProblemAnalyzer},
    frame::LazyFrame,
    function::NetworkFunctionCrd,
    graph::{Graph, GraphData, GraphEdges, GraphMetadataStandard, GraphScope},
    problem::VirtualProblem,
};

#[async_trait]
pub trait NetworkDependencySolver {
    async fn build_pipeline<A>(
        &self,
        analyzer: &A,
        problem: &VirtualProblem,
        spec: NetworkDependencySolverSpec,
    ) -> Result<NetworkDependencyPipeline<GraphData<LazyFrame>, A>>
    where
        A: NetworkAnalyzer;
}

pub struct NetworkDependencySolverSpec {
    pub functions: Vec<NetworkFunctionCrd>,
    pub graphs: Vec<Graph<LazyFrame>>,
}

pub type NetworkDependencyPipeline<G, A> =
    NetworkDependencyPipelineTemplate<G, BTreeMap<GraphScope, <A as NetworkAnalyzer>::Spec>>;

pub struct NetworkDependencyPipelineTemplate<
    G,
    A = VirtualProblemAnalyzer,
    M = GraphMetadataStandard,
> {
    pub graph: G,
    pub problem: VirtualProblem<A, M>,
    pub static_edges: Option<GraphEdges<LazyFrame>>,
}
