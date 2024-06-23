use std::net::SocketAddr;

use actix_web::{get, middleware, web::Data, App, HttpResponse, HttpServer, Responder};
use actix_web_opentelemetry::{RequestMetrics, RequestTracing};
use anyhow::{anyhow, Result};
use ark_core::env::infer;
use futures::TryFutureExt;
use tracing::{error, info, instrument, Level};

use crate::agent::Agent;

#[instrument(level = Level::INFO)]
#[get("/_health")]
async fn health() -> impl Responder {
    HttpResponse::Ok().json("healthy")
}

pub async fn loop_forever(agent: Agent) {
    match try_loop_forever(&agent).await {
        Ok(()) => agent.signal.terminate(),
        Err(error) => {
            error!("failed to operate http server: {error}");
            agent.signal.terminate_on_panic()
        }
    }
}

async fn try_loop_forever(agent: &Agent) -> Result<()> {
    info!("Starting http server...");

    // Initialize pipe
    let addr =
        infer::<_, SocketAddr>("BIND_ADDR").unwrap_or_else(|_| "0.0.0.0:80".parse().unwrap());

    let agent = Data::new(agent.clone());

    // Create a http server
    let server = HttpServer::new(move || {
        let app = App::new().app_data(Data::clone(&agent));
        let app = app
            .service(health)
            .service(crate::routes::problem::list)
            .service(crate::routes::problem::get)
            .service(crate::routes::problem::post)
            .service(crate::routes::solver::list)
            .service(crate::routes::solver::get)
            .service(crate::routes::solver::post);
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
