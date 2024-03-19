#[cfg(feature = "prometheus")]
mod prometheus;

use anyhow::Result;
use kubegraph_api::{connector::Connector, provider::NetworkGraphProvider};
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
