use anyhow::Result;
use dash_pipe_provider::{PipeEngine, PipeMessages};
use serde_json::Value;

fn main() {
    PipeEngine::from_env().loop_forever(tick)
}

async fn tick(input: PipeMessages<Value>) -> Result<PipeMessages<Value>> {
    Ok(input)
}
