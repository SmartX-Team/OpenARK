use std::sync::Arc;

use anyhow::{anyhow, Error, Result};
use ark_core_k8s::data::Url;
use async_trait::async_trait;
use clap::Parser;
use dash_pipe_provider::{
    storage::StorageIO, DynValue, FunctionContext, PipeArgs, PipeMessages, PyPipeMessage,
};
use derivative::Derivative;
use pyo3::{
    types::{PyAnyMethods, PyModule},
    PyObject, Python,
};
use serde::{Deserialize, Serialize};
use straw_provider_python::PluginBuilder;

fn main() {
    PipeArgs::<Function>::from_env().loop_forever()
}

#[derive(Clone, Debug, Serialize, Deserialize, Parser)]
pub struct FunctionArgs {
    #[arg(short, long, env = "PIPE_AI_MODEL", value_name = "URL")]
    ai_model: Url,

    #[arg(short, long, env = "PIPE_AI_MODEL_KIND", value_name = "KIND")]
    ai_model_kind: String,
}

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
        let FunctionArgs {
            ai_model: model,
            ai_model_kind: kind,
        } = args;

        let code = PluginBuilder::new().load_code(model)?;

        Ok(Self {
            tick: Python::with_gil(|py| {
                let module = PyModule::from_code_bound(py, code, "__straw__.py", "__straw__")?;
                let loader = module.getattr("load")?;
                let tick = loader.call1((model.to_string(), kind))?;
                Ok(tick.into())
            })
            .map_err(|error: Error| anyhow!("failed to init python tick function: {error}"))?,
        })
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
}
