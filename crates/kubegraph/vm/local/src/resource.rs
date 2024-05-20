use std::{collections::BTreeMap, sync::Arc};

use async_trait::async_trait;
use futures::{stream::FuturesUnordered, StreamExt};
use kube::ResourceExt;
use kubegraph_api::{
    connector::{NetworkConnector, NetworkConnectorCrd, NetworkConnectorType},
    function::NetworkFunctionCrd,
    graph::GraphScope,
    vm::NetworkVirtualMachine,
};
use tokio::{sync::Mutex, task::JoinHandle};
use tracing::{instrument, Level};

#[derive(Clone, Default)]
pub struct NetworkResourceDB {
    inner: Arc<Mutex<LocalConnectorDB>>,
}

#[async_trait]
impl ::kubegraph_api::connector::NetworkConnectorDB for NetworkResourceDB {
    #[instrument(level = Level::INFO, skip(self))]
    async fn delete_connector(&self, key: &GraphScope) {
        self.inner.lock().await.delete_connector(key)
    }

    #[instrument(level = Level::INFO, skip(self, object))]
    async fn insert_connector(&self, object: NetworkConnectorCrd) {
        self.inner.lock().await.insert_connector(object)
    }

    #[instrument(level = Level::INFO, skip(self))]
    async fn list_connectors(
        &self,
        r#type: NetworkConnectorType,
    ) -> Option<Vec<NetworkConnectorCrd>> {
        self.inner.lock().await.list_connectors(r#type)
    }
}

#[async_trait]
impl ::kubegraph_api::function::NetworkFunctionDB for NetworkResourceDB {
    #[instrument(level = Level::INFO, skip(self))]
    async fn delete_function(&self, key: &GraphScope) {
        self.inner.lock().await.delete_function(key)
    }

    #[instrument(level = Level::INFO, skip(self))]
    async fn insert_function(&self, object: NetworkFunctionCrd) {
        self.inner.lock().await.insert_function(object)
    }

    #[instrument(level = Level::INFO, skip(self))]
    async fn list_functions(&self) -> Vec<NetworkFunctionCrd> {
        self.inner.lock().await.list_functions()
    }
}

#[derive(Default)]
struct LocalConnectorDB {
    connectors: BTreeMap<GraphScope, NetworkConnectorCrd>,
    functions: BTreeMap<GraphScope, NetworkFunctionCrd>,
    has_updated: BTreeMap<NetworkConnectorType, bool>,
}

impl LocalConnectorDB {
    fn delete_connector(&mut self, key: &GraphScope) {
        let removed_object = self.connectors.remove(&key);

        if let Some(object) = removed_object {
            self.has_updated
                .entry(object.spec.to_ref())
                .and_modify(|updated| *updated = true);
        }
    }

    fn delete_function(&mut self, key: &GraphScope) {
        self.functions.remove(&key);
    }

    fn insert_connector(&mut self, object: NetworkConnectorCrd) {
        let key = GraphScope {
            namespace: object.namespace().unwrap_or_else(|| "default".into()),
            name: object.name_any(),
        };
        let src = object.spec.to_ref();

        self.connectors.insert(key, object);
        self.has_updated
            .entry(src)
            .and_modify(|updated| *updated = true);
    }

    fn insert_function(&mut self, object: NetworkFunctionCrd) {
        let key = GraphScope {
            namespace: object.namespace().unwrap_or_else(|| "default".into()),
            name: object.name_any(),
        };

        self.functions.insert(key, object);
    }

    fn list_connectors(
        &mut self,
        r#type: NetworkConnectorType,
    ) -> Option<Vec<NetworkConnectorCrd>> {
        let updated = self.has_updated.entry(r#type).or_insert(true);
        if *updated {
            *updated = false;
            let connectors: Vec<_> = self
                .connectors
                .values()
                .filter(|&cr| cr.spec == r#type)
                .cloned()
                .collect();

            if connectors.is_empty() {
                None
            } else {
                Some(connectors)
            }
        } else {
            None
        }
    }

    fn list_functions(&self) -> Vec<NetworkFunctionCrd> {
        self.functions.values().cloned().collect()
    }
}

pub(crate) struct NetworkResourceWorker {
    inner: JoinHandle<()>,
}

impl NetworkResourceWorker {
    pub(crate) fn spawn(vm: &(impl 'static + NetworkVirtualMachine)) -> Self {
        Self {
            inner: ::tokio::spawn(
                FuturesUnordered::from_iter(vec![
                    #[cfg(feature = "connector-prometheus")]
                    ::kubegraph_connector_prometheus::NetworkConnector::default()
                        .loop_forever(vm.clone()),
                    #[cfg(feature = "connector-simulation")]
                    ::kubegraph_connector_simulation::NetworkConnector::default()
                        .loop_forever(vm.clone()),
                ])
                .collect::<()>(),
            ),
        }
    }

    pub(crate) fn abort(&self) {
        self.inner.abort()
    }
}
