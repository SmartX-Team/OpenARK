use kube::Client;

use crate::graph::{Graph, GraphData, GraphEdges, GraphMetadata};

pub struct FunctionSpawnContext<'a, DB, T, M = GraphMetadata> {
    pub graph: Graph<GraphData<T>, M>,
    pub graph_db: &'a DB,
    pub kube: Client,
    pub metadata: super::FunctionMetadata,
    pub static_edges: Option<GraphEdges<T>>,
    pub template: super::NetworkFunctionTemplate,
}
