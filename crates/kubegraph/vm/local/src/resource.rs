use std::{collections::BTreeMap, sync::Arc};

use anyhow::{anyhow, Result};
use ark_core::signal::FunctionSignal;
use async_trait::async_trait;
use clap::Parser;
use futures::{stream::FuturesUnordered, StreamExt};
use kube::Client;
use kubegraph_api::{
    component::NetworkComponent,
    connector::{NetworkConnectorCrd, NetworkConnectorExt, NetworkConnectorType},
    function::NetworkFunctionCrd,
    graph::GraphScope,
    problem::NetworkProblemCrd,
    vm::NetworkVirtualMachine,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use tokio::{sync::Mutex, task::JoinHandle};
use tracing::{info, instrument, Level};

use crate::reloader::NetworkResourceReloader;

#[derive(
    Copy,
    Clone,
    Debug,
    Default,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    JsonSchema,
    Parser,
)]
#[clap(rename_all = "kebab-case")]
#[serde(rename_all = "camelCase")]
pub struct NetworkResourceDBArgs {}

#[derive(Clone)]
pub struct NetworkResourceDB {
    inner: Arc<Mutex<LocalResourceDB>>,
    kube: Client,
}

#[async_trait]
impl NetworkComponent for NetworkResourceDB {
    type Args = NetworkResourceDBArgs;

    async fn try_new(args: <Self as NetworkComponent>::Args, _: &FunctionSignal) -> Result<Self> {
        let NetworkResourceDBArgs {} = args;

        Ok(Self {
            inner: Arc::default(),
            kube: Client::try_default()
                .await
                .map_err(|error| anyhow!("failed to load kubernetes account: {error}"))?,
        })
    }
}

impl ::kubegraph_api::resource::NetworkResourceClient for NetworkResourceDB {
    fn kube(&self) -> &Client {
        &self.kube
    }
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
            Some(
                self.connectors
                    .values()
                    .filter(|&cr| cr.spec == r#type)
                    .cloned()
                    .collect(),
            )
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
    pub(crate) async fn try_spawn(
        signal: &FunctionSignal,
        vm: &(impl 'static + Clone + NetworkVirtualMachine),
    ) -> Result<Self> {
        Ok(Self {
            connector_db: NetworkConnectorDBWorker::spawn(vm),
            connector_reloader: NetworkResourceReloader::spawn(signal.clone(), vm),
            function_reloader: NetworkResourceReloader::spawn(signal.clone(), vm),
            problem_reloader: NetworkResourceReloader::spawn(signal.clone(), vm),
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
    fn spawn(vm: &(impl 'static + Clone + NetworkVirtualMachine)) -> Self {
        Self {
            inner: ::tokio::spawn(
                FuturesUnordered::from_iter(vec![
                    #[cfg(feature = "connector-fake")]
                    ::kubegraph_connector_fake::NetworkConnector::default()
                        .loop_forever(vm.clone()),
                    #[cfg(feature = "connector-http")]
                    ::kubegraph_connector_http::NetworkConnector::default()
                        .loop_forever(vm.clone()),
                    #[cfg(feature = "connector-local")]
                    ::kubegraph_connector_local::NetworkConnector::default()
                        .loop_forever(vm.clone()),
                    #[cfg(feature = "connector-prometheus")]
                    ::kubegraph_connector_prometheus::NetworkConnector::default()
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
