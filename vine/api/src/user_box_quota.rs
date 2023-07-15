use ark_core_k8s::data::ImagePullPolicy;
use k8s_openapi::api::core::v1::{ContainerPort, ResourceRequirements};
use kube::CustomResource;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, JsonSchema, CustomResource)]
#[kube(
    group = "vine.ulagbulag.io",
    version = "v1alpha1",
    kind = "UserBoxQuota",
    struct = "UserBoxQuotaCrd",
    shortname = "ubq",
    printcolumn = r#"{
        "name": "amount",
        "type": "number",
        "description": "allowed docker desktop image",
        "jsonPath": ".spec.desktop.container.image"
    }"#,
    printcolumn = r#"{
        "name": "created-at",
        "type": "date",
        "description": "created time",
        "jsonPath": ".metadata.creationTimestamp"
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
    pub volumes: UserBoxQuotaDesktopVolumesSpec,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct UserBoxQuotaDesktopContainerSpec {
    #[serde(default = "UserBoxQuotaDesktopContainerSpec::default_args")]
    pub args: Option<Vec<String>>,
    #[serde(default = "UserBoxQuotaDesktopContainerSpec::default_command")]
    pub command: Option<Vec<String>>,
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
        Some(vec!["/opt/scripts/entrypoint-desktop.sh".into()])
    }

    fn default_image() -> String {
        "quay.io/ulagbulag/openark-vine-desktop:latest".into()
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
pub struct UserBoxQuotaDesktopVolumesSpec {
    #[serde(default = "UserBoxQuotaDesktopVolumesSpec::default_containers")]
    pub containers: bool,
    #[serde(default = "UserBoxQuotaDesktopVolumesSpec::default_public")]
    pub public: bool,
    #[serde(default = "UserBoxQuotaDesktopVolumesSpec::default_static")]
    pub r#static: bool,
}

impl Default for UserBoxQuotaDesktopVolumesSpec {
    fn default() -> Self {
        Self {
            containers: Self::default_containers(),
            public: Self::default_public(),
            r#static: Self::default_static(),
        }
    }
}

impl UserBoxQuotaDesktopVolumesSpec {
    fn default_containers() -> bool {
        false
    }

    fn default_public() -> bool {
        true
    }

    fn default_static() -> bool {
        true
    }
}
