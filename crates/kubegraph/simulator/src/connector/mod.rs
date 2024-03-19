mod fake;

use anyhow::Result;
use kubegraph_api::{connector::Connector, provider::NetworkGraphProvider};
use tracing::error;

pub use self::fake::ConnectorArgs;

pub async fn loop_forever(graph: impl NetworkGraphProvider, args: ConnectorArgs) {
    if let Err(error) = try_loop_forever(graph, args).await {
        error!("failed to run connect job: {error}")
    }
}

async fn try_loop_forever(graph: impl NetworkGraphProvider, args: ConnectorArgs) -> Result<()> {
    self::fake::Connector::try_new(&args)?
        .loop_forever(graph)
        .await;
    Ok(())
}
