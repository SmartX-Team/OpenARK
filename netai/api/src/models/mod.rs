pub mod huggingface;

use std::path::Path;

use ipis::{async_trait::async_trait, core::anyhow::Result};

use crate::role::Role;

#[async_trait]
pub trait Model
where
    Self: Send + Sync,
{
    fn get_name(&self) -> String;

    fn get_namespace(&self) -> String;

    fn get_role(&self) -> Role;

    async fn download_to(&self, path: &Path) -> Result<()>;

    async fn verify(&self, path: &Path) -> Result<bool>;
}

#[async_trait]
impl<T> Model for &T
where
    T: Model,
{
    fn get_name(&self) -> String {
        (**self).get_name()
    }

    fn get_namespace(&self) -> String {
        (**self).get_namespace()
    }

    fn get_role(&self) -> Role {
        (**self).get_role()
    }

    async fn download_to(&self, path: &Path) -> Result<()> {
        (**self).download_to(path).await
    }

    async fn verify(&self, path: &Path) -> Result<bool> {
        (**self).verify(path).await
    }
}
