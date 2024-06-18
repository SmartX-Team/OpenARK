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

impl ModelStorageDatabaseSpec {
    #[inline]
    pub(super) fn endpoint(&self) -> Option<Url> {
        match self {
            Self::Borrowed(spec) => spec.endpoint(),
            Self::Owned(spec) => spec.endpoint(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ModelStorageDatabaseBorrowedSpec {
    pub url: Url,
}

impl ModelStorageDatabaseBorrowedSpec {
    #[inline]
    fn endpoint(&self) -> Option<Url> {
        Some(self.url.clone())
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ModelStorageDatabaseOwnedSpec {}

impl ModelStorageDatabaseOwnedSpec {
    #[inline]
    fn endpoint(&self) -> Option<Url> {
        None
    }
}
