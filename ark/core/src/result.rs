#[cfg(feature = "actix-web")]
use actix_web::HttpResponse;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "result", content = "spec")]
pub enum Result<T> {
    Ok(T),
    Err(String),
}

impl<T, E> From<::core::result::Result<T, E>> for Result<T>
where
    E: ToString,
{
    fn from(value: ::core::result::Result<T, E>) -> Self {
        match value {
            Ok(value) => Self::Ok(value),
            Err(error) => Self::Err(error.to_string()),
        }
    }
}

#[cfg(feature = "actix-web")]
impl<T> From<Result<T>> for HttpResponse
where
    T: Serialize,
{
    fn from(value: Result<T>) -> Self {
        match value {
            Result::Ok(_) => HttpResponse::Ok().json(value),
            Result::Err(_) => HttpResponse::Forbidden().json(value),
        }
    }
}
