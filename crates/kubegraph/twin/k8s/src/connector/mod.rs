#[cfg(feature = "prometheus")]
mod prometheus;

use anyhow::Result;
use kubegraph_api::{connector::NetworkConnector, db::NetworkGraphDB};
use tracing::error;

pub async fn loop_forever(graph: impl NetworkGraphDB) {
    if let Err(error) = try_loop_forever(graph).await {
        error!("failed to run connect job: {error}")
    }
}

#[cfg(feature = "prometheus")]
async fn try_loop_forever(graph: impl NetworkGraphDB) -> Result<()> {
    self::prometheus::Connector::default()
        .loop_forever(graph)
        .await;
    Ok(())
}
