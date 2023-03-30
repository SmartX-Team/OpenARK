use std::path::PathBuf;

use ipis::{
    core::{anyhow::Result, ndarray::IxDyn},
    env,
    tokio::fs,
};
use ort::{
    tensor::{DynOrtTensor, InputTensor},
    Environment, GraphOptimizationLevel, LoggingLevel, SessionBuilder,
};

use crate::{
    models::Model,
    role::Role,
    tensor::{TensorField, TensorFieldMap},
};

pub struct Session {
    inner: ::ort::Session,
    inputs: TensorFieldMap,
    outputs: TensorFieldMap,
    role: Role,
}

impl Session {
    pub async fn try_new(model: impl Model) -> Result<Self> {
        let session = load_model(&model).await?;
        let inputs = session
            .inputs
            .iter()
            .enumerate()
            .map(|(index, input)| {
                TensorField::try_from_input(index, input).map(|kind| (input.name.clone(), kind))
            })
            .collect::<Result<_>>()?;
        let outputs = session
            .outputs
            .iter()
            .enumerate()
            .map(|(index, output)| {
                TensorField::try_from_output(index, output).map(|kind| (output.name.clone(), kind))
            })
            .collect::<Result<_>>()?;
        let role = model.get_role();

        Ok(Self {
            inner: session,
            inputs,
            outputs,
            role,
        })
    }

    pub fn inputs(&self) -> &TensorFieldMap {
        &self.inputs
    }

    pub fn outputs(&self) -> &TensorFieldMap {
        &self.outputs
    }

    pub fn role(&self) -> &Role {
        &self.role
    }

    pub fn run_raw(
        &self,
        inputs: impl AsRef<[InputTensor]>,
    ) -> Result<Vec<DynOrtTensor<'_, IxDyn>>> {
        self.inner.run(inputs).map_err(Into::into)
    }
}

async fn load_model(model: impl Model) -> Result<::ort::Session> {
    // Specify model path
    let path: PathBuf = {
        let models_home: String = env::infer("MODEL_HOME").unwrap_or_else(|_| "/models".into());
        let namespace = if env::infer("MODEL_USE_NAMESPACE").unwrap_or(true) {
            model.get_namespace()
        } else {
            "".into()
        };
        let name = model.get_name();

        format!("{models_home}/{namespace}/{name}").parse()?
    };

    // Download model
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).await?;
    }
    if !fs::try_exists(&path).await? || !model.verify(&path).await? {
        model.download_to(&path).await?;
    }

    // Load model
    let environment = Environment::builder()
        .with_name(crate::consts::NAMESPACE)
        .with_log_level(LoggingLevel::Info)
        .build()?
        .into();
    let session = SessionBuilder::new(&environment)?
        .with_optimization_level(GraphOptimizationLevel::Level1)?
        .with_intra_threads(1)?
        .with_model_from_file(path)?;
    Ok(session)
}
