use actix_web::{get, web::Data, HttpResponse, Responder};
use ark_core::result::Result;
use kubegraph_client::NetworkGraphClient;
use tracing::{instrument, Level};

#[instrument(level = Level::INFO, skip(graph))]
#[get("/")]
pub async fn get(graph: Data<NetworkGraphClient>) -> impl Responder {
    HttpResponse::Ok().json(Result::from(graph.to_json().await))
}
