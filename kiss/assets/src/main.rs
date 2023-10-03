mod proxy;

use std::net::SocketAddr;

use actix_web::{
    get,
    web::{BytesMut, Data, Path, Payload},
    App, HttpRequest, HttpResponse, HttpServer, Responder,
};
use ark_core::{env::infer, tracer};
use futures::StreamExt;
use http_cache_reqwest::{CACacheManager, Cache, CacheMode, HttpCache};
use tracing::{info, warn};
use reqwest::{
    header::{HeaderName, HOST, ORIGIN, REFERER},
    Client, Method,
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

async fn resolve(
    req: HttpRequest,
    method: Method,
    mut payload: Payload,
    client: Data<ClientWithMiddleware>,
    config: Data<crate::proxy::ProxyConfig>,
    path: Path<(String, String)>,
) -> impl Responder {
    let (site, path) = path.into_inner();

    // payload is a stream of Bytes objects
    let mut body = BytesMut::new();
    while let Some(chunk) = payload.next().await {
        const MAX_SIZE: usize = 262_144; // max payload size is 256k

        match chunk {
            // limit max size of in-memory payload
            Ok(chunk) if (body.len() + chunk.len()) <= MAX_SIZE => {
                body.extend_from_slice(&chunk);
            }
            Ok(_) => {
                return HttpResponse::Forbidden().body("Overflowed");
            }
            Err(e) => {
                warn!("failed to get bytes: {e}");
                return HttpResponse::Forbidden().body("Err");
            }
        }
    }

    match config.search(&site, &path, req.query_string()) {
        Ok(path) => {
            // TODO: replace `body.to_vec()` into `payload` directly
            let mut builder = client.request(method.clone(), &path).body(body.to_vec());
            for (key, value) in req.headers() {
                if ![
                    HOST,
                    ORIGIN,
                    REFERER,
                    HeaderName::from_static("x-forwarded-host"),
                ]
                .contains(key)
                {
                    builder = builder.header(key, value);
                }
            }

            match builder.send().await {
                Ok(res) => {
                    let content_length = res.content_length();
                    let status = res.status();
                    info!("[{method}] {path:?} => {status}");

                    let mut builder = HttpResponse::build(status);
                    for (key, value) in res.headers() {
                        builder.append_header((key, value));
                    }
                    if let Some(content_length) = content_length {
                        builder.no_chunking(content_length);
                    }

                    builder.streaming(res.bytes_stream())
                }
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
    async fn try_main() -> ::anyhow::Result<()> {
        // Initialize config
        let addr =
            infer::<_, SocketAddr>("BIND_ADDR").unwrap_or_else(|_| "0.0.0.0:80".parse().unwrap());
        let config = Data::new(crate::proxy::ProxyConfig::load().await?);

        // Initialize client
        let client = Data::new({
            let mut builder = ClientBuilder::new(Client::new());
            if infer::<_, bool>("KISS_ASSETS_USE_CACHE").unwrap_or_default() {
                builder = builder.with(Cache(HttpCache {
                    mode: CacheMode::Default,
                    manager: CACacheManager {
                        path: infer("KISS_ASSETS_CACHE_DIR")
                            .unwrap_or_else(|_| "./http-cacache".into()),
                    },
                    options: None,
                }));
            }
            builder.build()
        });

        // Start web server
        HttpServer::new(move || {
            App::new()
                .app_data(Data::clone(&client))
                .app_data(Data::clone(&config))
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

    tracer::init_once();
    try_main().await.expect("running a server")
}
