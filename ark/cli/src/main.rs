mod args;
mod commands;

#[tokio::main]
async fn main() -> ::anyhow::Result<()> {
    use clap::Parser;

    self::args::Args::parse().run().await
}
