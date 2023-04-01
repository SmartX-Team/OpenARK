pub mod nlp;

use actix_multipart::form::{MultipartCollect, MultipartForm};
use actix_web::{dev::Payload, FromRequest, HttpRequest};
use inflector::Inflector;
use ipis::{
    async_trait::async_trait,
    core::anyhow::{anyhow, Result},
};
use serde::{Deserialize, Serialize};
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
    pub(super) async fn parse_multipart<T>(&mut self) -> Result<T>
    where
        T: MultipartCollect,
    {
        MultipartForm::<T>::from_request(&self.req, &mut self.payload)
            .await
            .map(|form| form.0)
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
}

impl Role {
    pub(crate) fn to_string_kebab_case(self) -> String {
        self.to_string().to_kebab_case()
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
        }
    }
}
