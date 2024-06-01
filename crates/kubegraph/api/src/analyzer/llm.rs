use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::graph::{GraphMetadataPinned, GraphMetadataRaw};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct VirtualProblemLLMAnalyzer {
    pub map: GraphMetadataPinned,
    pub original_metadata: GraphMetadataRaw,
}
