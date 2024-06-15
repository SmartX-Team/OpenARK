mod reloader;

use std::{process::exit, sync::Arc, time::Duration};

use anyhow::Result;
use ark_core::env::infer;
use async_trait::async_trait;
use hickory_server::{
    authority::Catalog,
    server::{Request, RequestHandler, ResponseHandler, ResponseInfo},
    ServerFuture,
};
use kube::Client;
use tokio::{
    net::{TcpListener, UdpSocket},
    spawn,
    sync::RwLock,
};
use tracing::{error, info};

#[derive(Clone)]
struct Handler {
    catalog: Arc<RwLock<Catalog>>,
}

impl Handler {
    async fn try_default() -> Result<Self> {
        Ok(Self {
            catalog: Arc::default(),
        })
    }
}

#[async_trait]
impl RequestHandler for Handler {
    async fn handle_request<R>(&self, request: &Request, response_handle: R) -> ResponseInfo
    where
        R: ResponseHandler,
    {
        self.catalog
            .read()
            .await
            .handle_request(request, response_handle)
            .await
        // let mut header = Header::new();
        // header.set_message_type(MessageType::Response);
        // header.into()
    }
}

async fn build_server(handler: Handler) -> Result<ServerFuture<Handler>> {
    let mut server = ServerFuture::new(handler);

    let addr: String = infer("BIND_ADDR").unwrap_or_else(|_| "0.0.0.0:53".into());

    let socket = UdpSocket::bind(&addr).await?;
    server.register_socket(socket);

    let listener = TcpListener::bind(&addr).await?;
    let timeout = Duration::from_secs(30);
    server.register_listener(listener, timeout);

    Ok(server)
}

#[::tokio::main]
async fn main() {
    ::ark_core::tracer::init_once();
    info!("Welcome to kiss-dns!");

    info!("Booting...");
    let handler = match Handler::try_default().await {
        Ok(handler) => handler,
        Err(error) => {
            error!("failed to init handler: {error}");
            exit(255)
        }
    };
    let mut server = match build_server(handler.clone()).await {
        Ok(server) => server,
        Err(error) => {
            error!("failed to init server: {error}");
            exit(255)
        }
    };

    let kube = match Client::try_default().await {
        Ok(kube) => kube,
        Err(error) => {
            error!("failed to init kubernetes client: {error}");
            exit(255)
        }
    };

    let ctx = match self::reloader::ReloaderContext::try_default().await {
        Ok(ctx) => ctx,
        Err(error) => {
            error!("failed to init reloader context: {error}");
            exit(255)
        }
    };

    info!("Registering side workers...");
    let workers = vec![spawn(self::reloader::loop_forever(ctx, kube, handler))];

    info!("Ready");
    let result = server.block_until_done().await;

    info!("Terminating...");
    for worker in workers {
        worker.abort();
    }

    if let Err(error) = result {
        error!("{error}");
        exit(1)
    };
}
