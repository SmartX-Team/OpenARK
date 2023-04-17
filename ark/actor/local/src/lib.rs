pub mod container_runtime;
mod template;

use ark_actor_api::{
    args::{ActorArgs, PackageFlags},
    repo::RepositoryManager,
};
use ipis::{async_trait::async_trait, core::anyhow::Result};

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

#[async_trait]
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

    async fn run(&self, name: &str, args: &[String]) -> Result<()> {
        let package = self.repos.get(name).await?;

        if !self.container_runtime.exists(&package).await? {
            self.flags.assert_add_if_not_exists(name)?;
            self.add(name).await?;
        }
        self.container_runtime.run(&package, args).await
    }
}
