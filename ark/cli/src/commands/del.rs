use ark_actor_api::PackageManager;
use clap::Parser;
use ipis::core::anyhow::Result;

#[derive(Parser)]
pub(crate) struct Args {
    /// Specify package name
    #[arg(env = "ARK_PACKAGE_NAME")]
    name: Vec<String>,
}

impl Args {
    pub(crate) async fn run(self, manager: impl PackageManager) -> Result<()> {
        for name in &self.name {
            manager.delete(name).await?;
        }
        Ok(())
    }
}
