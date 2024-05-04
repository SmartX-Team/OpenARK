use std::net::SocketAddr;

use actix_web::{get, web::Data, App, HttpResponse, HttpServer, Responder};
use actix_web_opentelemetry::{RequestMetrics, RequestTracing};
use anyhow::Result;
use ark_core::env::infer;
use kubegraph_api::db::NetworkGraphDB;
use tracing::{error, instrument, Level};

#[instrument(level = Level::INFO)]
#[get("/_health")]
async fn health() -> impl Responder {
    HttpResponse::Ok().json("healthy")
}

pub async fn loop_forever(graph: impl 'static + NetworkGraphDB) {
    if let Err(error) = try_loop_forever(graph).await {
        error!("failed to run http server: {error}")
    }
}

async fn try_loop_forever(graph: impl 'static + NetworkGraphDB) -> Result<()> {
    // Initialize pipe
    let addr =
        infer::<_, SocketAddr>("BIND_ADDR").unwrap_or_else(|_| "0.0.0.0:80".parse().unwrap());
    let graph = Data::new(graph);

    // Start web server
    HttpServer::new(move || {
        let app = App::new().app_data(Data::clone(&graph));
        let app = app
            .service(health)
            .service(crate::routes::network::get)
            .service(crate::routes::network::get_kind)
            .service(crate::routes::network::get_kind_namespace);
        app.wrap(RequestTracing::default())
            .wrap(RequestMetrics::default())
    })
    .bind(addr)
    .unwrap_or_else(|e| panic!("failed to bind to {addr}: {e}"))
    .run()
    .await
    .map_err(Into::into)
}
