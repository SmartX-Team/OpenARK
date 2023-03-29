use std::path::PathBuf;

use ipis::{core::anyhow::Result, env, tokio::fs};
use ort::{tensor::InputTensor, Environment, GraphOptimizationLevel, LoggingLevel, SessionBuilder};

use crate::{
    tensor::{TensorKind, TensorKindMap},
    models::Model,
    role::Role,
};

pub struct Session {
    inner: ::ort::Session,
    inputs: TensorKindMap,
    outputs: TensorKindMap,
    role: Role,
}

impl Session {
    pub async fn try_new(model: impl Model) -> Result<Self> {
        let session = load_model(model).await?;
        let inputs = session
            .inputs
            .iter()
            .map(|input| TensorKind::try_from(input).map(|kind| (input.name.clone(), kind)))
            .collect::<Result<_>>()?;
        let outputs = session
            .outputs
            .iter()
            .map(|output| TensorKind::try_from(output).map(|kind| (output.name.clone(), kind)))
            .collect::<Result<_>>()?;
        let role = Role::try_from_io(&inputs, &outputs)?;

        Ok(Self {
            inner: session,
            inputs,
            outputs,
            role,
        })
    }

    pub fn inputs(&self) -> &TensorKindMap {
        &self.inputs
    }

    pub fn outputs(&self) -> &TensorKindMap {
        &self.outputs
    }

    pub fn role(&self) -> &Role {
        &self.role
    }

    pub fn run(&self, inputs: impl AsRef<[InputTensor]>) -> Result<()> {
        let outputs = self.inner.run(inputs)?;
        todo!()
    }
}

async fn load_model(model: impl Model) -> Result<::ort::Session> {
    // Specify model path
    let path = {
        let mut models_home: PathBuf =
            env::infer("MODEL_HOME").unwrap_or_else(|_| "/models".into());

        models_home.push(model.get_namespace());
        models_home.push(model.get_name());
        models_home
    };

    // Download model
    fs::create_dir_all(&path).await?;
    if !fs::try_exists(&path).await? || !model.verify(&path).await? {
        model.download_to(&path).await?;
    }

    // Load model
    let environment = Environment::builder()
        .with_name("netai")
        .with_log_level(LoggingLevel::Info)
        .build()?
        .into();
    let session = SessionBuilder::new(&environment)?
        .with_optimization_level(GraphOptimizationLevel::Level1)?
        .with_intra_threads(1)?
        .with_model_from_file(path)?;
    Ok(session)
}
