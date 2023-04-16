pub mod container_runtime;
mod template;

use std::ffi::OsStr;

use ark_actor_api::{
    args::{ActorArgs, PackageFlags},
    repo::RepositoryManager,
};
use ipis::{
    async_trait,
    core::anyhow::{bail, Result},
};

pub struct PackageManager {
    container_runtime: self::container_runtime::ContainerRuntimeManager,
    flags: PackageFlags,
    repos: RepositoryManager,
    template: self::template::TemplateManager,
}

impl PackageManager {
    pub async fn try_new(args: &ActorArgs) -> Result<Self> {
        Ok(Self {
            container_runtime: self::container_runtime::ContainerRuntimeManager::try_new(
                args.container_runtime,
                args.container_image_name_prefix.clone(),
            )
            .await?,
            flags: args.flags.clone(),
            repos: RepositoryManager::try_from_local(&args.repository_home).await?,
            template: self::template::TemplateManager::try_from_local(
                &args.container_template_file,
            )
            .await?,
        })
    }
}

#[async_trait::async_trait]
impl ::ark_actor_api::PackageManager for PackageManager {
    async fn exists(&self, name: &str) -> Result<bool> {
        let package = self.repos.get(name).await?;

        self.container_runtime.exists(&package).await
    }

    async fn add(&self, name: &str) -> Result<()> {
        let package = self.repos.get(name).await?;

        let template = self.template.render_build(&package)?;
        self.container_runtime.build(&template).await
    }

    async fn delete(&self, name: &str) -> Result<()> {
        let package = self.repos.get(name).await?;

        if self.container_runtime.exists(&package).await? {
            self.container_runtime.remove(&package).await
        } else {
            Ok(())
        }
    }

    async fn run<I, S>(&self, name: &str, args: I) -> Result<()>
    where
        I: IntoIterator<Item = S> + Send,
        S: AsRef<OsStr>,
    {
        let package = self.repos.get(name).await?;

        if !self.container_runtime.exists(&package).await? {
            if self.flags.add_if_not_exists {
                self.add(name).await?;
            } else {
                bail!("failed to find a package; you may add the package: {name:?}")
            }
        }
        self.container_runtime.run(&package, args).await
    }
}
