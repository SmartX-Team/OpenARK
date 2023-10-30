use std::{collections::BTreeMap, net::Ipv4Addr};

use ark_core_k8s::data::Url;
use k8s_openapi::{
    api::core::v1::ResourceRequirements, apimachinery::pkg::api::resource::Quantity,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum ModelStorageObjectSpec {
    Borrowed(ModelStorageObjectBorrowedSpec),
    Cloned(ModelStorageObjectClonedSpec),
    Owned(#[serde(default)] ModelStorageObjectOwnedSpec),
}

impl Default for ModelStorageObjectSpec {
    fn default() -> Self {
        Self::Owned(Default::default())
    }
}

impl ModelStorageObjectSpec {
    pub(super) const fn is_unique(&self) -> bool {
        match self {
            Self::Borrowed(_) => false,
            Self::Cloned(_) => true,
            Self::Owned(_) => true,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ModelStorageObjectBorrowedSpec {
    #[serde(default, flatten)]
    pub reference: ModelStorageObjectRefSpec,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ModelStorageObjectClonedSpec {
    #[serde(flatten)]
    pub reference: ModelStorageObjectRefSpec,

    #[serde(default, flatten)]
    pub owned: ModelStorageObjectOwnedSpec,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ModelStorageObjectOwnedSpec {
    #[serde(default)]
    pub minio_console_external_service: ModelStorageObjectOwnedExternalServiceSpec,

    #[serde(default)]
    pub minio_external_service: ModelStorageObjectOwnedExternalServiceSpec,

    #[serde(default, flatten)]
    pub replication: ModelStorageObjectOwnedReplicationSpec,

    #[serde(default = "ModelStorageObjectOwnedSpec::default_runtime_class_name")]
    pub runtime_class_name: String,

    #[serde(default = "ModelStorageObjectOwnedSpec::default_storage_class_name")]
    pub storage_class_name: String,
}

impl Default for ModelStorageObjectOwnedSpec {
    fn default() -> Self {
        Self {
            minio_console_external_service: Default::default(),
            minio_external_service: Default::default(),
            replication: Default::default(),
            runtime_class_name: Self::default_runtime_class_name(),
            storage_class_name: Self::default_storage_class_name(),
        }
    }
}

impl ModelStorageObjectOwnedSpec {
    fn default_runtime_class_name() -> String {
        Default::default()
    }

    fn default_storage_class_name() -> String {
        "ceph-block".into()
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ModelStorageObjectOwnedReplicationSpec {
    #[serde(default = "ModelStorageObjectOwnedReplicationSpec::default_resources")]
    pub resources: ResourceRequirements,

    #[serde(default = "ModelStorageObjectOwnedReplicationSpec::default_total_nodes")]
    pub total_nodes: u32,

    #[serde(default = "ModelStorageObjectOwnedReplicationSpec::default_total_volumes_per_node")]
    pub total_volumes_per_node: u32,
}

impl Default for ModelStorageObjectOwnedReplicationSpec {
    fn default() -> Self {
        Self {
            resources: Self::default_resources(),
            total_nodes: Self::default_total_nodes(),
            total_volumes_per_node: Self::default_total_volumes_per_node(),
        }
    }
}

impl ModelStorageObjectOwnedReplicationSpec {
    pub const fn default_resources_cpu() -> &'static str {
        "16"
    }

    pub const fn default_resources_memory() -> &'static str {
        "31Gi"
    }

    pub const fn default_resources_storage() -> &'static str {
        "1TiB"
    }

    fn default_resources() -> ResourceRequirements {
        ResourceRequirements {
            requests: Some({
                let mut map = BTreeMap::default();
                map.insert("cpu".into(), Quantity(Self::default_resources_cpu().into()));
                map.insert(
                    "memory".into(),
                    Quantity(Self::default_resources_memory().into()),
                );
                map.insert(
                    "storage".into(),
                    Quantity(Self::default_resources_storage().into()),
                );
                map
            }),
            ..Default::default()
        }
    }

    const fn default_total_nodes() -> u32 {
        4
    }

    const fn default_total_volumes_per_node() -> u32 {
        4
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ModelStorageObjectOwnedExternalServiceSpec {
    #[serde(default)]
    pub address_pool: Option<String>,

    #[serde(default)]
    pub ip: Option<Ipv4Addr>,
}

impl ModelStorageObjectOwnedExternalServiceSpec {
    pub const fn is_enabled(&self) -> bool {
        self.address_pool.is_some() || self.ip.is_some()
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ModelStorageObjectRefSpec {
    pub endpoint: Url,
    #[serde(default)]
    pub secret_ref: ModelStorageObjectRefSecretRefSpec,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ModelStorageObjectRefSecretRefSpec {
    #[serde(default = "ModelStorageObjectRefSecretRefSpec::default_map_access_key")]
    pub map_access_key: String,
    #[serde(default = "ModelStorageObjectRefSecretRefSpec::default_map_secret_key")]
    pub map_secret_key: String,

    #[serde(default = "ModelStorageObjectRefSecretRefSpec::default_name")]
    pub name: String,
}

impl Default for ModelStorageObjectRefSecretRefSpec {
    fn default() -> Self {
        Self {
            map_access_key: Self::default_map_access_key(),
            map_secret_key: Self::default_map_secret_key(),
            name: Self::default_name(),
        }
    }
}

impl ModelStorageObjectRefSecretRefSpec {
    fn default_map_access_key() -> String {
        "CONSOLE_ACCESS_KEY".into()
    }

    fn default_map_secret_key() -> String {
        "CONSOLE_SECRET_KEY".into()
    }

    fn default_name() -> String {
        "object-storage-user-0".into()
    }
}
