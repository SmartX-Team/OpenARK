use std::sync::Arc;

use anyhow::{bail, Result};
use async_trait::async_trait;
use clap::{ArgAction, Parser};
use dash_pipe_provider::{
    FunctionContext, PipeArgs, PipeMessage, PipeMessages, PipePayload, StorageSet, StorageType,
    Stream,
};
use futures::StreamExt;
use serde::{Deserialize, Serialize};

fn main() {
    PipeArgs::<Function>::from_env().loop_forever()
}

#[derive(Clone, Debug, Serialize, Deserialize, Parser)]
pub struct FunctionArgs {
    #[arg(long, env = "PIPE_PERSISTENCE", action = ArgAction::SetTrue)]
    #[serde(default)]
    persistence: Option<bool>,
}

pub struct Function {
    ctx: FunctionContext,
    items: Stream,
}

#[async_trait(?Send)]
impl ::dash_pipe_provider::Function for Function {
    type Args = FunctionArgs;
    type Input = ();
    type Output = usize;

    async fn try_new(
        args: &<Self as ::dash_pipe_provider::Function>::Args,
        ctx: &mut FunctionContext,
        storage: &Arc<StorageSet>,
    ) -> Result<Self> {
        let storage_type = match args.persistence {
            Some(true) => StorageType::PERSISTENT,
            Some(false) | None => StorageType::TEMPORARY,
        };

        Ok(Self {
            ctx: ctx.clone(),
            items: storage.get(storage_type).list().await?,
        })
    }

    async fn tick(
        &mut self,
        _inputs: PipeMessages<<Self as ::dash_pipe_provider::Function>::Input>,
    ) -> Result<PipeMessages<<Self as ::dash_pipe_provider::Function>::Output>> {
        match self.items.next().await {
            // TODO: stream이 JSON 메타데이터를 포함한 PipeMessage Object를 배출
            Some(Ok((path, value))) => Ok(PipeMessages::Single(PipeMessage {
                payloads: vec![PipePayload::new(path.to_string(), value)],
                value: Default::default(),
            })),
            Some(Err(error)) => bail!("failed to load data: {error}"),
            None => self.ctx.terminate_ok(),
        }
    }
}
