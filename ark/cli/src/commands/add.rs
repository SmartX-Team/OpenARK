use ark_actor_api::PackageManager;
use clap::Parser;
use ipis::core::anyhow::Result;

use super::PackageManagerImpl;

#[derive(Parser)]
pub(crate) struct Args {
    /// Specify package name
    #[arg(env = "ARK_PACKAGE_NAME")]
    name: Vec<String>,
}

impl Args {
    pub(crate) async fn run(self, args: &::ark_actor_api::args::ActorArgs) -> Result<()> {
        let manager = PackageManagerImpl::try_new(args).await?;
        for name in &self.name {
            manager.add(name).await?;
        }
        Ok(())
    }
}
