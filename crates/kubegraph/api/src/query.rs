use core::fmt;
use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct NetworkQuery<M = NetworkQueryMetadata> {
    #[serde(flatten)]
    pub metadata: M,
    pub query: String,
}

impl NetworkQuery {
    pub const fn name(&self) -> &'static str {
        self.metadata.name()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct NetworkQueryMetadata {
    #[serde(default)]
    pub consts: BTreeMap<String, String>,
    pub r#type: NetworkQueryMetadataType,
}

impl NetworkQueryMetadata {
    pub const fn name(&self) -> &'static str {
        self.r#type.name()
    }
}

#[derive(
    Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
pub enum NetworkQueryMetadataType {
    Edge,
    Node,
}

impl fmt::Display for NetworkQueryMetadataType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.name().fmt(f)
    }
}

impl NetworkQueryMetadataType {
    pub const fn name(&self) -> &'static str {
        match self {
            Self::Edge => "edge",
            Self::Node => "node",
        }
    }
}
