use std::sync::Arc;

use anyhow::{anyhow, Result};
use ark_core_k8s::data::Url;
use async_trait::async_trait;
use futures::{stream::iter, StreamExt};
use kubegraph_api::{
    connector::{
        http::NetworkConnectorHttpSpec, NetworkConnectorCrd, NetworkConnectorKind,
        NetworkConnectorSpec, NetworkConnectorType,
    },
    frame::{DataFrame, LazyFrame},
    graph::{Graph, GraphData, GraphMetadata, GraphScope},
};
use reqwest::Client;
use tracing::{info, instrument, warn, Level};

#[derive(Default)]
pub struct NetworkConnector {
    // TODO: implement continuous pulling
    // clients: BTreeMap<NetworkConnectorHttpSpec, Option<Client>>,
    // db: Vec<NetworkConnectorHttpSpec>,
}

#[async_trait]
impl ::kubegraph_api::connector::NetworkConnector for NetworkConnector {
    fn connector_type(&self) -> NetworkConnectorType {
        NetworkConnectorType::Http
    }

    #[inline]
    fn name(&self) -> &str {
        "http"
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
                NetworkConnectorKind::Http(spec) => {
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
                    warn!("failed to load http connector ({namespace}/{name}): {error}");
                    None
                }
            }
        });

        Ok(data.collect().await)
    }
}

struct NetworkConnectorItem {
    client: Client,
    cr: Arc<NetworkConnectorCrd>,
    scope: GraphScope,
    url: Url,
}

impl NetworkConnectorItem {
    fn try_new(
        cr: Arc<NetworkConnectorCrd>,
        scope: GraphScope,
        spec: NetworkConnectorHttpSpec,
    ) -> Result<Self> {
        let NetworkConnectorHttpSpec { url } = spec;

        Ok(Self {
            cr,
            client: Client::new(),
            scope,
            url,
        })
    }

    #[instrument(level = Level::INFO, skip(self))]
    async fn load_graph_data(self) -> Result<Graph<GraphData<LazyFrame>, GraphMetadata>> {
        let Self {
            cr,
            client,
            scope,
            url,
        } = self;

        let GraphScope { namespace, name } = &scope;
        info!("Loading http connector: {namespace}/{name}");

        let response = client
            .get(format!("{url}/{namespace}/{name}"))
            .send()
            .await
            .map_err(|error| anyhow!("failed to request ({namespace}/{name}): {error}"))?;

        let graph: Graph<GraphData<DataFrame>> = response.json().await.map_err(|error| {
            anyhow!("failed to collect into dataframe ({namespace}/{name}): {error}")
        })?;

        let graph = Graph {
            connector: Some(cr),
            data: graph.data.lazy(),
            metadata: graph.metadata,
            scope,
        };
        Ok(graph)
    }
}
