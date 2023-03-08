use ipis::core::chrono::{DateTime, Utc};
use kube::CustomResource;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, CustomResource)]
#[kube(
    group = "dash.ulagbulag.io",
    version = "v1alpha1",
    kind = "Function",
    struct = "FunctionCrd",
    status = "FunctionStatus",
    shortname = "f",
    printcolumn = r#"{
        "name": "created-at",
        "type": "date",
        "description":"created time",
        "jsonPath":".metadata.creationTimestamp"
    }"#
)]
#[serde(rename_all = "camelCase")]
pub struct FunctionSpec {
    pub input: FunctionFieldsSpec,
    pub output: Option<FunctionFieldsSpec>,
    pub actor: FunctionActorSpec,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct FunctionStatus {
    pub state: Option<String>,
    pub last_updated: DateTime<Utc>,
}

pub type FunctionFieldsSpec = Vec<FunctionFieldSpec>;

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct FunctionFieldSpec {
    pub name: String,
    #[serde(flatten)]
    pub kind: FunctionFieldKindSpec,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum FunctionFieldKindSpec {
    Model {
        name: String,
    },
    Boolean {
        default: Option<bool>,
    },
    Integer {
        default: Option<i64>,
    },
    Float {
        default: Option<f64>,
    },
    OneOfStrings {
        default: Option<String>,
        choices: Vec<String>,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum FunctionActorSpec {
    Job(FunctionActorJobSpec),
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum FunctionActorJobSpec {
    ConfigMap(FunctionActorSourceConfigMapSpec),
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct FunctionActorSourceConfigMapSpec {
    pub name: String,
    pub namespace: String,
    pub path: String,
}
