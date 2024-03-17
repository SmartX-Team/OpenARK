use kube::CustomResource;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{r#box::BoxAccessSpec, rack::RackRef};

#[derive(
    Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema, CustomResource,
)]
#[kube(
    category = "kiss",
    group = "kiss.ulagbulag.io",
    version = "v1alpha1",
    kind = "NetBox",
    root = "NetBoxCrd",
    shortname = "nbox",
    printcolumn = r#"{
        "name": "rack",
        "type": "string",
        "description": "rack name where the netbox is located",
        "jsonPath": ".spec.rack.name"
    }"#,
    printcolumn = r#"{
        "name": "version",
        "type": "integer",
        "description": "netbox version",
        "jsonPath": ".metadata.generation"
    }"#
)]
#[serde(rename_all = "camelCase")]
pub struct NetBoxSpec {
    #[serde(default)]
    pub access: BoxAccessSpec,
    #[serde(default)]
    pub ethernet: NetBoxEthernetFeatures,
    #[serde(default)]
    pub rack: Option<RackRef>,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct NetBoxEthernetFeatures {
    #[serde(default = "NetBoxEthernetFeatures::default_route")]
    pub route: bool,
    #[serde(default = "NetBoxEthernetFeatures::default_switch")]
    pub switch: bool,
}

impl Default for NetBoxEthernetFeatures {
    #[inline]
    fn default() -> Self {
        Self {
            route: Self::default_route(),
            switch: Self::default_switch(),
        }
    }
}

impl NetBoxEthernetFeatures {
    #[inline]
    const fn default_route() -> bool {
        false
    }

    #[inline]
    const fn default_switch() -> bool {
        true
    }
}
