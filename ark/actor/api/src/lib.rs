pub mod args;
pub mod builder;
pub mod package;
pub mod repo;
pub mod runtime;

use ipis::{async_trait::async_trait, core::anyhow::Result};

#[async_trait]
pub trait PackageManager {
    async fn exists(&self, name: &str) -> Result<bool>;

    async fn add(&self, name: &str) -> Result<()>;

    async fn delete(&self, name: &str) -> Result<()>;

    async fn run(&self, name: &str, args: &[String], sync: bool) -> Result<()>;
}

#[async_trait]
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

    async fn run(&self, name: &str, args: &[String], sync: bool) -> Result<()> {
        (**self).run(name, args, sync).await
    }
}
