use std::{
    borrow::Cow,
    fmt,
    path::{Path, PathBuf},
};

use anyhow::{anyhow, bail, Result};
use ark_core_k8s::data::Name;
use clap::{Parser, Subcommand};
use dash_pipe_api::storage::StorageS3Args;
use itertools::Itertools;
use tokio::fs;

#[derive(Subcommand)]
pub(crate) enum Command {
    Mount(MountArgs),
    Unmount(UnmountArgs),
}

impl Command {
    pub(crate) async fn run(self) -> Result<()> {
        match self {
            Self::Mount(command) => command.run().await,
            Self::Unmount(command) => command.run().await,
        }
    }
}

#[derive(Parser)]
pub(crate) struct MountArgs {
    #[command(flatten)]
    s3: StorageS3Args,

    #[arg(value_name = "BUCKET_NAME")]
    source: Name,

    #[arg(value_name = "PATH")]
    target: PathBuf,
}

impl MountArgs {
    pub(crate) async fn run(self) -> Result<()> {
        if is_mounted(&self.target)? {
            return Ok(());
        }

        let passwd = S3PasswdFile {
            access_key: &self.s3.access_key,
            secret_key: &self.s3.secret_key,
            source: &self.source,
        };
        passwd.create().await?;

        self.exec().await
    }

    async fn exec(self) -> Result<()> {
        struct MountOption<'a> {
            key: Cow<'a, str>,
            value: Option<Cow<'a, str>>,
        }

        impl<'a> fmt::Display for MountOption<'a> {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                let Self { key, value } = self;
                match value {
                    Some(value) => write!(f, "{key}={value}"),
                    None => key.fmt(f),
                }
            }
        }

        let options = [
            MountOption {
                key: "passwd_file".into(),
                value: Some("./ark/cli/.s3-password".into()),
            },
            MountOption {
                key: "use_path_request_style".into(),
                value: None,
            },
            MountOption {
                key: "url".into(),
                value: Some(self.s3.s3_endpoint.to_string().into()),
            },
            MountOption {
                key: "endpoint".into(),
                value: Some(self.s3.region.into()),
            },
        ]
        .iter()
        .join(",");

        if ::tokio::process::Command::new("s3fs")
            .arg(self.source.as_str())
            .arg(self.target)
            .arg("-o")
            .arg(options)
            .spawn()
            .map_err(|error| anyhow!("failed to execute mount program: {error}"))?
            .wait()
            .await
            .map_err(|error| anyhow!("failed to wait mount process: {error}"))?
            .success()
        {
            Ok(())
        } else {
            bail!("failed to mount s3 bucket")
        }
    }
}

#[derive(Parser)]
pub(crate) struct UnmountArgs {
    #[arg(value_name = "PATH")]
    target: PathBuf,
}

impl UnmountArgs {
    pub(crate) async fn run(self) -> Result<()> {
        if !is_mounted(&self.target)? {
            return Ok(());
        }

        self.exec().await
    }

    async fn exec(self) -> Result<()> {
        if ::tokio::process::Command::new("umount")
            .arg(self.target)
            .spawn()
            .map_err(|error| anyhow!("failed to execute umount program: {error}"))?
            .wait()
            .await
            .map_err(|error| anyhow!("failed to wait umount process: {error}"))?
            .success()
        {
            Ok(())
        } else {
            bail!("failed to unmount s3 bucket")
        }
    }
}

struct S3PasswdFile<'a> {
    access_key: &'a str,
    secret_key: &'a str,
    source: &'a Name,
}

impl<'a> S3PasswdFile<'a> {
    async fn create(&self) -> Result<()> {
        let Self {
            access_key,
            secret_key,
            source,
        } = self;

        let dir = {
            let mut path = ::dirs::cache_dir()
                .ok_or_else(|| anyhow!("failed to infer user cache directory"))?;
            path.push("openark");
            path.push("storage");
            path
        };
        fs::create_dir_all(&dir)
            .await
            .map_err(|error| anyhow!("failed to create passwd directory: {error}"))?;

        let path = {
            let mut path = dir.clone();
            path.push(source.as_str());
            path
        };

        fs::write(path, format!("{access_key}:{secret_key}"))
            .await
            .map_err(|error| anyhow!("failed to create passwd file: {error}"))
    }
}

fn is_mounted(target: &Path) -> Result<bool> {
    ::procfs::mounts()
        .map(|infos| {
            infos
                .into_iter()
                .filter_map(|info| PathBuf::try_from(info.fs_file).ok())
                .any(|mountpoint| mountpoint == target)
        })
        .map_err(|error| anyhow!("failed to load mount information: {error}"))
}
