use anyhow::Result;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

pub trait LocalSolver<G, P> {
    type Output;

    fn step(&self, graph: G, problem: Problem<P>) -> Result<Self::Output> {
        let Problem {
            metadata,
            capacity,
            constraint,
        } = problem;

        match constraint {
            Some(constraint) => {
                let problem = MinCostProblem {
                    metadata,
                    capacity,
                    constraint,
                };
                self.step_min_cost(graph, problem)
            }
            None => {
                let problem = MaxFlowProblem { metadata, capacity };
                self.step_max_flow(graph, problem)
            }
        }
    }

    fn step_max_flow(&self, graph: G, problem: MaxFlowProblem<P>) -> Result<Self::Output>;

    fn step_min_cost(&self, graph: G, problem: MinCostProblem<P>) -> Result<Self::Output>;
}

#[derive(
    Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
pub struct Problem<T> {
    #[serde(default, flatten)]
    pub metadata: ProblemMetadata,
    pub capacity: T,
    #[serde(default)]
    pub constraint: Option<ProblemConstrait<T>>,
}

#[derive(
    Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
pub struct MaxFlowProblem<T> {
    #[serde(default, flatten)]
    pub metadata: ProblemMetadata,
    pub capacity: T,
}

impl<T> From<MaxFlowProblem<T>> for Problem<T> {
    fn from(value: MaxFlowProblem<T>) -> Self {
        let MaxFlowProblem { metadata, capacity } = value;
        Self {
            metadata,
            capacity,
            constraint: None,
        }
    }
}

#[derive(
    Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
pub struct MinCostProblem<T> {
    #[serde(default, flatten)]
    pub metadata: ProblemMetadata,
    pub capacity: T,
    pub constraint: ProblemConstrait<T>,
}

impl<T> From<MinCostProblem<T>> for Problem<T> {
    fn from(value: MinCostProblem<T>) -> Self {
        let MinCostProblem {
            metadata,
            capacity,
            constraint,
        } = value;
        Self {
            metadata,
            capacity,
            constraint: Some(constraint),
        }
    }
}

#[derive(
    Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
pub struct ProblemConstrait<T> {
    pub cost: T,
    pub supply: T,
}

#[derive(
    Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
pub struct ProblemMetadata {
    #[serde(default = "ProblemMetadata::default_flow")]
    pub flow: String,
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
            name: Self::default_name(),
            sink: Self::default_sink(),
            src: Self::default_src(),
            verbose: Self::default_verbose(),
        }
    }
}

impl ProblemMetadata {
    pub fn default_flow() -> String {
        "flow".into()
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
