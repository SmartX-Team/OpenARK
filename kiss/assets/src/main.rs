use std::{net::SocketAddr, sync::Arc};

use actix_web::{
    get,
    http::StatusCode,
    web::{Data, Path},
    App, HttpRequest, HttpResponse, HttpServer, Responder,
};
use ipis::{env::infer, log::warn, logger};
use kiss_api::proxy::ProxyConfig;

#[get("/")]
async fn index() -> impl Responder {
    HttpResponse::Ok().json("kiss-proxy")
}

#[get("/health")]
async fn health() -> impl Responder {
    HttpResponse::Ok().json("healthy")
}

async fn resolve(
    req: HttpRequest,
    config: Data<Arc<ProxyConfig>>,
    path: Path<(String, String)>,
) -> impl Responder {
    let (site, path) = path.into_inner();

    match config.search(&site, &path, req.query_string()) {
        Ok(path) => Ok(::actix_web_lab::web::Redirect::to(path)
            .using_status_code(StatusCode::MOVED_PERMANENTLY) // iPXE supported
            .respond_to(&req)),
        Err(e) => {
            warn!("Failed to parse path: {e}");
            HttpResponse::Forbidden().message_body(())
        }
    }
}

#[actix_web::main]
async fn main() {
    async fn try_main() -> ::ipis::core::anyhow::Result<()> {
        // Initialize config
        let addr =
            infer::<_, SocketAddr>("BIND_ADDR").unwrap_or_else(|_| "0.0.0.0:80".parse().unwrap());
        let config = Arc::new(ProxyConfig::load().await?);

        // Start web server
        HttpServer::new(move || {
            App::new()
                .app_data(Data::new(config.clone()))
                .service(index)
                .service(health)
                .route("/{site}/{path:.*}", ::actix_web::web::route().to(resolve))
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
