use ark_core_k8s::data::Url;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ModelStorageKubernetesSpec {}

impl ModelStorageKubernetesSpec {
    #[inline]
    pub(super) fn endpoint(&self) -> Option<Url> {
        None
    }
}
