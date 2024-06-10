use crate::graph::{Graph, GraphData, GraphEdges, GraphMetadata};

pub struct FunctionSpawnContext<T, M = GraphMetadata> {
    pub graph: Graph<GraphData<T>, M>,
    pub metadata: super::FunctionMetadata,
    pub static_edges: Option<GraphEdges<T>>,
    pub template: super::NetworkFunctionTemplate,
}
