mod args;
mod commands;
mod package;

use clap::Parser;
use ipis::{core::anyhow::Result, tokio};

#[tokio::main]
async fn main() -> Result<()> {
    self::args::Args::parse().run().await
}
