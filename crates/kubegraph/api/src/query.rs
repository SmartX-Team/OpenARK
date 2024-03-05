use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct NetworkQuery {
    pub query: String,
    pub sink: NetworkQueryNodeType,
    pub src: NetworkQueryNodeType,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct NetworkQueryNodeType {
    pub kind: NetworkQueryNodeValue,
    pub name: NetworkQueryNodeValue,
    pub namespace: NetworkQueryNodeValue,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(tag = "type", content = "value")]
pub enum NetworkQueryNodeValue {
    Key(String),
    Static(#[serde(default)] Option<String>),
}
