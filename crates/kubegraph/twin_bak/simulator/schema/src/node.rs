use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::value::NetworkValues;

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct NetworkNode {
    #[serde(flatten)]
    pub values: NetworkValues,
}
