use std::{path::PathBuf, sync::Arc};

use anyhow::{anyhow, Error, Result};
use async_trait::async_trait;
use clap::Parser;
use dash_pipe_provider::{
    storage::StorageIO, DynValue, FunctionContext, PipeMessage, PipeMessages, PyPipeMessage,
    RemoteFunction,
};
use derivative::Derivative;
use pyo3::{types::PyModule, PyObject, Python};
use serde::{Deserialize, Serialize};
use tokio::{fs::File, io::AsyncReadExt};

#[derive(Derivative)]
#[derivative(Debug)]
pub struct Function {
    #[derivative(Debug = "ignore")]
    tick: PyObject,
}

#[async_trait]
impl ::dash_pipe_provider::FunctionBuilder for Function {
    type Args = FunctionArgs;

    async fn try_new(
        args: &<Self as ::dash_pipe_provider::FunctionBuilder>::Args,
        _ctx: Option<&mut FunctionContext>,
        _storage: &Arc<StorageIO>,
    ) -> Result<Self> {
        args.build().await
    }
}

#[async_trait]
impl ::dash_pipe_provider::RemoteFunction for Function {
    type Input = DynValue;
    type Output = DynValue;

    async fn call(
        &self,
        inputs: PipeMessages<<Self as RemoteFunction>::Input>,
    ) -> Result<PipeMessages<<Self as RemoteFunction>::Output>> {
        let inputs: Vec<PyPipeMessage> = inputs.into();
        let outputs: Vec<PyPipeMessage> = Python::with_gil(|py| {
            self.tick
                .call1(py, (inputs,))
                .map_err(|error| anyhow!("failed to execute python script: {error}"))
                .and_then(|outputs| {
                    outputs.extract(py).map_err(|error| {
                        anyhow!("failed to extract python script outputs: {error}")
                    })
                })
        })?;
        Ok(PipeMessages::Batch(
            outputs.into_iter().map(Into::into).collect(),
        ))
    }

    async fn call_one(
        &self,
        input: PipeMessage<<Self as RemoteFunction>::Input>,
    ) -> Result<PipeMessage<<Self as RemoteFunction>::Output>> {
        self.call(PipeMessages::Single(input))
            .await?
            .try_into_single()
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, Parser)]
pub struct FunctionArgs {
    #[arg(short, long, env = "PIPE_PYTHON_SCRIPT", value_name = "PATH")]
    pub python_script: PathBuf,

    #[arg(
        long,
        env = "PIPE_PYTHON_TICK_METHOD",
        value_name = "NAME",
        default_value_t = FunctionArgs::default_python_tick_method(),
    )]
    #[serde(default = "FunctionArgs::default_python_tick_method")]
    pub python_tick_method: String,
}

impl FunctionArgs {
    fn default_python_tick_method() -> String {
        Self::default_python_tick_method_str().into()
    }

    pub const fn default_python_tick_method_str() -> &'static str {
        "tick"
    }
}

impl FunctionArgs {
    pub async fn build(&self) -> Result<Function> {
        let Self {
            python_tick_method: tick_name,
            python_script: file_path,
        } = self;

        let code = {
            let mut file = File::open(file_path).await?;
            let mut buf = Default::default();
            file.read_to_string(&mut buf)
                .await
                .map_err(|error| anyhow!("failed to load python script: {error}"))?;
            buf
        };

        let file_name = file_path
            .to_str()
            .ok_or_else(|| anyhow!("failed to parse python script path"))?;

        Ok(Function {
            tick: Python::with_gil(|py| {
                let module = PyModule::from_code(py, &code, file_name, "__dash_pipe__")?;
                let tick = module.getattr(tick_name.as_str()).map_err(|error| {
                    anyhow!("failed to load python tick function {tick_name:?}: {error}")
                })?;
                Ok(tick.into())
            })
            .map_err(|error: Error| anyhow!("failed to init python tick function: {error}"))?,
        })
    }
}
