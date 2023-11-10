mod routes;

use std::net::SocketAddr;

use actix_web::{get, web::Data, App, HttpResponse, HttpServer, Responder};
use actix_web_opentelemetry::{RequestMetrics, RequestTracing};
use anyhow::Result;
use ark_core::{env::infer, tracer};
use kube::Client;
use opentelemetry::global;

#[get("/")]
async fn index() -> impl Responder {
    HttpResponse::Ok().json("dash-router")
}

#[get("/health")]
async fn health() -> impl Responder {
    HttpResponse::Ok().json("healthy")
}

#[actix_web::main]
async fn main() {
    async fn try_main() -> Result<()> {
        // Initialize kubernetes client
        let addr =
            infer::<_, SocketAddr>("BIND_ADDR").unwrap_or_else(|_| "0.0.0.0:80".parse().unwrap());
        let client = Data::new(Client::try_default().await?);

        // Start web server
        HttpServer::new(move || {
            let app = App::new().app_data(Data::clone(&client));
            let app = app.service(index).service(health);
            app.wrap(RequestTracing::default())
                .wrap(RequestMetrics::default())
        })
        .bind(addr)
        .unwrap_or_else(|e| panic!("failed to bind to {addr}: {e}"))
        .run()
        .await
        .map_err(Into::into)
    }

    tracer::init_once();
    try_main().await.expect("running a server");
    global::shutdown_tracer_provider()
}
