use k8s_openapi::chrono::{DateTime, Utc};
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
    status = "ArkPackageStatus",
    shortname = "ark",
    namespaced,
    printcolumn = r#"{
        "name": "state",
        "type": "string",
        "description":"state of the package",
        "jsonPath":".status.state"
    }"#,
    printcolumn = r#"{
        "name": "created-at",
        "type": "date",
        "description":"created time",
        "jsonPath":".metadata.creationTimestamp"
    }"#
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
pub struct ArkPackageStatus {
    #[serde(default)]
    pub state: ArkPackageState,
    pub spec: Option<ArkPackageSpec>,
    pub last_updated: DateTime<Utc>,
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
    pub features: Vec<ArkPermissionFeature>,
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
    pub uid: i64,
    pub gid: i64,
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
pub enum ArkPackageState {
    Pending,
    Building,
    Failed,
    Timeout,
    Ready,
}

impl Default for ArkPackageState {
    fn default() -> Self {
        Self::Pending
    }
}
