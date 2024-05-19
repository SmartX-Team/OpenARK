pub mod r#virtual;

use kube::CustomResource;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::graph::GraphMetadata;

#[derive(
    Clone,
    Debug,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    JsonSchema,
    CustomResource,
)]
#[kube(
    group = "kubegraph.ulagbulag.io",
    version = "v1alpha1",
    kind = "NetworkProblem",
    root = "NetworkProblemCrd",
    shortname = "np",
    namespaced,
    printcolumn = r#"{
        "name": "created-at",
        "type": "date",
        "description": "created time",
        "jsonPath": ".metadata.creationTimestamp"
    }"#,
    printcolumn = r#"{
        "name": "version",
        "type": "integer",
        "description": "problem version",
        "jsonPath": ".metadata.generation"
    }"#
)]
#[serde(rename_all = "camelCase")]
pub struct ProblemSpec {
    #[serde(default, flatten)]
    pub metadata: GraphMetadata,

    #[serde(default = "ProblemSpec::default_verbose")]
    pub verbose: bool,
}

impl Default for ProblemSpec {
    fn default() -> Self {
        Self {
            metadata: GraphMetadata::default(),
            verbose: Self::default_verbose(),
        }
    }
}

impl ProblemSpec {
    pub const MAX_CAPACITY: u64 = u64::MAX >> 32;

    const fn default_verbose() -> bool {
        false
    }
}
