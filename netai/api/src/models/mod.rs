use std::path::Path;

use ipis::{async_trait::async_trait, core::anyhow::Result};

#[async_trait]
pub trait Model {
    fn get_name(&self) -> String;

    fn get_namespace(&self) -> String;

    async fn download_to(&self, path: &Path) -> Result<()>;

    async fn verify(&self, path: &Path) -> Result<bool>;
}
