use ark_core_k8s::data::Url;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::query::{NetworkQuery, NetworkQueryMetadata};

#[derive(
    Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "camelCase")]
pub struct NetworkConnectorPrometheusSpec<M = NetworkQueryMetadata> {
    pub template: NetworkQuery<M>,
    pub url: Url,
}

impl NetworkConnectorPrometheusSpec {
    pub const fn name(&self) -> &'static str {
        self.template.name()
    }
}
