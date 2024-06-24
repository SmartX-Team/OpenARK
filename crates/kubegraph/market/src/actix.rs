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
            .service(crate::routes::product::list)
            .service(crate::routes::product::list_price)
            .service(crate::routes::product::get)
            .service(crate::routes::product::put)
            .service(crate::routes::product::delete)
            .service(crate::routes::r#pub::list)
            .service(crate::routes::r#pub::get)
            .service(crate::routes::r#pub::put)
            .service(crate::routes::r#pub::delete)
            .service(crate::routes::sub::list)
            .service(crate::routes::sub::get)
            .service(crate::routes::sub::put)
            .service(crate::routes::sub::delete);
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
