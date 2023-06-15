use k8s_openapi::api::core::v1::ResourceRequirements;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use strum::{Display, EnumString};
use vine_api::user_auth::Url;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum ModelStorageObjectSpec {
    Borrowed(ModelStorageObjectBorrowedSpec),
    Owned(#[serde(default)] ModelStorageObjectOwnedSpec),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ModelStorageObjectBorrowedSpec {
    pub endpoint: Url,
    #[serde(default)]
    pub read_only: bool,
    pub secret_ref: ModelStorageObjectBorrowedSecretRefSpec,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ModelStorageObjectBorrowedSecretRefSpec {
    #[serde(default = "ModelStorageObjectBorrowedSecretRefSpec::default_map_access_key")]
    pub map_access_key: String,
    #[serde(default = "ModelStorageObjectBorrowedSecretRefSpec::default_map_secret_key")]
    pub map_secret_key: String,

    pub name: String,
}

impl ModelStorageObjectBorrowedSecretRefSpec {
    fn default_map_access_key() -> String {
        "accessKeyID".into()
    }

    fn default_map_secret_key() -> String {
        "secretAccessKey".into()
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ModelStorageObjectOwnedSpec {
    #[serde(default)]
    pub deletion_policy: ModelStorageObjectDeletionPolicy,

    #[serde(default)]
    pub resources: ResourceRequirements,
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
