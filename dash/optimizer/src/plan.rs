use anyhow::Result;
use async_trait::async_trait;
use tokio::{sync::mpsc, task::yield_now};
use tracing::{error, instrument, Level};

use crate::ctx::OptimizerContext;

/// Task
impl OptimizerContext {
    #[instrument(level = Level::INFO, skip_all)]
    pub(super) async fn loop_forever_plan(self, mut rx: mpsc::Receiver<Box<dyn Plan>>) {
        while let Some(plan) = rx.recv().await {
            // yield per every loop
            yield_now().await;

            match plan.exec(&self).await {
                Ok(()) => continue,
                Err(error) => {
                    error!("failed to spawn plan: {error}");
                }
            }
        }
    }
}

#[async_trait]
pub trait Plan
where
    Self: 'static + Send + Sync,
{
    async fn exec(&self, ctx: &OptimizerContext) -> Result<()>;
}
