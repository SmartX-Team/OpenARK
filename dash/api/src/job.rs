use std::collections::BTreeMap;

use dash_provider_api::FunctionChannel;
use k8s_openapi::chrono::{DateTime, Utc};
use kube::CustomResource;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use strum::{Display, EnumString};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema, CustomResource)]
#[kube(
    group = "dash.ulagbulag.io",
    version = "v1alpha1",
    kind = "DashJob",
    struct = "DashJobCrd",
    status = "DashJobStatus",
    shortname = "djob",
    namespaced,
    printcolumn = r#"{
        "name": "state",
        "type": "string",
        "description": "state of the dash job",
        "jsonPath": ".status.state"
    }"#,
    printcolumn = r#"{
        "name": "created-at",
        "type": "date",
        "description": "created time",
        "jsonPath": ".metadata.creationTimestamp"
    }"#
)]
#[serde(rename_all = "camelCase")]
pub struct DashJobSpec {
    pub function: String,
    #[serde(default)]
    pub value: BTreeMap<String, Value>,
}

impl DashJobCrd {
    pub const LABEL_TARGET_FUNCTION: &'static str = "dash.ulagbulag.io/target-function";
    pub const LABEL_TARGET_FUNCTION_NAMESPACE: &'static str =
        "dash.ulagbulag.io/target-function-namespace";
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct DashJobStatus {
    #[serde(default)]
    pub channel: Option<FunctionChannel>,
    #[serde(default)]
    pub state: DashJobState,
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
pub enum DashJobState {
    Pending,
    Running,
    Completed,
}

impl Default for DashJobState {
    fn default() -> Self {
        Self::Pending
    }
}
