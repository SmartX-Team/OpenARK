use std::{ops::RangeInclusive, sync::Arc, time::Duration};

use anyhow::Result;
use async_trait::async_trait;
use clap::Parser;
use dash_pipe_provider::{
    storage::StorageIO, FunctionContext, PipeArgs, PipeMessage, PipeMessages,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use tokio::time::{sleep, Instant};

fn main() {
    PipeArgs::<Function>::from_env().loop_forever()
}

#[derive(Clone, Debug, Serialize, Deserialize, Parser)]
pub struct FunctionArgs {
    #[arg(long, env = "PIPE_INTERVAL_MS", value_name = "MILLISECONDS", default_value_t = FunctionArgs::default_interval_ms(),)]
    #[serde(default = "FunctionArgs::default_interval_ms")]
    interval_ms: u64,
}

impl FunctionArgs {
    pub fn default_interval_ms() -> u64 {
        1_000 // 1 second
    }
}

#[derive(Debug)]
pub struct Function {
    args: FunctionArgs,
    instant: Instant,
    iteration: RangeInclusive<u64>,
}

#[async_trait]
impl ::dash_pipe_provider::FunctionBuilder for Function {
    type Args = FunctionArgs;

    async fn try_new(
        args: &<Self as ::dash_pipe_provider::FunctionBuilder>::Args,
        ctx: Option<&mut FunctionContext>,
        _storage: &Arc<StorageIO>,
    ) -> Result<Self> {
        if let Some(ctx) = ctx {
            ctx.disable_load();
        }

        Ok(Self {
            args: args.clone(),
            instant: Instant::now(),
            iteration: 0..=u64::MAX,
        })
    }
}

#[async_trait]
impl ::dash_pipe_provider::Function for Function {
    type Input = ();
    type Output = Ping;

    async fn tick(
        &mut self,
        _inputs: PipeMessages<<Self as ::dash_pipe_provider::Function>::Input>,
    ) -> Result<PipeMessages<<Self as ::dash_pipe_provider::Function>::Output>> {
        let index = self.iteration.next();

        // wait for fit interval
        if let Some(delay) = index.and_then(|index| {
            index
                .checked_mul(self.args.interval_ms)
                .map(Duration::from_millis)
        }) {
            let elapsed = self.instant.elapsed();
            if delay > elapsed {
                sleep(delay - elapsed).await;
            }
        }

        Ok(PipeMessages::Single(PipeMessage::new(Ping { index })))
    }
}

#[derive(
    Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
pub struct Ping {
    index: Option<u64>,
}
