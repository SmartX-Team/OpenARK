mod current;
mod latest;

use std::{cmp::Ordering, time::Duration};

use anyhow::Result;
use tracing::{info, warn};

async fn sync_cluster(
    current_handler: &self::current::Handler,
    latest_handler: &self::latest::Handler,
) -> Result<()> {
    // request the release info
    let latest = latest_handler.get_version().await?;
    let current = match current_handler.get_version(&latest).await? {
        Some(version) => version,
        None => return Ok(()),
    };

    // if possible, update the cluster
    match latest.cmp(&current) {
        Ordering::Greater => {
            info!("Found the newer version: {current} -> {latest}");
            current_handler.upgrade(&current, &latest).await
        }
        Ordering::Less => {
            warn!("Current version is ahead of official release: {latest} > {current}");
            Ok(())
        }
        Ordering::Equal => {
            info!("The current version is the latest one: {current}");
            Ok(())
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    // initialize tracer
    ::ark_core::tracer::init_once();

    // create the handlers
    let current = self::current::Handler::try_default().await?;
    let latest = self::latest::Handler::default();

    // sync the cluster periodically
    loop {
        sync_cluster(&current, &latest).await?;
        ::tokio::time::sleep(Duration::from_secs(5 * 60)).await;
    }
}
