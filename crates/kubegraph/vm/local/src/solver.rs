use anyhow::Result;
use ark_core::signal::FunctionSignal;
use async_trait::async_trait;
use clap::{Parser, ValueEnum};
use kubegraph_api::{
    component::NetworkComponent,
    frame::LazyFrame,
    graph::{GraphData, GraphMetadataPinned},
    problem::ProblemSpec,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use tracing::{instrument, Level};

#[derive(
    Copy,
    Clone,
    Debug,
    Default,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    JsonSchema,
    Parser,
)]
#[clap(rename_all = "kebab-case")]
#[serde(rename_all = "camelCase")]
pub struct NetworkSolverArgs {
    #[arg(
        long,
        env = "KUBEGRAPH_SOLVER",
        value_enum,
        value_name = "IMPL",
        default_value_t = NetworkSolverType::default(),
    )]
    #[serde(default)]
    pub solver: NetworkSolverType,

    #[cfg(feature = "solver-ortools")]
    #[command(flatten)]
    #[serde(default)]
    pub ortools: <::kubegraph_solver_ortools::NetworkSolver as NetworkComponent>::Args,
}

#[derive(
    Copy,
    Clone,
    Debug,
    Default,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    JsonSchema,
    ValueEnum,
)]
#[clap(rename_all = "kebab-case")]
#[serde(rename_all = "kebab-case")]
pub enum NetworkSolverType {
    #[cfg_attr(not(feature = "solver-ortools"), default)]
    Disabled,
    #[cfg(feature = "solver-ortools")]
    #[default]
    Ortools,
}

#[derive(Clone)]
pub enum NetworkSolver {
    Disabled,
    #[cfg(feature = "solver-ortools")]
    Ortools(::kubegraph_solver_ortools::NetworkSolver),
}

#[async_trait]
impl NetworkComponent for NetworkSolver {
    type Args = NetworkSolverArgs;

    #[instrument(level = Level::INFO)]
    async fn try_new(
        args: <Self as NetworkComponent>::Args,
        signal: &FunctionSignal,
    ) -> Result<Self> {
        let NetworkSolverArgs {
            solver,
            #[cfg(feature = "solver-ortools")]
            ortools,
        } = args;

        match solver {
            NetworkSolverType::Disabled => {
                let _ = signal;
                Ok(Self::Disabled)
            }
            #[cfg(feature = "solver-ortools")]
            NetworkSolverType::Ortools => Ok(Self::Ortools(
                ::kubegraph_solver_ortools::NetworkSolver::try_new(ortools, signal).await?,
            )),
        }
    }
}

#[async_trait]
impl ::kubegraph_api::solver::NetworkSolver<GraphData<LazyFrame>> for NetworkSolver {
    type Output = GraphData<LazyFrame>;

    #[instrument(level = Level::INFO, skip(self, graph, problem))]
    async fn solve(
        &self,
        graph: GraphData<LazyFrame>,
        problem: &ProblemSpec<GraphMetadataPinned>,
    ) -> Result<Self::Output> {
        match self {
            Self::Disabled => {
                let _ = problem;
                Ok(graph)
            }
            #[cfg(feature = "solver-ortools")]
            Self::Ortools(runtime) => runtime.solve(graph, problem).await,
        }
    }
}
