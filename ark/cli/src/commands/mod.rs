mod storage;

use anyhow::Result;
use clap::Subcommand;

#[derive(Subcommand)]
pub(crate) enum Command {
    Query(::dash_query_cli::QueryArgs),

    #[command(flatten)]
    Storage(self::storage::Command),
}

impl Command {
    pub(crate) async fn run(self) -> Result<()> {
        match self {
            Command::Query(command) => command.run().await,
            Command::Storage(command) => command.run().await,
        }
    }
}
