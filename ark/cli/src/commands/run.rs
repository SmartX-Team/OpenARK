use ark_actor_api::PackageManager;
use clap::Parser;
use ipis::core::anyhow::Result;

#[derive(Parser)]
pub(crate) struct Args {
    /// Specify package name
    #[arg(env = "ARK_PACKAGE_NAME")]
    name: String,

    /// Specify command-line arguments
    #[arg(last = true)]
    args: Vec<String>,

    /// Whether the spawned process depends on the main process
    #[arg(long, env = "ARK_JOB_DETACH")]
    detach: bool,
}

impl Args {
    pub(crate) async fn run(self, manager: impl PackageManager) -> Result<()> {
        manager.run(&self.name, &self.args, !self.detach).await
    }
}
