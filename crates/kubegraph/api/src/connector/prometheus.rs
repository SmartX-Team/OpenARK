use ark_core_k8s::data::Url;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::query::NetworkQuery;

#[derive(
    Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "camelCase")]
pub struct NetworkConnectorPrometheusSpec {
    pub template: NetworkQuery,
    pub url: Url,
}

impl NetworkConnectorPrometheusSpec {
    pub const fn name(&self) -> &'static str {
        self.template.name()
    }
}
