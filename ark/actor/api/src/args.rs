use std::path::PathBuf;

use clap::Parser;
use ipis::core::anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use strum::{Display, EnumString};

#[derive(Parser)]
pub struct ActorArgs {
    #[arg(long, global = true, env = "ARK_FLAG_ADD_IF_NOT_EXISTS")]
    pub add_if_not_exists: bool,

    /// Specify container image name prefix
    #[arg(
        long,
        global = true,
        env = Self::ARK_CONTAINER_IMAGE_NAME_PREFIX_KEY,
        default_value = Self::ARK_CONTAINER_IMAGE_NAME_PREFIX_VALUE,
    )]
    pub container_image_name_prefix: String,

    /// Specify container runtime engine
    #[arg(long, global = true, env = Self::ARK_CONTAINER_RUNTIME_KEY)]
    pub container_runtime: Option<ContainerRuntimeKind>,

    /// Specify container template file
    #[arg(
        long,
        global = true,
        env = Self::ARK_CONTAINER_TEMPLATE_FILE_KEY,
        default_value = Self::ARK_CONTAINER_TEMPLATE_FILE_VALUE,
    )]
    pub container_template_file: PathBuf,

    /// Whether the spawned process depends on the main process
    #[arg(long, env = Self::ARK_PULL_KEY, default_value_t = ActorArgs::ARK_PULL_VALUE)]
    pub detach: bool,

    /// Whether to pull prebuilt images when possible
    #[arg(long, global = true, env = "ARK_PULL")]
    pub pull: bool,

    /// Specify repository home
    #[arg(
        long,
        global = true,
        env = Self::ARK_REPOSITORY_HOME_KEY,
        default_value = Self::ARK_REPOSITORY_HOME_VALUE,
    )]
    pub repository_home: PathBuf,
}

impl ActorArgs {
    pub(crate) const ARK_CONTAINER_IMAGE_NAME_PREFIX_KEY: &'static str =
        "ARK_CONTAINER_IMAGE_NAME_PREFIX";

    pub(crate) const ARK_CONTAINER_IMAGE_NAME_PREFIX_VALUE: &'static str =
        "registry.ark.svc.ops.openark";

    pub(crate) const ARK_CONTAINER_RUNTIME_KEY: &'static str = "ARK_CONTAINER_RUNTIME";

    pub const ARK_CONTAINER_TEMPLATE_FILE_KEY: &'static str = "ARK_CONTAINER_TEMPLATE_FILE";
    pub const ARK_CONTAINER_TEMPLATE_FILE_VALUE: &'static str =
        "./templates/ark/templates/Containerfile.j2";

    pub const ARK_PULL_KEY: &'static str = "ARK_PULL";
    pub const ARK_PULL_VALUE: bool = false;

    pub(crate) const ARK_REPOSITORY_HOME_KEY: &'static str = "ARK_REPOSITORY_HOME";
    pub(crate) const ARK_REPOSITORY_HOME_VALUE: &'static str = "./templates/ark/repos/";

    pub fn assert_add_if_not_exists(&self, name: &str) -> Result<()> {
        if self.add_if_not_exists {
            Ok(())
        } else {
            bail!("failed to find a package; you may add the package: {name:?}")
        }
    }

    pub const fn sync(&self) -> bool {
        !self.detach
    }
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
