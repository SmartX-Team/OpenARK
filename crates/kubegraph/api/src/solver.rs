use anyhow::Result;

use crate::problem::ProblemSpec;

pub trait LocalSolver<G> {
    type Output;

    fn step(&self, graph: G, problem: ProblemSpec) -> Result<Self::Output>;
}
