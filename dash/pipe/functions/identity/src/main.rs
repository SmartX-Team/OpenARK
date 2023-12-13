use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use clap::{ArgAction, Parser};
use dash_pipe_provider::{
    storage::StorageIO, DynValue, FunctionContext, PipeArgs, PipeMessage, PipeMessages, PipePayload,
};
use serde::{Deserialize, Serialize};

fn main() {
    PipeArgs::<Function>::from_env().loop_forever()
}

#[derive(Clone, Debug, Serialize, Deserialize, Parser)]
pub struct FunctionArgs {
    #[arg(long, env = "PIPE_IDENTITY_WRITE_TO_PERSISTENT_STORAGE", action = ArgAction::SetTrue)]
    #[serde(default)]
    write_to_persistent_storage: Option<bool>,
}

#[derive(Debug)]
pub struct Function {
    args: FunctionArgs,
}

#[async_trait]
impl ::dash_pipe_provider::FunctionBuilder for Function {
    type Args = FunctionArgs;

    async fn try_new(
        args: &<Self as ::dash_pipe_provider::FunctionBuilder>::Args,
        ctx: &mut FunctionContext,
        _storage: &Arc<StorageIO>,
    ) -> Result<Self> {
        ctx.disable_load();
        ctx.disable_store();

        Ok(Self { args: args.clone() })
    }
}

#[async_trait]
impl ::dash_pipe_provider::Function for Function {
    type Input = DynValue;
    type Output = DynValue;

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

fn pack_payload(mut message: PipeMessage<DynValue>) -> Result<PipeMessage<DynValue>> {
    message.payloads = vec![PipePayload::new(
        "test".into(),
        Some((&message).try_into()?),
    )];
    Ok(message)
}
