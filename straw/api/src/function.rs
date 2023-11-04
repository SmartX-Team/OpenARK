use ark_core_k8s::data::Url;
use k8s_openapi::api::core::v1::EnvVar;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use strum::{Display, EnumString};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(transparent)]
pub struct StrawFunction {
    pub straw: Vec<StrawNode>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct StrawNode {
    pub name: String,
    #[serde(default)]
    pub env: Vec<EnvVar>,
    pub src: Url,
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
pub enum StrawFunctionType {
    /// Dynamic function to check models.
    /// No metadata is collected.
    OneShot,
    /// Static function bound to the models.
    /// Collects output metadata in storage.
    Pipe,
}
