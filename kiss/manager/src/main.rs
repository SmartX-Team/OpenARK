#![deny(
    clippy::all,
    clippy::cargo,
    clippy::nursery,
    clippy::pedantic,
    clippy::restriction
)]

use std::time::Duration;

use ipis::{
    core::anyhow::Result,
    env::infer,
    log::{info, warn},
    tokio,
};
use octocrab::repos::RepoHandler;
use semver::Version;

const REPOSITORY_OWNER: &str = "ulagbulag-village";
const REPOSITORY_NAME: &str = "netai-cloud";

async fn sync_cluster(repo: &RepoHandler<'_>) -> Result<()> {
    // request the latest release info
    let release = repo.releases().get_latest().await?;

    // compare with the current release tag
    if !release.tag_name.starts_with("v") {
        warn!("Received unexpected version tag: {:?}", &release.tag_name);
        return Ok(());
    }
    let latest = Version::parse(&release.tag_name[1..]).unwrap();
    let current = Version::parse("0.0.1").unwrap();

    // if possible, update the cluster
    if latest > current {
        info!("Found the newer version: {current} -> {latest}");
        upgrade_cluster(latest).await
    } else if latest < current {
        warn!("Current version is ahead of official release: {latest} > {current}");
        Ok(())
    } else {
        info!("The current version is the latest one: {current}");
        Ok(())
    }
}

async fn upgrade_cluster(version: Version) -> Result<()> {
    todo!()
}

#[tokio::main]
async fn main() -> Result<()> {
    // get environment variables
    let repo_owner = infer("REPO_OWNER").unwrap_or_else(|_| REPOSITORY_OWNER.to_string());
    let repo_name = infer("REPO_NAME").unwrap_or_else(|_| REPOSITORY_NAME.to_string());

    // create a repository handler
    let instance = octocrab::instance();
    let handler = instance.repos(&repo_owner, &repo_name);

    // sync the cluster periodically
    loop {
        sync_cluster(&handler).await?;
        tokio::time::sleep(Duration::from_secs(5 * 60)).await;
    }
}
