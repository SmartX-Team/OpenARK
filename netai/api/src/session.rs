use std::path::PathBuf;

use actix_web::{dev::Payload, http::header, HttpRequest, HttpResponse};
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
    models::{Model, ModelKind},
    role::{BoxSolver, Request, Response, Role},
    tensor::{TensorField, TensorFieldMap},
};

pub struct Session {
    inner: ::ort::Session,
    inputs: TensorFieldMap,
    outputs: TensorFieldMap,
    model: Box<dyn Model>,
    role: Role,
    solver: BoxSolver,
}

impl Session {
    pub async fn try_default() -> Result<Self> {
        match env::infer("MODEL_KIND")? {
            ModelKind::Huggingface => {
                Self::try_new(crate::models::huggingface::Model {
                    repo: env::infer("MODEL_REPO")?,
                    role: env::infer("MODEL_ROLE")?,
                })
                .await
            }
        }
    }

    pub async fn try_new(model: impl 'static + Model) -> Result<Self> {
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

        let solver = role.load_solver(&model).await?;

        Ok(Self {
            inner: session,
            inputs,
            outputs,
            model: Box::new(model),
            role,
            solver,
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

    pub fn model(&self) -> &dyn Model {
        &*self.model
    }

    pub fn run_raw(
        &self,
        inputs: impl AsRef<[InputTensor]>,
    ) -> Result<Vec<DynOrtTensor<'_, IxDyn>>> {
        self.inner.run(inputs).map_err(Into::into)
    }

    pub async fn run_web(&self, req: HttpRequest, payload: Payload) -> HttpResponse {
        let request = Request { req, payload };

        match self.solver.solve(self, request).await {
            Ok(Response::Json(value)) => HttpResponse::Ok()
                .insert_header((header::CONTENT_TYPE, mime::APPLICATION_JSON))
                .body(value),
            Err(e) => HttpResponse::Forbidden().body(e.to_string()),
        }
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
