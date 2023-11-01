use ark_core_k8s::data::Url;
use k8s_openapi::api::core::v1::EnvVar;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(transparent)]
pub struct StrawPipe {
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
