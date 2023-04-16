use std::path::PathBuf;

use clap::Parser;
use ipis::core::anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use strum::{Display, EnumString};

#[derive(Parser)]
pub struct Args {
    #[command(flatten)]
    pub flags: PackageFlags,

    /// Specify container image name prefix
    #[arg(
        long,
        env = "ARK_CONTAINER_IMAGE_NAME_PREFIX",
        default_value = "quay.io/ulagbulag/openark-package-"
    )]
    pub container_image_name_prefix: String,

    /// Specify container runtime engine
    #[arg(long, env = "ARK_CONTAINER_RUNTIME")]
    pub container_runtime: Option<ContainerRuntimeKind>,

    /// Specify container template file
    #[arg(
        long,
        env = "ARK_CONTAINER_TEMPLATE_FILE",
        default_value = "./templates/Containerfile.j2"
    )]
    pub container_template_file: PathBuf,

    /// Specify repository home
    #[arg(long, env = "ARK_REPOSITORY_HOME", default_value = "./repos/")]
    pub repository_home: PathBuf,
}

#[derive(Clone, Debug, Parser)]
pub struct PackageFlags {
    #[arg(long, env = "ARK_FLAG_ADD_IF_NOT_EXISTS")]
    pub add_if_not_exists: bool,
}

#[derive(Copy, Clone, Debug, Display, EnumString, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[strum(serialize_all = "camelCase")]
pub enum ContainerRuntimeKind {
    Docker,
    Podman,
}

impl ContainerRuntimeKind {
    pub fn parse(kind: Option<Self>) -> Result<(Self, PathBuf)> {
        match kind {
            Some(kind) => ::which::which(kind.to_string())
                .map(|path| (kind, path))
                .map_err(Into::into),
            None => {
                for kind in &[Self::Docker, Self::Podman] {
                    match ::which::which(kind.to_string()) {
                        Ok(path) => return Ok((*kind, path)),
                        Err(_) => continue,
                    }
                }
                bail!("failed to find container runtimes; have you ever installed?")
            }
        }
    }
}
