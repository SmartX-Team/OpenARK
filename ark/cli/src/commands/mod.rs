mod add;
mod del;
mod run;

#[cfg(feature = "local")]
pub use ark_actor_local::PackageManager as PackageManagerImpl;
use clap::Subcommand;
use ipis::core::anyhow::Result;

#[derive(Subcommand)]
pub(crate) enum Command {
    Add(self::add::Args),
    Del(self::del::Args),
    Run(self::run::Args),
}

impl Command {
    pub(crate) async fn run(self, args: &::ark_actor_api::args::Args) -> Result<()> {
        match self {
            Self::Add(command) => command.run(args).await,
            Self::Del(command) => command.run(args).await,
            Self::Run(command) => command.run(args).await,
        }
    }
}
