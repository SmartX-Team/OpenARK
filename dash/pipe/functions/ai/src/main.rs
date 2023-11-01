mod plugin;

use std::sync::Arc;

use anyhow::{anyhow, Error, Result};
use ark_core_k8s::data::Url;
use async_trait::async_trait;
use clap::Parser;
use dash_pipe_provider::{
    storage::StorageIO, FunctionContext, PipeArgs, PipeMessages, PyPipeMessage,
};
use pyo3::{types::PyModule, PyObject, Python};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use strum::{Display, EnumString};

fn main() {
    PipeArgs::<Function>::from_env().loop_forever()
}

#[derive(Clone, Debug, Serialize, Deserialize, Parser)]
pub struct FunctionArgs {
    #[arg(short, long, env = "PIPE_AI_MODEL", value_name = "URL")]
    ai_model: Url,

    #[arg(short, long, env = "PIPE_AI_MODEL_KIND", value_name = "KIND")]
    ai_model_kind: ModelKind,
}

#[derive(Copy, Clone, Debug, Display, EnumString, Serialize, Deserialize, Parser)]
pub enum ModelKind {
    QuestionAnswering,
    Summarization,
    TextGeneration,
    Translation,
    ZeroShotClassification,
}

pub struct Function {
    tick: PyObject,
}

#[async_trait(?Send)]
impl ::dash_pipe_provider::FunctionBuilder for Function {
    type Args = FunctionArgs;

    async fn try_new(
        args: &<Self as ::dash_pipe_provider::FunctionBuilder>::Args,
        _ctx: &mut FunctionContext,
        _storage: &Arc<StorageIO>,
    ) -> Result<Self> {
        let FunctionArgs {
            ai_model: model,
            ai_model_kind: kind,
        } = args;

        let code = self::plugin::Plugin::new().load_code(model)?;

        Ok(Self {
            tick: Python::with_gil(|py| {
                let module = PyModule::from_code(py, code, "__dash_pipe__.py", "__dash_pipe__")?;
                let loader = module.getattr("load")?;
                let tick = loader.call1((model.to_string(), kind.to_string()))?;
                Ok(tick.into())
            })
            .map_err(|error: Error| anyhow!("failed to init python tick function: {error}"))?,
        })
    }
}

#[async_trait(?Send)]
impl ::dash_pipe_provider::Function for Function {
    type Input = Value;
    type Output = Value;

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
