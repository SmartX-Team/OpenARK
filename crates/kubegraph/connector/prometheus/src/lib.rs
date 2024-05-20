use std::str::FromStr;

use anyhow::{anyhow, Result};
use async_trait::async_trait;
use futures::{stream::iter, StreamExt};
use kube::ResourceExt;
use kubegraph_api::{
    connector::{
        prometheus::NetworkConnectorPrometheusSpec, NetworkConnectorCrd, NetworkConnectorKind,
        NetworkConnectorSpec, NetworkConnectorType,
    },
    frame::LazyFrame,
    graph::{Graph, GraphData, GraphMetadata, GraphMetadataExt, GraphScope},
    query::{
        NetworkQuery, NetworkQueryEdgeMetadata, NetworkQueryMetadata, NetworkQueryNodeMetadata,
        NetworkQueryValue,
    },
};
use polars::{lazy::dsl, prelude::LiteralValue, series::Series};
use prometheus_http_query::{response::InstantVector, Client};
use tracing::{info, instrument, warn, Level};

#[derive(Default)]
pub struct NetworkConnector {
    // TODO: implement continuous pulling
    // clients: BTreeMap<NetworkConnectorPrometheusSpec, Option<Client>>,
    // db: Vec<NetworkConnectorPrometheusSpec>,
}

#[async_trait]
impl ::kubegraph_api::connector::NetworkConnector for NetworkConnector {
    fn connector_type(&self) -> NetworkConnectorType {
        NetworkConnectorType::Prometheus
    }

    #[inline]
    fn name(&self) -> &str {
        "prometheus"
    }

    #[instrument(level = Level::INFO, skip(self, connectors))]
    async fn pull(
        &mut self,
        connectors: Vec<NetworkConnectorCrd>,
    ) -> Result<Vec<Graph<LazyFrame>>> {
        let items = connectors.into_iter().filter_map(|crd| {
            let scope = GraphScope {
                namespace: crd.namespace()?,
                name: crd.name_any(),
            };
            let NetworkConnectorSpec { metadata, kind } = crd.spec;
            let metadata = GraphMetadata::Raw(metadata);

            match kind {
                NetworkConnectorKind::Prometheus(spec) => {
                    match NetworkConnectorItem::try_new(scope, metadata, spec) {
                        Ok(item) => Some(item),
                        Err(error) => {
                            warn!("{error}");
                            None
                        }
                    }
                }
                _ => None,
            }
        });

        let data = iter(items).filter_map(|item| async move {
            let GraphScope { namespace, name } = item.scope.clone();
            match item.load_graph_data().await {
                Ok(data) => Some(data),
                Err(error) => {
                    warn!("failed to load prometheus connector ({namespace}/{name}): {error}");
                    None
                }
            }
        });

        Ok(data.collect().await)
    }
}

struct NetworkConnectorItem<T, M> {
    client: Client,
    metadata: M,
    query: NetworkQuery<T>,
    scope: GraphScope,
}

impl<M> NetworkConnectorItem<NetworkQueryMetadata, M> {
    fn try_new(
        scope: GraphScope,
        metadata: M,
        spec: NetworkConnectorPrometheusSpec,
    ) -> Result<Self> {
        #[instrument(level = Level::INFO, skip(spec))]
        fn load_client(spec: &NetworkConnectorPrometheusSpec) -> Result<Client> {
            let NetworkConnectorPrometheusSpec { template: _, url } = spec;

            Client::from_str(url.as_str())
                .map_err(|error| anyhow!("failed to init prometheus client {url:?}: {error}"))
        }

        Ok(Self {
            client: load_client(&spec)?,
            metadata,
            query: spec.template,
            scope,
        })
    }

    #[instrument(level = Level::INFO, skip(self))]
    async fn load_graph_data(self) -> Result<Graph<LazyFrame, M>>
    where
        M: GraphMetadataExt,
    {
        let Self {
            client,
            metadata,
            scope,
            query:
                NetworkQuery {
                    metadata: query_metadata,
                    query,
                },
        } = self;

        match query_metadata {
            NetworkQueryMetadata::Edge(query_metadata) => {
                NetworkConnectorItem {
                    client,
                    metadata,
                    scope,
                    query: NetworkQuery {
                        metadata: query_metadata,
                        query,
                    },
                }
                .load_edges_data()
                .await
            }
            NetworkQueryMetadata::Node(query_metadata) => {
                NetworkConnectorItem {
                    client,
                    metadata,
                    scope,
                    query: NetworkQuery {
                        metadata: query_metadata,
                        query,
                    },
                }
                .load_nodes_data()
                .await
            }
        }
    }
}

