pub mod args;
pub mod builder;
pub mod package;
pub mod repo;
pub mod runtime;

#[::async_trait::async_trait]
pub trait PackageManager {
    async fn exists(&self, name: &str) -> ::anyhow::Result<bool>;

    async fn add(&self, name: &str) -> ::anyhow::Result<()>;

    async fn delete(&self, name: &str) -> ::anyhow::Result<()>;

    async fn run(&self, name: &str, args: &[String]) -> ::anyhow::Result<()>;
}

#[::async_trait::async_trait]
impl PackageManager for Box<dyn PackageManager + Send + Sync> {
    async fn exists(&self, name: &str) -> ::anyhow::Result<bool> {
        (**self).exists(name).await
    }

    async fn add(&self, name: &str) -> ::anyhow::Result<()> {
        (**self).add(name).await
    }

    async fn delete(&self, name: &str) -> ::anyhow::Result<()> {
        (**self).delete(name).await
    }

    async fn run(&self, name: &str, args: &[String]) -> ::anyhow::Result<()> {
        (**self).run(name, args).await
    }
}
