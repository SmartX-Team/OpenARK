#[cfg(feature = "connector-prometheus")]
pub mod prometheus;
#[cfg(feature = "connector-simulation")]
pub mod simulation;

use anyhow::Result;
use async_trait::async_trait;
use futures::{stream::FuturesUnordered, TryStreamExt};
use kube::CustomResource;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use tokio::time::{sleep, Instant};
use tracing::{error, info, instrument, Level};

use crate::{
    frame::LazyFrame,
    graph::{Graph, GraphMetadataRaw, NetworkGraphDB},
    resource::{NetworkResource, NetworkResourceDB},
    vm::NetworkVirtualMachine,
};

#[async_trait]
pub trait NetworkConnector {
    fn connector_type(&self) -> NetworkConnectorType;

    fn name(&self) -> &str;

    #[instrument(level = Level::INFO, skip(self, vm))]
    async fn loop_forever(mut self, vm: impl NetworkVirtualMachine)
    where
        Self: Sized,
    {
        let name = self.name();
        info!("Starting {name} connector...");

        loop {
            let instant = Instant::now();

            if let Some(connectors) = vm.resource_db().list(self.connector_type()).await {
                let name = self.name();
                info!("Reloading {name} connector...");

                match self.pull(connectors).await {
                    Ok(data) => {
                        if let Err(error) = data
                            .into_iter()
                            .map(|data| vm.graph_db().insert(data))
                            .collect::<FuturesUnordered<_>>()
                            .try_collect::<()>()
                            .await
                        {
                            let name = self.name();
                            error!("failed to store graphs from {name:?}: {error}");
                        }
                    }
                    Err(error) => {
                        let name = self.name();
                        error!("failed to pull graphs from {name:?}: {error}");
                    }
                }
            }

            if let Some(interval) = vm.interval() {
                let elapsed = instant.elapsed();
                if elapsed < interval {
                    sleep(interval - elapsed).await;
                }
            }
        }
    }

    async fn pull(&mut self, connectors: Vec<NetworkConnectorCrd>)
        -> Result<Vec<Graph<LazyFrame>>>;
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
#[schemars(bound = "M: Default + JsonSchema")]
#[serde(rename_all = "camelCase")]
pub struct NetworkConnectorSpec<M = GraphMetadataRaw> {
    #[serde(default)]
    pub metadata: M,
    #[serde(flatten)]
    pub kind: NetworkConnectorKind,
}

impl NetworkResource for NetworkConnectorCrd {
    type Filter = NetworkConnectorType;

    fn description(&self) -> String {
        self.spec.name()
    }
}

impl<M> NetworkConnectorSpec<M> {
    pub fn name(&self) -> String {
        self.kind.name()
    }

    pub const fn to_ref(&self) -> NetworkConnectorType {
        self.kind.to_ref()
    }
}

impl<M> PartialEq<NetworkConnectorType> for NetworkConnectorSpec<M> {
    fn eq(&self, other: &NetworkConnectorType) -> bool {
        self.to_ref() == *other
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[non_exhaustive]
#[serde(rename_all = "camelCase")]
pub enum NetworkConnectorKind {
    #[cfg(feature = "connector-prometheus")]
    Prometheus(self::prometheus::NetworkConnectorPrometheusSpec),
    #[cfg(feature = "connector-simulation")]
    Simulation(self::simulation::NetworkConnectorSimulationSpec),
}

impl NetworkConnectorKind {
    fn name(&self) -> String {
        match self {
            #[cfg(feature = "connector-prometheus")]
            Self::Prometheus(spec) => format!(
                "{type}/{spec}",
                type = NetworkConnectorType::Prometheus.name(),
                spec = spec.name(),
            ),
            #[cfg(feature = "connector-simulation")]
            Self::Simulation(_) => NetworkConnectorType::Simulation.name().into(),
        }
    }

    const fn to_ref(&self) -> NetworkConnectorType {
        match self {
            #[cfg(feature = "connector-prometheus")]
            Self::Prometheus(_) => NetworkConnectorType::Prometheus,
            #[cfg(feature = "connector-simulation")]
            Self::Simulation(_) => NetworkConnectorType::Simulation,
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
#[non_exhaustive]
pub enum NetworkConnectorType {
    #[cfg(feature = "connector-prometheus")]
    Prometheus,
    #[cfg(feature = "connector-simulation")]
    Simulation,
}

impl NetworkConnectorType {
    pub const fn name(&self) -> &'static str {
        match self {
            #[cfg(feature = "connector-prometheus")]
            Self::Prometheus => "prometheus",
            #[cfg(feature = "connector-simulation")]
            Self::Simulation => "simulation",
        }
    }
}
