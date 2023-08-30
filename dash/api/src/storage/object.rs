use std::collections::BTreeMap;

use k8s_openapi::{
    api::core::v1::ResourceRequirements, apimachinery::pkg::api::resource::Quantity,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use strum::{Display, EnumString};
use vine_api::user_auth::Url;

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
    pub deletion_policy: ModelStorageObjectDeletionPolicy,

    #[serde(default = "ModelStorageObjectOwnedSpec::default_resources")]
    pub resources: ResourceRequirements,
}

impl Default for ModelStorageObjectOwnedSpec {
    fn default() -> Self {
        Self {
            deletion_policy: Default::default(),
            resources: Self::default_resources(),
        }
    }
}

impl ModelStorageObjectOwnedSpec {
    fn default_resources() -> ResourceRequirements {
        ResourceRequirements {
            requests: Some({
                let mut map = BTreeMap::default();
                map.insert("storage".into(), Quantity("64Mi".into()));
                map
            }),
            ..Default::default()
        }
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
pub enum ModelStorageObjectDeletionPolicy {
    Retain,
}

impl Default for ModelStorageObjectDeletionPolicy {
    fn default() -> Self {
        Self::Retain
    }
}
