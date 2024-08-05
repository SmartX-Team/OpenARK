use std::collections::BTreeMap;

use anyhow::Result;
use async_trait::async_trait;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{
    function::NetworkFunctionCrd,
    graph::{GraphData, GraphEdges, GraphMetadataPinned, GraphScope},
    problem::VirtualProblem,
};

#[async_trait]
pub trait NetworkTrader<T> {
    fn is_enabled(&self) -> bool {
        true
    }

    async fn is_locked(&self, problem: &VirtualProblem) -> Result<bool>
    where
        T: 'async_trait;

    async fn register(&self, ctx: NetworkTraderContext<T>) -> Result<()>
    where
        T: 'async_trait;
}

#[async_trait]
impl<T> NetworkTrader<T> for ()
where
    T: Send,
{
    fn is_enabled(&self) -> bool {
        false
    }

    async fn is_locked(&self, _: &VirtualProblem) -> Result<bool>
    where
        T: 'async_trait,
    {
        Ok(false)
    }

    async fn register(&self, _: NetworkTraderContext<T>) -> Result<()>
    where
        T: 'async_trait,
    {
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct NetworkTraderContext<T> {
    pub functions: BTreeMap<GraphScope, NetworkFunctionCrd>,
    pub graph: GraphData<T>,
    pub problem: VirtualProblem<GraphMetadataPinned>,
    pub static_edges: Option<GraphEdges<T>>,
}
