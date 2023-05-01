use actix_multipart::{
    form::{FieldReader, Limits, MultipartCollect, MultipartForm},
    Field, MultipartError,
};
use actix_web::{dev::Payload, http::header, web::Json, FromRequest, HttpRequest};
use anyhow::{anyhow, bail, Result};
use futures::{future::LocalBoxFuture, FutureExt, TryFutureExt};
use serde::{de::DeserializeOwned, Deserialize, Serialize};

pub struct Request {
    pub req: HttpRequest,
    pub payload: Payload,
}

impl Request {
    pub async fn parse<T>(&mut self) -> Result<T>
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

pub enum Response {
    Json(String),
}

impl Response {
    pub fn from_json<T>(value: &T) -> Result<Self>
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
struct Text(String);

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
