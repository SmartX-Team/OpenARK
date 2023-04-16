pub mod args;
pub mod package;
pub mod repo;

use std::ffi::OsStr;

use ipis::{async_trait, core::anyhow::Result};

#[async_trait::async_trait]
pub trait PackageManager {
    async fn exists(&self, name: &str) -> Result<bool>;

    async fn add(&self, name: &str) -> Result<()>;

    async fn delete(&self, name: &str) -> Result<()>;

    async fn run<I, S>(&self, name: &str, args: I) -> Result<()>
    where
        I: IntoIterator<Item = S> + Send,
        S: AsRef<OsStr>;
}
