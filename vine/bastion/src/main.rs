use std::net::SocketAddr;

use actix_web::{get, web::Data, App, HttpRequest, HttpResponse, HttpServer, Responder};
use ipis::{core::anyhow::Result, env::infer, log::error, logger};
use vine_api::{kube::Client, user_auth::UserAuthResponse};

#[get("/")]
async fn index() -> impl Responder {
    HttpResponse::Ok().json("vine-bastion")
}

#[get("/health")]
async fn health() -> impl Responder {
    HttpResponse::Ok().json("healthy")
}

#[get("/auth")]
async fn get_auth(request: HttpRequest, client: Data<Client>) -> impl Responder {
    match ::vine_rbac::auth::execute(request, client).await {
        Ok(response) if matches!(response, UserAuthResponse::Accept { .. }) => {
            HttpResponse::Ok().json(response)
        }
        Ok(response) => HttpResponse::Forbidden().json(response),
        Err(e) => {
            error!("failed to register a client: {e}");
            HttpResponse::InternalServerError().finish()
        }
    }
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
                .service(index)
                .service(health)
                .service(get_auth)
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
