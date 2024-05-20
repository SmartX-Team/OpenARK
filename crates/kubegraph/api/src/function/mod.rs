#[cfg(feature = "function-dummy")]
pub mod dummy;

use async_trait::async_trait;
use kube::CustomResource;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::graph::GraphScope;

#[async_trait]
pub trait NetworkFunctionDB {
    async fn delete_function(&self, key: &GraphScope);

    async fn insert_function(&self, object: NetworkFunctionCrd);

    async fn list_functions(&self) -> Vec<NetworkFunctionCrd>;
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema, CustomResource)]
#[kube(
    group = "kubegraph.ulagbulag.io",
    version = "v1alpha1",
    kind = "NetworkFunction",
    root = "NetworkFunctionCrd",
    shortname = "nf",
    namespaced,
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
#[serde(rename_all = "camelCase")]
pub struct NetworkFunctionSpec {
    #[serde(flatten)]
    pub kind: NetworkFunctionKind,
    #[serde(flatten)]
    pub metadata: NetworkFunctionMetadata,
}

#[derive(
    Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "camelCase")]
pub struct NetworkFunctionMetadata<Script = String> {
    #[serde(default)]
    pub filter: Option<Script>,
    pub script: Script,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub enum NetworkFunctionKind {
    #[cfg(feature = "function-dummy")]
    Dummy(self::dummy::NetworkFunctionDummySpec),
}

#[derive(
    Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "camelCase")]
pub struct FunctionMetadata {
    pub name: String,
}

impl FunctionMetadata {
    pub const NAME_STATIC: &'static str = "static";
}
