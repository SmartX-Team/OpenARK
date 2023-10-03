use std::path::PathBuf;

use actix_web::{dev::Payload, http::header, HttpRequest, HttpResponse};
use anyhow::{bail, Result};
use ark_core::env;
use ort::{
    value::DynArrayRef, Environment, ExecutionProvider, GraphOptimizationLevel, LoggingLevel,
    SessionBuilder, Value,
};
use tokio::fs;

use crate::{
    io::{Request, Response},
    models::{Model, ModelKind},
    role::Role,
    tensor::{OutputTensor, TensorField, TensorFieldMap, TensorType},
    BoxSolver,
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
            .filter_map(|(index, output)| {
                TensorField::try_from_output(index, output).map(|kind| (output.name.clone(), kind))
            })
            .collect();
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

    pub(crate) fn run_raw(&self, inputs: &[DynArrayRef]) -> Result<SessionOutput> {
        let inputs: Vec<_> = inputs
            .iter()
            .map(|array| match array {
                DynArrayRef::Bool(array) => DynArrayRef::Bool(array.view().into()),
                DynArrayRef::Int8(array) => DynArrayRef::Int8(array.view().into()),
                DynArrayRef::Int16(array) => DynArrayRef::Int16(array.view().into()),
                DynArrayRef::Int32(array) => DynArrayRef::Int32(array.view().into()),
                DynArrayRef::Int64(array) => DynArrayRef::Int64(array.view().into()),
                DynArrayRef::Uint8(array) => DynArrayRef::Uint8(array.view().into()),
                DynArrayRef::Uint16(array) => DynArrayRef::Uint16(array.view().into()),
                DynArrayRef::Uint32(array) => DynArrayRef::Uint32(array.view().into()),
                DynArrayRef::Uint64(array) => DynArrayRef::Uint64(array.view().into()),
                DynArrayRef::Bfloat16(array) => DynArrayRef::Bfloat16(array.view().into()),
                DynArrayRef::Float16(array) => DynArrayRef::Float16(array.view().into()),
                DynArrayRef::Float(array) => DynArrayRef::Float(array.view().into()),
                DynArrayRef::Double(array) => DynArrayRef::Double(array.view().into()),
                DynArrayRef::String(array) => DynArrayRef::String(array.view().into()),
            })
            .collect();
        let inputs = inputs
            .iter()
            .map(|array| match array {
                DynArrayRef::Bool(array) => Value::from_array(self.inner.allocator(), array),
                DynArrayRef::Int8(array) => Value::from_array(self.inner.allocator(), array),
                DynArrayRef::Int16(array) => Value::from_array(self.inner.allocator(), array),
                DynArrayRef::Int32(array) => Value::from_array(self.inner.allocator(), array),
                DynArrayRef::Int64(array) => Value::from_array(self.inner.allocator(), array),
                DynArrayRef::Uint8(array) => Value::from_array(self.inner.allocator(), array),
                DynArrayRef::Uint16(array) => Value::from_array(self.inner.allocator(), array),
                DynArrayRef::Uint32(array) => Value::from_array(self.inner.allocator(), array),
                DynArrayRef::Uint64(array) => Value::from_array(self.inner.allocator(), array),
                DynArrayRef::Bfloat16(array) => Value::from_array(self.inner.allocator(), array),
                DynArrayRef::Float16(array) => Value::from_array(self.inner.allocator(), array),
                DynArrayRef::Float(array) => Value::from_array(self.inner.allocator(), array),
                DynArrayRef::Double(array) => Value::from_array(self.inner.allocator(), array),
                DynArrayRef::String(array) => Value::from_array(self.inner.allocator(), array),
            })
            .collect::<Result<_, _>>()?;
        self.inner
            .run(inputs)
            .map(|inner| SessionOutput {
                session: self,
                inner,
            })
            .map_err(Into::into)
    }

    pub async fn run_web(&self, req: HttpRequest, payload: Payload) -> HttpResponse {
        let request = Request { req, payload };

        match self.solver.solve(self, request).await {
            Ok(Response::Json(value)) => HttpResponse::Ok()
                .insert_header((header::CONTENT_TYPE, ::mime::APPLICATION_JSON))
                .body(value),
            Err(e) => HttpResponse::Forbidden().body(e.to_string()),
        }
    }
}

pub(crate) struct SessionOutput<'a> {
    session: &'a Session,
    inner: Vec<Value<'static>>,
}

impl<'a> SessionOutput<'a> {
    pub(crate) fn try_extract(&self, name: &str) -> Result<OutputTensor<'_>> {
        match self.session.outputs.get(name) {
            Some(field) => {
                let output = self.inner.get(field.index).expect("index should be valid");
                match field.tensor_type {
                    TensorType::Bool => output
                        .try_extract::<i8>()
                        .map_err(Into::into)
                        .map(Into::into),
                    TensorType::Int8 => output
                        .try_extract::<i8>()
                        .map(Into::into)
                        .map_err(Into::into),
                    TensorType::Int16 => output
                        .try_extract::<i16>()
                        .map(Into::into)
                        .map_err(Into::into),
                    TensorType::Int32 => output
                        .try_extract::<i32>()
                        .map(Into::into)
                        .map_err(Into::into),
                    TensorType::Int64 => output
                        .try_extract::<i64>()
                        .map(Into::into)
                        .map_err(Into::into),
                    TensorType::Uint8 => output
                        .try_extract::<u8>()
                        .map(Into::into)
                        .map_err(Into::into),
                    TensorType::Uint16 => output
                        .try_extract::<u16>()
                        .map(Into::into)
                        .map_err(Into::into),
                    TensorType::Uint32 => output
                        .try_extract::<u32>()
                        .map(Into::into)
                        .map_err(Into::into),
                    TensorType::Uint64 => output
                        .try_extract::<u64>()
                        .map(Into::into)
                        .map_err(Into::into),
                    TensorType::Bfloat16 => output
                        .try_extract::<::half::bf16>()
                        .map(Into::into)
                        .map_err(Into::into),
                    TensorType::Float16 => output
                        .try_extract::<::half::f16>()
                        .map(Into::into)
                        .map_err(Into::into),
                    TensorType::Float32 => output
                        .try_extract::<f32>()
                        .map(Into::into)
                        .map_err(Into::into),
                    TensorType::Float64 => output
                        .try_extract::<f64>()
                        .map(Into::into)
                        .map_err(Into::into),
                    TensorType::String => output
                        .try_extract::<String>()
                        .map(Into::into)
                        .map_err(Into::into),
                }
            }
            None => bail!("no such output tensor: {name:?}"),
        }
    }
}

async fn load_model(model: impl Model) -> Result<::ort::Session> {
    // Specify model path
    let path: PathBuf = {
        let models_home = env::infer_string("MODEL_HOME").unwrap_or_else(|_| "/models".into());
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
        .with_name(::netai_api::consts::NAMESPACE)
        .with_log_level(LoggingLevel::Info)
        .build()?
        .into();
    let session = SessionBuilder::new(&environment)?
        .with_execution_providers([
            ExecutionProvider::CUDA(Default::default()),
            ExecutionProvider::ACL(Default::default()),
            ExecutionProvider::TensorRT(Default::default()),
            ExecutionProvider::CPU(Default::default()),
        ])?
        .with_optimization_level(GraphOptimizationLevel::Level3)?
        .with_model_from_file(path)?;
    Ok(session)
}
