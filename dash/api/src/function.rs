use ark_core_k8s::data::Name;
use chrono::{DateTime, Utc};
use kube::CustomResource;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use straw_api::function::{StrawFunction, StrawFunctionType};
use strum::{Display, EnumString};

use crate::model::ModelFieldsNativeSpec;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema, CustomResource)]
#[kube(
    group = "dash.ulagbulag.io",
    version = "v1alpha1",
    kind = "Function",
    struct = "FunctionCrd",
    status = "FunctionStatus",
    shortname = "f",
    namespaced,
    printcolumn = r#"{
        "name": "state",
        "type": "string",
        "description": "state of the function",
        "jsonPath": ".status.state"
    }"#,
    printcolumn = r#"{
        "name": "created-at",
        "type": "date",
        "description": "created time",
        "jsonPath": ".metadata.creationTimestamp"
    }"#,
    printcolumn = r#"{
        "name": "version",
        "type": "integer",
        "description": "model version",
        "jsonPath": ".metadata.generation"
    }"#
)]
#[serde(rename_all = "camelCase")]
pub struct FunctionSpec<Spec = Name, Exec = FunctionExec> {
    pub input: Spec,
    pub output: Spec,
    #[serde(default, flatten)]
    pub exec: Exec,
    #[serde(rename = "type")]
    pub type_: StrawFunctionType,
    pub volatility: FunctionVolatility,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum FunctionExec {
    Placeholder {},
    Straw(StrawFunction),
}

impl Default for FunctionExec {
    fn default() -> Self {
        Self::Placeholder {}
    }
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
pub enum FunctionVolatility {
    /// Immutable function.
    /// If the same input value is received during execution of the function,
    /// the same output value is always returned.
    Immutable,
    /// Stable function.
    /// If the same input value is received during execution of the function,
    /// the output value may change.
    /// However, if the same input value is input with the same timestamp,
    /// the same output value is always returned.
    Stable,
    /// Volatile function.
    /// This can produce different results every time
    /// even if the same input value is input.
    Volatile,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct FunctionStatus {
    #[serde(default)]
    pub state: FunctionState,
    pub spec: Option<FunctionSpec<ModelFieldsNativeSpec>>,
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
pub enum FunctionState {
    #[default]
    Pending,
    Ready,
}
