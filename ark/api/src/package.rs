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
        "description": "state of the package",
        "jsonPath": ".status.state"
    }"#,
    printcolumn = r#"{
        "name": "created-at",
        "type": "date",
        "description": "created time",
        "jsonPath": ".metadata.creationTimestamp"
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
    Container {
        #[serde(default)]
        base: ArkPackageContainerSpec,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ArkPackageContainerSpec {
    #[serde(default = "ArkPackageDependencySpec::default_dist")]
    dist: ArkPackageDependencySpec,
    #[serde(default)]
    dependencies: Vec<ArkPackageDependencySpec>,
    #[serde(default)]
    entrypoint: Vec<String>,
}

impl Default for ArkPackageContainerSpec {
    fn default() -> Self {
        Self {
            dist: ArkPackageDependencySpec::default_dist(),
            dependencies: Default::default(),
            entrypoint: Default::default(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ArkPackageDependencySpec {
    name: String,
    #[serde(default = "ArkPackageDependencySpec::default_version")]
    version: String,
}

impl ArkPackageDependencySpec {
    fn default_dist() -> Self {
        Self {
            name: Self::default_dist_name(),
            version: Self::default_version(),
        }
    }

    fn default_dist_name() -> String {
        "archlinux".into()
    }

    fn default_version() -> String {
        "latest".into()
    }
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
    #[serde(default = "ArkUserSpec::default_name")]
    pub name: String,
    #[serde(default = "ArkUserSpec::default_uid")]
    pub uid: i64,
    #[serde(default = "ArkUserSpec::default_gid")]
    pub gid: i64,
    #[serde(default = "ArkUserSpec::default_shell")]
    shell: String,
    #[serde(default = "ArkUserSpec::default_sudo")]
    sudo: bool,
}

impl Default for ArkUserSpec {
    fn default() -> Self {
        Self {
            name: Self::default_name(),
            uid: Self::default_uid(),
            gid: Self::default_gid(),
            shell: Self::default_shell(),
            sudo: Self::default_sudo(),
        }
    }
}

impl ArkUserSpec {
    fn default_name() -> String {
        "user".into()
    }

    const fn default_uid() -> i64 {
        2000
    }

    const fn default_gid() -> i64 {
        2000
    }

    fn default_shell() -> String {
        "sh".into()
    }

    const fn default_sudo() -> bool {
        false
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
