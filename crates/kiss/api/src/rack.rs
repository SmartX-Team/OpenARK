use kube::CustomResource;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(
    Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema, CustomResource,
)]
#[kube(
    category = "kiss",
    group = "kiss.ulagbulag.io",
    version = "v1alpha1",
    kind = "Rack",
    root = "RackCrd",
    printcolumn = r#"{
        "name": "version",
        "type": "integer",
        "description": "rack version",
        "jsonPath": ".metadata.generation"
    }"#
)]
#[serde(rename_all = "camelCase")]
pub struct RackSpec {
    #[serde(default)]
    pub depth: RackDepth,
    pub size: u8,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum RackDepth {
    #[default]
    Full,
    Half,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RackRef {
    #[serde(default)]
    pub depth: RackRefDepth,
    pub name: String,
    pub size: RackRefSize,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RackRefSize {
    pub begin: u8,
    pub end: u8,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum RackRefDepth {
    Back,
    Front,
    #[default]
    Full,
}
