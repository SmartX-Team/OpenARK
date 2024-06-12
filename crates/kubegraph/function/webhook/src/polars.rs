use anyhow::Result;
use async_trait::async_trait;
use kubegraph_api::{
    function::{spawn::FunctionSpawnContext, webhook::NetworkFunctionWebhookSpec},
    graph::{Graph, GraphData, GraphEdges, GraphMetadataExt},
};
use pl::lazy::frame::LazyFrame;
use serde::Serialize;
use tracing::{instrument, Level};

#[async_trait]
impl<DB, M> super::NetworkFunctionWebhook<DB, LazyFrame, M> for NetworkFunctionWebhookSpec
where
    DB: Sync,
    M: GraphMetadataExt + Serialize,
{
    #[instrument(level = Level::INFO, skip(self, ctx))]
    async fn spawn(&self, ctx: FunctionSpawnContext<'async_trait, DB, LazyFrame, M>) -> Result<()>
    where
        DB: 'async_trait + Send,
        M: 'async_trait + Send,
    {
        let FunctionSpawnContext {
            graph:
                Graph {
                    connector,
                    data,
                    metadata: graph_metadata,
                    scope: graph_scope,
                },
            graph_db,
            kube,
            metadata,
            static_edges,
            template,
        } = ctx;

        let ctx = FunctionSpawnContext {
            graph: Graph {
                connector,
                data: GraphData::<::kubegraph_api::frame::LazyFrame>::from(data),
                metadata: graph_metadata,
                scope: graph_scope,
            },
            graph_db,
            kube,
            metadata,
            static_edges: static_edges
                .map(GraphEdges::into_inner)
                .map(Into::into)
                .map(GraphEdges::new),
            template,
        };
        self.spawn(ctx).await
    }
}
