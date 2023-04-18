use std::path::PathBuf;

use clap::Parser;
use ipis::core::anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use strum::{Display, EnumString};

#[derive(Parser)]
pub struct ActorArgs {
    #[command(flatten)]
    pub flags: PackageFlags,

    /// Specify container image name prefix
    #[arg(
        long,
        env = Self::ARK_CONTAINER_IMAGE_NAME_PREFIX_KEY,
        default_value = Self::ARK_CONTAINER_IMAGE_NAME_PREFIX_VALUE,
    )]
    pub container_image_name_prefix: String,

    /// Specify container runtime engine
    #[arg(long, env = Self::ARK_CONTAINER_RUNTIME_KEY)]
    pub container_runtime: Option<ContainerRuntimeKind>,

    /// Specify container template file
    #[arg(
        long,
        env = Self::ARK_CONTAINER_TEMPLATE_FILE_KEY,
        default_value = Self::ARK_CONTAINER_TEMPLATE_FILE_VALUE,
    )]
    pub container_template_file: PathBuf,

    /// Specify repository home
    #[arg(
        long,
        env = Self::ARK_REPOSITORY_HOME_KEY,
        default_value = Self::ARK_REPOSITORY_HOME_VALUE,
    )]
    pub repository_home: PathBuf,
}

impl ActorArgs {
    pub(crate) const ARK_CONTAINER_IMAGE_NAME_PREFIX_KEY: &'static str =
        "ARK_CONTAINER_IMAGE_NAME_PREFIX";
    pub(crate) const ARK_CONTAINER_IMAGE_NAME_PREFIX_VALUE: &'static str =
        "quay.io/ulagbulag/openark-package-";

    pub(crate) const ARK_CONTAINER_RUNTIME_KEY: &'static str = "ARK_CONTAINER_RUNTIME";

    pub const ARK_CONTAINER_TEMPLATE_FILE_KEY: &'static str = "ARK_CONTAINER_TEMPLATE_FILE";
    pub const ARK_CONTAINER_TEMPLATE_FILE_VALUE: &'static str =
        "./templates/ark/templates/Containerfile.j2";

    pub(crate) const ARK_REPOSITORY_HOME_KEY: &'static str = "ARK_REPOSITORY_HOME";
    pub(crate) const ARK_REPOSITORY_HOME_VALUE: &'static str = "./templates/ark/repos/";
}

#[derive(Clone, Debug, Parser)]
pub struct PackageFlags {
    #[arg(long, env = "ARK_FLAG_ADD_IF_NOT_EXISTS")]
    pub add_if_not_exists: bool,
}

impl PackageFlags {
    pub fn assert_add_if_not_exists(&self, name: &str) -> Result<()> {
        if self.add_if_not_exists {
            Ok(())
        } else {
            bail!("failed to find a package; you may add the package: {name:?}")
        }
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