impl<M> NetworkConnectorItem<NetworkQueryEdgeMetadata, M> {
    #[instrument(level = Level::INFO, skip(self))]
    async fn load_edges_data(self) -> Result<Graph<LazyFrame, M>>
    where
        M: GraphMetadataExt,
    {
        let Self {
            client,
            metadata,
            scope,
            query:
                NetworkQuery {
                    metadata:
                        NetworkQueryEdgeMetadata {
                            mut extras,
                            interval_ms: value_interval_ms,
                            sink: value_sink,
                            src: value_src,
                        },
                    query,
                },
        } = self;
        let key_interval_ms = metadata.interval_ms();
        let key_src = metadata.src();
        let key_sink = metadata.sink();

        let GraphScope { namespace, name } = &scope;
        info!("Loading prometheus edges connector: {namespace}/{name}");

        // Evaluate a PromQL query.
        let response = client.query(query).get().await?;
        let (data, _) = response.into_inner();
        let vectors = data.into_vector().ok().unwrap();

        // Collect columns
        let mut columns = vec![
            collect_polars_column(&vectors, key_src, value_src),
            collect_polars_column(&vectors, key_sink, value_sink),
            collect_polars_column(&vectors, key_interval_ms, value_interval_ms),
        ];
        if let Some(key_extras) = metadata.extras() {
            for (id, key) in key_extras {
                if let Some(value) = extras.remove(id) {
                    columns.push(collect_polars_column(&vectors, key, value));
                }
            }
        }

        let edges = ::polars::lazy::frame::LazyFrame::default()
            .with_columns(columns)
            .collect()
            .map_err(|error| anyhow!("failed to collect edges into dataframe: {error}"))?;

        let graph = Graph {
            data: GraphData {
                edges: edges.into(),
                nodes: LazyFrame::Empty,
            },
            metadata,
            scope,
        };
        Ok(graph)
    }
}

impl<M> NetworkConnectorItem<NetworkQueryNodeMetadata, M> {
    #[instrument(level = Level::INFO, skip(self))]
    async fn load_nodes_data(self) -> Result<Graph<LazyFrame, M>>
    where
        M: GraphMetadataExt,
    {
        let Self {
            client,
            metadata,
            scope,
            query:
                NetworkQuery {
                    metadata:
                        NetworkQueryNodeMetadata {
                            mut extras,
                            interval_ms: value_interval_ms,
                            name: value_name,
                        },
                    query,
                },
        } = self;
        let key_interval_ms = metadata.interval_ms();
        let key_name = metadata.name();

        let GraphScope { namespace, name } = &scope;
        info!("Loading prometheus nodes connector: {namespace}/{name}");

        // Evaluate a PromQL query.
        let response = client.query(query).get().await?;
        let (data, _) = response.into_inner();
        let vectors = data.into_vector().ok().unwrap();

        // Collect columns
        let mut columns = vec![
            collect_polars_column(&vectors, key_name, value_name),
            collect_polars_column(&vectors, key_interval_ms, value_interval_ms),
        ];
        if let Some(key_extras) = metadata.extras() {
            for (id, key) in key_extras {
                if let Some(value) = extras.remove(id) {
                    columns.push(collect_polars_column(&vectors, key, value));
                }
            }
        }

        let nodes = ::polars::lazy::frame::LazyFrame::default()
            .with_columns(columns)
            .collect()
            .map_err(|error| anyhow!("failed to collect nodes into dataframe: {error}"))?;

        let graph = Graph {
            data: GraphData {
                edges: LazyFrame::Empty,
                nodes: nodes.into(),
            },
            metadata,
            scope,
        };
        Ok(graph)
    }
}

fn collect_polars_column(
    vectors: &[InstantVector],
    key: &str,
    value: NetworkQueryValue,
) -> dsl::Expr {
    match value {
        NetworkQueryValue::Key(metric_key) => {
            let series = match metric_key.as_str() {
                "le" => Series::from_iter(vectors.iter().map(|vector| vector.sample().timestamp())),
                "value" => Series::from_iter(vectors.iter().map(|vector| vector.sample().value())),
                metric_key => Series::from_iter(
                    vectors
                        .iter()
                        .filter_map(|vector| vector.metric().get(metric_key))
                        .map(|value| value.as_str()),
                ),
            };

            dsl::lit(series.with_name(key))
        }
        NetworkQueryValue::Static(Some(value)) => dsl::lit(LiteralValue::String(value)).alias(key),
        NetworkQueryValue::Static(None) => dsl::lit(LiteralValue::Null).alias(key),
    }
}
