use std::{collections::BTreeMap, sync::Arc};

use anyhow::Result;
use async_trait::async_trait;

use crate::{
    connector::NetworkConnectorCrd,
    frame::LazyFrame,
    function::NetworkFunctionCrd,
    graph::{Graph, GraphData, GraphEdges, GraphScope},
    problem::VirtualProblem,
};

#[async_trait]
pub trait NetworkDependencySolver {
    async fn build_pipeline(
        &self,
        problem: &VirtualProblem,
        spec: NetworkDependencySolverSpec,
    ) -> Result<NetworkDependencyPipelineTemplate<GraphData<LazyFrame>>>;
}

pub struct NetworkDependencySolverSpec {
    pub functions: BTreeMap<GraphScope, NetworkFunctionCrd>,
    pub graphs: Vec<Graph<GraphData<LazyFrame>>>,
}

pub struct NetworkDependencyPipeline<G> {
    pub connectors: BTreeMap<GraphScope, Arc<NetworkConnectorCrd>>,
    pub functions: BTreeMap<GraphScope, NetworkFunctionCrd>,
    pub template: NetworkDependencyPipelineTemplate<G>,
}

pub struct NetworkDependencyPipelineTemplate<G> {
    pub graph: G,
    pub static_edges: Option<GraphEdges<LazyFrame>>,
}
