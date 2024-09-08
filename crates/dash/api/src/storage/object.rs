use std::net::Ipv4Addr;

use ark_core_k8s::data::Url;
use k8s_openapi::{
    api::core::v1::ResourceRequirements, apimachinery::pkg::api::resource::Quantity,
};
use maplit::btreemap;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::model_user::ModelUserAccessTokenSecretRefSpec;

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
    #[inline]
    pub(super) fn endpoint(&self, namespace: &str) -> Option<Url> {
        match self {
            Self::Borrowed(spec) => spec.endpoint(),
            Self::Cloned(spec) => spec.endpoint(namespace),
            Self::Owned(spec) => spec.endpoint(namespace),
        }
    }

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

impl ModelStorageObjectBorrowedSpec {
    #[inline]
    fn endpoint(&self) -> Option<Url> {
        self.reference.endpoint()
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ModelStorageObjectClonedSpec {
    #[serde(flatten)]
    pub reference: ModelStorageObjectRefSpec,

    #[serde(default, flatten)]
    pub owned: ModelStorageObjectOwnedSpec,
}

impl ModelStorageObjectClonedSpec {
    #[inline]
    fn endpoint(&self, namespace: &str) -> Option<Url> {
        self.owned.endpoint(namespace)
    }
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

    #[inline]
    fn endpoint(&self, namespace: &str) -> Option<Url> {
        get_kubernetes_minio_endpoint(namespace)
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
            requests: Some(btreemap! {
                "cpu".into() => Quantity(Self::default_resources_cpu().into()),
                "memory".into() => Quantity(Self::default_resources_memory().into()),
                "storage".into() => Quantity(Self::default_resources_storage().into()),
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
    pub secret_ref: ModelUserAccessTokenSecretRefSpec,
}

impl ModelStorageObjectRefSpec {
    #[inline]
    fn endpoint(&self) -> Option<Url> {
        Some(self.endpoint.clone())
    }
}

#[inline]
pub fn get_kubernetes_minio_endpoint(namespace: &str) -> Option<Url> {
    format!("http://object-storage.{namespace}.svc")
        .parse()
        .ok()
}
