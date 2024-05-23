use std::{collections::BTreeMap, sync::Arc};

use anyhow::{anyhow, Result};
use async_trait::async_trait;
use futures::{stream::FuturesUnordered, StreamExt};
use kube::Client;
use kubegraph_api::{
    connector::{NetworkConnector, NetworkConnectorCrd, NetworkConnectorType},
    function::NetworkFunctionCrd,
    graph::GraphScope,
    problem::NetworkProblemCrd,
    vm::NetworkVirtualMachine,
};
use tokio::{sync::Mutex, task::JoinHandle};
use tracing::{info, instrument, Level};

use crate::reloader::NetworkResourceReloader;

#[derive(Clone, Default)]
pub struct NetworkResourceDB {
    inner: Arc<Mutex<LocalResourceDB>>,
}

#[async_trait]
impl ::kubegraph_api::resource::NetworkResourceDB<NetworkConnectorCrd> for NetworkResourceDB {
    #[instrument(level = Level::INFO, skip(self))]
    async fn delete(&self, key: &GraphScope) {
        self.inner.lock().await.delete_connector(key)
    }

    #[instrument(level = Level::INFO, skip(self, object))]
    async fn insert(&self, object: NetworkConnectorCrd) {
        self.inner.lock().await.insert_connector(object)
    }

    #[instrument(level = Level::INFO, skip(self))]
    async fn list(&self, r#type: NetworkConnectorType) -> Option<Vec<NetworkConnectorCrd>> {
        self.inner.lock().await.list_connectors(r#type)
    }
}

#[async_trait]
impl ::kubegraph_api::resource::NetworkResourceDB<NetworkFunctionCrd> for NetworkResourceDB {
    #[instrument(level = Level::INFO, skip(self))]
    async fn delete(&self, key: &GraphScope) {
        self.inner.lock().await.delete_function(key)
    }

    #[instrument(level = Level::INFO, skip(self))]
    async fn insert(&self, object: NetworkFunctionCrd) {
        self.inner.lock().await.insert_function(object)
    }

    #[instrument(level = Level::INFO, skip(self))]
    async fn list(&self, (): ()) -> Option<Vec<NetworkFunctionCrd>> {
        Some(self.inner.lock().await.list_functions())
    }
}

#[async_trait]
impl ::kubegraph_api::resource::NetworkResourceDB<NetworkProblemCrd> for NetworkResourceDB {
    #[instrument(level = Level::INFO, skip(self))]
    async fn delete(&self, key: &GraphScope) {
        self.inner.lock().await.delete_problem(key)
    }

    #[instrument(level = Level::INFO, skip(self))]
    async fn insert(&self, object: NetworkProblemCrd) {
        self.inner.lock().await.insert_problem(object)
    }

    #[instrument(level = Level::INFO, skip(self))]
    async fn list(&self, (): ()) -> Option<Vec<NetworkProblemCrd>> {
        Some(self.inner.lock().await.list_problems())
    }
}

#[derive(Default)]
struct LocalResourceDB {
    connectors: BTreeMap<GraphScope, NetworkConnectorCrd>,
    connectors_has_updated: BTreeMap<NetworkConnectorType, bool>,
    functions: BTreeMap<GraphScope, NetworkFunctionCrd>,
    problems: BTreeMap<GraphScope, NetworkProblemCrd>,
}

impl LocalResourceDB {
    fn delete_connector(&mut self, key: &GraphScope) {
        let removed_object = self.connectors.remove(&key);

        if let Some(object) = removed_object {
            self.connectors_has_updated
                .entry(object.spec.to_ref())
                .and_modify(|updated| *updated = true);
        }
    }

    fn insert_connector(&mut self, object: NetworkConnectorCrd) {
        let key = GraphScope::from_resource(&object);
        let src = object.spec.to_ref();

        self.connectors.insert(key, object);
        self.connectors_has_updated
            .entry(src)
            .and_modify(|updated| *updated = true);
    }

    fn list_connectors(
        &mut self,
        r#type: NetworkConnectorType,
    ) -> Option<Vec<NetworkConnectorCrd>> {
        let updated = self.connectors_has_updated.entry(r#type).or_insert(true);
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
}

impl LocalResourceDB {
    fn delete_function(&mut self, key: &GraphScope) {
        self.functions.remove(&key);
    }

    fn insert_function(&mut self, object: NetworkFunctionCrd) {
        let key = GraphScope::from_resource(&object);

        self.functions.insert(key, object);
    }

    fn list_functions(&self) -> Vec<NetworkFunctionCrd> {
        self.functions.values().cloned().collect()
    }
}

impl LocalResourceDB {
    fn delete_problem(&mut self, key: &GraphScope) {
        self.problems.remove(&key);
    }

    fn insert_problem(&mut self, object: NetworkProblemCrd) {
        let key = GraphScope::from_resource(&object);

        self.problems.insert(key, object);
    }

    fn list_problems(&self) -> Vec<NetworkProblemCrd> {
        self.problems.values().cloned().collect()
    }
}

pub(crate) struct NetworkResourceWorker {
    connector_db: NetworkConnectorDBWorker,
    connector_reloader: NetworkResourceReloader<NetworkConnectorCrd>,
    function_reloader: NetworkResourceReloader<NetworkFunctionCrd>,
    problem_reloader: NetworkResourceReloader<NetworkProblemCrd>,
}

impl NetworkResourceWorker {
    pub(crate) async fn try_spawn(vm: &(impl 'static + NetworkVirtualMachine)) -> Result<Self> {
        let client = Client::try_default()
            .await
            .map_err(|error| anyhow!("failed to load kubernetes account: {error}"))?;

        Ok(Self {
            connector_db: NetworkConnectorDBWorker::spawn(vm),
            connector_reloader: NetworkResourceReloader::spawn(client.clone(), vm),
            function_reloader: NetworkResourceReloader::spawn(client.clone(), vm),
            problem_reloader: NetworkResourceReloader::spawn(client, vm),
        })
    }

    pub(crate) fn abort(&self) {
        self.connector_db.abort();
        self.connector_reloader.abort();
        self.function_reloader.abort();
        self.problem_reloader.abort();
    }
}

struct NetworkConnectorDBWorker {
    inner: JoinHandle<()>,
}

impl NetworkConnectorDBWorker {
    fn spawn(vm: &(impl 'static + NetworkVirtualMachine)) -> Self {
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

    fn abort(&self) {
        info!("Stopping all connectors...");
        self.inner.abort()
    }
}
