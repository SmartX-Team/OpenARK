mod upgrade;

use anyhow::Result;
use clap::Subcommand;
use tracing::{instrument, Level};

#[derive(Clone, Debug, Subcommand)]
pub enum ClusterArgs {
    ClusterUpgrade(self::upgrade::ClusterUpgradeArgs),
}

impl ClusterArgs {
    #[instrument(level = Level::INFO, err(Display))]
    pub async fn run(self) -> Result<()> {
        match self {
            Self::ClusterUpgrade(command) => command.run().await,
        }
    }
}
