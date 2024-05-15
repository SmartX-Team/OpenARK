use anyhow::Result;

use crate::problem::ProblemSpec;

pub trait LocalTwin<G> {
    type Output;

    fn execute(&self, graph: G, problem: &ProblemSpec) -> Result<Self::Output>;
}
