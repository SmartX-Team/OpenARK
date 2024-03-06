use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(
    Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "camelCase")]
pub struct NetworkQuery {
    pub link: NetworkQueryNodeType,
    pub query: String,
    pub sink: NetworkQueryNodeType,
    pub src: NetworkQueryNodeType,
}

#[derive(
    Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "camelCase")]
pub struct NetworkQueryNodeType {
    pub kind: NetworkQueryNodeValue,
    pub name: NetworkQueryNodeValue,
    pub namespace: NetworkQueryNodeValue,
}

#[derive(
    Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "camelCase", tag = "type", content = "value")]
pub enum NetworkQueryNodeValue {
    Key(String),
    Static(#[serde(default, skip_serializing_if = "Option::is_none")] Option<String>),
}
