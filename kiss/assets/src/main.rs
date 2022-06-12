use std::sync::Arc;

use actix_web::{
    get,
    web::{Data, Path},
    App, HttpResponse, HttpServer, Responder,
};
use http_cache_reqwest::{CACacheManager, Cache, CacheMode, HttpCache};
use ipis::{log::info, logger};
use kiss_api::proxy::ProxyConfig;
use mime::OCTET_STREAM;
use reqwest::{
    header::{HeaderValue, CONTENT_TYPE},
    Client,
};
use reqwest_middleware::{ClientBuilder, ClientWithMiddleware};

#[get("/")]
async fn index() -> impl Responder {
    HttpResponse::Ok().json("kiss-proxy")
}

#[get("/health")]
async fn health() -> impl Responder {
    HttpResponse::Ok().json("healthy")
}

#[get("/{site}/{path:.*}")]
async fn resolve(
    client: Data<ClientWithMiddleware>,
    config: Data<Arc<ProxyConfig>>,
    path: Path<(String, String)>,
) -> impl Responder {
    let (site, path) = path.into_inner();

    match config.search(&site, &path) {
        Ok(path) => {
            info!("Downloading {path:?}...");
            match client.get(&path).send().await {
                Ok(res) => HttpResponse::Ok()
                    .content_type(
                        res.headers()
                            .get(CONTENT_TYPE)
                            .unwrap_or(&HeaderValue::from_static(OCTET_STREAM.as_str())),
                    )
                    .streaming(res.bytes_stream()),
                Err(e) => {
                    HttpResponse::Forbidden().body(format!("failed to find the url {path:?}: {e}"))
                }
            }
        }
        Err(e) => HttpResponse::Forbidden().body(e.to_string()),
    }
}

#[actix_web::main]
async fn main() {
    async fn try_main() -> ::ipis::core::anyhow::Result<()> {
        // Initialize config
        let addr = "0.0.0.0:8888";
        let config = Arc::new(ProxyConfig::load().await?);

        // Initialize cache client
        let client = ClientBuilder::new(Client::new())
            .with(Cache(HttpCache {
                mode: CacheMode::Default,
                manager: CACacheManager::default(),
                options: None,
            }))
            .build();

        // Start web server
        HttpServer::new(move || {
            App::new()
                .app_data(Data::new(client.clone()))
                .app_data(Data::new(config.clone()))
                .service(index)
                .service(health)
                .service(resolve)
        })
        .bind(addr)
        .unwrap_or_else(|e| panic!("failed to bind to {addr}: {e}"))
        .shutdown_timeout(5)
        .run()
        .await
        .map_err(Into::into)
    }

    logger::init_once();
    try_main().await.expect("running a server")
}
