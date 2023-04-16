use kube::CustomResource;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use strum::{Display, EnumString};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema, CustomResource)]
#[kube(
    group = "ark.ulagbulag.io",
    version = "v1alpha1",
    kind = "ArkBuild",
    struct = "ArkBuildCrd",
    shortname = "ab"
)]
#[serde(rename_all = "camelCase")]
pub struct ArkBuildSpec {
    pub base: ArkBuildBaseSpec,
    pub permissions: Vec<ArkPermissionSpec>,
    pub user: ArkUserSpec,
}

impl ArkBuildCrd {
    pub fn get_image_version(&self) -> &str {
        "latest"
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ArkBuildBaseSpec {
    dist: ArkBuildDependencySpec,
    dependencies: Vec<ArkBuildDependencySpec>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ArkBuildDependencySpec {
    name: String,
    version: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ArkPermissionSpec {
    pub name: ArkPermissionKind,
    pub features: Vec<String>,
}

#[derive(
    Copy, Clone, Debug, Display, EnumString, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "camelCase")]
#[strum(serialize_all = "camelCase")]
pub enum ArkPermissionKind {
    Audio,
    Graphics,
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
