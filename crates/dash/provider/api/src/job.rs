use k8s_openapi::apimachinery::pkg::apis::meta::v1::LabelSelector;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Payload<Value> {
    pub task_name: String,
    #[serde(default)]
    pub namespace: Option<String>,
    pub value: Value,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct TaskChannelKindJob {
    #[serde(default, flatten)]
    pub metadata: TaskActorJobMetadata,
    pub templates: Vec<TemplateRef>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct TaskActorJobMetadata {
    #[serde(default)]
    pub container: Option<String>,
    #[serde(default)]
    pub label_selector: LabelSelector,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct TemplateRef {
    pub name: String,
}
