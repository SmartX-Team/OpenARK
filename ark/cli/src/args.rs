use std::env;

use ark_provider_api::{args::ActorArgs, PackageManager};
use clap::{value_parser, ArgAction, Parser};
use ipis::{core::anyhow::Result, futures::TryFutureExt, logger};
use strum::{Display, EnumString};

type BoxPackageManager = Box<dyn PackageManager + 'static + Send + Sync>;

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
pub(crate) struct Args {
    #[command(flatten)]
    common: ArgsCommon,

    #[command(subcommand)]
    command: crate::commands::Command,
}

impl Args {
    pub(crate) async fn run(self) -> Result<()> {
        self.common
            .run()
            .and_then(|manager| self.command.run(manager))
            .await
    }
}

#[derive(Parser)]
pub(crate) struct ArgsCommon {
    /// Turn debugging information on
    #[arg(short, long, global = true, env = "ARK_DEBUG", action = ArgAction::Count)]
    #[arg(value_parser = value_parser!(u8).range(..=3))]
    debug: u8,

    #[command(flatten)]
    actor: ActorArgs,

    /// Which provider to use
    #[arg(long, global = true, env = "ARK_PROVIDER")]
    #[cfg_attr(all(not(feature = "local"), feature = "kubernetes"), arg(default_value_t = Provider::Kubernetes))]
    #[cfg_attr(feature = "local", arg(default_value_t = Provider::Local))]
    provider: Provider,
}

impl ArgsCommon {
    async fn run(self) -> Result<BoxPackageManager> {
        self.init_logger();
        self.provider.init_package_manager(self.actor).await
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

#[derive(Copy, Clone, Debug, Display, EnumString, Parser)]
#[strum(serialize_all = "camelCase")]
pub(crate) enum Provider {
    #[cfg(feature = "kubernetes")]
    Kubernetes,
    #[cfg(feature = "local")]
    Local,
}

impl Provider {
    async fn init_package_manager(&self, args: ActorArgs) -> Result<BoxPackageManager> {
        match self {
            #[cfg(feature = "kubernetes")]
            Self::Kubernetes => ::ark_provider_kubernetes::PackageManager::try_new(args)
                .and_then(::ark_provider_kubernetes::PackageManager::try_into_owned_session)
                .await
                .map(|manager| Box::new(manager) as BoxPackageManager),
            #[cfg(feature = "local")]
            Self::Local => ::ark_provider_local::PackageManager::try_new(args)
                .await
                .map(|manager| Box::new(manager) as BoxPackageManager),
        }
    }
}
