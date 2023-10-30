use ark_core_k8s::data::Url;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

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
