use anyhow::Result;

use crate::solver::Problem;

pub trait LocalTwin<G, P> {
    type Output;

    fn execute(&self, graph: G, problem: &Problem<P>) -> Result<Self::Output>;
}
