use std::sync::Arc;

use ipis::{
    core::anyhow::{bail, Result},
    env::infer,
    log::warn,
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
    const REPOSITORY_NAME: &'static str = "noah-cloud";
    const REPOSITORY_OWNER: &'static str = "ulagbulag-village";

    const MAX_RETRY: usize = 5;

    pub async fn get_version(&self) -> Result<Version> {
        // request the latest release info
        let release = 'load_release: loop {
            for retry in 0..Self::MAX_RETRY {
                match self
                    .instance
                    .repos(&self.repo_owner, &self.repo_name)
                    .releases()
                    .get_latest()
                    .await
                {
                    Ok(release) => break 'load_release release,
                    Err(e) => {
                        if retry + 1 == Self::MAX_RETRY {
                            warn!("Maximum retry failed");
                            return Err(e.into());
                        } else {
                            continue;
                        }
                    }
                }
            }
        };

        // compare with the current release tag
        if !release.tag_name.starts_with('v') {
            bail!("Received unexpected version tag: {:?}", &release.tag_name);
        }
        Version::parse(&release.tag_name[1..]).map_err(Into::into)
    }
}
