use anyhow::Result;
use ark_core::signal::FunctionSignal;
use async_trait::async_trait;
use clap::{Parser, ValueEnum};
use kubegraph_api::{
    component::NetworkComponent,
    frame::LazyFrame,
    graph::{Graph, GraphFilter, GraphScope},
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use tracing::{instrument, Level};

#[derive(
    Clone,
    Debug,
    Default,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    JsonSchema,
    Parser,
)]
#[clap(rename_all = "kebab-case")]
#[serde(rename_all = "camelCase")]
pub struct NetworkGraphDBArgs {
    #[arg(
        long,
        env = "KUBEGRAPH_GRAPH_DB",
        value_enum,
        value_name = "IMPL",
        default_value_t = NetworkGraphDBType::default(),
    )]
    #[serde(default)]
    pub graph_db: NetworkGraphDBType,

    #[cfg(feature = "graph-local")]
    #[command(flatten)]
    #[serde(default)]
    pub local: <::kubegraph_graph_local::NetworkGraphDB as NetworkComponent>::Args,

    #[cfg(feature = "graph-memory")]
    #[command(flatten)]
    #[serde(default)]
    pub memory: <::kubegraph_graph_memory::NetworkGraphDB as NetworkComponent>::Args,
}

#[derive(
    Copy,
    Clone,
    Debug,
    Default,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    JsonSchema,
    ValueEnum,
)]
#[clap(rename_all = "kebab-case")]
#[serde(rename_all = "kebab-case")]
pub enum NetworkGraphDBType {
    #[cfg(feature = "graph-local")]
    #[default]
    Local,
    #[cfg(feature = "graph-memory")]
    #[default]
    Memory,
}

#[derive(Clone)]
pub enum NetworkGraphDB {
    #[cfg(feature = "graph-local")]
    Local(::kubegraph_graph_local::NetworkGraphDB),
    #[cfg(feature = "graph-memory")]
    Memory(::kubegraph_graph_memory::NetworkGraphDB),
}

#[async_trait]
impl NetworkComponent for NetworkGraphDB {
    type Args = NetworkGraphDBArgs;

    #[instrument(level = Level::INFO)]
    async fn try_new(
        args: <Self as NetworkComponent>::Args,
        signal: &FunctionSignal,
    ) -> Result<Self> {
        let NetworkGraphDBArgs {
            graph_db,
            #[cfg(feature = "graph-local")]
            local,
            #[cfg(feature = "graph-memory")]
            memory,
        } = args;

        match graph_db {
            #[cfg(feature = "graph-local")]
            NetworkGraphDBType::Local => Ok(Self::Local(
                ::kubegraph_graph_local::NetworkGraphDB::try_new(local, signal).await?,
            )),
            #[cfg(feature = "graph-memory")]
            NetworkGraphDBType::Memory => Ok(Self::Memory(
                ::kubegraph_graph_memory::NetworkGraphDB::try_new(memory, signal).await?,
            )),
        }
    }
}

#[async_trait]
impl ::kubegraph_api::graph::NetworkGraphDB for NetworkGraphDB {
    #[instrument(level = Level::INFO, skip(self))]
    async fn get(&self, scope: &GraphScope) -> Result<Option<Graph<LazyFrame>>> {
        match self {
            #[cfg(feature = "graph-local")]
            Self::Local(runtime) => runtime.get(scope).await,
            #[cfg(feature = "graph-memory")]
            Self::Memory(runtime) => runtime.get(scope).await,
        }
    }

    #[instrument(level = Level::INFO, skip(self, graph))]
    async fn insert(&self, graph: Graph<LazyFrame>) -> Result<()> {
        match self {
            #[cfg(feature = "graph-local")]
            Self::Local(runtime) => runtime.insert(graph).await,
            #[cfg(feature = "graph-memory")]
            Self::Memory(runtime) => runtime.insert(graph).await,
        }
    }

    #[instrument(level = Level::INFO, skip(self))]
    async fn list(&self, filter: &GraphFilter) -> Result<Vec<Graph<LazyFrame>>> {
        match self {
            #[cfg(feature = "graph-local")]
            Self::Local(runtime) => runtime.list(filter).await,
            #[cfg(feature = "graph-memory")]
            Self::Memory(runtime) => runtime.list(filter).await,
        }
    }

    #[instrument(level = Level::INFO, skip(self))]
    async fn close(&self) -> Result<()> {
        match self {
            #[cfg(feature = "graph-local")]
            Self::Local(runtime) => runtime.close().await,
            #[cfg(feature = "graph-memory")]
            Self::Memory(runtime) => runtime.close().await,
        }
    }
}
