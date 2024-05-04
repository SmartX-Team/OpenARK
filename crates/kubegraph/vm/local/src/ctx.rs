use anyhow::{anyhow, Result};
use kubegraph_api::{frame::LazyFrame, graph::Graph};

#[derive(Default)]
pub struct Context {
    pub(crate) graph: Graph<Option<LazyFrame>>,
    pub(crate) vm: crate::lazy::LazyVirtualMachine,
}

impl Context {
    pub(crate) fn to_graph(&self) -> Result<Graph<LazyFrame>> {
        let Graph { edges, nodes } = self.graph.clone();
        Ok(Graph {
            edges: edges.ok_or_else(|| anyhow!("undefined edges"))?,
            nodes: nodes.ok_or_else(|| anyhow!("undefined nodes"))?,
        })
    }
}
