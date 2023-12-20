use std::time::Duration;

use anyhow::Result;
use async_trait::async_trait;
use tokio::time::sleep;
use tracing::{error, info};

use crate::ctx::WorldContext;

#[derive(Clone)]
pub struct MetricSyncer {
    ctx: WorldContext,
}

impl MetricSyncer {
    pub fn new(ctx: WorldContext) -> Self {
        Self { ctx }
    }
}

#[async_trait]
impl crate::service::Service for MetricSyncer {
    async fn loop_forever(self) -> Result<()> {
        info!("creating service: metric syncer");

        const INTERVAL_SYNC: Duration = WorldContext::INTERVAL_FLUSH;
        const INTERVAL_COLLECT: Duration = Duration::from_secs(5 * 60);

        loop {
            match self.ctx.get_all_metrics_with_last(INTERVAL_COLLECT).await {
                Ok(metrics) => {
                    let mut world = self.ctx.data.write().await;
                    world.update_metrics(metrics).await
                }
                Err(error) => {
                    error!("{error}")
                }
            }
            sleep(INTERVAL_SYNC).await;
        }
    }
}
