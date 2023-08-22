use std::env;

use anyhow::Result;
use ark_core::logger;
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
        self.init_logger()
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
