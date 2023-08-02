use std::collections::BTreeMap;

use dash_api::model::ModelFieldNativeSpec;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

pub type NaturalFields = BTreeMap<String, NaturalField>;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct NaturalField {
    pub native: ModelFieldNativeSpec,
    pub description: Option<String>,
}
