use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use anyhow::{anyhow, Result};
use async_trait::async_trait;
use byte_unit::Byte;
use bytes::Bytes;
use clap::Parser;
use dash_pipe_provider::{
    storage::StorageIO, FunctionContext, PipeArgs, PipeMessage, PipeMessages, PipePayload,
};
use serde::{Deserialize, Serialize};

fn main() {
    PipeArgs::<Function>::from_env().loop_forever()
}

#[derive(Clone, Debug, Serialize, Deserialize, Parser)]
pub struct FunctionArgs {
    #[arg(long, env = "PIPE_PERFORMANCE_TEST_DATA_SIZE", value_name = "SIZE")]
    data_size: Byte,

    #[arg(long, env = "PIPE_PERFORMANCE_TEST_PAYLOAD_SIZE", value_name = "SIZE")]
    payload_size: Option<Byte>,

    #[arg(
        long,
        env = "PIPE_PERFORMANCE_TEST_DURATION_MS",
        value_name = "MILLISECONDS"
    )]
    duration_ms: Option<u64>,
}

pub struct Function {
    ctx: FunctionContext,
    data_size: usize,
    duration: Option<Duration>,
    payload_size: Option<usize>,
    timestamp: Option<Instant>,
}

#[async_trait(?Send)]
impl ::dash_pipe_provider::Function for Function {
    type Args = FunctionArgs;
    type Input = Bytes;
    type Output = Bytes;

    async fn try_new(
        FunctionArgs {
            data_size,
            duration_ms,
            payload_size,
        }: &<Self as ::dash_pipe_provider::Function>::Args,
        ctx: &mut FunctionContext,
        _storage: &Arc<StorageIO>,
    ) -> Result<Self> {
        Ok(Self {
            ctx: ctx.clone(),
            data_size: data_size
                .get_bytes()
                .try_into()
                .map_err(|error| anyhow!("too large data size: {error}"))?,
            duration: duration_ms.map(Duration::from_millis),
            payload_size: payload_size
                .map(|size| {
                    size.get_bytes()
                        .try_into()
                        .map_err(|error| anyhow!("too large data size: {error}"))
                })
                .transpose()?,
            timestamp: None,
        })
    }

    async fn tick(
        &mut self,
        inputs: PipeMessages<<Self as ::dash_pipe_provider::Function>::Input>,
    ) -> Result<PipeMessages<<Self as ::dash_pipe_provider::Function>::Output>> {
        if self.timestamp.is_none() {
            self.timestamp = Some(Instant::now());
        }
        if let Some(duration) = self.duration {
            if self.timestamp.unwrap().elapsed() >= duration {
                return self.ctx.terminate_ok();
            }
        }

        Ok(match inputs {
            PipeMessages::None => PipeMessages::Single(self.create_packet()),
            PipeMessages::Single(message) => PipeMessages::Single(message),
            PipeMessages::Batch(messages) => PipeMessages::Batch(messages),
        })
    }
}

impl Function {
    fn create_packet(&self) -> PipeMessage<<Self as ::dash_pipe_provider::Function>::Output> {
        PipeMessage::new(self.create_payload(), self.create_value())
    }

    fn create_payload(&self) -> Vec<PipePayload> {
        match self.payload_size {
            Some(size) => vec![PipePayload::new("payload".into(), create_data(size))],
            None => Default::default(),
        }
    }

    fn create_value(&self) -> <Self as ::dash_pipe_provider::Function>::Output {
        create_data(self.data_size)
    }
}

fn create_data(size: usize) -> Bytes {
    Bytes::from(vec![98u8; size])
}
