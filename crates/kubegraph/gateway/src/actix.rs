use std::net::SocketAddr;

use actix_web::{get, middleware, web::Data, App, HttpResponse, HttpServer, Responder};
use actix_web_opentelemetry::{RequestMetrics, RequestTracing};
use anyhow::{anyhow, Result};
use ark_core::{env::infer, signal::FunctionSignal};
use futures::TryFutureExt;
use kubegraph_api::{
    graph::NetworkGraphDB,
    vm::{NetworkFallbackPolicy, NetworkVirtualMachine},
};
use tokio::time::sleep;
use tracing::{error, info, instrument, warn, Level};

#[instrument(level = Level::INFO)]
#[get("/_health")]
async fn health() -> impl Responder {
    HttpResponse::Ok().json("healthy")
}

pub async fn loop_forever(signal: FunctionSignal, vm: impl NetworkVirtualMachine) {
    loop {
        if let Err(error) = try_loop_forever(&vm).await {
            error!("failed to operate http server: {error}");

            match vm.fallback_policy() {
                NetworkFallbackPolicy::Interval { interval } => {
                    warn!("restarting http server in {interval:?}...");
                    sleep(interval).await;
                    info!("Restarted http server");
                }
                NetworkFallbackPolicy::Never => {
                    signal.terminate_on_panic();
                    break;
                }
            }
        }
    }
}

async fn try_loop_forever(vm: &impl NetworkVirtualMachine) -> Result<()> {
    info!("Starting http server...");

    // Initialize pipe
    let addr =
        infer::<_, SocketAddr>("BIND_ADDR").unwrap_or_else(|_| "0.0.0.0:80".parse().unwrap());

    let graph_db: Box<dyn Send + NetworkGraphDB> = Box::new(vm.graph_db().clone());
    let graph_db = Data::new(graph_db);

    // Create a http server
    let server = HttpServer::new(move || {
        let app = App::new().app_data(Data::clone(&graph_db));
        let app = app
            .service(health)
            .service(crate::routes::graph::get)
            .service(crate::routes::graph::post);
        app.wrap(middleware::NormalizePath::new(
            middleware::TrailingSlash::Trim,
        ))
        .wrap(RequestTracing::default())
        .wrap(RequestMetrics::default())
    })
    .bind(addr)
    .map_err(|error| anyhow!("failed to bind to {addr}: {error}"))?;

    // Start http server
    server.run().map_err(Into::into).await
}
