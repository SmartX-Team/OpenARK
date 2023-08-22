use anyhow::Result;
use clap::Subcommand;

#[derive(Subcommand)]
pub(crate) enum Command {}

impl Command {
    pub(crate) async fn run(self) -> Result<()> {
        match self {}
    }
}
