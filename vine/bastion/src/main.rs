mod routes;

use std::net::SocketAddr;

use actix_web::{get, web::Data, App, HttpResponse, HttpServer, Responder};
use ipis::{core::anyhow::Result, env::infer, logger};
use vine_api::kube::Client;

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
            App::new()
                .app_data(Data::clone(&client))
                .service(health)
                .service(crate::routes::auth::get)
                .service(crate::routes::r#box::login::get)
                .service(crate::routes::welcome::get)
        })
        .bind(addr)
        .unwrap_or_else(|e| panic!("failed to bind to {addr}: {e}"))
        .run()
        .await
        .map_err(Into::into)
    }

    logger::init_once();
    try_main().await.expect("running a server")
}
