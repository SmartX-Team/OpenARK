use std::sync::Arc;

use anyhow::Result;
use clap::Parser;
use dash_pipe_provider::{PipeEngine, PipeMessages};
use serde::{Deserialize, Serialize};

fn main() {
    PipeEngine::from_env().loop_forever(tick)
}

#[derive(Clone, Debug, Parser, Serialize, Deserialize)]
pub struct FunctionArgs {}

async fn tick(
    _args: Arc<FunctionArgs>,
    input: PipeMessages<String>,
) -> Result<Option<PipeMessages<String>>> {
    // TODO: to be implemented
    dbg!(&input);
    Ok(Some(input))
}
