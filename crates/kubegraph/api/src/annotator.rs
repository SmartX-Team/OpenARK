use anyhow::Result;
use async_trait::async_trait;
use kube::CustomResource;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::vm::Script;

#[async_trait]
pub trait NetworkAnnotator<G>
where
    G: Send,
{
    async fn annotate(
        &self,
        graph: G,
        spec: &NetworkAnnotationSpec,
    ) -> Result<NetworkAnnotationSpec<Script>>
    where
        G: 'async_trait;
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema, CustomResource)]
#[kube(
    group = "kubegraph.ulagbulag.io",
    version = "v1alpha1",
    kind = "NetworkAnnotation",
    root = "NetworkAnnotationCrd",
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
        "description": "annotation version",
        "jsonPath": ".metadata.generation"
    }"#
)]
#[serde(rename_all = "camelCase")]
pub struct NetworkAnnotationSpec<Script = String> {
    #[serde(default)]
    pub filter: Option<Script>,
    pub script: Script,
}
