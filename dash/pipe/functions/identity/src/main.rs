use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use clap::{ArgAction, Parser};
use dash_pipe_provider::{
    FunctionContext, PipeArgs, PipeMessage, PipeMessages, PipePayload, StorageSet,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

fn main() {
    PipeArgs::<Function>::from_env().loop_forever()
}

#[derive(Clone, Debug, Serialize, Deserialize, Parser)]
pub struct FunctionArgs {
    #[arg(long, env = "PIPE_IDENTITY_WRITE_TO_PERSISTENT_STORAGE", action = ArgAction::SetTrue)]
    #[serde(default)]
    write_to_persistent_storage: Option<bool>,
}

pub struct Function {
    args: FunctionArgs,
}

#[async_trait(?Send)]
impl ::dash_pipe_provider::Function for Function {
    type Args = FunctionArgs;
    type Input = Value;
    type Output = Value;

    async fn try_new(
        args: &<Self as ::dash_pipe_provider::Function>::Args,
        _ctx: &mut FunctionContext,
        _storage: &Arc<StorageSet>,
    ) -> Result<Self> {
        Ok(Self { args: args.clone() })
    }

    async fn tick(
        &mut self,
        inputs: PipeMessages<<Self as ::dash_pipe_provider::Function>::Input>,
    ) -> Result<PipeMessages<<Self as ::dash_pipe_provider::Function>::Output>> {
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
    message.payloads = vec![PipePayload::new("test".into(), message.to_json_bytes()?)];
    Ok(message)
}
