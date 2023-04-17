pub mod args;
pub mod package;
pub mod repo;

use ipis::{async_trait, core::anyhow::Result};

#[async_trait::async_trait]
pub trait PackageManager {
    async fn exists(&self, name: &str) -> Result<bool>;

    async fn add(&self, name: &str) -> Result<()>;

    async fn delete(&self, name: &str) -> Result<()>;

    async fn run(&self, name: &str, args: &[String]) -> Result<()>;
}

#[async_trait::async_trait]
impl PackageManager for Box<dyn PackageManager + Send + Sync> {
    async fn exists(&self, name: &str) -> Result<bool> {
        (**self).exists(name).await
    }

    async fn add(&self, name: &str) -> Result<()> {
        (**self).add(name).await
    }

    async fn delete(&self, name: &str) -> Result<()> {
        (**self).delete(name).await
    }

    async fn run(&self, name: &str, args: &[String]) -> Result<()> {
        (**self).run(name, args).await
    }
}
