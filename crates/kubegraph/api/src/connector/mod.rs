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
    function::NetworkFunctionCrd,
    graph::{Graph, GraphScope, NetworkGraphDB},
    vm::NetworkVirtualMachine,
};

#[async_trait]
pub trait NetworkConnectorDB {
    async fn delete_connector(&self, key: &GraphScope);

    async fn delete_function(&self, key: &GraphScope);

    async fn insert_connector(&self, object: NetworkConnectorCrd);

    async fn insert_function(&self, object: NetworkFunctionCrd);

    async fn list_connectors(
        &self,
        r#type: NetworkConnectorSourceType,
    ) -> Option<Vec<NetworkConnectorCrd>>;

    async fn list_functions(&self) -> Vec<NetworkFunctionCrd>;
}

#[async_trait]
pub trait NetworkConnector {
    fn connection_type(&self) -> NetworkConnectorSourceType;

    fn name(&self) -> &str;

    #[instrument(level = Level::INFO, skip(self, vm))]
    async fn loop_forever(mut self, vm: impl NetworkVirtualMachine)
    where
        Self: Sized,
    {
        let interval = vm.interval();

        loop {
            let instant = Instant::now();

            if let Some(connectors) = vm
                .connector_db()
                .list_connectors(self.connection_type())
                .await
            {
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

            let elapsed = instant.elapsed();
            if let Some(interval) = interval {
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
                type = NetworkConnectorSourceType::Prometheus.name(),
                spec = spec.name(),
            ),
            #[cfg(feature = "connector-simulation")]
            Self::Simulation(_) => NetworkConnectorSourceType::Simulation.name().into(),
        }
    }

    pub const fn to_ref(&self) -> NetworkConnectorSourceType {
        match self {
            #[cfg(feature = "connector-prometheus")]
            Self::Prometheus(_) => NetworkConnectorSourceType::Prometheus,
            #[cfg(feature = "connector-simulation")]
            Self::Simulation(_) => NetworkConnectorSourceType::Simulation,
        }
    }
}

impl PartialEq<NetworkConnectorSourceType> for NetworkConnectorSpec {
    fn eq(&self, other: &NetworkConnectorSourceType) -> bool {
        self.to_ref() == *other
    }
}

#[derive(
    Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub enum NetworkConnectorSourceType {
    #[cfg(feature = "connector-prometheus")]
    Prometheus,
    #[cfg(feature = "connector-simulation")]
    Simulation,
}

impl NetworkConnectorSourceType {
    pub const fn name(&self) -> &'static str {
        match self {
            #[cfg(feature = "connector-prometheus")]
            Self::Prometheus => "prometheus",
            #[cfg(feature = "connector-simulation")]
            Self::Simulation => "simulation",
        }
    }
}
