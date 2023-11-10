mod routes;

use std::net::SocketAddr;

use actix_cors::Cors;
use actix_web::{get, web::Data, App, HttpResponse, HttpServer, Responder};
use actix_web_opentelemetry::{RequestMetrics, RequestTracing};
use anyhow::Result;
use ark_core::{env::infer, tracer};
use kube::Client;
use opentelemetry::global;
use tracing::{instrument, Level};

#[instrument(level = Level::INFO)]
#[get("/")]
async fn index() -> impl Responder {
    HttpResponse::Ok().json("dash-gateway")
}

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

        // Start web server
        HttpServer::new(move || {
            let cors = Cors::default()
                .allow_any_header()
                .allow_any_method()
                .allow_any_origin();

            let app = App::new().app_data(Data::clone(&client));
            let app = app
                .service(index)
                .service(health)
                .service(crate::routes::task::get)
                .service(crate::routes::task::get_list)
                .service(crate::routes::job::batch::post)
                .service(crate::routes::job::single::delete)
                .service(crate::routes::job::single::get)
                .service(crate::routes::job::single::get_list)
                .service(crate::routes::job::single::get_list_with_task_name)
                .service(crate::routes::job::single::get_stream_logs)
                .service(crate::routes::job::single::post)
                .service(crate::routes::job::single::post_restart)
                .service(crate::routes::model::get)
                .service(crate::routes::model::get_task_list)
                .service(crate::routes::model::get_item)
                .service(crate::routes::model::get_item_list)
                .service(crate::routes::model::get_list);
            let app = ::vine_plugin::register(app);
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
