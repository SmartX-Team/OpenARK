use ark_core_k8s::data::ImagePullPolicy;
use k8s_openapi::api::core::v1::{ContainerPort, EnvVar, ResourceRequirements};
use kube::CustomResource;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use strum::{Display, EnumString};

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, JsonSchema, CustomResource)]
#[kube(
    group = "vine.ulagbulag.io",
    version = "v1alpha1",
    kind = "UserBoxQuota",
    root = "UserBoxQuotaCrd",
    shortname = "ubq",
    printcolumn = r#"{
        "name": "image",
        "type": "string",
        "description": "allowed docker desktop image",
        "jsonPath": ".spec.desktop.container.image"
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
        "description": "quota version",
        "jsonPath": ".metadata.generation"
    }"#
)]
#[serde(rename_all = "camelCase")]
pub struct UserBoxQuotaSpec {
    #[serde(default)]
    pub compute: ResourceRequirements,
    #[serde(default)]
    pub desktop: UserBoxQuotaDesktopSpec,
    #[serde(default)]
    pub storage: ResourceRequirements,
    #[serde(default)]
    pub storage_class_name: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct UserBoxQuotaDesktopSpec {
    #[serde(default)]
    pub container: UserBoxQuotaDesktopContainerSpec,
    #[serde(default)]
    pub context: UserBoxQuotaDesktopContextSpec,
    #[serde(default)]
    pub host: UserBoxQuotaDesktopHostSpec,
    #[serde(default)]
    pub user: UserBoxQuotaDesktopUserSpec,
    #[serde(default)]
    pub volumes: UserBoxQuotaDesktopVolumesSpec,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct UserBoxQuotaDesktopContainerSpec {
    #[serde(default = "UserBoxQuotaDesktopContainerSpec::default_args")]
    pub args: Option<Vec<String>>,
    #[serde(default = "UserBoxQuotaDesktopContainerSpec::default_command")]
    pub command: Option<Vec<String>>,
    #[serde(default = "UserBoxQuotaDesktopContainerSpec::default_env")]
    pub env: Option<Vec<EnvVar>>,
    #[serde(default = "UserBoxQuotaDesktopContainerSpec::default_image")]
    pub image: String,
    #[serde(default = "UserBoxQuotaDesktopContainerSpec::default_image_pull_policy")]
    pub image_pull_policy: ImagePullPolicy,
    #[serde(default = "UserBoxQuotaDesktopContainerSpec::default_ports")]
    pub ports: Vec<ContainerPort>,
}

impl Default for UserBoxQuotaDesktopContainerSpec {
    fn default() -> Self {
        Self {
            args: Self::default_args(),
            command: Self::default_command(),
            env: Self::default_env(),
            image: Self::default_image(),
            image_pull_policy: Self::default_image_pull_policy(),
            ports: Self::default_ports(),
        }
    }
}

impl UserBoxQuotaDesktopContainerSpec {
    fn default_args() -> Option<Vec<String>> {
        None
    }

    fn default_command() -> Option<Vec<String>> {
        Some(vec![
            "/usr/bin/env".into(),
            "/usr/bin/systemctl".into(),
            "init".into(),
            "vine-desktop.service".into(),
            "--system".into(),
        ])
    }

    fn default_env() -> Option<Vec<EnvVar>> {
        None
    }

    fn default_image() -> String {
        "quay.io/ulagbulag/openark-vine-desktop:latest-rockylinux".into()
    }

    fn default_image_pull_policy() -> ImagePullPolicy {
        ImagePullPolicy::Always
    }

    fn default_ports() -> Vec<ContainerPort> {
        vec![ContainerPort {
            name: Some("http".into()),
            container_port: 8080,
            ..Default::default()
        }]
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct UserBoxQuotaDesktopContextSpec {
    #[serde(default = "UserBoxQuotaDesktopContextSpec::default_gid")]
    pub gid: u32,
    #[serde(default = "UserBoxQuotaDesktopContextSpec::default_root")]
    pub root: bool,
    #[serde(default = "UserBoxQuotaDesktopContextSpec::default_sudo")]
    pub sudo: bool,
    #[serde(default = "UserBoxQuotaDesktopContextSpec::default_uid")]
    pub uid: u32,
}

impl Default for UserBoxQuotaDesktopContextSpec {
    fn default() -> Self {
        Self {
            gid: Self::default_gid(),
            root: Self::default_root(),
            sudo: Self::default_sudo(),
            uid: Self::default_uid(),
        }
    }
}

impl UserBoxQuotaDesktopContextSpec {
    fn default_gid() -> u32 {
        2000
    }

    fn default_root() -> bool {
        false
    }

    fn default_sudo() -> bool {
        true
    }

    fn default_uid() -> u32 {
        2000
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct UserBoxQuotaDesktopHostSpec {
    #[serde(default = "UserBoxQuotaDesktopHostSpec::default_ipc")]
    pub ipc: bool,
    #[serde(default = "UserBoxQuotaDesktopHostSpec::default_network")]
    pub network: bool,
    #[serde(default = "UserBoxQuotaDesktopHostSpec::default_privileged")]
    pub privileged: bool,
}

impl Default for UserBoxQuotaDesktopHostSpec {
    fn default() -> Self {
        Self {
            ipc: Self::default_ipc(),
            network: Self::default_network(),
            privileged: Self::default_privileged(),
        }
    }
}

impl UserBoxQuotaDesktopHostSpec {
    fn default_ipc() -> bool {
        true
    }

    fn default_network() -> bool {
        false
    }

    fn default_privileged() -> bool {
        true
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct UserBoxQuotaDesktopUserSpec {
    #[serde(default = "UserBoxQuotaDesktopUserSpec::default_lang")]
    pub lang: String,
    #[serde(default)]
    pub lc: UserBoxQuotaDesktopUserLcSpec,
    #[serde(default = "UserBoxQuotaDesktopUserSpec::default_locale")]
    pub locale: String,
    #[serde(default)]
    pub template: UserBoxQuotaDesktopUserTemplateSpec,
}

impl Default for UserBoxQuotaDesktopUserSpec {
    fn default() -> Self {
        Self {
            lang: Self::default_lang(),
            lc: Default::default(),
            locale: Self::default_locale(),
            template: Default::default(),
        }
    }
}

impl UserBoxQuotaDesktopUserSpec {
    fn default_lang() -> String {
        "ko_KR.UTF-8".into()
    }

    fn default_locale() -> String {
        Self::default_lang()
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct UserBoxQuotaDesktopUserLcSpec {
    #[serde(default = "UserBoxQuotaDesktopUserLcSpec::default_all")]
    pub all: String,
}

impl Default for UserBoxQuotaDesktopUserLcSpec {
    fn default() -> Self {
        Self {
            all: Self::default_all(),
        }
    }
}

impl UserBoxQuotaDesktopUserLcSpec {
    fn default_all() -> String {
        UserBoxQuotaDesktopUserSpec::default_lang()
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct UserBoxQuotaDesktopUserTemplateSpec {
    #[serde(default = "UserBoxQuotaDesktopUserTemplateSpec::default_fonts_url")]
    pub fonts_url: String,
    #[serde(default = "UserBoxQuotaDesktopUserTemplateSpec::default_git")]
    pub git: String,
    #[serde(default = "UserBoxQuotaDesktopUserTemplateSpec::default_git_branch")]
    pub git_branch: String,
    #[serde(default = "UserBoxQuotaDesktopUserTemplateSpec::default_icons_url")]
    pub icons_url: String,
    #[serde(default = "UserBoxQuotaDesktopUserTemplateSpec::default_themes_url")]
    pub themes_url: String,
}

impl Default for UserBoxQuotaDesktopUserTemplateSpec {
    fn default() -> Self {
        Self {
            fonts_url: Self::default_fonts_url(),
            git: Self::default_git(),
            git_branch: Self::default_git_branch(),
            icons_url: Self::default_icons_url(),
            themes_url: Self::default_themes_url(),
        }
    }
}

impl UserBoxQuotaDesktopUserTemplateSpec {
    fn default_fonts_url() -> String {
        Default::default()
    }

    fn default_git() -> String {
        "https://github.com/ulagbulag/openark-desktop-template.git".into()
    }

    fn default_git_branch() -> String {
        "master".into()
    }

    fn default_icons_url() -> String {
        Default::default()
    }

    fn default_themes_url() -> String {
        Default::default()
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct UserBoxQuotaDesktopVolumesSpec {
    #[serde(default = "UserBoxQuotaDesktopVolumesSpec::default_containers")]
    pub containers: UserBoxQuotaDesktopVolumeKind,
    #[serde(default = "UserBoxQuotaDesktopVolumesSpec::default_home")]
    pub home: UserBoxQuotaDesktopVolumeKind,
    #[serde(default = "UserBoxQuotaDesktopVolumesSpec::default_home_base")]
    pub home_base: String,
    #[serde(default = "UserBoxQuotaDesktopVolumesSpec::default_public")]
    pub public: bool,
    #[serde(default = "UserBoxQuotaDesktopVolumesSpec::default_static")]
    pub r#static: bool,
}

impl Default for UserBoxQuotaDesktopVolumesSpec {
    fn default() -> Self {
        Self {
            containers: Self::default_containers(),
            home: Self::default_home(),
            home_base: Self::default_home_base(),
            public: Self::default_public(),
            r#static: Self::default_static(),
        }
    }
}

impl UserBoxQuotaDesktopVolumesSpec {
    fn default_containers() -> UserBoxQuotaDesktopVolumeKind {
        UserBoxQuotaDesktopVolumeKind::LocalShared
    }

    fn default_home() -> UserBoxQuotaDesktopVolumeKind {
        UserBoxQuotaDesktopVolumeKind::LocalOwned
    }

    fn default_home_base() -> String {
        "/".into()
    }

    fn default_public() -> bool {
        true
    }

    fn default_static() -> bool {
        true
    }
}

#[derive(
    Copy,
    Clone,
    Debug,
    Display,
    Default,
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
pub enum UserBoxQuotaDesktopVolumeKind {
    #[default]
    LocalOwned,
    LocalShared,
    RemoteOwned,
    Temporary,
}
