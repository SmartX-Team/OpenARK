pub mod job;

#[cfg(feature = "actix-web")]
use actix_web::HttpResponse;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "result", content = "spec")]
pub enum SessionResult<T = FunctionChannel> {
    Ok(T),
    Err(String),
}

impl<T, E> From<Result<T, E>> for SessionResult<T>
where
    E: ToString,
{
    fn from(value: Result<T, E>) -> Self {
        match value {
            Ok(value) => Self::Ok(value),
            Err(error) => Self::Err(error.to_string()),
        }
    }
}

#[cfg(feature = "actix-web")]
impl<T> From<SessionResult<T>> for HttpResponse
where
    T: Serialize,
{
    fn from(value: SessionResult<T>) -> Self {
        match value {
            SessionResult::Ok(_) => HttpResponse::Ok().json(value),
            SessionResult::Err(_) => HttpResponse::Forbidden().json(value),
        }
    }
}

impl<T> SessionResult<T> {
    pub fn and_then<F, T2, E>(self, f: F) -> SessionResult<T2>
    where
        F: FnOnce(T) -> Result<T2, E>,
        E: ToString,
    {
        match self {
            Self::Ok(e) => f(e).into(),
            Self::Err(e) => SessionResult::Err(e),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FunctionChannel {
    pub metadata: SessionContextMetadata,
    pub actor: FunctionChannelKind,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "spec")]
pub enum FunctionChannelKind {
    Job(self::job::FunctionChannelKindJob),
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionContext<Spec> {
    pub metadata: SessionContextMetadata,
    pub spec: Spec,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionContextMetadata {
    pub name: String,
    pub namespace: String,
}
