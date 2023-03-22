use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use vine_api::user_auth::Url;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum ModelStorageDatabaseSpec {
    Native(#[serde(default)] ModelStorageDatabaseNativeSpec),
    External(ModelStorageDatabaseExternalSpec),
}

impl Default for ModelStorageDatabaseSpec {
    fn default() -> Self {
        Self::Native(Default::default())
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ModelStorageDatabaseNativeSpec {}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ModelStorageDatabaseExternalSpec {
    pub url: Url,
}
