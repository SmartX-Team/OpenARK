use std::{env, future::Future};

use anyhow::Result;
use ark_core::{logger, result::Result as SessionResult};
use clap::{value_parser, ArgAction, Parser, Subcommand};
use dash_provider::{client::FunctionSession, input::InputFieldString};
use dash_provider_api::SessionContextMetadata;
use kube::Client;
use serde::Serialize;
use serde_json::Value;

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[command(flatten)]
    common: ArgsCommon,

    #[command(subcommand)]
    command: Commands,
}

impl Args {
    async fn run(self) -> SessionResult<Value> {
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
    Create(CommandSession),
    Delete(CommandSession),
    Exists(CommandSession),
    Restart(CommandSession),
}

impl Commands {
    async fn run(self) -> SessionResult<Value> {
        let kube = match Client::try_default().await {
            Ok(kube) => kube,
            Err(e) => return Err(e).into(),
        };

        match self {
            Self::Create(command) => command.create(kube).await.into(),
            Self::Delete(command) => command.delete(kube).await.into(),
            Self::Exists(command) => command.exists(kube).await.into(),
            Self::Restart(command) => command.restart(kube).await.into(),
        }
    }
}

/// Create a resource from a file or from stdin.
#[derive(Clone, Parser)]
struct CommandSession {
    /// Set a function name
    #[arg(short, long, env = "DASH_FUNCTION", value_name = "NAME")]
    function: String,

    /// Set values by manual
    #[arg(short = 'v', long = "value")]
    inputs: Vec<InputFieldString>,

    /// Set a target namespace
    #[arg(long, env = "DASH_NAMESPACE", value_name = "NAMESPACE")]
    namespace: Option<String>,
}

impl CommandSession {
    async fn run<F, Fut, R>(self, kube: Client, f: F) -> Result<Value>
    where
        F: FnOnce(Client, SessionContextMetadata, Vec<InputFieldString>) -> Fut,
        Fut: Future<Output = Result<R>>,
        R: Serialize,
    {
        let metadata = SessionContextMetadata {
            name: self.function,
            namespace: self
                .namespace
                .unwrap_or_else(|| kube.default_namespace().to_string()),
        };
        f(kube, metadata, self.inputs)
            .await
            .and_then(|value| ::serde_json::to_value(value).map_err(Into::into))
    }

    async fn create(self, kube: Client) -> Result<Value> {
        self.run(kube, |kube, metadata, inputs| async move {
            FunctionSession::create(kube, &metadata, inputs).await
        })
        .await
    }

    async fn delete(self, kube: Client) -> Result<Value> {
        self.run(kube, |kube, metadata, inputs| async move {
            FunctionSession::delete(kube, &metadata, inputs).await
        })
        .await
    }

    async fn exists(self, kube: Client) -> Result<Value> {
        self.run(kube, |kube, metadata, inputs| async move {
            FunctionSession::exists(kube, &metadata, inputs).await
        })
        .await
    }

    async fn restart(self, kube: Client) -> Result<Value> {
        self.clone().delete(kube.clone()).await?;
        self.create(kube).await
    }
}

#[tokio::main]
async fn main() {
    let result = Args::parse().run().await;
    match serde_json::to_string_pretty(&result) {
        Ok(result) => println!("{result}"),
        Err(_) => println!("{result:#?}"),
    }
}
