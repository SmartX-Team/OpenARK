use std::net::SocketAddr;

use actix_web::{get, App, HttpResponse, HttpServer, Responder};
use actix_web_opentelemetry::{RequestMetrics, RequestTracing};
use anyhow::Result;
use ark_core::env::infer;
use kubegraph_api::vm::NetworkVirtualMachine;
use tracing::{error, instrument, Level};

#[instrument(level = Level::INFO)]
#[get("/_health")]
async fn health() -> impl Responder {
    HttpResponse::Ok().json("healthy")
}

pub async fn loop_forever(vm: impl NetworkVirtualMachine) {
    if let Err(error) = try_loop_forever(vm).await {
        error!("failed to run http server: {error}")
    }
}

async fn try_loop_forever(vm: impl NetworkVirtualMachine) -> Result<()> {
    // Initialize pipe
    let addr =
        infer::<_, SocketAddr>("BIND_ADDR").unwrap_or_else(|_| "0.0.0.0:80".parse().unwrap());
    // let graph_db: Data<Arc<dyn NetworkGraphDB>> = Data::new(Arc::new(vm.graph_db().clone()));

    // Start web server
    HttpServer::new(move || {
        let app = App::new();
        // let app = app.app_data(Data::clone(&graph_db));
        let app = app.service(health)
        // .service(crate::routes::network::get)
        ;
        app.wrap(RequestTracing::default())
            .wrap(RequestMetrics::default())
    })
    .bind(addr)
    .unwrap_or_else(|e| panic!("failed to bind to {addr}: {e}"))
    .run()
    .await
    .map_err(Into::into)
}
