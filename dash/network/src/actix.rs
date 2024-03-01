use std::net::SocketAddr;

use actix_web::{get, web::Data, App, HttpResponse, HttpServer, Responder};
use actix_web_opentelemetry::{RequestMetrics, RequestTracing};
use anyhow::Result;
use ark_core::env::infer;
use dash_network_api::ArcNetworkGraph;
use tracing::{instrument, Level};

#[instrument(level = Level::INFO)]
#[get("/_health")]
async fn health() -> impl Responder {
    HttpResponse::Ok().json("healthy")
}

pub async fn loop_forever(graph: ArcNetworkGraph) {
    try_loop_forever(graph).await.expect("running a server");
}

async fn try_loop_forever(graph: ArcNetworkGraph) -> Result<()> {
    // Initialize pipe
    let addr =
        infer::<_, SocketAddr>("BIND_ADDR").unwrap_or_else(|_| "0.0.0.0:80".parse().unwrap());
    let graph = Data::new(graph);

    // Start web server
    HttpServer::new(move || {
        let app = App::new().app_data(Data::clone(&graph));
        let app = app
            .service(health)
            .service(crate::routes::edge::get)
            .service(crate::routes::network::get)
            .service(crate::routes::node::get);
        app.wrap(RequestTracing::default())
            .wrap(RequestMetrics::default())
    })
    .bind(addr)
    .unwrap_or_else(|e| panic!("failed to bind to {addr}: {e}"))
    .run()
    .await
    .map_err(Into::into)
}
