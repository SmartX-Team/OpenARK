use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use clap::{ArgAction, Parser};
use dash_pipe_provider::{
    storage::StorageIO, PipeArgs, PipeMessage, PipeMessages, PipePayload, TaskContext,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

fn main() {
    PipeArgs::<Task>::from_env().loop_forever()
}

#[derive(Clone, Debug, Serialize, Deserialize, Parser)]
pub struct TaskArgs {
    #[arg(long, env = "PIPE_IDENTITY_WRITE_TO_PERSISTENT_STORAGE", action = ArgAction::SetTrue)]
    #[serde(default)]
    write_to_persistent_storage: Option<bool>,
}

pub struct Task {
    args: TaskArgs,
}

#[async_trait(?Send)]
impl ::dash_pipe_provider::Task for Task {
    type Args = TaskArgs;
    type Input = Value;
    type Output = Value;

    async fn try_new(
        args: &<Self as ::dash_pipe_provider::Task>::Args,
        ctx: &mut TaskContext,
        _storage: &Arc<StorageIO>,
    ) -> Result<Self> {
        ctx.disable_load();

        Ok(Self { args: args.clone() })
    }

    async fn tick(
        &mut self,
        inputs: PipeMessages<<Self as ::dash_pipe_provider::Task>::Input>,
    ) -> Result<PipeMessages<<Self as ::dash_pipe_provider::Task>::Output>> {
        match self.args.write_to_persistent_storage {
            Some(true) => Ok(match inputs {
                PipeMessages::None => PipeMessages::None,
                PipeMessages::Single(message) => PipeMessages::Single(pack_payload(message)?),
                PipeMessages::Batch(messages) => PipeMessages::Batch(
                    messages
                        .into_iter()
                        .map(pack_payload)
                        .collect::<Result<_>>()?,
                ),
            }),
            Some(false) | None => Ok(inputs),
        }
    }
}

fn pack_payload(mut message: PipeMessage<Value>) -> Result<PipeMessage<Value>> {
    message.payloads = vec![PipePayload::new("test".into(), message.to_bytes()?)];
    Ok(message)
}
