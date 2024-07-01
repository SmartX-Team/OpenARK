use anyhow::Result;
use async_trait::async_trait;
use kubegraph_api::market::{price::PriceHistogram, product::ProductSpec, trade::TradeTemplate};

#[async_trait]
pub trait MarketSolver {
    async fn solve(
        &self,
        product: &ProductSpec,
        histogram: PriceHistogram,
    ) -> Result<Vec<TradeTemplate>>;
}
