mod add;
mod del;
mod run;

use anyhow::Result;
use ark_provider_api::PackageManager;
use clap::Subcommand;

#[derive(Subcommand)]
pub(crate) enum Command {
    Add(self::add::Args),
    Del(self::del::Args),
    Run(self::run::Args),
}

impl Command {
    pub(crate) async fn run(self, manager: impl PackageManager) -> Result<()> {
        match self {
            Self::Add(command) => command.run(manager).await,
            Self::Del(command) => command.run(manager).await,
            Self::Run(command) => command.run(manager).await,
        }
    }
}
