use std::sync::Arc;

use ipis::{
    core::anyhow::{bail, Result},
    env::infer,
};
use octocrab::Octocrab;
use semver::Version;

pub struct Handler {
    instance: Arc<Octocrab>,
    repo_name: String,
    repo_owner: String,
}

impl Default for Handler {
    fn default() -> Self {
        Self {
            instance: Default::default(),
            repo_name: infer("REPO_NAME").unwrap_or_else(|_| Self::REPOSITORY_NAME.into()),
            repo_owner: infer("REPO_OWNER").unwrap_or_else(|_| Self::REPOSITORY_OWNER.into()),
        }
    }
}

impl Handler {
    const REPOSITORY_NAME: &str = "netai-cloud";
    const REPOSITORY_OWNER: &str = "ulagbulag-village";

    pub async fn get_version(&self) -> Result<Version> {
        // request the latest release info
        let release = self
            .instance
            .repos(&self.repo_owner, &self.repo_name)
            .releases()
            .get_latest()
            .await?;

        // compare with the current release tag
        if !release.tag_name.starts_with("v") {
            bail!("Received unexpected version tag: {:?}", &release.tag_name);
        }
        Version::parse(&release.tag_name[1..]).map_err(Into::into)
    }
}
