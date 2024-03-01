use actix_web::{get, web::Data, HttpResponse, Responder};
use dash_network_api::ArcNetworkGraph;
use tracing::{instrument, Level};

#[instrument(level = Level::INFO)]
#[get("/")]
pub async fn get(graph: Data<ArcNetworkGraph>) -> impl Responder {
    HttpResponse::Ok().json(graph.to_json().await)
}
