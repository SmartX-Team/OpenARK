mod prometheus;

use std::time::{Duration, Instant};

use anyhow::Result;
use async_trait::async_trait;
use dash_network_api::graph::ArcNetworkGraph;
use tokio::time::sleep;
use tracing::error;

pub async fn loop_forever(graph: ArcNetworkGraph) {
    let agent = self::prometheus::Pull::default();
    agent.loop_forever(graph).await
}

#[async_trait]
trait Pull {
    const NAME: &'static str;
    const INTERVAL: Duration;

    async fn loop_forever(&self, graph: ArcNetworkGraph) {
        let name = <Self as Pull>::NAME;
        let interval = <Self as Pull>::INTERVAL;

        loop {
            let instant = Instant::now();
            if let Err(error) = self.pull(&graph).await {
                error!("failed to pull dataset from {name:?}: {error}");
            }

            let elapsed = instant.elapsed();
            if elapsed < interval {
                sleep(interval - elapsed).await;
            }
        }
    }

    async fn pull(&self, graph: &ArcNetworkGraph) -> Result<()>;
}
