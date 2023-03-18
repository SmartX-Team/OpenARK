use std::env;

use clap::{value_parser, ArgAction, Parser, Subcommand};
use dash_actor_api::input::{InputTemplate, SetField};
use ipis::{core::anyhow::Result, logger, tokio};

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[command(flatten)]
    common: ArgsCommon,

    #[command(subcommand)]
    command: Commands,
}

impl Args {
    async fn run(self) -> Result<()> {
        self.common.run()?;
        self.command.run().await?;
        Ok(())
    }
}

#[derive(Parser)]
struct ArgsCommon {
    /// Turn debugging information on
    #[arg(short, long, action = ArgAction::Count)]
    #[arg(value_parser = value_parser!(u8).range(..=3))]
    debug: u8,
}

impl ArgsCommon {
    fn run(self) -> Result<()> {
        self.init_logger();
        Ok(())
    }

    fn init_logger(&self) {
        // You can see how many times a particular flag or argument occurred
        // Note, only flags can have multiple occurrences
        let debug_level = match self.debug {
            0 => "WARN",
            1 => "INFO",
            2 => "DEBUG",
            3 => "TRACE",
            level => unreachable!("too high debug level: {level}"),
        };
        env::set_var("RUST_LOG", debug_level);
        logger::init_once();
    }
}

#[derive(Subcommand)]
enum Commands {
    Create(CommandCreate),
}

impl Commands {
    async fn run(self) -> Result<()> {
        match self {
            Self::Create(command) => command.run().await,
        }
    }
}

/// Create a resource from a file or from stdin.
#[derive(Parser)]
struct CommandCreate {
    /// Set fields by manual
    #[arg(long)]
    set: Vec<SetField>,
}

impl CommandCreate {
    async fn run(self) -> Result<()> {
        let mut input = InputTemplate::default();
        input.update_fields(self.set)?;

        dbg!(&input);
        todo!()
    }
}

#[tokio::main]
async fn main() {
    Args::parse().run().await.expect("running a command")
}
