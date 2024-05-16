#[cfg(feature = "connector-prometheus")]
pub mod prometheus;
#[cfg(feature = "connector-simulation")]
pub mod simulation;

use std::time::{Duration, Instant};

use anyhow::Result;
use async_trait::async_trait;
use kube::CustomResource;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use tokio::time::sleep;
use tracing::error;

use crate::db::NetworkGraphDB;

#[async_trait]
pub trait NetworkConnectors {
    async fn add_connector(&self, object: NetworkConnectorCrd);

    async fn delete_connector(&self, namespace: String, name: String);

    async fn get_connectors(
        &self,
        r#type: NetworkConnectorSourceRef,
    ) -> Option<Vec<NetworkConnectorCrd>>;
}

#[async_trait]
pub trait NetworkConnector {
    fn name(&self) -> &str;

    fn interval(&self) -> Duration {
        Duration::from_secs(15)
    }

    async fn loop_forever(mut self, graph: impl NetworkConnectors + NetworkGraphDB)
    where
        Self: Sized,
    {
        let interval = <Self as NetworkConnector>::interval(&self);

        loop {
            let instant = Instant::now();
            if let Err(error) = self.pull(&graph).await {
                let name = <Self as NetworkConnector>::name(&self);
                error!("failed to connect to dataset from {name:?}: {error}");
            }

            let elapsed = instant.elapsed();
            if elapsed < interval {
                sleep(interval - elapsed).await;
            }
        }
    }

    async fn pull(&mut self, graph: &(impl NetworkConnectors + NetworkGraphDB)) -> Result<()>;
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
#[non_exhaustive]
pub enum NetworkConnectorSpec {
    #[cfg(feature = "connector-prometheus")]
    Prometheus(self::prometheus::NetworkConnectorPrometheusSpec),
    #[cfg(feature = "connector-simulation")]
    Simulation(self::simulation::NetworkConnectorSimulationSpec),
}

impl NetworkConnectorSpec {
    pub fn name(&self) -> String {
        match self {
            #[cfg(feature = "connector-prometheus")]
            Self::Prometheus(spec) => format!(
                "{type}/{spec}",
                type = NetworkConnectorSourceRef::Prometheus.name(),
                spec = spec.name(),
            ),
            #[cfg(feature = "connector-simulation")]
            Self::Simulation(_) => NetworkConnectorSourceRef::Simulation.name().into(),
        }
    }

    pub const fn to_ref(&self) -> NetworkConnectorSourceRef {
        match self {
            #[cfg(feature = "connector-prometheus")]
            Self::Prometheus(_) => NetworkConnectorSourceRef::Prometheus,
            #[cfg(feature = "connector-simulation")]
            Self::Simulation(_) => NetworkConnectorSourceRef::Simulation,
        }
    }
}

impl PartialEq<NetworkConnectorSourceRef> for NetworkConnectorSpec {
    fn eq(&self, other: &NetworkConnectorSourceRef) -> bool {
        self.to_ref() == *other
    }
}

#[derive(
    Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub enum NetworkConnectorSourceRef {
    #[cfg(feature = "connector-prometheus")]
    Prometheus,
    #[cfg(feature = "connector-simulation")]
    Simulation,
}

impl NetworkConnectorSourceRef {
    pub const fn name(&self) -> &'static str {
        match self {
            #[cfg(feature = "connector-prometheus")]
            Self::Prometheus => "prometheus",
            #[cfg(feature = "connector-simulation")]
            Self::Simulation => "simulation",
        }
    }
}
