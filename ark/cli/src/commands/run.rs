use ark_actor_api::PackageManager;
use clap::Parser;
use ipis::core::anyhow::Result;

use super::PackageManagerImpl;

#[derive(Parser)]
pub(crate) struct Args {
    /// Specify package name
    #[arg(env = "ARK_PACKAGE_NAME")]
    name: String,

    /// Specify command-line arguments
    #[arg(last = true)]
    args: Vec<String>,
}

impl Args {
    pub(crate) async fn run(self, args: &::ark_actor_api::args::ActorArgs) -> Result<()> {
        let manager = PackageManagerImpl::try_new(args).await?;
        manager.run(&self.name, &self.args).await
    }
}
