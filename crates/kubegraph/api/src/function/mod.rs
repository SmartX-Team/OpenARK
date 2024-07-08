pub mod annotation;
pub mod call;
#[cfg(feature = "function-fake")]
pub mod fake;
#[cfg(feature = "function-entrypoint")]
pub mod service;
pub mod spawn;
pub mod webhook;

use kube::{CustomResource, CustomResourceExt};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{graph::GraphScope, resource::NetworkResource};

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
    pub template: NetworkFunctionTemplate,
}

impl NetworkResource for NetworkFunctionCrd {
    type Filter = ();

    fn description(&self) -> String {
        <Self as NetworkResource>::type_name().into()
    }

    fn type_name() -> &'static str
    where
        Self: Sized,
    {
        <Self as CustomResourceExt>::crd_name()
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum NetworkFunctionKind {
    Annotation(self::annotation::NetworkFunctionAnnotationSpec),
    #[cfg(feature = "function-fake")]
    Fake(self::fake::NetworkFunctionFakeSpec),
    #[cfg(feature = "function-webhook")]
    Webhook(self::webhook::NetworkFunctionWebhookSpec),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct NetworkFunctionTemplate<Script = String> {
    #[serde(default)]
    pub filter: Option<Script>,
    pub script: Script,
}

#[derive(
    Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "camelCase")]
pub struct FunctionMetadata {
    pub scope: GraphScope,
}

impl FunctionMetadata {
    pub const NAME_STATIC: &'static str = "__static__";
}
