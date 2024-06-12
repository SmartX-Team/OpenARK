use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{
    frame::DataFrame,
    graph::{Graph, GraphData, GraphEdges, GraphMetadataPinned},
};

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct FunctionCallRequest<T = DataFrame, M = GraphMetadataPinned> {
    pub graph: Graph<GraphData<T>, M>,
    pub metadata: super::FunctionMetadata,
    pub static_edges: Option<GraphEdges<T>>,
    pub template: super::NetworkFunctionTemplate,
}
