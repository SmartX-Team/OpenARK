mod model;

use std::sync::Arc;

use anyhow::{anyhow, Result};
use async_trait::async_trait;
use futures::{stream::iter, StreamExt};
use kubegraph_api::{
    connector::{
        fake::NetworkConnectorFakeSpec, NetworkConnectorCrd, NetworkConnectorKind,
        NetworkConnectorSpec, NetworkConnectorType,
    },
    frame::LazyFrame,
    graph::{Graph, GraphData, GraphMetadataRaw, GraphScope},
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
    ) -> Result<Vec<Graph<GraphData<LazyFrame>>>> {
        let items = connectors.into_iter().filter_map(|object| {
            let cr = Arc::new(object.clone());
            let scope = GraphScope::from_resource(&object);
            let NetworkConnectorSpec { kind } = object.spec;

            match kind {
                NetworkConnectorKind::Fake(spec) => Some(NetworkConnectorItem { cr, scope, spec }),
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
    cr: Arc<NetworkConnectorCrd>,
    scope: GraphScope,
    spec: NetworkConnectorFakeSpec,
}

impl NetworkConnectorItem {
    #[instrument(level = Level::INFO, skip(self))]
    async fn load_graph_data(self) -> Result<Graph<GraphData<LazyFrame>>> {
        let Self {
            cr,
            scope,
            spec: NetworkConnectorFakeSpec { edges, nodes },
        } = self;

        let GraphScope { namespace, name } = &scope;
        info!("Loading fake connector: {namespace}/{name}");

        let edges = edges.generate(&scope).map_err(|error| {
            anyhow!("failed to generate fake edges ({namespace}/{name}): {error}")
        })?;
        let nodes = nodes.generate(&scope).map_err(|error| {
            anyhow!("failed to generate fake nodes ({namespace}/{name}): {error}")
        })?;

        let metadata = nodes
            .as_ref()
            .map(|nodes| GraphMetadataRaw::from_polars(&nodes))
            .unwrap_or_default()
            .into();

        Ok(Graph {
            connector: Some(cr.clone()),
            data: GraphData {
                edges: edges.map(Into::into).unwrap_or_default(),
                nodes: nodes.map(Into::into).unwrap_or_default(),
            },
            metadata,
            scope,
        })
    }
}
