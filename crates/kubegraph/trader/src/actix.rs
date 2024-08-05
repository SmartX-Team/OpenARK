use actix_web::{
    get, middleware, post,
    web::{Data, Json},
    App, HttpResponse, HttpServer, Responder,
};
use actix_web_opentelemetry::{RequestMetrics, RequestTracing};
use anyhow::{anyhow, Result};
use ark_core::result::Result as HttpResult;
use futures::TryFutureExt;
use kubegraph_api::market::transaction::TransactionReceipt;
use tracing::{error, info, instrument, Level};

use crate::db::NetworkTraderDB;

#[instrument(level = Level::INFO)]
#[get("/")]
async fn home() -> impl Responder {
    HttpResponse::Ok().json(env!("CARGO_PKG_NAME"))
}

#[instrument(level = Level::INFO)]
#[get("/_health")]
async fn health() -> impl Responder {
    HttpResponse::Ok().json("healthy")
}

#[instrument(level = Level::INFO, skip(db))]
#[post("/")]
async fn handle(db: Data<NetworkTraderDB>, receipt: Json<TransactionReceipt>) -> impl Responder {
    // TODO: to be implemented
    HttpResponse::Ok().json(HttpResult::Ok(receipt.0))
}

pub async fn loop_forever(db: NetworkTraderDB) {
    match try_loop_forever(&db).await {
        Ok(()) => db.signal.terminate(),
        Err(error) => {
            error!("failed to operate http server: {error}");
            db.signal.terminate_on_panic()
        }
    }
}

async fn try_loop_forever(db: &NetworkTraderDB) -> Result<()> {
    info!("Starting trader webhook http server...");

    // Initialize pipe
    let addr = db.webhook_addr();

    let db = Data::new(db.clone());

    // Create a http server
    let server = HttpServer::new(move || {
        let app = App::new().app_data(Data::clone(&db));
        let app = app.service(home).service(health).service(handle);
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
