use kubegraph_api::graph::NetworkValue;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use strum::{Display, EnumString};

use crate::derive::NetworkDerive;

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct NetworkConstraint {
    #[serde(default)]
    pub node_affinity: NetworkNodeAffinity,

    #[serde(default)]
    pub value_affinity: NetworkValueAffinity,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct NetworkNodeAffinity {
    pub preferred: Vec<NetworkNodeAffinityWeightedPreference>,
    pub required: NetworkNodeAffinityPreference,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct NetworkNodeAffinityWeightedPreference {
    #[serde(default)]
    pub preference: NetworkNodeAffinityPreference,
    pub weight: u64,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct NetworkNodeAffinityPreference {
    #[serde(default)]
    pub match_expressions: Vec<NetworkNodeAffinityMatchExpression>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct NetworkNodeAffinityMatchExpression {
    pub r#type: String,
    #[serde(flatten)]
    pub operator: NetworkNodeAffinityMatchExpressionOperator,
}

#[derive(
    Copy, Clone, Debug, Display, EnumString, PartialEq, Serialize, Deserialize, JsonSchema,
)]
#[serde(tag = "operator", content = "value")]
pub enum NetworkNodeAffinityMatchExpressionOperator {
    Above(NetworkValue),
    Below(NetworkValue),
    Is(NetworkValue),
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct NetworkValueAffinity {
    #[serde(default)]
    pub match_expressions: Vec<NetworkValueAffinityMatchExpression>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct NetworkValueAffinityMatchExpression {
    pub r#type: String,
    #[serde(default)]
    pub derive: NetworkDerive,
}
