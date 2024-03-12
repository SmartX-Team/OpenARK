use std::{
    fmt,
    ops::RangeInclusive,
    sync::Arc,
    time::{Duration, Instant},
};

use anyhow::Result;
use async_trait::async_trait;
use clap::Parser;
use derivative::Derivative;
use serde::{Deserialize, Serialize};
use tokio::time::sleep;
use tracing::{instrument, Level};

use crate::{message::PipeMessages, storage::StorageIO};

use super::{Function, FunctionBuilder, FunctionContext};

#[async_trait]
impl<F> FunctionBuilder for Connector<F>
where
    F: Send + FunctionBuilder,
    <F as FunctionBuilder>::Args: Sync + fmt::Debug,
{
    type Args = Args<<F as FunctionBuilder>::Args>;

    #[instrument(level = Level::INFO, skip_all, err(Display))]
    async fn try_new(
        args: &<Self as FunctionBuilder>::Args,
        ctx: Option<&mut FunctionContext>,
        storage: &Arc<StorageIO>,
    ) -> Result<Self> {
        Ok(Self {
            args: args.connector,
            function: <F as FunctionBuilder>::try_new(&args.function, ctx, storage).await?,
            instant: Instant::now(),
            iteration: 0..=u64::MAX,
        })
    }
}

#[async_trait]
impl<F> Function for Connector<F>
where
    F: Send + Function + FunctionBuilder,
{
    type Input = <F as Function>::Input;
    type Output = <F as Function>::Output;

    async fn tick(
        &mut self,
        inputs: PipeMessages<<Self as Function>::Input>,
    ) -> Result<PipeMessages<<Self as Function>::Output>> {
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

        self.function.tick(inputs).await
    }
}

#[derive(Derivative)]
#[derivative(Debug(bound = "
        F: fmt::Debug,
        <F as FunctionBuilder>::Args: fmt::Debug,
    "))]
pub struct Connector<F>
where
    F: FunctionBuilder,
{
    args: ConnectorArgs,
    function: F,
    instant: Instant,
    iteration: RangeInclusive<u64>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Parser)]
pub struct Args<F>
where
    F: ::clap::Args,
{
    #[command(flatten)]
    connector: ConnectorArgs,

    #[command(flatten)]
    function: F,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Parser)]
pub struct ConnectorArgs {
    #[arg(long, env = "PIPE_INTERVAL_MS", value_name = "MILLISECONDS")]
    #[serde(default)]
    interval_ms: Option<u64>,
}
