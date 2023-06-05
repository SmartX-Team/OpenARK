pub mod db;
pub mod kubernetes;
pub mod lake;
pub mod warehouse;

use chrono::{DateTime, Utc};
use kube::CustomResource;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use strum::{Display, EnumString};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema, CustomResource)]
#[kube(
    group = "dash.ulagbulag.io",
    version = "v1alpha1",
    kind = "ModelStorage",
    struct = "ModelStorageCrd",
    status = "ModelStorageStatus",
    shortname = "ms",
    namespaced,
    printcolumn = r#"{
        "name": "state",
        "type": "string",
        "description": "state of the model storage",
        "jsonPath": ".status.state"
    }"#,
    printcolumn = r#"{
        "name": "created-at",
        "type": "date",
        "description": "created time",
        "jsonPath": ".metadata.creationTimestamp"
    }"#,
    printcolumn = r#"{
        "name": "updated-at",
        "type": "date",
        "description": "updated time",
        "jsonPath": ".status.lastUpdated"
    }"#
)]
#[serde(rename_all = "camelCase")]
pub struct ModelStorageSpec {
    #[serde(flatten)]
    pub kind: ModelStorageKindSpec,
    #[serde(default)]
    pub default: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum ModelStorageKindSpec {
    Database(self::db::ModelStorageDatabaseSpec),
    Kubernetes(self::kubernetes::ModelStorageKubernetesSpec),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ModelStorageStatus {
    #[serde(default)]
    pub state: ModelStorageState,
    pub kind: Option<ModelStorageKindSpec>,
    pub last_updated: DateTime<Utc>,
}

#[derive(
    Copy,
    Clone,
    Debug,
    Display,
    EnumString,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    JsonSchema,
)]
pub enum ModelStorageState {
    Pending,
    Ready,
}

impl Default for ModelStorageState {
    fn default() -> Self {
        Self::Pending
    }
}
