use std::net::SocketAddr;

use actix_web::{
    get, post,
    web::{Data, Payload},
    App, HttpRequest, HttpResponse, HttpServer, Responder,
};
use ipis::{env::infer, log::warn, logger};
use netai_api::session::Session;

#[get("/")]
async fn index() -> impl Responder {
    HttpResponse::Ok().json("netai-endpoint")
}

#[get("/health")]
async fn health() -> impl Responder {
    HttpResponse::Ok().json("healthy")
}

#[post("/")]
async fn run(session: Data<Session>, req: HttpRequest, payload: Payload) -> impl Responder {
    session.run_web(req, payload.into_inner()).await
}

mod metadata {
    use super::*;

    #[get("/LICENSE")]
    async fn license(session: Data<Session>) -> impl Responder {
        text_to_response(session.model().get_license().await)
    }

    #[get("/README.md")]
    async fn readme(session: Data<Session>) -> impl Responder {
        text_to_response(session.model().get_readme().await)
    }

    fn text_to_response(result: ::ipis::core::anyhow::Result<Option<String>>) -> impl Responder {
        match result {
            Result::Ok(Some(value)) => HttpResponse::Ok().body(value),
            Result::Ok(None) => HttpResponse::NotFound().finish(),
            Result::Err(e) => {
                warn!("failed to get metadata: {e}");
                HttpResponse::InternalServerError().finish()
            }
        }
    }
}

#[actix_web::main]
async fn main() {
    async fn try_main() -> ::ipis::core::anyhow::Result<()> {
        // Initialize config
        let addr =
            infer::<_, SocketAddr>("BIND_ADDR").unwrap_or_else(|_| "0.0.0.0:80".parse().unwrap());
        let session = Data::new(Session::try_default().await?);

        // Start web server
        HttpServer::new(move || {
            App::new()
                .app_data(Data::clone(&session))
                .service(index)
                .service(health)
                .service(run)
                .service(metadata::license)
                .service(metadata::readme)
        })
        .bind(addr)
        .unwrap_or_else(|e| panic!("failed to bind to {addr}: {e}"))
        .shutdown_timeout(30 * 60)
        .run()
        .await
        .map_err(Into::into)
    }

    logger::init_once();
    try_main().await.expect("running a server")
}
