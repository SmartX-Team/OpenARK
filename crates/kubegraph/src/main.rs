mod connector;

use std::process::exit;

use kubegraph_client::NetworkGraphClient;
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

    let graph = match NetworkGraphClient::try_default().await {
        Ok(graph) => graph,
        Err(error) => {
            error!("failed to init network graph client: {error}");
            exit(1);
        }
    };

    let handler = spawn(crate::connector::loop_forever(graph.clone()));

    info!("Ready");
    signal.wait_to_terminate().await;

    info!("Terminating...");
    handler.abort();

    if let Err(error) = graph.close().await {
        error!("{error}");
    };

    info!("Terminated.");
    global::shutdown_tracer_provider();
}
