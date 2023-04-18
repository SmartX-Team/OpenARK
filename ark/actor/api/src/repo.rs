use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

use ipis::{
    core::anyhow::{bail, Error, Result},
    env,
    tokio::fs,
};

use crate::{args::ActorArgs, package::Package};

pub struct RepositoryManager {
    repos: BTreeMap<String, Repository>,
}

impl RepositoryManager {
    pub async fn try_default() -> Result<Self> {
        let home: PathBuf =
            env::infer::<_, PathBuf>(ActorArgs::ARK_REPOSITORY_HOME_KEY).or_else(|_| {
                ActorArgs::ARK_REPOSITORY_HOME_VALUE
                    .try_into()
                    .map_err(Error::from)
            })?;
        Self::try_from_local(&home).await
    }

    pub async fn try_from_local(home: &Path) -> Result<Self> {
        Ok(Self {
            repos: {
                let mut buf = BTreeMap::default();

                let mut repos = fs::read_dir(home).await?;
                while let Some(repo) = repos.next_entry().await? {
                    let name = match repo.file_name().to_str() {
                        Some(name) => name.to_string(),
                        None => continue,
                    };
                    buf.insert(name, Repository::from_local(&repo.path()));
                }

                buf
            },
        })
    }

    pub async fn get(&self, name: &str) -> Result<Package> {
        for package in self.repos.values().map(|repo| repo.get(name)) {
            match package.await? {
                Some(package) => {
                    return Ok(package);
                }
                _ => continue,
            }
        }
        bail!("failed to find a package: {name:?}")
    }
}

enum Repository {
    Local { home: PathBuf },
}

impl Repository {
    const PACKAGE_FILE: &'static str = "package.yaml";

    fn from_local(home: &Path) -> Self {
        Self::Local {
            home: home.to_path_buf(),
        }
    }

    async fn get(&self, name: &str) -> Result<Option<Package>> {
        match self {
            Self::Local { home } => {
                let home = {
                    let mut path = home.clone();
                    path.push(name);
                    path
                };

                let read_to_string = |name| {
                    fs::read_to_string({
                        let mut path = home.clone();
                        path.push(name);
                        path
                    })
                };

                if fs::try_exists(&home).await? {
                    Ok(Some(Package {
                        name: name.to_string(),
                        resource: read_to_string(Self::PACKAGE_FILE)
                            .await
                            .map_err(Error::from)
                            .and_then(|text| ::serde_yaml::from_str(&text).map_err(Into::into))?,
                    }))
                } else {
                    Ok(None)
                }
            }
        }
    }
}
