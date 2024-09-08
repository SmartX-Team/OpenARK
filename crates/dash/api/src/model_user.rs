use kube::CustomResource;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema, CustomResource)]
#[kube(
    group = "dash.ulagbulag.io",
    version = "v1alpha1",
    kind = "ModelUser",
    root = "ModelUserCrd",
    shortname = "mu",
    namespaced,
    printcolumn = r#"{
        "name": "created-at",
        "type": "date",
        "description": "created time",
        "jsonPath": ".metadata.creationTimestamp"
    }"#,
    printcolumn = r#"{
        "name": "version",
        "type": "integer",
        "description": "model user version",
        "jsonPath": ".metadata.generation"
    }"#
)]
#[serde(rename_all = "camelCase")]
pub struct ModelUserSpec {
    #[serde(default)]
    pub access_token: Option<ModelUserAccessTokenSpec>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum ModelUserAccessTokenSpec {
    SecretRef(ModelUserAccessTokenSecretRefSpec),
}

impl Default for ModelUserAccessTokenSpec {
    fn default() -> Self {
        Self::SecretRef(ModelUserAccessTokenSecretRefSpec::default())
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ModelUserAccessTokenSecretRefSpec {
    #[serde(default = "ModelUserAccessTokenSecretRefSpec::default_map_access_key")]
    pub map_access_key: String,
    #[serde(default = "ModelUserAccessTokenSecretRefSpec::default_map_secret_key")]
    pub map_secret_key: String,

    #[serde(default = "ModelUserAccessTokenSecretRefSpec::default_name")]
    pub name: String,
}

impl Default for ModelUserAccessTokenSecretRefSpec {
    fn default() -> Self {
        Self {
            map_access_key: Self::default_map_access_key(),
            map_secret_key: Self::default_map_secret_key(),
            name: Self::default_name(),
        }
    }
}

impl ModelUserAccessTokenSecretRefSpec {
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
