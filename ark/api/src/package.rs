use std::collections::BTreeSet;

use kube::CustomResource;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use strum::{Display, EnumString};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema, CustomResource)]
#[kube(
    group = "ark.ulagbulag.io",
    version = "v1alpha1",
    kind = "ArkPackage",
    struct = "ArkPackageCrd",
    shortname = "ab",
    namespaced
)]
#[serde(rename_all = "camelCase")]
pub struct ArkPackageSpec {
    #[serde(flatten)]
    pub kind: ArkPackageKindSpec,
    #[serde(default)]
    pub permissions: Vec<ArkPermissionSpec>,
    #[serde(default)]
    pub user: ArkUserSpec,
}

impl ArkPackageCrd {
    pub fn get_image_version(&self) -> &str {
        "latest"
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum ArkPackageKindSpec {
    Container { base: ArkPackageContainerSpec },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ArkPackageContainerSpec {
    dist: ArkPackageDependencySpec,
    #[serde(default)]
    dependencies: Vec<ArkPackageDependencySpec>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ArkPackageDependencySpec {
    name: String,
    version: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ArkPermissionSpec {
    pub name: ArkPermissionKind,
    pub features: BTreeSet<ArkPermissionFeature>,
}

#[derive(
    Copy,
    Clone,
    Debug,
    Display,
    EnumString,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    JsonSchema,
)]
#[serde(rename_all = "camelCase")]
#[strum(serialize_all = "camelCase")]
pub enum ArkPermissionKind {
    Audio,
    Graphics,
}

#[derive(
    Copy,
    Clone,
    Debug,
    Display,
    EnumString,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    JsonSchema,
)]
#[serde(rename_all = "camelCase")]
#[strum(serialize_all = "camelCase")]
pub enum ArkPermissionFeature {
    All,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ArkUserSpec {
    pub name: String,
    pub uid: u32,
    pub gid: u32,
    shell: String,
    sudo: bool,
}

impl Default for ArkUserSpec {
    fn default() -> Self {
        Self {
            name: "user".into(),
            uid: 2000,
            gid: 2000,
            shell: "sh".into(),
            sudo: false,
        }
    }
}
