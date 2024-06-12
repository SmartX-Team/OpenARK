#[cfg(feature = "df-polars")]
extern crate polars as pl;

#[cfg(feature = "df-polars")]
mod polars;

use anyhow::{bail, Result};
use async_trait::async_trait;
use kubegraph_api::{
    frame::LazyFrame,
    graph::{GraphData, GraphEdges, NetworkGraphDB},
    runner::NetworkRunnerContext,
};
use tracing::{instrument, Level};

#[derive(Clone, Debug, Default)]
pub struct NetworkRunner {}

#[async_trait]
impl<DB> ::kubegraph_api::runner::NetworkRunner<DB, LazyFrame> for NetworkRunner
where
    DB: NetworkGraphDB,
{
    #[instrument(level = Level::INFO, skip(self, ctx))]
    async fn execute<'a>(&self, ctx: NetworkRunnerContext<'a, DB, LazyFrame>) -> Result<()> {
        let NetworkRunnerContext {
            connectors,
            functions,
            graph,
            graph_db,
            kube,
            problem,
            static_edges,
        } = ctx;

        match (graph, static_edges.map(GraphEdges::into_inner)) {
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
            ) => {
                bail!("cannot execute simulator runner with empty graph")
            }

            #[cfg(feature = "df-polars")]
            (
                GraphData {
                    edges: LazyFrame::Polars(edges),
                    nodes: LazyFrame::Polars(nodes),
                },
                None | Some(LazyFrame::Empty),
            ) => {
                let ctx = NetworkRunnerContext {
                    connectors,
                    functions,
                    graph: GraphData { edges, nodes },
                    graph_db,
                    kube,
                    problem,
                    static_edges: None,
                };
                self.execute(ctx).await.map(Into::into)
            }
            #[cfg(feature = "df-polars")]
            (
                GraphData {
                    edges: LazyFrame::Polars(edges),
                    nodes: LazyFrame::Polars(nodes),
                },
                Some(LazyFrame::Polars(static_edges)),
            ) => {
                let ctx = NetworkRunnerContext {
                    connectors,
                    functions,
                    graph: GraphData { edges, nodes },
                    graph_db,
                    kube,
                    problem,
                    static_edges: Some(GraphEdges::new(static_edges)),
                };
                self.execute(ctx).await.map(Into::into)
            }
        }
    }
}
