use std::net::SocketAddr;

use actix_web::{get, middleware, App, HttpResponse, HttpServer, Responder};
use actix_web_opentelemetry::{RequestMetrics, RequestTracing};
use anyhow::{anyhow, Result};
use ark_core::{env::infer, signal::FunctionSignal};
use futures::TryFutureExt;
use tracing::{error, info, instrument, Level};

#[instrument(level = Level::INFO)]
#[get("/_health")]
async fn health() -> impl Responder {
    HttpResponse::Ok().json("healthy")
}

pub async fn loop_forever(signal: FunctionSignal) {
    match try_loop_forever().await {
        Ok(()) => signal.terminate(),
        Err(error) => {
            error!("failed to operate http server: {error}");
            signal.terminate_on_panic()
        }
    }
}

async fn try_loop_forever() -> Result<()> {
    info!("Starting http server...");

    // Initialize pipe
    let addr =
        infer::<_, SocketAddr>("BIND_ADDR").unwrap_or_else(|_| "0.0.0.0:80".parse().unwrap());

    // Create a http server
    let server = HttpServer::new(move || {
        let app = App::new();
        let app = app.service(health).service(crate::routes::post);
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
