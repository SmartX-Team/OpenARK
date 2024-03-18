use ark_core_k8s::data::Url;
use kube::CustomResource;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::query::NetworkQuery;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema, CustomResource)]
#[kube(
    group = "dash.ulagbulag.io",
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
pub struct NetworkConnectorSpec<T = NetworkConnectorType> {
    pub r#type: T,
    pub query: NetworkQuery,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum NetworkConnectorType {
    Prometheus(NetworkConnectorPrometheusSpec),
}

impl NetworkConnectorType {
    pub const fn to_ref(&self) -> NetworkConnectorTypeRef {
        match self {
            Self::Prometheus(_) => NetworkConnectorTypeRef::Prometheus,
        }
    }
}

impl PartialEq<NetworkConnectorTypeRef> for NetworkConnectorType {
    fn eq(&self, other: &NetworkConnectorTypeRef) -> bool {
        self.to_ref() == *other
    }
}

#[derive(
    Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "camelCase")]
pub enum NetworkConnectorTypeRef {
    Prometheus,
}

#[derive(
    Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "camelCase")]
pub struct NetworkConnectorPrometheusSpec {
    pub url: Url,
}
