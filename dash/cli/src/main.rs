use std::env;

use clap::{value_parser, ArgAction, Parser, Subcommand};
use dash_actor::{
    client::{FunctionSession, SessionContextMetadata, SessionResult},
    input::InputFieldString,
};
use ipis::{core::anyhow::Result, logger, tokio};
use kiss_api::kube::Client;

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[command(flatten)]
    common: ArgsCommon,

    #[command(subcommand)]
    command: Commands,
}

impl Args {
    async fn run(self) -> SessionResult {
        match self.common.run() {
            Ok(()) => self.command.run().await,
            Err(e) => Err(e).into(),
        }
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
    async fn run(self) -> SessionResult {
        let kube = match Client::try_default().await {
            Ok(kube) => kube,
            Err(e) => return Err(e).into(),
        };

        match self {
            Self::Create(command) => command.run(kube).await,
        }
    }
}

/// Create a resource from a file or from stdin.
#[derive(Parser)]
struct CommandCreate {
    /// Set a function name
    #[arg(short, long, env = "DASH_FUNCTION", value_name = "NAME")]
    function: String,

    /// Set values by manual
    #[arg(short = 'v', long = "value")]
    inputs: Vec<InputFieldString>,
}

impl CommandCreate {
    async fn run(self, kube: Client) -> SessionResult {
        let metadata = SessionContextMetadata {
            name: self.function,
            namespace: kube.default_namespace().to_string(),
        };
        FunctionSession::create_raw(kube, &metadata, self.inputs).await
    }
}

#[tokio::main]
async fn main() {
    let result = Args::parse().run().await;
    match ::serde_json::to_string_pretty(&result) {
        Ok(result) => println!("{result}"),
        Err(_) => println!("{result:#?}"),
    }
}
