use std::time::Duration;

use anyhow::Result;
use ark_core::signal::FunctionSignal;
use async_trait::async_trait;
use clap::Parser;
use futures::TryStreamExt;
use kubegraph_api::{
    component::NetworkComponent, market::price::PriceHistogram, vm::NetworkFallbackPolicy,
};
use kubegraph_market_client::{MarketClient, MarketClientArgs};
use kubegraph_market_solver_api::MarketSolver as _;
use serde::{Deserialize, Serialize};
use tokio::time::{sleep, Instant};
use tracing::{error, info, warn};

#[derive(Clone)]
pub struct MarketAgent {
    client: MarketClient,
    fallback_policy: NetworkFallbackPolicy,
    signal: FunctionSignal,
    solver: crate::solver::MarketSolver,
}

#[async_trait]
impl NetworkComponent for MarketAgent {
    type Args = MarketAgentArgs;

    async fn try_new(
        args: <Self as NetworkComponent>::Args,
        signal: &FunctionSignal,
    ) -> Result<Self> {
        let MarketAgentArgs {
            client,
            fallback_policy,
            solver,
        } = args;

        Ok(Self {
            client: MarketClient::try_new(client, signal).await?,
            fallback_policy,
            signal: signal.clone(),
            solver: crate::solver::MarketSolver::try_new(solver, signal).await?,
        })
    }
}

impl MarketAgent {
    pub async fn loop_forever(self) {
        loop {
            if let Err(error) = self.try_loop_forever().await {
                error!("failed to operate kubegraph solver: {error}");

                match self.fallback_policy {
                    NetworkFallbackPolicy::Interval { interval } => {
                        warn!("restarting kubegraph solver in {interval:?}...");
                        sleep(interval).await;
                        info!("Restarted kubegraph solver");
                    }
                    NetworkFallbackPolicy::Never => {
                        self.signal.terminate_on_panic();
                        break;
                    }
                }
            }
        }
    }

    async fn try_loop_forever(&self) -> Result<()> {
        while !self.signal.is_terminating() {
            let instant = Instant::now();
            let product_ids: Vec<_> = self.client.list_product_ids().try_collect().await?;

            for prod_id in product_ids {
                let product = match self.client.get_product(prod_id).await? {
                    Some(product) => product,
                    None => continue,
                };

                let histogram: PriceHistogram = self
                    .client
                    .list_price_histogram(prod_id)
                    .try_collect()
                    .await?;

                let templates = self.solver.solve(&product, histogram).await?;

                for template in templates {
                    self.client.trade(prod_id, &template).await?
                }
            }

            let elapsed = instant.elapsed();
            let interval = Duration::from_secs(1);
            if elapsed < interval {
                let remaining = interval - elapsed;
                sleep(remaining).await
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, Parser)]
#[clap(rename_all = "kebab-case")]
#[serde(rename_all = "camelCase")]
pub struct MarketAgentArgs {
    #[command(flatten)]
    pub client: MarketClientArgs,

    #[arg(
        long,
        env = "KUBEGRAPH_MARKET_SOLVER_FALLBACK_POLICY",
        value_name = "POLICY",
        default_value_t = NetworkFallbackPolicy::default(),
    )]
    #[serde(default)]
    pub fallback_policy: NetworkFallbackPolicy,

    #[command(flatten)]
    pub solver: crate::solver::MarketSolverArgs,
}
