use anyhow::Result;
use ark_core::signal::FunctionSignal;
use async_trait::async_trait;
use clap::{Parser, ValueEnum};
use kubegraph_api::{
    component::NetworkComponent,
    market::{price::PriceHistogram, product::ProductSpec, trade::TradeTemplate},
};
use serde::{Deserialize, Serialize};
use tracing::{instrument, Level};

#[derive(Clone)]
pub enum MarketSolver {
    #[cfg(feature = "market-solver-greedy")]
    Greedy(::kubegraph_market_solver_greedy::MarketSolver),
}

#[async_trait]
impl NetworkComponent for MarketSolver {
    type Args = MarketSolverArgs;

    async fn try_new(
        args: <Self as NetworkComponent>::Args,
        signal: &FunctionSignal,
    ) -> Result<Self> {
        let MarketSolverArgs { solver, greedy } = args;

        match solver {
            MarketSolverType::Greedy => {
                ::kubegraph_market_solver_greedy::MarketSolver::try_new(greedy, signal)
                    .await
                    .map(Self::Greedy)
            }
        }
    }
}

#[async_trait]
impl ::kubegraph_market_solver_api::MarketSolver for MarketSolver {
    #[instrument(level = Level::INFO, skip(self, product, histogram))]
    async fn solve(
        &self,
        product: &ProductSpec,
        histogram: PriceHistogram,
    ) -> Result<Vec<TradeTemplate>> {
        match self {
            #[cfg(feature = "market-solver-greedy")]
            Self::Greedy(solver) => solver.solve(product, histogram).await,
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, Parser)]
#[clap(rename_all = "camelCase")]
#[serde(rename_all = "kebab-case")]
pub struct MarketSolverArgs {
    #[arg(
        long,
        env = "KUBEGRAPH_SOLVER",
        value_enum,
        value_name = "IMPL",
        default_value_t = MarketSolverType::default(),
    )]
    #[serde(default)]
    pub solver: MarketSolverType,

    #[cfg(feature = "market-solver-greedy")]
    #[command(flatten)]
    #[serde(default)]
    pub greedy: <::kubegraph_market_solver_greedy::MarketSolver as NetworkComponent>::Args,
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
    ValueEnum,
)]
#[clap(rename_all = "kebab-case")]
#[serde(rename_all = "kebab-case")]
pub enum MarketSolverType {
    #[cfg(feature = "market-solver-greedy")]
    #[default]
    Greedy,
}
