use std::path::PathBuf;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(
    Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "camelCase")]
pub struct NetworkConnectorLocalSpec {
    pub path: PathBuf,

    #[serde(default = "NetworkConnectorLocalSpec::default_key_edges")]
    pub key_edges: String,
    #[serde(default = "NetworkConnectorLocalSpec::default_key_nodes")]
    pub key_nodes: String,
}

impl NetworkConnectorLocalSpec {
    fn default_key_edges() -> String {
        "edges.csv".into()
    }

    fn default_key_nodes() -> String {
        "nodes.csv".into()
    }
}
