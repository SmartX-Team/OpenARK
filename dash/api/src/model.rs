use ipis::core::chrono::{DateTime, Utc};
use kube::CustomResource;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use strum::{Display, EnumString};

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, CustomResource)]
#[kube(
    group = "dash.ulagbulag.io",
    version = "v1alpha1",
    kind = "Model",
    struct = "ModelCrd",
    status = "ModelStatus",
    shortname = "m",
    printcolumn = r#"{
        "name": "state",
        "type": "string",
        "description":"state of the model",
        "jsonPath":".status.state"
    }"#,
    printcolumn = r#"{
        "name": "created-at",
        "type": "date",
        "description":"created time",
        "jsonPath":".metadata.creationTimestamp"
    }"#,
    printcolumn = r#"{
        "name": "updated-at",
        "type": "date",
        "description":"updated time",
        "jsonPath":".status.lastUpdated"
    }"#
)]
#[serde(rename_all = "camelCase")]
pub enum ModelSpec {
    Fields(ModelFieldsSpec),
    CustomResourceDefinition(ModelCustomResourceDefinitionSpec),
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ModelStatus {
    pub state: Option<String>,
    pub last_updated: DateTime<Utc>,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ModelFieldsSpec {
    pub name: String,
    #[serde(flatten)]
    pub kind: ModelFieldKindSpec,
    pub nullable: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum ModelFieldKindSpec {
    Boolean {
        default: Option<bool>,
    },
    Integer {
        default: Option<i64>,
    },
    Float {
        default: Option<f64>,
    },
    String {
        default: Option<String>,
    },
    OneOfStrings {
        default: Option<String>,
        choices: Vec<String>,
    },
    Model {
        name: String,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ModelCustomResourceDefinitionSpec {
    pub name: String,
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
pub enum ModelState {
    Pending,
    Ready,
}
