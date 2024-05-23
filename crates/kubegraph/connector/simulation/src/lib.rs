use std::path::Path;

use anyhow::{anyhow, Result};
use async_trait::async_trait;
use futures::{stream::iter, StreamExt};
use kubegraph_api::{
    connector::{
        simulation::NetworkConnectorSimulationSpec, NetworkConnectorCrd, NetworkConnectorKind,
        NetworkConnectorSpec, NetworkConnectorType,
    },
    frame::LazyFrame,
    graph::{Graph, GraphData, GraphMetadata, GraphScope},
};
use polars::{
    io::{csv::read::CsvReadOptions, SerReader},
    lazy::frame::IntoLazy,
};
use tokio::fs;
use tracing::{info, instrument, warn, Level};

#[derive(Default)]
pub struct NetworkConnector {}

#[async_trait]
impl ::kubegraph_api::connector::NetworkConnector for NetworkConnector {
    #[inline]
    fn connector_type(&self) -> NetworkConnectorType {
        NetworkConnectorType::Simulation
    }

    #[inline]
    fn name(&self) -> &str {
        "simulation"
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
                NetworkConnectorKind::Simulation(spec) => Some(NetworkConnectorItem {
                    metadata,
                    scope,
                    spec,
                }),
                _ => None,
            }
        });

        let data = iter(items).filter_map(|item| async move {
            let GraphScope { namespace, name } = item.scope.clone();
            match item.load_graph_data().await {
                Ok(data) => Some(data),
                Err(error) => {
                    warn!("failed to load simulation connector ({namespace}/{name}): {error}");
                    None
                }
            }
        });

        Ok(data.collect().await)
    }
}

#[derive(Clone, Debug)]
struct NetworkConnectorItem {
    metadata: GraphMetadata,
    scope: GraphScope,
    spec: NetworkConnectorSimulationSpec,
}

impl NetworkConnectorItem {
    #[instrument(level = Level::INFO, skip(self))]
    async fn load_graph_data(self) -> Result<Graph<LazyFrame>> {
        let Self {
            metadata,
            scope,
            spec:
                NetworkConnectorSimulationSpec {
                    path: base_dir,
                    key_edges,
                    key_nodes,
                },
        } = self;

        let GraphScope { namespace, name } = &scope;
        info!("Loading simulation connector: {namespace}/{name}");

        Ok(Graph {
            data: GraphData {
                edges: load_csv(&base_dir, &key_edges).await?,
                nodes: load_csv(&base_dir, &key_nodes).await?,
            },
            metadata,
            scope,
        })
    }
}

#[instrument(level = Level::INFO)]
async fn load_csv(base_dir: &Path, filename: &str) -> Result<LazyFrame> {
    let mut path = base_dir.to_path_buf();
    path.push(filename);

    if fs::try_exists(&path).await? {
        CsvReadOptions::default()
            .with_has_header(true)
            .try_into_reader_with_file_path(Some(path.to_path_buf()))
            .map_err(
                |error| anyhow!("failed to load file {path}: {error}", path = path.display(),),
            )?
            .finish()
            .map(|df| LazyFrame::Polars(df.lazy()))
            .map_err(|error| {
                anyhow!(
                    "failed to parse file {path}: {error}",
                    path = path.display(),
                )
            })
    } else {
        Ok(LazyFrame::Empty)
    }
}
