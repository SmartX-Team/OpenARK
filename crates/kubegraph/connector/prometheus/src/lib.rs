use std::{collections::BTreeMap, str::FromStr};

use anyhow::{anyhow, Result};
use async_trait::async_trait;
use futures::{stream::iter, StreamExt};
use kubegraph_api::{
    connector::{
        prometheus::NetworkConnectorPrometheusSpec, NetworkConnectorCrd, NetworkConnectorKind,
        NetworkConnectorSpec, NetworkConnectorType,
    },
    frame::LazyFrame,
    graph::{Graph, GraphData, GraphMetadata, GraphMetadataExt, GraphScope},
    query::{NetworkQuery, NetworkQueryMetadata, NetworkQueryMetadataType},
};
use polars::{
    datatypes::DataType,
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
    ) -> Result<Vec<Graph<LazyFrame>>> {
        let items = connectors.into_iter().filter_map(|object| {
            let scope = GraphScope::from_resource(&object);
            let NetworkConnectorSpec { metadata, kind } = object.spec;
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
    async fn load_graph_data(self) -> Result<Graph<LazyFrame, GraphMetadata>>
    where
        M: GraphMetadataExt,
    {
        let Self {
            client,
            metadata,
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
        let df = collect_polars_columns(vectors, consts)
            .map_err(|error| anyhow!("failed to collect {type} into dataframe: {error}"))?;
        let metadata = collect_extras(&df, metadata);

        let graph = Graph {
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

fn collect_extras<M>(df: &DataFrame, metadata: M) -> GraphMetadata
where
    M: GraphMetadataExt,
{
    match metadata.into() {
        GraphMetadata::Raw(mut metadata) => {
            let extras = &mut metadata.extras;
            for column in df.get_columns() {
                let key = column.name();
                if column.is_empty()
                    || matches!(column.dtype(), DataType::Null)
                    || extras.contains_key(key)
                {
                    continue;
                }

                extras.insert(key.into(), key.into());
            }
            GraphMetadata::Raw(metadata)
        }
        metadata => metadata,
    }
}
