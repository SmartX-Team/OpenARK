use chrono::{DateTime, Utc};
use dash_provider_api::job::TaskActorJobMetadata;
use kube::CustomResource;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use strum::{Display, EnumString};

use crate::model::{ModelFieldKindNativeSpec, ModelFieldKindSpec, ModelFieldsSpec};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema, CustomResource)]
#[kube(
    group = "dash.ulagbulag.io",
    version = "v1alpha1",
    kind = "Task",
    root = "TaskCrd",
    status = "TaskStatus",
    shortname = "ta",
    namespaced,
    printcolumn = r#"{
        "name": "state",
        "type": "string",
        "description": "state of the task",
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
pub struct TaskSpec<Kind = ModelFieldKindSpec> {
    pub input: ModelFieldsSpec<Kind>,
    pub actor: TaskActorSpec,
}

impl TaskCrd {
    pub fn get_native_spec(&self) -> &TaskSpec<ModelFieldKindNativeSpec> {
        self.status
            .as_ref()
            .and_then(|status| status.spec.as_ref())
            .expect("native spec should not be empty")
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct TaskStatus {
    #[serde(default)]
    pub state: TaskState,
    pub spec: Option<TaskSpec<ModelFieldKindNativeSpec>>,
    pub last_updated: DateTime<Utc>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum TaskActorSpec {
    Job(TaskActorJobSpec),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct TaskActorJobSpec {
    #[serde(default, flatten)]
    pub metadata: TaskActorJobMetadata,
    pub source: TaskActorSourceSpec,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum TaskActorSourceSpec {
    ConfigMapRef(TaskActorSourceConfigMapRefSpec),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct TaskActorSourceConfigMapRefSpec {
    pub name: String,
    pub path: String,
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
pub enum TaskState {
    #[default]
    Pending,
    Ready,
}
