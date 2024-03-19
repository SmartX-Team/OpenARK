mod actix;
mod connector;
mod routes;

use std::process::exit;

use clap::Parser;
use kubegraph_api::provider::NetworkGraphProvider;
use opentelemetry::global;
use tokio::spawn;
use tracing::{error, info};

#[cfg(feature = "local")]
type DefaultNetworkGraphProvider = ::kubegraph_provider_local::NetworkGraphProvider;

#[derive(Parser)]
struct Args {
    #[command(flatten)]
    connector: self::connector::ConnectorArgs,

    #[command(flatten)]
    graph: ::kubegraph_provider_local::NetworkGraphProviderArgs,
}

#[tokio::main]
async fn main() {
    ::ark_core::tracer::init_once();

    let Args { connector, graph } = Args::parse();

    let signal = ::ark_core::signal::FunctionSignal::default();
    if let Err(error) = signal.trap_on_sigint() {
        error!("{error}");
        return;
    }

    let graph = match DefaultNetworkGraphProvider::try_new(&graph).await {
        Ok(graph) => graph,
        Err(error) => {
            error!("failed to init network graph provider: {error}");
            exit(1);
        }
    };

    let handlers = vec![
        spawn(crate::actix::loop_forever(graph.clone())),
        spawn(crate::connector::loop_forever(graph.clone(), connector)),
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
