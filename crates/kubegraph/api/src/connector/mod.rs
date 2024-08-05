#[cfg(feature = "connector-fake")]
pub mod fake;
#[cfg(feature = "connector-http")]
pub mod http;
#[cfg(feature = "connector-local")]
pub mod local;
#[cfg(feature = "connector-prometheus")]
pub mod prometheus;

use std::collections::BTreeMap;

use anyhow::Result;
use async_trait::async_trait;
use futures::{stream::FuturesUnordered, TryStreamExt};
use kube::{CustomResource, CustomResourceExt};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use tokio::time::{sleep, Instant};
use tracing::{error, info, instrument, Level};

use crate::{
    frame::LazyFrame,
    graph::{Graph, GraphData, GraphScope, NetworkGraphDB},
    resource::{NetworkResource, NetworkResourceDB},
    visualizer::NetworkVisualizerExt,
    vm::{NetworkVirtualMachine, NetworkVirtualMachineRestartPolicy},
};

#[async_trait]
pub trait NetworkConnectorExt
where
    Self: NetworkConnector,
{
    #[instrument(level = Level::INFO, skip(self, vm))]
    async fn loop_forever(mut self, vm: impl NetworkVirtualMachine)
    where
        Self: Sized,
    {
        let name = self.name();
        info!("Starting {name} connector...");

        let mut inited = false;
        let mut scopes = BTreeMap::default();
        loop {
            let instant = Instant::now();

            if let Some(connectors) = vm.resource_db().list(self.connector_type()).await {
                inited = true;

                let name = self.name();
                info!("Reloading {name} connector...");

                // Collect all new/updated resource scopes
                let mut new_connectors = Vec::default();
                let mut new_scopes = BTreeMap::default();
                for cr in connectors {
                    let scope = GraphScope::from_resource::<NetworkConnectorCrd>(&cr);
                    let version = cr.metadata.resource_version.clone();

                    if scopes
                        .get(&scope) // updated
                        .map(|last_version| last_version != &version)
                        // new
                        .unwrap_or(true)
                    {
                        new_connectors.push(cr);
                    }
                    new_scopes.insert(scope, version);
                }

                // Collect all removed scopes
                let mut events: Vec<_> = scopes
                    .keys()
                    .filter(|&scope| !new_scopes.contains_key(scope))
                    .cloned()
                    .map(NetworkConnectorEvent::Deleted)
                    .collect();

                match self.pull(new_connectors).await {
                    Ok(data) => {
                        // Collect all new/updated resources
                        events.extend(data.into_iter().map(NetworkConnectorEvent::Applied));

                        // Notify all events
                        match events
                            .into_iter()
                            .map(|event| match event {
                                NetworkConnectorEvent::Applied(data) => vm.graph_db().insert(data),
                                NetworkConnectorEvent::Deleted(scope) => {
                                    vm.graph_db().remove(scope)
                                }
                            })
                            .collect::<FuturesUnordered<_>>()
                            .try_collect::<()>()
                            .await
                        {
                            Ok(()) => {
                                // Update the scopes database
                                scopes = new_scopes;
                            }
                            Err(error) => {
                                let name = self.name();
                                error!("failed to store graphs from {name:?}: {error}");
                            }
                        }
                    }
                    Err(error) => {
                        let name = self.name();
                        error!("failed to pull graphs from {name:?}: {error}");
                    }
                }
            }

            let interval = match vm.restart_policy() {
                NetworkVirtualMachineRestartPolicy::Always => {
                    if inited {
                        NetworkVirtualMachineRestartPolicy::DEFAULT_INTERVAL
                    } else {
                        NetworkVirtualMachineRestartPolicy::DEFAULT_INTERVAL_INIT
                    }
                }
                NetworkVirtualMachineRestartPolicy::Manually => {
                    if inited {
                        match vm.visualizer().wait_to_next().await {
                            Ok(()) => continue,
                            Err(error) => {
                                error!("failed to wait visualizer next event: {error}");
                                break;
                            }
                        }
                    } else {
                        NetworkVirtualMachineRestartPolicy::DEFAULT_INTERVAL_INIT
                    }
                }
                NetworkVirtualMachineRestartPolicy::Interval { interval } => interval,
                NetworkVirtualMachineRestartPolicy::Never => {
                    if inited {
                        let name = self.name();
                        info!("Completed {name} connector");
                        break;
                    } else {
                        NetworkVirtualMachineRestartPolicy::DEFAULT_INTERVAL_INIT
                    }
                }
            };
            let elapsed = instant.elapsed();
            if elapsed < interval {
                sleep(interval - elapsed).await;
            }
        }
    }
}

#[async_trait]
impl<T> NetworkConnectorExt for T where Self: NetworkConnector {}

#[async_trait]
pub trait NetworkConnector {
    fn connector_type(&self) -> NetworkConnectorType;

