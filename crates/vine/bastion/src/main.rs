#![recursion_limit = "256"]

mod routes;

use std::net::SocketAddr;

use actix_web::{get, middleware, web::Data, App, HttpResponse, HttpServer, Responder};
use actix_web_opentelemetry::{RequestMetrics, RequestTracing};
use anyhow::Result;
use ark_core::{env::infer, tracer};
use kube::Client;
use opentelemetry::global;
use tera::Tera;
use tracing::{instrument, Level};

#[instrument(level = Level::INFO)]
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

        // Initialize tera
        let mut tera = Tera::default();
        tera.add_raw_template(
            crate::routes::r#box::login::TEMPLATE_NAME,
            crate::routes::r#box::login::TEMPLATE_CONTENT,
        )?;
        let tera = Data::new(tera);

        // Start web server
        HttpServer::new(move || {
            let app = App::new()
                .app_data(Data::clone(&client))
                .app_data(Data::clone(&tera));
            let app = app
                .service(health)
                .service(crate::routes::auth::get)
                .service(crate::routes::r#box::login::get)
                .service(crate::routes::install_os::get)
                .service(crate::routes::reserved::get)
                .service(crate::routes::welcome::get);
            app.wrap(middleware::NormalizePath::new(
                middleware::TrailingSlash::Trim,
            ))
            .wrap(RequestMetrics::default())
            .wrap(RequestTracing::default())
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
