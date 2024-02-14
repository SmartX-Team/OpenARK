mod routes;

use std::net::SocketAddr;

use actix_cors::Cors;
use actix_web::{get, web::Data, App, HttpResponse, HttpServer, Responder};
use actix_web_opentelemetry::{RequestMetrics, RequestTracing};
use anyhow::Result;
use ark_core::{env::infer, tracer};
use dash_pipe_provider::PipeClient;
use opentelemetry::global;
use tracing::{instrument, Level};

#[instrument(level = Level::INFO)]
#[get("/")]
async fn index() -> impl Responder {
    HttpResponse::Ok().json("dash-broker-web")
}

#[instrument(level = Level::INFO)]
#[get("/health")]
async fn health() -> impl Responder {
    HttpResponse::Ok().json("healthy")
}

#[actix_web::main]
async fn main() {
    async fn try_main() -> Result<()> {
        // Initialize pipe
        let addr =
            infer::<_, SocketAddr>("BIND_ADDR").unwrap_or_else(|_| "0.0.0.0:80".parse().unwrap());
        let ctx = Data::new(PipeClient::try_default().await?);

        // Start web server
        HttpServer::new(move || {
            let cors = Cors::default()
                .allow_any_header()
                .allow_any_method()
                .allow_any_origin();

            let app = App::new().app_data(Data::clone(&ctx));
            let app = app
                .service(index)
                .service(health)
                .service(crate::routes::rest::get)
                .service(crate::routes::rest::post);
            app.wrap(cors)
                .wrap(RequestTracing::default())
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
