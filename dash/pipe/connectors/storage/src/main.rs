use std::sync::Arc;

use anyhow::{anyhow, Result};
use async_trait::async_trait;
use clap::Parser;
use dash_pipe_provider::{
    FunctionContext, MetadataStorageExt, PipeArgs, PipeMessage, PipeMessages, StorageIO, Stream,
};
use futures::TryStreamExt;
use serde::{Deserialize, Serialize};
use serde_json::Value;

fn main() {
    PipeArgs::<Function>::from_env().loop_forever()
}

#[derive(Clone, Debug, Serialize, Deserialize, Parser)]
pub struct FunctionArgs {}

pub struct Function {
    ctx: FunctionContext,
    items: Stream<PipeMessage<Value>>,
}

#[async_trait(?Send)]
impl ::dash_pipe_provider::Function for Function {
    type Args = FunctionArgs;
    type Input = Value;
    type Output = Value;

    async fn try_new(
        FunctionArgs {}: &<Self as ::dash_pipe_provider::Function>::Args,
        ctx: &mut FunctionContext,
        storage: &Arc<StorageIO>,
    ) -> Result<Self> {
        Ok(Self {
            ctx: {
                ctx.disable_store_metadata();
                ctx.clone()
            },
            items: storage.input.get_default_metadata().list_as_empty().await?,
        })
    }

    async fn tick(
        &mut self,
        _inputs: PipeMessages<<Self as ::dash_pipe_provider::Function>::Input>,
    ) -> Result<PipeMessages<<Self as ::dash_pipe_provider::Function>::Output>> {
        match self
            .items
            .try_next()
            .await
            .map_err(|error| anyhow!("failed to load data: {error}"))?
        {
            Some(value) => Ok(PipeMessages::Single(value)),
            None => self.ctx.terminate_ok(),
        }
    }
}
