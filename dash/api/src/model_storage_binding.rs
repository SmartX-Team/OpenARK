use chrono::{DateTime, Utc};
use kube::CustomResource;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use strum::{Display, EnumString};

use crate::{model::ModelSpec, storage::ModelStorageSpec};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema, CustomResource)]
#[kube(
    group = "dash.ulagbulag.io",
    version = "v1alpha1",
    kind = "ModelStorageBinding",
    struct = "ModelStorageBindingCrd",
    status = "ModelStorageBindingStatus",
    shortname = "msb",
    printcolumn = r#"{
        "name": "state",
        "type": "string",
        "description": "state of the binding",
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
pub struct ModelStorageBindingSpec {
    pub model: String,
    pub storage: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ModelStorageBindingStatus {
    #[serde(default)]
    pub state: ModelStorageBindingState,
    pub model: Option<ModelSpec>,
    pub storage: Option<ModelStorageSpec>,
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
pub enum ModelStorageBindingState {
    Pending,
    Ready,
}

impl Default for ModelStorageBindingState {
    fn default() -> Self {
        Self::Pending
    }
}
