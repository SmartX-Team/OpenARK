use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::graph::NetworkNodeKey;

#[derive(
    Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "camelCase")]
pub struct GetEdge {
    pub namespace: String,
    pub link_kind: String,
    pub node_kind: String,
    pub link_name: String,
    pub sink_name: String,
    pub src_name: String,
}

#[derive(
    Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
pub struct GetGraph {}

#[derive(
    Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
#[serde(transparent)]
pub struct GetNode(pub NetworkNodeKey);
