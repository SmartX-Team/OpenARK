use std::collections::BTreeMap;

use kube::CustomResource;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema, CustomResource)]
#[kube(
    group = "vine.ulagbulag.io",
    version = "v1alpha1",
    kind = "Display",
    root = "DisplayCrd",
    shortname = "disp",
    printcolumn = r#"{
        "name": "created-at",
        "type": "date",
        "description": "created time",
        "jsonPath": ".metadata.creationTimestamp"
    }"#,
    printcolumn = r#"{
        "name": "version",
        "type": "integer",
        "description": "display version",
        "jsonPath": ".metadata.generation"
    }"#
)]
#[serde(rename_all = "camelCase")]
pub struct DisplaySpec {
    #[serde(default, flatten)]
    pub kind: DisplayKindSpec,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum DisplayKindSpec {
    X11(#[serde(default)] DisplayKindX11Spec),
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct DisplayKindX11Spec {
    #[serde(default)]
    pub monitors: BTreeMap<String, DisplayKindX11SectionMonitorSpec>,

    #[serde(default, flatten)]
    pub others: BTreeMap<String, BTreeMap<String, DisplayKindX11SectionTemplateSpec>>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct DisplayKindX11SectionMonitorSpec {
    #[serde(default, flatten)]
    pub others: DisplayKindX11SectionTemplateSpec,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct DisplayKindX11SectionTemplateSpec {
    #[serde(default)]
    pub driver: Option<String>,
    #[serde(default)]
    pub options: BTreeMap<String, String>,

    #[serde(default, flatten)]
    pub others: BTreeMap<String, Vec<String>>,
}
