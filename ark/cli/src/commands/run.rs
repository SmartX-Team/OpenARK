use anyhow::Result;
use ark_provider_api::PackageManager;
use clap::Parser;

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
    pub(crate) async fn run(self, manager: impl PackageManager) -> Result<()> {
        manager.run(&self.name, &self.args).await
    }
}
