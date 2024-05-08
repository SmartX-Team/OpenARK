use anyhow::Result;
use kubegraph_api::{
    frame::LazyFrame,
    graph::{Graph, IntoGraph},
};

#[derive(Clone, Default)]
pub struct GraphContext {
    pub(crate) graph: Graph<Option<LazyFrame>>,
}

impl IntoGraph<LazyFrame> for GraphContext {
    fn try_into_graph(self) -> Result<Graph<LazyFrame>> {
        let Graph { edges, nodes } = self.graph;
        Ok(Graph {
            edges: edges.unwrap_or(LazyFrame::Empty),
            nodes: nodes.unwrap_or(LazyFrame::Empty),
        })
    }
}
