mod args;
mod commands;
mod package;

#[tokio::main]
async fn main() -> ::anyhow::Result<()> {
    use clap::Parser;

    self::args::Args::parse().run().await
}
