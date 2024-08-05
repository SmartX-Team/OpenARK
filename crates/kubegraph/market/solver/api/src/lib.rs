use anyhow::Result;
use async_trait::async_trait;
use kubegraph_api::market::{
    price::PriceHistogram, product::ProductSpec, transaction::TransactionTemplate, BaseModel,
};

#[async_trait]
pub trait MarketSolver {
    async fn solve(
        &self,
        prod_id: <ProductSpec as BaseModel>::Id,
        product: &ProductSpec,
        histogram: PriceHistogram,
    ) -> Result<Vec<TransactionTemplate>>;
}
