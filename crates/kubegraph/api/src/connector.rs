use std::time::{Duration, Instant};

use anyhow::Result;
use ark_core_k8s::data::Url;
use async_trait::async_trait;
use kube::CustomResource;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use tokio::time::sleep;
use tracing::error;

use crate::{db::NetworkGraphDB, query::NetworkQuery};

#[async_trait]
pub trait NetworkConnector {
    fn name(&self) -> &str;

    fn interval(&self) -> Duration {
        Duration::from_secs(15)
    }

    async fn loop_forever(mut self, graph: impl NetworkGraphDB)
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

    async fn pull(&mut self, graph: &impl NetworkGraphDB) -> Result<()>;
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema, CustomResource)]
#[kube(
    group = "kubegraph.ulagbulag.io",
    version = "v1alpha1",
    kind = "NetworkConnector",
    root = "NetworkConnectorCrd",
    shortname = "nc",
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
pub struct NetworkConnectorSpec<T = NetworkConnectorSource> {
    pub src: T,
    pub template: NetworkQuery,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum NetworkConnectorSource {
    Prometheus(NetworkConnectorPrometheusSpec),
}

impl NetworkConnectorSource {
    pub const fn to_ref(&self) -> NetworkConnectorSourceRef {
        match self {
            Self::Prometheus(_) => NetworkConnectorSourceRef::Prometheus,
        }
    }
}

impl PartialEq<NetworkConnectorSourceRef> for NetworkConnectorSource {
    fn eq(&self, other: &NetworkConnectorSourceRef) -> bool {
        self.to_ref() == *other
    }
}

#[derive(
    Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "camelCase")]
pub enum NetworkConnectorSourceRef {
    Prometheus,
}

#[derive(
    Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "camelCase")]
pub struct NetworkConnectorPrometheusSpec {
    pub url: Url,
}
