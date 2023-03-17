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
    CustomResourceDefinitionRef(ModelCustomResourceDefinitionRefSpec),
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ModelStatus {
    pub state: Option<ModelState>,
    pub fields: Option<ModelFieldsSpec>,
    pub last_updated: DateTime<Utc>,
}

pub type ModelFieldsSpec = Vec<ModelFieldSpec>;

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ModelFieldSpec {
    pub name: String,
    #[serde(flatten)]
    pub kind: ModelFieldKindSpec,
    #[serde(default)]
    pub nullable: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum ModelFieldKindSpec {
    // BEGIN primitive types
    Boolean {
        #[serde(default)]
        default: Option<bool>,
    },
    Integer {
        #[serde(default)]
        default: Option<i64>,
        #[serde(default)]
        minimum: Option<i64>,
        #[serde(default)]
        maximum: Option<i64>,
    },
    Number {
        #[serde(default)]
        default: Option<f64>,
        #[serde(default)]
        minimum: Option<f64>,
        #[serde(default)]
        maximum: Option<f64>,
    },
    String {
        #[serde(default)]
        default: Option<String>,
    },
    OneOfStrings {
        #[serde(default)]
        default: Option<String>,
        choices: Vec<String>,
    },
    // BEGIN string formats
    DateTime {
        #[serde(default)]
        default: Option<ModelFieldDateTimeDefaultType>,
    },
    Ip {},
    Uuid {},
    // BEGIN aggregation types
    Array {
        #[serde(default)]
        children: Vec<String>,
    },
    Object {
        #[serde(default)]
        children: Vec<String>,
        #[serde(default)]
        dynamic: bool,
    },
    // BEGIN reference types
    Model {
        name: String,
    },
}

impl ModelFieldKindSpec {
    pub fn to_type(&self) -> ModelFieldKindType {
        match self {
            // BEGIN primitive types
            Self::Boolean { .. } => ModelFieldKindType::Boolean,
            Self::Integer { .. } => ModelFieldKindType::Integer,
            Self::Number { .. } => ModelFieldKindType::Number,
            Self::String { .. } => ModelFieldKindType::String,
            Self::OneOfStrings { .. } => ModelFieldKindType::OneOfStrings,
            // BEGIN string formats
            Self::DateTime { .. } => ModelFieldKindType::DateTime,
            Self::Ip { .. } => ModelFieldKindType::Ip,
            Self::Uuid { .. } => ModelFieldKindType::Uuid,
            // BEGIN aggregation types
            Self::Array { .. } => ModelFieldKindType::Array,
            Self::Object { .. } => ModelFieldKindType::Object,
            // BEGIN reference types
            Self::Model { .. } => ModelFieldKindType::Model,
        }
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
pub enum ModelFieldKindType {
    // BEGIN primitive types
    Boolean,
    Integer,
    Number,
    String,
    OneOfStrings,
    // BEGIN string formats
    DateTime,
    Ip,
    Uuid,
    // BEGIN aggregation types
    Array,
    Object,
    // BEGIN reference types
    Model,
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
pub enum ModelFieldDateTimeDefaultType {
    Now,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ModelCustomResourceDefinitionRefSpec {
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
