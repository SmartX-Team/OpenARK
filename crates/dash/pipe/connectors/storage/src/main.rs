use std::sync::Arc;

use anyhow::{anyhow, Result};
use async_trait::async_trait;
use clap::Parser;
use dash_pipe_provider::{
    connector::Connector,
    storage::{MetadataStorageExt, StorageIO, Stream},
    DefaultModelIn, DynValue, FunctionContext, FunctionSignalExt, PipeArgs, PipeMessage,
    PipeMessages,
};
use derivative::Derivative;
use futures::TryStreamExt;
use serde::{Deserialize, Serialize};

fn main() {
    PipeArgs::<Connector<Function>>::from_env()
        .with_default_model_in(DefaultModelIn::ModelOut)
        .loop_forever()
}

#[derive(Clone, Debug, Serialize, Deserialize, Parser)]
pub struct FunctionArgs {}

#[derive(Derivative)]
#[derivative(Debug)]
pub struct Function {
    ctx: FunctionContext,
    #[derivative(Debug = "ignore")]
    items: Stream<PipeMessage<DynValue>>,
}

#[async_trait]
impl ::dash_pipe_provider::FunctionBuilder for Function {
    type Args = FunctionArgs;

    async fn try_new(
        FunctionArgs {}: &<Self as ::dash_pipe_provider::FunctionBuilder>::Args,
        ctx: &mut FunctionContext,
        storage: &Arc<StorageIO>,
    ) -> Result<Self> {
        Ok(Self {
            ctx: {
                ctx.disable_load();
                ctx.disable_store();
                ctx.disable_store_metadata();
                ctx.clone()
            },
            items: storage.input.get_default_metadata().list_as_empty().await?,
        })
    }
}

#[async_trait]
impl ::dash_pipe_provider::Function for Function {
    type Input = DynValue;
    type Output = DynValue;

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
