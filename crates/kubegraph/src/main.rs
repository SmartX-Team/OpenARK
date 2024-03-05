mod actix;
mod connector;
mod routes;

use std::process::exit;

use kubegraph_client::NetworkGraphClient;
use opentelemetry::global;
use tokio::spawn;
use tracing::{error, info};

#[tokio::main]
async fn main() {
    ::ark_core::tracer::init_once();

    let signal = ::dash_pipe_provider::FunctionSignal::default();
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

    let handlers = vec![
        spawn(crate::actix::loop_forever(graph.clone())),
        spawn(crate::connector::loop_forever(graph.clone())),
    ];
    signal.wait_to_terminate().await;

    info!("Terminating...");
    for handler in handlers {
        handler.abort();
    }

    info!("Terminated.");
    global::shutdown_tracer_provider();
}
