mod model;

use anyhow::{anyhow, Result};
use async_trait::async_trait;
use futures::{stream::iter, StreamExt};
use kubegraph_api::{
    connector::{
        fake::NetworkConnectorFakeSpec, NetworkConnectorCrd, NetworkConnectorKind,
        NetworkConnectorSpec, NetworkConnectorType,
    },
    frame::LazyFrame,
    graph::{Graph, GraphData, GraphMetadata, GraphScope},
};
use tracing::{info, instrument, warn, Level};

use crate::model::DataGenerator;

#[derive(Default)]
pub struct NetworkConnector {}

#[async_trait]
impl ::kubegraph_api::connector::NetworkConnector for NetworkConnector {
    #[inline]
    fn connector_type(&self) -> NetworkConnectorType {
        NetworkConnectorType::Fake
    }

    #[inline]
    fn name(&self) -> &str {
        "fake"
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
                NetworkConnectorKind::Fake(spec) => Some(NetworkConnectorItem {
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
                    warn!("failed to load fake connector ({namespace}/{name}): {error}");
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
    spec: NetworkConnectorFakeSpec,
}

impl NetworkConnectorItem {
    #[instrument(level = Level::INFO, skip(self))]
    async fn load_graph_data(self) -> Result<Graph<LazyFrame>> {
        let Self {
            metadata,
            scope,
            spec: NetworkConnectorFakeSpec { edges, nodes },
        } = self;

        let GraphScope { namespace, name } = &scope;
        info!("Loading fake connector: {namespace}/{name}");

        dbg!(nodes.clone().generate(&scope)?.collect().await?);

        Ok(Graph {
            data: GraphData {
                edges: edges.generate(&scope).map_err(|error| {
                    anyhow!("failed to generate fake edges ({namespace}/{name}): {error}")
                })?,
                nodes: nodes.generate(&scope).map_err(|error| {
                    anyhow!("failed to generate fake nodes ({namespace}/{name}): {error}")
                })?,
            },
            metadata,
            scope,
        })
    }
}
