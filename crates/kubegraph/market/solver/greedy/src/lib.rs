use anyhow::Result;
use async_trait::async_trait;
use kubegraph_api::market::{price::PriceHistogram, product::ProductSpec, trade::TradeTemplate};
use tracing::{instrument, Level};

#[derive(Clone, Debug, Default)]
pub struct MarketSolver {}

#[async_trait]
impl ::kubegraph_market_solver_api::MarketSolver for MarketSolver {
    #[instrument(level = Level::INFO, skip(self, product, histogram))]
    async fn solve(
        &self,
        product: &ProductSpec,
        histogram: PriceHistogram,
    ) -> Result<Vec<TradeTemplate>> {
        todo!()
    }
}
