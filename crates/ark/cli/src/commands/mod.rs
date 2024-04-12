mod session;
mod storage;

use anyhow::Result;
use clap::Subcommand;
use tracing::{instrument, Level};

#[derive(Clone, Debug, Subcommand)]
pub(crate) enum Command {
    #[command(flatten)]
    Cluster(::kiss_cli::ClusterArgs),

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
            Self::Cluster(command) => command.run().await,
            Self::Query(command) => command.run().await,
            Self::Session(command) => command.run().await,
            Self::Storage(command) => command.run().await,
        }
    }
}
