use anyhow::Result;
use async_trait::async_trait;

use crate::ctx::OptimizerContext;

#[async_trait]
pub trait Plan
where
    Self: 'static + Send + Sync,
{
    async fn exec(&self, ctx: &OptimizerContext) -> Result<()>;
}
