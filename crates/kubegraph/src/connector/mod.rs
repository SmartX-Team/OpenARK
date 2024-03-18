#[cfg(feature = "prometheus")]
mod prometheus;

use std::time::{Duration, Instant};

use anyhow::Result;
use async_trait::async_trait;
use kubegraph_api::provider::NetworkGraphProvider;
use tokio::time::sleep;
use tracing::error;

pub async fn loop_forever(graph: impl NetworkGraphProvider) {
    if let Err(error) = try_loop_forever(graph).await {
        error!("failed to run connect job: {error}")
    }
}

#[cfg(feature = "prometheus")]
async fn try_loop_forever(graph: impl NetworkGraphProvider) -> Result<()> {
    self::prometheus::Connector::default()
        .loop_forever(graph)
        .await;
    Ok(())
}

#[async_trait]
trait Connector {
    fn name(&self) -> &str;

    fn interval(&self) -> Duration {
        Duration::from_secs(15)
    }

    async fn loop_forever(mut self, graph: impl NetworkGraphProvider)
    where
        Self: Sized,
    {
        let interval = <Self as Connector>::interval(&self);

        loop {
            let instant = Instant::now();
            if let Err(error) = self.pull(&graph).await {
                let name = <Self as Connector>::name(&self);
                error!("failed to connect to dataset from {name:?}: {error}");
            }

            let elapsed = instant.elapsed();
            if elapsed < interval {
                sleep(interval - elapsed).await;
            }
        }
    }

    async fn pull(&mut self, graph: &impl NetworkGraphProvider) -> Result<()>;
}
