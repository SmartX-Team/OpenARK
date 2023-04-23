pub mod container_runtime;
pub mod template;

use ark_provider_api::{args::ActorArgs, repo::RepositoryManager};
use ipis::{async_trait::async_trait, core::anyhow::Result};

pub struct PackageManager {
    args: ActorArgs,
    container_runtime: self::container_runtime::ContainerRuntimeManager,
    repos: RepositoryManager,
    template: self::template::TemplateManager,
}

impl PackageManager {
    pub async fn try_new(args: ActorArgs) -> Result<Self> {
        Ok(Self {
            container_runtime: self::container_runtime::ContainerRuntimeManager::try_new(&args)
                .await?,
            repos: RepositoryManager::try_from_local(&args.repository_home).await?,
            template: self::template::TemplateManager::try_from_local(
                &args.container_template_file,
            )
            .await?,
            args,
        })
    }
}

#[async_trait]
impl ::ark_provider_api::PackageManager for PackageManager {
    async fn exists(&self, name: &str) -> Result<bool> {
        let package = self.repos.get(name).await?;

        self.container_runtime.exists(&package).await
    }

    async fn add(&self, name: &str) -> Result<()> {
        let package = self.repos.get(name).await?;

        if self.args.pull && !self.container_runtime.exists(&package).await? {
            self.container_runtime.pull(&package).await?;
        }

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
            self.args.assert_add_if_not_exists(name)?;
            self.add(name).await?;
        }

        self.container_runtime.run(&package, args).await
    }
}
