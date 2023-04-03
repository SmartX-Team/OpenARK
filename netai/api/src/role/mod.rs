pub mod nlp;

use actix_multipart::{
    form::{FieldReader, Limits, MultipartCollect, MultipartForm},
    Field, MultipartError,
};
use actix_web::{dev::Payload, http::header, web::Json, FromRequest, HttpRequest};
use ipis::{
    async_trait::async_trait,
    core::anyhow::{anyhow, bail, Result},
    futures::{future::LocalBoxFuture, FutureExt, TryFutureExt},
};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use strum::{Display, EnumString};

use crate::models::{Model, ModelKind};

pub(super) type BoxSolver = Box<dyn Solver + Send + Sync>;

#[async_trait(?Send)]
pub(super) trait Solver {
    async fn solve(&self, session: &crate::session::Session, request: Request) -> Result<Response>;
}

pub(super) struct Request {
    pub(super) req: HttpRequest,
    pub(super) payload: Payload,
}

impl Request {
    async fn parse<T>(&mut self) -> Result<T>
    where
        T: DeserializeOwned + MultipartCollect,
    {
        match self.req.headers().get(header::CONTENT_TYPE) {
            Some(content_type) => match content_type.to_str()? {
                s if s.starts_with(::mime::APPLICATION_JSON.essence_str()) => {
                    self.parse_json().await
                }
                s if s.starts_with(::mime::MULTIPART_FORM_DATA.essence_str()) => {
                    self.parse_multipart().await
                }
                s => bail!("Unsupported Content Type: {s:?}"),
            },
            None => bail!("Content Type Header is required"),
        }
    }

    async fn parse_json<T>(&mut self) -> Result<T>
    where
        T: DeserializeOwned,
    {
        Json::<T>::from_request(&self.req, &mut self.payload)
            .await
            .map(|value| value.0)
            .map_err(|e| anyhow!("{e}"))
    }

    async fn parse_multipart<T>(&mut self) -> Result<T>
    where
        T: MultipartCollect,
    {
        MultipartForm::<T>::from_request(&self.req, &mut self.payload)
            .await
            .map(|value| value.0)
            .map_err(|e| anyhow!("{e}"))
    }
}

pub(super) enum Response {
    Json(String),
}

impl Response {
    fn from_json<T>(value: &T) -> Result<Self>
    where
        T: Serialize,
    {
        ::serde_json::to_string(value)
            .map(Self::Json)
            .map_err(Into::into)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Text(pub String);

impl From<String> for Text {
    fn from(value: String) -> Self {
        Self(value)
    }
}

impl From<Text> for String {
    fn from(value: Text) -> Self {
        value.0
    }
}

impl<'t> FieldReader<'t> for Text {
    type Future = LocalBoxFuture<'t, Result<Self, MultipartError>>;

    fn read_field(req: &'t HttpRequest, field: Field, limits: &'t mut Limits) -> Self::Future {
        <::actix_multipart::form::text::Text<String> as FieldReader<'t>>::read_field(
            req, field, limits,
        )
        .map_ok(|value| Self(value.0))
        .boxed_local()
    }
}

#[derive(
    Copy,
    Clone,
    Debug,
    Display,
    EnumString,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
)]
pub enum Role {
    // NLP
    QuestionAnswering,
    ZeroShotClassification,
}

impl Role {
    pub(crate) const fn to_huggingface_feature(self) -> &'static str {
        match self {
            Self::QuestionAnswering => "question-answering",
            Self::ZeroShotClassification => "sequence-classification",
        }
    }

    pub(crate) async fn load_solver(&self, model: impl Model) -> Result<BoxSolver> {
        match self {
            // NLP
            Self::QuestionAnswering => match model.get_kind() {
                ModelKind::Huggingface => {
                    self::nlp::question_answering::Solver::load_from_huggingface(&model.get_repo())
                        .await
                        .map(|solver| Box::new(solver) as BoxSolver)
                }
            },
            Self::ZeroShotClassification => match model.get_kind() {
                ModelKind::Huggingface => {
                    self::nlp::zero_shot_classification::Solver::load_from_huggingface(
                        &model.get_repo(),
                    )
                    .await
                    .map(|solver| Box::new(solver) as BoxSolver)
                }
            },
        }
    }
}
