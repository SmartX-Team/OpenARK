use ark_core_k8s::data::Url;
use kube::CustomResource;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::query::NetworkQuery;

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
