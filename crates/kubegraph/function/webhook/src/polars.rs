use std::ops::{Add, Sub};

use anyhow::{anyhow, Result};
use async_trait::async_trait;
use kubegraph_api::{
    function::{spawn::FunctionSpawnContext, webhook::NetworkFunctionWebhookSpec},
    graph::{Graph, GraphData, GraphEdges, GraphMetadataExt, ScopedNetworkGraphDB},
};
use pl::lazy::{
    dsl,
    frame::{IntoLazy, LazyFrame},
};
use tracing::{instrument, Level};

#[async_trait]
impl<DB, M> super::NetworkFunctionWebhook<DB, LazyFrame, M> for NetworkFunctionWebhookSpec
where
    DB: ScopedNetworkGraphDB<::kubegraph_api::frame::LazyFrame, M>,
    M: GraphMetadataExt,
{
    #[instrument(level = Level::INFO, skip(self, ctx))]
    async fn spawn(&self, ctx: FunctionSpawnContext<'async_trait, DB, LazyFrame, M>) -> Result<()>
    where
        DB: 'async_trait + Send,
        M: 'async_trait + Send,
    {
        let Self { endpoint } = self;
        let FunctionSpawnContext {
            graph:
                Graph {
                    connector,
                    data: GraphData { edges, nodes },
                    metadata: graph_metadata,
                    scope: graph_scope,
                },
            graph_db,
            kube: _,
            metadata: _,
            static_edges,
            template: _,
        } = ctx;

        todo!()
    }
}
