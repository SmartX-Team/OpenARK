use actix_web::{
    get, post,
    web::{Data, Path},
    HttpResponse, Responder,
};
use ark_core::result::Result;
use tracing::{instrument, Level};

use crate::agent::Agent;

#[instrument(level = Level::INFO, skip(agent))]
#[get("/p")]
pub async fn list(agent: Data<Agent>) -> impl Responder {
    // TODO: to be implemented
    HttpResponse::Ok().json(Result::from(::anyhow::Result::<_>::Ok("unimplemented")))
}

#[instrument(level = Level::INFO, skip(agent))]
#[get("/p/{name}")]
pub async fn get(agent: Data<Agent>, name: Path<String>) -> impl Responder {
    // TODO: to be implemented
    HttpResponse::Ok().json(Result::from(::anyhow::Result::<_>::Ok("unimplemented")))
}

#[instrument(level = Level::INFO, skip(agent))]
#[post("/p")]
pub async fn post(agent: Data<Agent>) -> impl Responder {
    // TODO: to be implemented
    HttpResponse::Ok().json(Result::from(::anyhow::Result::<_>::Ok("unimplemented")))
}
