use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct NetworkConstraint<Filter = String> {
    #[serde(default)]
    pub filters: Vec<Filter>,

    #[serde(default)]
    pub r#where: Vec<Filter>,
}
