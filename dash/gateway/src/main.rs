mod routes;

use std::net::SocketAddr;

use actix_cors::Cors;
use actix_web::{get, web::Data, App, HttpResponse, HttpServer, Responder};
use anyhow::Result;
use ark_core::{env::infer, logger};
use kube::Client;

#[get("/")]
async fn index() -> impl Responder {
    HttpResponse::Ok().json("dash-gateway")
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
            let cors = Cors::default()
                .allow_any_header()
                .allow_any_method()
                .allow_any_origin();

            App::new()
                .app_data(Data::clone(&client))
                .service(index)
                .service(health)
                .service(crate::routes::function::get)
                .service(crate::routes::function::get_list)
                .service(crate::routes::job::batch::post)
                .service(crate::routes::job::single::delete)
                .service(crate::routes::job::single::get)
                .service(crate::routes::job::single::get_list)
                .service(crate::routes::job::single::get_list_with_function_name)
                .service(crate::routes::job::single::get_stream_logs)
                .service(crate::routes::job::single::post)
                .service(crate::routes::job::single::post_restart)
                .service(crate::routes::model::get)
                .service(crate::routes::model::get_function_list)
                .service(crate::routes::model::get_item)
                .service(crate::routes::model::get_item_list)
                .service(crate::routes::model::get_list)
                .service(crate::routes::user::get)
                .wrap(cors)
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
