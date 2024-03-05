use kube::CustomResource;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::query::NetworkQueryNodeType;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema, CustomResource)]
#[kube(
    group = "dash.ulagbulag.io",
    version = "v1alpha1",
    kind = "Function",
    root = "FunctionCrd",
    status = "FunctionStatus",
    shortname = "f",
    namespaced,
    printcolumn = r#"{
        "name": "state",
        "type": "string",
        "description": "state of the function",
        "jsonPath": ".status.state"
    }"#,
    printcolumn = r#"{
        "name": "created-at",
        "type": "date",
        "description": "created time",
        "jsonPath": ".metadata.creationTimestamp"
    }"#,
    printcolumn = r#"{
        "name": "version",
        "type": "integer",
        "description": "function version",
        "jsonPath": ".metadata.generation"
    }"#
)]
pub struct NetworkConnector {
    pub query: String,
    pub sink: NetworkQueryNodeType,
    pub src: NetworkQueryNodeType,
}
