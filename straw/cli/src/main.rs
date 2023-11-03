use std::{env, error::Error, process::exit, str::FromStr};

use anyhow::{anyhow, Result};
use ark_core::tracer;
use ark_core_k8s::data::Url;
use clap::{value_parser, ArgAction, Parser, Subcommand};
use k8s_openapi::api::core::v1::EnvVar;
use kube::Client;
use straw_api::{
    pipe::{StrawNode, StrawPipe},
    plugin::PluginContext,
};
use straw_provider::StrawSession;
use tracing::{error, info};

#[tokio::main]
async fn main() {
    match Args::parse().run().await {
        Ok(()) => (),
        Err(error) => {
            error!("{error}");
            exit(1)
        }
    }
}

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[command(flatten)]
    common: ArgsCommon,

    #[command(subcommand)]
    command: Commands,

    /// Set a default k8s namespace
    #[arg(short, long, env = "NAMESPACE", value_name = "NAME")]
    namespace: Option<String>,
}

impl Args {
    async fn run(self) -> Result<()> {
        self.common.run()?;
        self.command.run(self.namespace).await
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
    /// Create a straw function
    Create(CommandSession),
    /// Delete a straw function
    Delete(CommandSession),
    /// Check whether the straw function exists
    Exists(CommandSession),
}

impl Commands {
    async fn run(self, namespace: Option<String>) -> Result<()> {
        let kube = Client::try_default().await?;
        let session = StrawSession::new(kube, namespace);

        match self {
            Self::Create(command) => {
                let (ctx, pipe) = command.into();
                session.create(&ctx, &pipe).await
            }
            Self::Delete(command) => {
                let (_, pipe) = command.into();
                session.delete(&pipe).await
            }
            Self::Exists(command) => {
                let (_, pipe) = command.into();
                session
                    .exists(&pipe)
                    .await
                    .map(|exists| info!("exists: {exists}"))
            }
        }
    }
}

#[derive(Clone, Parser)]
struct CommandSession {
    #[command(flatten)]
    ctx: PluginContext,

    /// Set a straw name
    #[arg(short, long, env = "STRAW_NAME", value_name = "NAME")]
    name: String,

    /// Set straw environment variables
    #[arg(short = 'v', long = "value", value_parser = parse_kv::<String, String>)]
    env: Vec<(String, String)>,

    /// Set a straw provider source
    #[arg(long, env = "STRAW_SRC", value_name = "URL")]
    src: Url,
}

impl From<CommandSession> for (PluginContext, StrawPipe) {
    fn from(value: CommandSession) -> Self {
        let CommandSession {
            ctx,
            name,
            env,
            src,
        } = value;
        let pipe = StrawPipe {
            straw: vec![StrawNode {
                name,
                env: env
                    .into_iter()
                    .map(|(key, value)| EnvVar {
                        name: key,
                        value: Some(value),
                        value_from: None,
                    })
                    .collect(),
                src,
            }],
        };

        (ctx, pipe)
    }
}

fn parse_kv<T, V>(s: &str) -> Result<(T, V)>
where
    T: FromStr,
    T::Err: Error + Send + Sync + 'static,
    V: FromStr,
    V::Err: Error + Send + Sync + 'static,
{
    let pos = s
        .find('=')
        .ok_or_else(|| anyhow!("invalid KEY=value: no `=` found in `{s}`"))?;
    Ok((s[..pos].parse()?, s[pos + 1..].parse()?))
}
