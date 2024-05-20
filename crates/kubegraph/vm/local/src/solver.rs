use anyhow::Result;
use async_trait::async_trait;
use kubegraph_api::{
    frame::LazyFrame,
    graph::{GraphData, GraphMetadataStandard},
    problem::ProblemSpec,
};

#[derive(Clone)]
pub struct NetworkSolver {
    #[cfg(feature = "solver-ortools")]
    ortools: ::kubegraph_solver_ortools::NetworkSolver,
}

impl NetworkSolver {
    pub async fn try_default() -> Result<Self> {
        Ok(Self {
            #[cfg(feature = "solver-ortools")]
            ortools: ::kubegraph_solver_ortools::NetworkSolver::default(),
        })
    }

    fn get_default_solver(
        &self,
    ) -> &impl ::kubegraph_api::solver::NetworkSolver<GraphData<LazyFrame>, Output = GraphData<LazyFrame>>
    {
        #[cfg(feature = "solver-ortools")]
        {
            &self.ortools
        }
    }
}

#[async_trait]
impl ::kubegraph_api::solver::NetworkSolver<GraphData<LazyFrame>> for NetworkSolver {
    type Output = GraphData<LazyFrame>;

    async fn solve(
        &self,
        graph: GraphData<LazyFrame>,
        problem: &ProblemSpec<GraphMetadataStandard>,
    ) -> Result<Self::Output> {
        self.get_default_solver().solve(graph, problem).await
    }
}
