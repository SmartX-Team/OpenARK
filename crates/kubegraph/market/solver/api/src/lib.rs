use anyhow::Result;
use async_trait::async_trait;
use kubegraph_api::market::{
    price::PriceHistogram, product::ProductSpec, transaction::TransactionTemplate,
};

#[async_trait]
pub trait MarketSolver {
    async fn solve(
        &self,
        product: &ProductSpec,
        histogram: PriceHistogram,
    ) -> Result<Vec<TransactionTemplate>>;
}
