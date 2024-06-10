use std::ops::{Add, Sub};

use anyhow::{anyhow, Result};
use async_trait::async_trait;
use kubegraph_api::{
    function::{fake::NetworkFunctionFakeSpec, spawn::FunctionSpawnContext},
    graph::{Graph, GraphData, GraphEdges, GraphMetadataExt, ScopedNetworkGraphDB},
};
use pl::lazy::{
    dsl,
    frame::{IntoLazy, LazyFrame},
};
use tracing::{instrument, Level};

#[async_trait]
impl<DB, M> super::NetworkFunctionFake<DB, LazyFrame, M> for NetworkFunctionFakeSpec
where
    DB: ScopedNetworkGraphDB<::kubegraph_api::frame::LazyFrame, M>,
    M: GraphMetadataExt,
{
    #[instrument(level = Level::INFO, skip(self, graph_db, ctx))]
    async fn spawn(self, graph_db: &DB, ctx: FunctionSpawnContext<LazyFrame, M>) -> Result<()>
    where
        M: 'async_trait + Send,
    {
        let Self {} = self;
        let FunctionSpawnContext {
            graph:
                Graph {
                    connector,
                    data: GraphData { edges, nodes },
                    metadata: graph_metadata,
                    scope: graph_scope,
                },
            metadata: _,
            static_edges,
            template: _,
        } = ctx;

        let key_connector = graph_metadata.connector();
        let key_flow = graph_metadata.flow();
        let key_name = graph_metadata.name();
        let key_src = graph_metadata.src();
        let key_sink = graph_metadata.sink();
        let key_supply = graph_metadata.supply();

        // Step 1. Define a problem
        let key_flow_in = format!("{key_sink}.{key_flow}");
        let key_flow_out = format!("{key_src}.{key_flow}");

        // Step 2. Apply edge flows to node supply
        let updated_nodes = nodes
            .left_join(
                edges
                    .clone()
                    .filter(dsl::col(key_flow).gt(0i64))
                    .select([dsl::col(key_src), dsl::col(key_flow).alias(&key_flow_out)]),
                dsl::col(key_name),
                dsl::col(key_src),
            )
            .left_join(
                edges
                    .clone()
                    .filter(dsl::col(key_flow).gt(0i64))
                    .select([dsl::col(key_sink), dsl::col(key_flow).alias(&key_flow_in)]),
                dsl::col(key_name),
                dsl::col(key_sink),
            )
            .with_column(
                dsl::col(key_supply)
                    .sub(dsl::col(&key_flow_out).fill_null(0i64))
                    .add(dsl::col(&key_flow_in).fill_null(0i64)),
            )
            .drop([key_connector, &key_flow_in, &key_flow_out]);

        // Step 3. Collect once
        let collected_nodes = updated_nodes
            .collect()
            .map_err(|error| anyhow!("failed to collect nodes: {error}"))?
            .lazy();

        // Step 4. Upload to the DB
        let graph = Graph {
            connector,
            data: GraphData {
                edges: static_edges
                    .map(GraphEdges::into_inner)
                    .map(Into::into)
                    .unwrap_or_default(),
                nodes: collected_nodes.into(),
            },
            metadata: graph_metadata,
            scope: graph_scope,
        };
        graph_db.insert(graph).await
    }
}