    fn name(&self) -> &str;

    async fn pull(
        &mut self,
        connectors: Vec<NetworkConnectorCrd>,
    ) -> Result<Vec<Graph<GraphData<LazyFrame>>>>;
}

enum NetworkConnectorEvent {
    Applied(Graph<GraphData<LazyFrame>>),
    Deleted(GraphScope),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema, CustomResource)]
#[kube(
    group = "kubegraph.ulagbulag.io",
    version = "v1alpha1",
    kind = "NetworkConnector",
    root = "NetworkConnectorCrd",
    shortname = "nc",
    namespaced,
    printcolumn = r#"{
        "name": "created-at",
        "type": "date",
        "description": "created time",
        "jsonPath": ".metadata.creationTimestamp"
    }"#,
    printcolumn = r#"{
        "name": "version",
        "type": "integer",
        "description": "connector version",
        "jsonPath": ".metadata.generation"
    }"#
)]
#[serde(rename_all = "camelCase")]
pub struct NetworkConnectorSpec {
    #[serde(flatten)]
    pub kind: NetworkConnectorKind,
}

impl NetworkResource for NetworkConnectorCrd {
    type Filter = NetworkConnectorType;

    fn description(&self) -> String {
        self.spec.name()
    }

    fn type_name() -> &'static str
    where
        Self: Sized,
    {
        <Self as CustomResourceExt>::crd_name()
    }
}

impl NetworkConnectorSpec {
    pub fn name(&self) -> String {
        self.kind.name()
    }

    pub const fn to_ref(&self) -> NetworkConnectorType {
        self.kind.to_ref()
    }
}

impl PartialEq<NetworkConnectorType> for NetworkConnectorSpec {
    fn eq(&self, other: &NetworkConnectorType) -> bool {
        self.to_ref() == *other
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[non_exhaustive]
#[serde(rename_all = "camelCase")]
pub enum NetworkConnectorKind {
    Unknown {},
    #[cfg(feature = "connector-fake")]
    Fake(self::fake::NetworkConnectorFakeSpec),
    #[cfg(feature = "connector-local")]
    Http(self::http::NetworkConnectorHttpSpec),
    #[cfg(feature = "connector-local")]
    Local(self::local::NetworkConnectorLocalSpec),
    #[cfg(feature = "connector-prometheus")]
    Prometheus(self::prometheus::NetworkConnectorPrometheusSpec),
}

impl NetworkConnectorKind {
    fn name(&self) -> String {
        match self {
            Self::Unknown {} => NetworkConnectorType::Unknown.name().into(),
            #[cfg(feature = "connector-fake")]
            Self::Fake(_) => NetworkConnectorType::Fake.name().into(),
            #[cfg(feature = "connector-http")]
            Self::Http(_) => NetworkConnectorType::Http.name().into(),
            #[cfg(feature = "connector-local")]
            Self::Local(_) => NetworkConnectorType::Local.name().into(),
            #[cfg(feature = "connector-prometheus")]
            Self::Prometheus(spec) => format!(
                "{type}/{spec}",
                type = NetworkConnectorType::Prometheus.name(),
                spec = spec.name(),
            ),
        }
    }

    const fn to_ref(&self) -> NetworkConnectorType {
        match self {
            Self::Unknown {} => NetworkConnectorType::Unknown,
            #[cfg(feature = "connector-fake")]
            Self::Fake(_) => NetworkConnectorType::Fake,
            #[cfg(feature = "connector-http")]
            Self::Http(_) => NetworkConnectorType::Http,
            #[cfg(feature = "connector-local")]
            Self::Local(_) => NetworkConnectorType::Local,
            #[cfg(feature = "connector-prometheus")]
            Self::Prometheus(_) => NetworkConnectorType::Prometheus,
        }
    }
}

impl PartialEq<NetworkConnectorType> for NetworkConnectorKind {
    fn eq(&self, other: &NetworkConnectorType) -> bool {
        self.to_ref() == *other
    }
}

#[derive(
    Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "camelCase")]
pub enum NetworkConnectorType {
    Unknown,
    #[cfg(feature = "connector-fake")]
    Fake,
    #[cfg(feature = "connector-http")]
    Http,
    #[cfg(feature = "connector-local")]
    Local,
    #[cfg(feature = "connector-prometheus")]
    Prometheus,
}

impl NetworkConnectorType {
    pub const fn name(&self) -> &'static str {
        match self {
            Self::Unknown => "unknown",
            #[cfg(feature = "connector-fake")]
            Self::Fake => "fake",
            #[cfg(feature = "connector-http")]
            Self::Http => "http",
            #[cfg(feature = "connector-local")]
            Self::Local => "local",
            #[cfg(feature = "connector-prometheus")]
            Self::Prometheus => "prometheus",
        }
    }
}
