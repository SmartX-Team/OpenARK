use anyhow::Result;
use dash_pipe_provider::{PipeEngine, PipeMessages};

fn main() {
    PipeEngine::from_env().loop_forever(tick)
}

async fn tick(input: PipeMessages<String>) -> Result<PipeMessages<String>> {
    // TODO: to be implemented
    dbg!(&input);
    Ok(input)
}
