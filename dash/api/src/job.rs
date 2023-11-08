use std::collections::BTreeMap;

use dash_provider_api::TaskChannel;
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
    root = "DashJobCrd",
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
    pub task: String,
    #[serde(default)]
    #[schemars(schema_with = "DashJobCrd::preserve_arbitrary")]
    pub value: BTreeMap<String, Value>,
}

impl DashJobCrd {
    pub const FINALIZER_NAME: &'static str = "dash.ulagbulag.io/finalizer-jobs";

    pub const LABEL_TARGET_TASK: &'static str = "dash.ulagbulag.io/target-task";
    pub const LABEL_TARGET_TASK_NAMESPACE: &'static str = "dash.ulagbulag.io/target-task-namespace";

    fn preserve_arbitrary(
        _gen: &mut ::schemars::gen::SchemaGenerator,
    ) -> ::schemars::schema::Schema {
        let mut obj = ::schemars::schema::SchemaObject::default();
        obj.extensions
            .insert("x-kubernetes-preserve-unknown-fields".into(), true.into());
        ::schemars::schema::Schema::Object(obj)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct DashJobStatus {
    #[serde(default)]
    pub channel: Option<TaskChannel>,
    #[serde(default)]
    pub state: DashJobState,
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
pub enum DashJobState {
    #[default]
    Pending,
    Running,
    Error,
    Completed,
    Deleting,
}
