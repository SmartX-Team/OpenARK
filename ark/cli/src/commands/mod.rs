mod add;
mod del;
mod run;

use ark_actor_api::PackageManager;
use clap::Subcommand;
use ipis::core::anyhow::Result;

#[derive(Subcommand)]
pub(crate) enum Command {
    Add(self::add::Args),
    Del(self::del::Args),
    Run(self::run::Args),
}

impl Command {
    pub(crate) async fn run(
        self,
        manager: impl PackageManager,
        args: &::ark_actor_api::args::ActorArgs,
    ) -> Result<()> {
        match self {
            Self::Add(command) => command.run(manager, args).await,
            Self::Del(command) => command.run(manager, args).await,
            Self::Run(command) => command.run(manager, args).await,
        }
    }
}
