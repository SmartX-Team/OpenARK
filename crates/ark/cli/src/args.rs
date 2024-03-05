use anyhow::Result;
use ark_core::tracer;
use clap::{value_parser, ArgAction, Parser};

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
        self.common.run();
        self.command.run().await
    }
}

#[derive(Parser)]
pub(crate) struct ArgsCommon {
    /// Turn debugging information on
    #[arg(short, long, global = true, env = "ARK_DEBUG", action = ArgAction::Count)]
    #[arg(value_parser = value_parser!(u8).range(..=3))]
    debug: u8,
}

impl ArgsCommon {
    fn run(self) {
        tracer::init_once_with_level_int(self.debug, true)
    }
}
