use std::path::PathBuf;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::graph::NetworkGraphMetadata;

#[derive(
    Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "camelCase")]
pub struct NetworkConnectorSimulationSpec {
    #[serde(default)]
    pub metadata: NetworkGraphMetadata,
    pub path: PathBuf,

    #[serde(default = "NetworkConnectorSimulationSpec::default_key_edges")]
    pub key_edges: String,
    #[serde(default = "NetworkConnectorSimulationSpec::default_key_nodes")]
    pub key_nodes: String,
}

impl NetworkConnectorSimulationSpec {
    fn default_key_edges() -> String {
        "edges.csv".into()
    }

    fn default_key_nodes() -> String {
        "nodes.csv".into()
    }
}
