use chrono::{DateTime, Utc};
use k8s_openapi::api::core::v1::ResourceRequirements;
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
        "name": "storage",
        "type": "string",
        "description": "attached storage name",
        "jsonPath": ".spec.storageName"
    }"#,
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
    #[serde(default)]
    pub affinity: ModelClaimAffinity,
    #[serde(default = "ModelClaimSpec::default_allow_replacement")]
    pub allow_replacement: bool,
    #[serde(default)]
    pub binding_policy: ModelClaimBindingPolicy,
    #[serde(default)]
    pub deletion_policy: ModelClaimDeletionPolicy,
    #[serde(default)]
    pub resources: Option<ResourceRequirements>,
    #[serde(default)]
    pub storage: Option<ModelStorageKind>,
    #[serde(default)]
    pub storage_name: Option<String>,
}

impl ModelClaimCrd {
    pub const FINALIZER_NAME: &'static str = "dash.ulagbulag.io/finalizer-model-claims";
}

impl Default for ModelClaimSpec {
    fn default() -> Self {
        Self {
            affinity: ModelClaimAffinity::default(),
            allow_replacement: Self::default_allow_replacement(),
            binding_policy: ModelClaimBindingPolicy::default(),
            deletion_policy: ModelClaimDeletionPolicy::default(),
            resources: None,
            storage: None,
            storage_name: None,
        }
    }
}

impl ModelClaimSpec {
    const fn default_allow_replacement() -> bool {
        true
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ModelClaimAffinity {
    #[serde(default)]
    pub placement_affinity: ModelClaimAffinityRequirements,
    #[serde(default)]
    pub replacement_affinity: ModelClaimAffinityRequirements,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ModelClaimAffinityRequirements {
    #[serde(default)]
    pub preferred: Vec<ModelClaimPreferredAffinity>,
    #[serde(default)]
    pub required: Vec<ModelClaimAffinityPreference>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ModelClaimPreferredAffinity {
    #[serde(default, flatten)]
    pub base: ModelClaimAffinityPreference,
    pub weight: u8,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ModelClaimAffinityPreference {
    #[serde(default)]
    pub match_expressions: Vec<ModelClaimAffinityExpression>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ModelClaimAffinityExpression {
    pub query: String,
    #[serde(default)]
    pub source: ModelClaimAffinityExpressionSource,
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
pub enum ModelClaimAffinityExpressionSource {
    #[default]
    Prometheus,
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
    pub resources: Option<ResourceRequirements>,
    #[serde(default)]
    pub storage: Option<ModelStorageKind>,
    #[serde(default)]
    pub storage_name: Option<String>,
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
