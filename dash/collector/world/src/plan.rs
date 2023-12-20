use anyhow::Result;
use async_trait::async_trait;
use tokio::{sync::mpsc, task::yield_now};
use tracing::{error, info};

use crate::ctx::WorldContext;

pub struct PlanRunner {
    ctx: WorldContext,
    rx: mpsc::Receiver<Box<dyn Plan>>,
}

impl PlanRunner {
    pub fn new(ctx: WorldContext, rx: mpsc::Receiver<Box<dyn Plan>>) -> Self {
        Self { ctx, rx }
    }
}

#[async_trait]
impl crate::service::Service for PlanRunner {
    async fn loop_forever(mut self) -> Result<()> {
        info!("creating service: plan runner");

        while let Some(plan) = self.rx.recv().await {
            // yield per every loop
            yield_now().await;

            match plan.exec(&self.ctx).await {
                Ok(()) => continue,
                Err(error) => {
                    error!("failed to spawn plan: {error}");
                }
            }
        }
        Ok(())
    }
}

#[async_trait]
pub trait Plan
where
    Self: 'static + Send + Sync,
{
    async fn exec(&self, ctx: &WorldContext) -> Result<()>;
}
