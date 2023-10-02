use std::sync::Arc;

use anyhow::Result;
use clap::{ArgAction, Parser};
use dash_pipe_provider::{PipeEngine, PipeMessage, PipeMessages, PipePayload};
use serde::{Deserialize, Serialize};
use serde_json::Value;

fn main() {
    PipeEngine::from_env().loop_forever(tick)
}

#[derive(Clone, Debug, Parser, Serialize, Deserialize)]
pub struct FunctionArgs {
    #[arg(long, env = "PIPE_IDENTITY_WRITE_TO_PERSISTENT_STORAGE", action = ArgAction::SetTrue)]
    #[serde(default)]
    write_to_persistent_storage: Option<bool>,
}

async fn tick(
    args: Arc<FunctionArgs>,
    input: PipeMessages<Value>,
) -> Result<Option<PipeMessages<Value>>> {
    match args.write_to_persistent_storage {
        Some(true) => Ok(Some(match input {
            PipeMessages::Single(message) => PipeMessages::Single(pack_payload(message)?),
            PipeMessages::Batch(messages) => PipeMessages::Batch(
                messages
                    .into_iter()
                    .map(pack_payload)
                    .collect::<Result<_>>()?,
            ),
        })),
        Some(false) | None => Ok(Some(input)),
    }
}

fn pack_payload(mut message: PipeMessage<Value>) -> Result<PipeMessage<Value>> {
    message.payloads = vec![PipePayload::new("test".into(), message.to_json_bytes()?)];
    Ok(message)
}
