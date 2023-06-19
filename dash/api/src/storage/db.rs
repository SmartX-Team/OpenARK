use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use vine_api::user_auth::Url;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum ModelStorageDatabaseSpec {
    Borrowed(ModelStorageDatabaseBorrowedSpec),
    Owned(#[serde(default)] ModelStorageDatabaseOwnedSpec),
}

impl Default for ModelStorageDatabaseSpec {
    fn default() -> Self {
        Self::Owned(Default::default())
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ModelStorageDatabaseBorrowedSpec {
    pub url: Url,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ModelStorageDatabaseOwnedSpec {}
