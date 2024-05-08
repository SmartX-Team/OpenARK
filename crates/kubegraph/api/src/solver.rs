use anyhow::Result;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

pub trait LocalSolver<G, P> {
    type Output;

    fn step(&self, graph: G, problem: Problem<P>) -> Result<Self::Output>;
}

#[derive(
    Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
pub struct Problem<T> {
    #[serde(default, flatten)]
    pub metadata: ProblemMetadata,
    pub capacity: T,
    pub cost: T,
    pub supply: T,
}

#[derive(
    Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
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
