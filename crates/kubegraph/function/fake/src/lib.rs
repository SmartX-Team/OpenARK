#[cfg(feature = "df-polars")]
extern crate polars as pl;

#[cfg(feature = "df-polars")]
mod polars;

use anyhow::{bail, Result};
use async_trait::async_trait;
use kubegraph_api::{
    frame::LazyFrame,
    function::{fake::NetworkFunctionFakeSpec, spawn::FunctionSpawnContext},
    graph::{Graph, GraphData, GraphEdges, GraphMetadataExt, ScopedNetworkGraphDB},
};
use tracing::{instrument, Level};

#[async_trait]
pub trait NetworkFunctionFake<DB, T, M>
where
    DB: ScopedNetworkGraphDB<LazyFrame, M>,
{
    async fn spawn(self, graph_db: &DB, ctx: FunctionSpawnContext<T, M>) -> Result<()>
    where
        M: 'async_trait + Send;
}

#[async_trait]
impl<DB, M> NetworkFunctionFake<DB, LazyFrame, M> for NetworkFunctionFakeSpec
where
    DB: ScopedNetworkGraphDB<LazyFrame, M>,
    M: GraphMetadataExt,
{
    #[instrument(level = Level::INFO, skip(self, graph_db, ctx))]
    async fn spawn(self, graph_db: &DB, ctx: FunctionSpawnContext<LazyFrame, M>) -> Result<()>
    where
        M: 'async_trait + Send,
    {
        let FunctionSpawnContext {
            graph:
                Graph {
                    connector,
                    data: graph_data,
                    metadata: graph_metadata,
                    scope: graph_scope,
                },
            metadata,
            static_edges,
            template,
        } = ctx;

        match (graph_data, static_edges.map(GraphEdges::into_inner)) {
            (
                GraphData {
                    edges: LazyFrame::Empty,
                    nodes: _,
                },
                _,
            )
            | (
                GraphData {
                    edges: _,
                    nodes: LazyFrame::Empty,
                },
                _,
            ) => bail!("cannot spawn a fake function with empty graph"),

            #[cfg(feature = "df-polars")]
            (
                GraphData {
                    edges: LazyFrame::Polars(edges),
                    nodes: LazyFrame::Polars(nodes),
                },
                None | Some(LazyFrame::Empty),
            ) => {
                let ctx = FunctionSpawnContext {
                    graph: Graph {
                        connector,
                        data: GraphData { edges, nodes },
                        metadata: graph_metadata,
                        scope: graph_scope,
                    },
                    metadata,
                    static_edges: None,
                    template,
                };
                self.spawn(graph_db, ctx).await
            }
            #[cfg(feature = "df-polars")]
            (
                GraphData {
                    edges: LazyFrame::Polars(edges),
                    nodes: LazyFrame::Polars(nodes),
                },
                Some(LazyFrame::Polars(static_edges)),
            ) => {
                let ctx = FunctionSpawnContext {
                    graph: Graph {
                        connector,
                        data: GraphData { edges, nodes },
                        metadata: graph_metadata,
                        scope: graph_scope,
                    },
                    metadata,
                    static_edges: Some(GraphEdges::new(static_edges)),
                    template,
                };
                self.spawn(graph_db, ctx).await
            }
        }
    }
}
