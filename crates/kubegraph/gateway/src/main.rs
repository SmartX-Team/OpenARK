mod actix;
mod connector;
mod db;
mod reloader;
mod routes;
mod vm;

use std::process::exit;

use kubegraph_api::db::NetworkGraphDB;
use opentelemetry::global;
use tokio::spawn;
use tracing::{error, info};

#[tokio::main]
async fn main() {
    ::ark_core::tracer::init_once();

    let signal = ::ark_core::signal::FunctionSignal::default();
    if let Err(error) = signal.trap_on_sigint() {
        error!("{error}");
        return;
    }

    let graph = match self::db::NetworkGraphDB::try_default().await {
        Ok(graph) => graph,
        Err(error) => {
            error!("failed to init network graph DB: {error}");
            exit(1);
        }
    };

    let handlers = vec![
        spawn(crate::actix::loop_forever(graph.clone())),
        spawn(crate::connector::loop_forever(graph.clone())),
        spawn(crate::reloader::loop_forever(graph.clone())),
        spawn(crate::vm::loop_forever(graph.clone())),
    ];

    info!("Ready");
    signal.wait_to_terminate().await;

    info!("Terminating...");
    for handler in handlers {
        handler.abort();
    }

    if let Err(error) = graph.close().await {
        error!("{error}");
    };

    info!("Terminated.");
    global::shutdown_tracer_provider();
}
