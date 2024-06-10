use std::{collections::BTreeMap, sync::Arc};

use anyhow::Result;
use async_trait::async_trait;
use futures::{future::BoxFuture, stream::FuturesUnordered, TryStreamExt};
use kubegraph_api::{
    connector::NetworkConnectorCrd,
    function::{
        spawn::FunctionSpawnContext, FunctionMetadata, NetworkFunctionCrd, NetworkFunctionKind,
    },
    graph::{
        Graph, GraphData, GraphEdges, GraphMetadataPinnedExt, GraphScope, NetworkGraphDB,
        ScopedNetworkGraphDB,
    },
    problem::{ProblemSpec, VirtualProblem},
    runner::NetworkRunnerContext,
};
use pl::lazy::{dsl, frame::LazyFrame};
use tracing::{instrument, Level};

#[async_trait]
impl<DB> ::kubegraph_api::runner::NetworkRunner<DB, LazyFrame> for super::NetworkRunner
where
    DB: NetworkGraphDB,
{
    #[instrument(level = Level::INFO, skip(self, ctx))]
    async fn execute<'a>(&self, ctx: NetworkRunnerContext<'a, DB, LazyFrame>) -> Result<()> {
        // Step 1. Collect graph data
        let NetworkRunnerContext {
            connectors,
            functions,
            graph: GraphData { edges, nodes },
            graph_db,
            problem:
                VirtualProblem {
                    filter: _,
                    scope: _,
                    spec:
                        ProblemSpec {
                            metadata,
                            verbose: _,
                        },
                },
            static_edges,
        } = ctx;

        // Step 2. Disaggregate nodes by connector
        let all_nodes = collect_by_connectors(connectors, &metadata, &nodes);

        // Step 3. Disaggregate edges by function
        let all_functions = all_nodes.flat_map(|nodes| {
            collect_by_functions(&graph_db, &functions, &edges, static_edges.as_ref(), nodes)
        });

        // Step 4. Spawn all tasks
        all_functions
            .collect::<FuturesUnordered<_>>()
            .try_collect()
            .await
    }
}

fn collect_by_connectors<'a, M>(
    connectors: BTreeMap<GraphScope, Arc<NetworkConnectorCrd>>,
    metadata: &'a M,
    nodes: &'a LazyFrame,
) -> impl 'a + Iterator<Item = Graph<LazyFrame, M>>
where
    M: 'a + Clone + GraphMetadataPinnedExt,
{
    connectors.into_iter().map(move |(scope, connector)| Graph {
        connector: Some(connector),
        data: filter_nodes(metadata, &scope, nodes.clone()),
        metadata: metadata.clone(),
        scope,
    })
}

fn collect_by_functions<'a, DB, M>(
    graph_db: &'a DB,
    functions: &'a BTreeMap<GraphScope, NetworkFunctionCrd>,
    edges: &'a LazyFrame,
    static_edges: Option<&'a GraphEdges<LazyFrame>>,
    nodes: Graph<LazyFrame, M>,
) -> impl Iterator<Item = BoxFuture<'a, Result<()>>>
where
    DB: ScopedNetworkGraphDB<::kubegraph_api::frame::LazyFrame, M>,
    M: 'a + Send + Clone + GraphMetadataPinnedExt,
{
    let Graph {
        connector,
        data: nodes,
        metadata: graph_metadata,
        scope: graph_scope,
    } = nodes;

    functions
        .iter()
        .filter_map(move |(function_scope, function)| {
            let ctx = FunctionSpawnContext {
                graph: Graph {
                    connector: connector.clone(),
                    data: GraphData {
                        edges: filter_edges(&graph_metadata, function_scope, edges.clone()),
                        nodes: nodes.clone(),
                    },
                    metadata: graph_metadata.clone(),
                    scope: graph_scope.clone(),
                },
                metadata: FunctionMetadata {
                    scope: function_scope.clone(),
                },
                static_edges: static_edges
                    .cloned()
                    .map(GraphEdges::into_inner)
                    .map(|edges| filter_edges(&graph_metadata, function_scope, edges))
                    .map(GraphEdges::new),
                template: function.spec.template.clone(),
            };

            match function.spec.kind {
                NetworkFunctionKind::Annotation(_) => None,
                #[cfg(feature = "function-fake")]
                NetworkFunctionKind::Fake(spec) => {
                    use kubegraph_function_fake::NetworkFunctionFake;
                    Some(spec.spawn(graph_db, ctx))
                }
                _ => None,
            }
        })
}

fn filter_edges<M>(metadata: &M, scope: &GraphScope, edges: LazyFrame) -> LazyFrame
where
    M: GraphMetadataPinnedExt,
{
    filter_with(scope, metadata.function(), edges)
}

fn filter_nodes<M>(metadata: &M, scope: &GraphScope, nodes: LazyFrame) -> LazyFrame
where
    M: GraphMetadataPinnedExt,
{
    filter_with(scope, metadata.connector(), nodes)
}

fn filter_with(scope: &GraphScope, key: &str, value: LazyFrame) -> LazyFrame {
    value.filter(dsl::col(key).eq(dsl::lit(scope.name.as_str())))
}
