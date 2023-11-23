mod session;
mod storage;

use anyhow::Result;
use clap::Subcommand;
use tracing::{instrument, Level};

#[derive(Clone, Debug, Subcommand)]
pub(crate) enum Command {
    Query(::dash_query_cli::QueryArgs),

    #[command(flatten)]
    Session(self::session::Command),

    #[command(flatten)]
    Storage(self::storage::Command),
}

impl Command {
    #[instrument(level = Level::INFO, err(Display))]
    pub(crate) async fn run(self) -> Result<()> {
        match self {
            Command::Query(command) => command.run().await,
            Command::Session(command) => command.run().await,
            Command::Storage(command) => command.run().await,
        }
    }
}
