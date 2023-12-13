use std::{ops::RangeInclusive, sync::Arc, time::Duration};

use anyhow::{anyhow, Result};
use async_trait::async_trait;
use clap::Parser;
use dash_pipe_provider::{
    storage::{MetadataStorageExt, StorageIO, Stream},
    DefaultModelIn, DynValue, FunctionContext, PipeArgs, PipeMessage, PipeMessages,
};
use derivative::Derivative;
use futures::TryStreamExt;
use serde::{Deserialize, Serialize};
use tokio::time::{sleep, Instant};

fn main() {
    PipeArgs::<Function>::from_env()
        .with_default_model_in(DefaultModelIn::ModelOut)
        .loop_forever()
}

#[derive(Clone, Debug, Serialize, Deserialize, Parser)]
pub struct FunctionArgs {
    #[arg(long, env = "PIPE_INTERVAL_MS", value_name = "MILLISECONDS")]
    #[serde(default)]
    interval_ms: Option<u64>,
}

#[derive(Derivative)]
#[derivative(Debug)]
pub struct Function {
    args: FunctionArgs,
    ctx: FunctionContext,
    instant: Instant,
    #[derivative(Debug = "ignore")]
    items: Stream<PipeMessage<DynValue>>,
    iteration: RangeInclusive<u64>,
}

#[async_trait]
impl ::dash_pipe_provider::FunctionBuilder for Function {
    type Args = FunctionArgs;

    async fn try_new(
        args: &<Self as ::dash_pipe_provider::FunctionBuilder>::Args,
        ctx: &mut FunctionContext,
        storage: &Arc<StorageIO>,
    ) -> Result<Self> {
        Ok(Self {
            args: args.clone(),
            ctx: {
                ctx.disable_load();
                ctx.disable_store();
                ctx.disable_store_metadata();
                ctx.clone()
            },
            instant: Instant::now(),
            items: storage.input.get_default_metadata().list_as_empty().await?,
            iteration: 0..=u64::MAX,
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
        // wait for fit interval
        if let Some(delay) = self.args.interval_ms.and_then(|interval_ms| {
            self.iteration
                .next()
                .and_then(|iteration| iteration.checked_mul(interval_ms))
                .map(Duration::from_millis)
        }) {
            let elapsed = self.instant.elapsed();
            if delay > elapsed {
                sleep(delay - elapsed).await;
            }
        }

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
