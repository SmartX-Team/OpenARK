use std::{collections::BTreeMap, str::FromStr, sync::Arc};

use anyhow::{anyhow, Result};
use async_trait::async_trait;
use futures::{stream::iter, StreamExt};
use kubegraph_api::{
    connector::{
        prometheus::NetworkConnectorPrometheusSpec, NetworkConnectorCrd, NetworkConnectorKind,
        NetworkConnectorSpec, NetworkConnectorType,
    },
    frame::LazyFrame,
    graph::{Graph, GraphData, GraphMetadata, GraphMetadataRaw, GraphScope},
    query::{NetworkQuery, NetworkQueryMetadata, NetworkQueryMetadataType},
};
use polars::{
    error::PolarsError,
    frame::DataFrame,
    lazy::{dsl, frame::LazyFrame as PolarsLazyFrame},
    series::Series,
};
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
    ) -> Result<Vec<Graph<GraphData<LazyFrame>>>> {
        let items = connectors.into_iter().filter_map(|object| {
            let cr = Arc::new(object.clone());
            let scope = GraphScope::from_resource(&object);
            let NetworkConnectorSpec { kind } = object.spec;

            match kind {
                NetworkConnectorKind::Prometheus(spec) => {
                    match NetworkConnectorItem::try_new(cr, scope, spec) {
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

struct NetworkConnectorItem<T> {
    client: Client,
    cr: Arc<NetworkConnectorCrd>,
    query: NetworkQuery<T>,
    scope: GraphScope,
}

impl NetworkConnectorItem<NetworkQueryMetadata> {
    fn try_new(
        cr: Arc<NetworkConnectorCrd>,
        scope: GraphScope,
        spec: NetworkConnectorPrometheusSpec,
    ) -> Result<Self> {
        #[instrument(level = Level::INFO, skip(spec))]
        fn load_client(spec: &NetworkConnectorPrometheusSpec) -> Result<Client> {
            let NetworkConnectorPrometheusSpec { template: _, url } = spec;

            Client::from_str(url.as_str())
                .map_err(|error| anyhow!("failed to init prometheus client {url:?}: {error}"))
        }

        Ok(Self {
            cr,
            client: load_client(&spec)?,
            query: spec.template,
            scope,
        })
    }

    #[instrument(level = Level::INFO, skip(self))]
    async fn load_graph_data(self) -> Result<Graph<GraphData<LazyFrame>, GraphMetadata>> {
        let Self {
            cr,
            client,
            scope,
            query:
                NetworkQuery {
                    metadata: NetworkQueryMetadata { consts, r#type },
                    query,
                },
        } = self;

        let GraphScope { namespace, name } = &scope;
        info!("Loading prometheus {type} connector: {namespace}/{name}");

        // Evaluate a PromQL query.
        let response = client.query(query).get().await?;
        let (data, _) = response.into_inner();
        let vectors = data.into_vector().ok().unwrap();

        // Collect columns
        let df = collect_polars_columns(vectors, consts).map_err(|error| {
            anyhow!("failed to collect {type} into dataframe ({namespace}/{name}): {error}")
        })?;
        let metadata = GraphMetadataRaw::from_polars(&df).into();

        let graph = Graph {
            connector: Some(cr.clone()),
            data: match r#type {
                NetworkQueryMetadataType::Edge => GraphData {
                    edges: df.into(),
                    nodes: LazyFrame::Empty,
                },
                NetworkQueryMetadataType::Node => GraphData {
                    edges: LazyFrame::Empty,
                    nodes: df.into(),
                },
            },
            metadata,
            scope,
        };
        Ok(graph)
    }
}

fn collect_polars_columns(
    vectors: Vec<InstantVector>,
    consts: BTreeMap<String, String>,
) -> Result<DataFrame, PolarsError> {
    // trust the first row's column names
    let column_names: Vec<_> = match vectors.first() {
        Some(row) => row.metric().keys().map(|key| key.as_str()).collect(),
        None => return Ok(DataFrame::default()),
    };

    // collect all known columns
    let columns = column_names.into_iter().map(|name| {
        vectors
            .iter()
            .filter_map(|row| row.metric().get(name))
            .map(|value| value.as_str())
            .collect::<Series>()
            .with_name(name)
    });

    // collect all metric values
    let columns = columns
        .chain(Some(
            vectors
                .iter()
                .map(|row| row.sample().timestamp())
                .collect::<Series>()
                .with_name("timestamp"),
        ))
        .chain(Some(
            vectors
                .iter()
                .map(|row| row.sample().value())
                .collect::<Series>()
                .with_name("value"),
        ));

    // collect all constants
    let columns = columns.map(dsl::lit).chain(
        consts
            .into_iter()
            .map(|(name, value)| dsl::lit(value).alias(&name)),
    );

    // finalize
    PolarsLazyFrame::default()
        .with_columns(&columns.collect::<Vec<_>>())
        .collect()
}
