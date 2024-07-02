use std::{net::SocketAddr, sync::Arc};

use actix_web::{
    get,
    http::Method,
    middleware,
    web::{resource, route, Data, Json},
    App, HttpResponse, HttpServer, Resource, Responder,
};
use actix_web_opentelemetry::{RequestMetrics, RequestTracing};
use anyhow::{anyhow, Result};
use ark_core::{env::infer, result::Result as HttpResult, signal::FunctionSignal};
use clap::Parser;
use futures::TryFutureExt;
use tokio::time::sleep;
use tracing::{error, info, instrument, warn, Level};

use crate::{component::NetworkComponent, vm::NetworkFallbackPolicy};

use super::{super::call::FunctionCallRequest, NetworkFunctionService, NetworkFunctionServiceExt};

#[instrument(level = Level::INFO)]
#[get("/_health")]
async fn health() -> impl Responder {
    HttpResponse::Ok().json("healthy")
}

#[instrument(level = Level::INFO, skip(function, request))]
async fn handler<F>(function: Data<F>, Json(request): Json<FunctionCallRequest>) -> impl Responder
where
    F: NetworkFunctionService,
{
    HttpResponse::Ok().json(HttpResult::from(
        function.into_inner().as_ref().handle(request).await,
    ))
}

pub(super) async fn loop_forever<F>(signal: FunctionSignal, function: Arc<F>)
where
    F: 'static + NetworkFunctionServiceExt,
    <F as NetworkComponent>::Args: Parser,
{
    loop {
        if let Err(error) = try_loop_forever(&function).await {
            error!("failed to operate http server: {error}");

            match function.fallback_policy() {
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

async fn try_loop_forever<F>(function: &Arc<F>) -> Result<()>
where
    F: 'static + NetworkFunctionServiceExt,
    <F as NetworkComponent>::Args: Parser,
{
    info!("Starting http server...");

    // Initialize pipe
    let addr =
        infer::<_, SocketAddr>("BIND_ADDR").unwrap_or_else(|_| "0.0.0.0:80".parse().unwrap());

    let function = Data::from(function.clone());

    // Create a http server
    let server = HttpServer::new(move || {
        let app = App::new().app_data(Data::clone(&function));
        let app = app.service(health).service(build_route::<F>("/"));
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

fn build_route<F>(path: impl ToString) -> Resource
where
    F: 'static + NetworkFunctionService,
{
    resource(path.to_string()).route(route().method(Method::POST).to(handler::<F>))
}
