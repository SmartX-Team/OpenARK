use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::field::NaturalFields;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Session {
    pub metadata: SessionMetadata,
    pub fields: NaturalFields,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SessionMetadata {
    pub username: String,
}
