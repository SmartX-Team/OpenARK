pub mod constraint;
pub mod derive;
pub mod edge;
pub mod function;
pub mod node;
pub mod value;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct NetworkObjectCrd {
    pub api_version: String,
    pub metadata: NetworkObjectMetadata,
    #[serde(flatten)]
    pub template: NetworkObjectTemplate,
}

#[derive(
    Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "camelCase")]
pub struct NetworkObjectMetadata {
    pub name: String,
    pub namespace: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", content = "spec")]
pub enum NetworkObjectTemplate {
    Constraint(#[serde(default)] self::constraint::NetworkConstraint),
    // Edge(#[serde(default)] self::edge::NetworkEdge),
    Function(#[serde(default)] self::function::NetworkFunction),
    Node(#[serde(default)] self::node::NetworkNode),
}
