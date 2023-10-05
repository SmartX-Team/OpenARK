use std::path::PathBuf;

use anyhow::{anyhow, Error, Result};
use async_trait::async_trait;
use clap::Parser;
use dash_pipe_provider::{PipeArgs, PipeMessages, PyPipeMessage};
use pyo3::{types::PyModule, PyObject, Python};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::{fs::File, io::AsyncReadExt};

fn main() {
    PipeArgs::<Function>::from_env().loop_forever()
}

#[derive(Clone, Debug, Serialize, Deserialize, Parser)]
pub struct FunctionArgs {
    #[arg(short, long, env = "PIPE_PYTHON_SCRIPT", value_name = "PATH")]
    python_script: PathBuf,
}

pub struct Function {
    tick: PyObject,
}

#[async_trait(?Send)]
impl ::dash_pipe_provider::Function for Function {
    type Args = FunctionArgs;
    type Input = Value;
    type Output = Value;

    async fn try_new(args: &<Self as ::dash_pipe_provider::Function>::Args) -> Result<Self> {
        let FunctionArgs {
            python_script: file_path,
        } = args;

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

        Ok(Self {
            tick: Python::with_gil(|py| {
                let module = PyModule::from_code(py, &code, file_name, "__dash_pipe__")?;
                let tick = module.getattr("tick")?;
                Ok(tick.into())
            })
            .map_err(|error: Error| anyhow!("failed to init python script: {error}"))?,
        })
    }

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
