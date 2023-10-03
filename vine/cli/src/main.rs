use std::env;

use anyhow::Result;
use ark_core::tracer;
use clap::{value_parser, ArgAction, Parser, Subcommand};
use futures::future::try_join_all;
use kube::Client;
use serde::{Deserialize, Serialize};

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
        self.command.run().await
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
        self.init_tracer();
        Ok(())
    }

    fn init_tracer(&self) {
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
        tracer::init_once();
    }
}

#[derive(Subcommand)]
enum Commands {
    Login(CommandSession),
    LoginBatch(BatchCommandSession),
    Logout(CommandSession),
    LogoutBatch(BatchCommandSession),
}

impl Commands {
    async fn run(self) -> Result<()> {
        let kube = Client::try_default().await?;

        fn show_output<T>(response: T) -> Result<()>
        where
            T: Serialize,
        {
            ::serde_json::to_string_pretty(&response)
                .map(|response| {
                    println!("{}", response);
                })
                .map_err(Into::into)
        }

        match self {
            Self::Login(CommandSession {
                box_name,
                user_name,
            }) => ::vine_rbac::login::execute(&kube, &box_name, &user_name)
                .await
                .and_then(show_output),
            Self::LoginBatch(BatchCommandSession { csv }) => {
                let mut rdr = ::csv::Reader::from_path(csv)?;
                try_join_all(rdr.deserialize().map(|result| {
                    let kube = kube.clone();
                    async move {
                        match result {
                            Ok(CommandSession {
                                box_name,
                                user_name,
                            }) => ::vine_rbac::login::execute(&kube, &box_name, &user_name)
                                .await
                                .and_then(show_output),
                            Err(e) => Err(e.into()),
                        }
                    }
                }))
                .await
                .map(|_| ())
            }
            Self::Logout(CommandSession {
                box_name,
                user_name,
            }) => ::vine_rbac::logout::execute(&kube, &box_name, &user_name)
                .await
                .and_then(show_output),
            Self::LogoutBatch(BatchCommandSession { csv }) => {
                let mut rdr = ::csv::Reader::from_path(csv)?;
                try_join_all(rdr.deserialize().map(|result| {
                    let kube = kube.clone();
                    async move {
                        match result {
                            Ok(CommandSession {
                                box_name,
                                user_name,
                            }) => ::vine_rbac::logout::execute(&kube, &box_name, &user_name)
                                .await
                                .and_then(show_output),
                            Err(e) => Err(e.into()),
                        }
                    }
                }))
                .await
                .map(|_| ())
            }
        }
    }
}

#[derive(Parser, Serialize, Deserialize)]
struct CommandSession {
    /// Set a box name
    #[arg(short, long, env = "VINE_SESSION_BOX", value_name = "BOX")]
    #[serde(alias = "box")]
    box_name: String,

    /// Set a user name
    #[arg(short, long, env = "VINE_SESSION_USER", value_name = "USER")]
    #[serde(alias = "user")]
    user_name: String,
}

#[derive(Parser)]
struct BatchCommandSession {
    /// Set a box name
    #[arg(short, long, env = "VINE_SESSION_BATCH_CSV", value_name = "FILE")]
    csv: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    Args::parse().run().await
}
