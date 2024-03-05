use chrono::{DateTime, Utc};
use kube::CustomResource;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use strum::{Display, EnumString};

use crate::storage::ModelStorageKind;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema, CustomResource)]
#[kube(
    group = "dash.ulagbulag.io",
    version = "v1alpha1",
    kind = "ModelClaim",
    root = "ModelClaimCrd",
    status = "ModelClaimStatus",
    shortname = "mc",
    namespaced,
    printcolumn = r#"{
        "name": "state",
        "type": "string",
        "description": "state of the model claim",
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
    }"#,
    printcolumn = r#"{
        "name": "version",
        "type": "integer",
        "description": "model claim version",
        "jsonPath": ".metadata.generation"
    }"#
)]
#[serde(rename_all = "camelCase")]
pub struct ModelClaimSpec {
    #[serde(default = "ModelClaimSpec::default_allow_replacement")]
    pub allow_replacement: bool,
    #[serde(default)]
    pub binding_policy: ModelClaimBindingPolicy,
    #[serde(default)]
    pub deletion_policy: ModelClaimDeletionPolicy,
    pub storage: Option<ModelStorageKind>,
}

impl ModelClaimCrd {
    pub const FINALIZER_NAME: &'static str = "dash.ulagbulag.io/finalizer-model-claims";
}

impl Default for ModelClaimSpec {
    fn default() -> Self {
        Self {
            allow_replacement: Self::default_allow_replacement(),
            binding_policy: ModelClaimBindingPolicy::default(),
            deletion_policy: ModelClaimDeletionPolicy::default(),
            storage: None,
        }
    }
}

impl ModelClaimSpec {
    const fn default_allow_replacement() -> bool {
        true
    }
}

#[derive(
    Copy,
    Clone,
    Debug,
    Display,
    Default,
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
pub enum ModelClaimBindingPolicy {
    Balanced,
    #[default]
    LowestCopy,
    LowestLatency,
}

#[derive(
    Copy,
    Clone,
    Debug,
    Display,
    Default,
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
pub enum ModelClaimDeletionPolicy {
    Delete,
    #[default]
    Retain,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ModelClaimStatus {
    #[serde(default)]
    pub state: ModelClaimState,
    #[serde(default)]
    pub spec: Option<ModelClaimSpec>,
    pub last_updated: DateTime<Utc>,
}

#[derive(
    Copy,
    Clone,
    Debug,
    Display,
    Default,
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
pub enum ModelClaimState {
    #[default]
    Pending,
    Ready,
    Replacing,
    Deleting,
}
