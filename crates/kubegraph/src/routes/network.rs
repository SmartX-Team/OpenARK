use actix_web::{get, web::Data, HttpResponse, Responder};
use ark_core::result::Result;
use kubegraph_api::provider::NetworkGraphProvider;
use tracing::{instrument, Level};

#[instrument(level = Level::INFO, skip(graph))]
#[get("/")]
pub async fn get(graph: Data<crate::DefaultNetworkGraphProvider>) -> impl Responder {
    HttpResponse::Ok().json(Result::Ok(graph.get_entries().await))
}
