pub mod huggingface;

use std::path::Path;

use anyhow::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use strum::{Display, EnumString};

use crate::role::Role;

#[async_trait]
pub trait Model
where
    Self: Send + Sync,
{
    fn get_name(&self) -> String;

    fn get_namespace(&self) -> String;

    fn get_repo(&self) -> String;

    fn get_role(&self) -> Role;

    fn get_kind(&self) -> ModelKind;

    async fn get_license(&self) -> Result<Option<String>>;

    async fn get_readme(&self) -> Result<Option<String>>;

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

    fn get_repo(&self) -> String {
        (**self).get_repo()
    }

    fn get_role(&self) -> Role {
        (**self).get_role()
    }

    fn get_kind(&self) -> ModelKind {
        (**self).get_kind()
    }

    async fn get_license(&self) -> Result<Option<String>> {
        (**self).get_license().await
    }

    async fn get_readme(&self) -> Result<Option<String>> {
        (**self).get_readme().await
    }

    async fn download_to(&self, path: &Path) -> Result<()> {
        (**self).download_to(path).await
    }

    async fn verify(&self, path: &Path) -> Result<bool> {
        (**self).verify(path).await
    }
}

#[derive(
    Copy,
    Clone,
    Debug,
    Display,
    EnumString,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
)]
pub enum ModelKind {
    Huggingface,
}
