use kube::CustomResource;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

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
    shortname = "prob",
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
    pub metadata: ProblemMetadata,
    #[serde(default = "ProblemSpec::default_capacity")]
    pub capacity: String,
    #[serde(default = "ProblemSpec::default_supply")]
    pub supply: String,
    #[serde(default = "ProblemSpec::default_unit_cost")]
    pub unit_cost: String,
}

impl Default for ProblemSpec {
    fn default() -> Self {
        Self {
            metadata: ProblemMetadata::default(),
            capacity: Self::default_capacity(),
            supply: Self::default_supply(),
            unit_cost: Self::default_unit_cost(),
        }
    }
}

impl ProblemSpec {
    pub fn default_capacity() -> String {
        "capacity".into()
    }

    pub fn default_supply() -> String {
        "supply".into()
    }

    pub fn default_unit_cost() -> String {
        "unit_cost".into()
    }
}

#[derive(
    Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "camelCase")]
pub struct ProblemMetadata {
    #[serde(default = "ProblemMetadata::default_flow")]
    pub flow: String,
    #[serde(default = "ProblemMetadata::default_function")]
    pub function: String,
    #[serde(default = "ProblemMetadata::default_name")]
    pub name: String,
    #[serde(default = "ProblemMetadata::default_sink")]
    pub sink: String,
    #[serde(default = "ProblemMetadata::default_src")]
    pub src: String,

    #[serde(default = "ProblemMetadata::default_verbose")]
    pub verbose: bool,
}

impl Default for ProblemMetadata {
    fn default() -> Self {
        Self {
            flow: Self::default_flow(),
            function: Self::default_function(),
            name: Self::default_name(),
            sink: Self::default_sink(),
            src: Self::default_src(),
            verbose: Self::default_verbose(),
        }
    }
}

impl ProblemMetadata {
    pub const MAX_CAPACITY: u64 = u64::MAX >> 32;

    pub fn default_flow() -> String {
        "flow".into()
    }

    pub fn default_function() -> String {
        "function".into()
    }

    pub fn default_name() -> String {
        "name".into()
    }

    pub fn default_link() -> String {
        "link".into()
    }

    pub fn default_sink() -> String {
        "sink".into()
    }

    pub fn default_src() -> String {
        "src".into()
    }

    pub const fn default_verbose() -> bool {
        false
    }
}
