use ark_core_k8s::data::Name;
use chrono::{DateTime, Utc};
use kube::CustomResource;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use straw_api::pipe::StrawPipe;
use strum::{Display, EnumString};

use crate::model::ModelFieldsNativeSpec;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema, CustomResource)]
#[kube(
    group = "dash.ulagbulag.io",
    version = "v1alpha1",
    kind = "Pipe",
    struct = "PipeCrd",
    status = "PipeStatus",
    shortname = "pi",
    namespaced,
    printcolumn = r#"{
        "name": "state",
        "type": "string",
        "description": "state of the pipe",
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
pub struct PipeSpec<Spec = Name, Exec = PipeExec> {
    pub input: Spec,
    pub output: Spec,
    #[serde(default, flatten)]
    pub exec: Exec,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum PipeExec {
    Placeholder {},
    Straw(StrawPipe),
}

impl Default for PipeExec {
    fn default() -> Self {
        Self::Placeholder {}
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PipeStatus {
    #[serde(default)]
    pub state: PipeState,
    pub spec: Option<PipeSpec<ModelFieldsNativeSpec>>,
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
pub enum PipeState {
    #[default]
    Pending,
    Ready,
}
