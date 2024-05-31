use anyhow::{anyhow, Result};
use ark_core::signal::FunctionSignal;
use async_trait::async_trait;
use clap::Parser;
use kubegraph_api::{
    component::NetworkComponent,
    frame::{DataFrame, LazyFrame},
    graph::{Graph, GraphFilter, GraphScope},
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use sled::{Config, Db};
use tracing::{info, instrument, Level};

#[derive(
    Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema, Parser,
)]
#[clap(rename_all = "kebab-case")]
#[serde(rename_all = "camelCase")]
pub struct NetworkGraphDBArgs {
    #[arg(
        long,
        env = "KUBEGRAPH_GRAPH_DB_PATH",
        value_name = "PATH",
        default_value_t = NetworkGraphDBArgs::default_db_path(),
    )]
    #[serde(default = "NetworkGraphDBArgs::default_db_path")]
    db_path: String,
}

impl Default for NetworkGraphDBArgs {
    fn default() -> Self {
        Self {
            db_path: Self::default_db_path(),
        }
    }
}

impl NetworkGraphDBArgs {
    fn default_db_path() -> String {
        "default.sled".into()
    }
}

#[derive(Clone)]
pub struct NetworkGraphDB {
    db: Db,
}

#[async_trait]
impl NetworkComponent for NetworkGraphDB {
    type Args = NetworkGraphDBArgs;

    #[instrument(level = Level::INFO)]
    async fn try_new(args: <Self as NetworkComponent>::Args, _: &FunctionSignal) -> Result<Self> {
        info!("Loading local db...");

        let NetworkGraphDBArgs { db_path } = args;

        Ok(Self {
            db: Config::default()
                .path(db_path)
                .open()
                .map_err(|error| anyhow!("failed to open local db: {error}"))?,
        })
    }
}

#[async_trait]
impl ::kubegraph_api::graph::NetworkGraphDB for NetworkGraphDB {
    #[instrument(level = Level::INFO, skip(self))]
    async fn get(&self, scope: &GraphScope) -> Result<Option<Graph<LazyFrame>>> {
        let key = ::serde_json::to_vec(scope)?;

        self.db
            .get(&key)
            .map_err(|error| anyhow!("failed to get a graph from local db: {error}"))
            .and_then(|maybe_graph| {
                maybe_graph
                    .map(|graph| {
                        ::serde_json::from_slice::<Graph<DataFrame>>(&graph)
                            .map_err(Into::into)
                            .map(|graph| graph.lazy())
                    })
                    .transpose()
            })
    }

    #[instrument(level = Level::INFO, skip(self, graph))]
    async fn insert(&self, graph: Graph<LazyFrame>) -> Result<()> {
        let graph = graph.collect().await?;
        let key = ::serde_json::to_vec(&graph.scope)?;
        let value = ::serde_json::to_vec(&graph)?;

        self.db
            .insert(key, value)
            .map(|_| ())
            .map_err(|error| anyhow!("failed to insert graph into local db: {error}"))
    }

    #[instrument(level = Level::INFO, skip(self))]
    async fn list(&self, filter: &GraphFilter) -> Result<Vec<Graph<LazyFrame>>> {
        Ok(self
            .db
            .iter()
            .filter_map(|result| result.ok())
            .filter_map(|(key, value)| {
                let key = ::serde_json::from_slice(&key).ok()?;
                let value = ::serde_json::from_slice::<Graph<DataFrame>>(&value).ok()?;
                Some((key, value))
            })
            .filter(|(key, _)| filter.contains(key))
            .map(|(_, value)| value.lazy())
            .collect())
    }

    #[instrument(level = Level::INFO, skip(self))]
    async fn close(&self) -> Result<()> {
        info!("Closing local db...");

        self.db
            .flush_async()
            .await
            .map(|_| ())
            .map_err(|error| anyhow!("failed to flush local db: {error}"))
    }
}
