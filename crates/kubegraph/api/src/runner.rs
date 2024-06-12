use std::{collections::BTreeMap, sync::Arc};

use anyhow::Result;
use async_trait::async_trait;
use kube::Client;

use crate::{
    connector::NetworkConnectorCrd,
    function::NetworkFunctionCrd,
    graph::{
        GraphData, GraphEdges, GraphMetadataPinned, GraphScope, NetworkGraphDB,
        ScopedNetworkGraphDBContainer,
    },
    problem::VirtualProblem,
};

#[async_trait]
pub trait NetworkRunner<DB, T>
where
    DB: NetworkGraphDB,
{
    async fn execute<'a>(&self, ctx: NetworkRunnerContext<'a, DB, T>) -> Result<()>;
}

pub struct NetworkRunnerContext<'a, DB, T>
where
    DB: NetworkGraphDB,
{
    pub connectors: BTreeMap<GraphScope, Arc<NetworkConnectorCrd>>,
    pub functions: BTreeMap<GraphScope, NetworkFunctionCrd>,
    pub graph: GraphData<T>,
    pub graph_db: ScopedNetworkGraphDBContainer<'a, DB>,
    pub kube: &'a Client,
    pub problem: VirtualProblem<GraphMetadataPinned>,
    pub static_edges: Option<GraphEdges<T>>,
}
