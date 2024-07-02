mod actix;
mod routes;

use ark_core::signal::FunctionSignal;
use tokio::{spawn, task::JoinHandle};
use tracing::{error, info};

#[::tokio::main]
async fn main() {
    ::ark_core::tracer::init_once();
    info!("Welcome to kubegraph market blackhole function!");

    let signal = FunctionSignal::default().trap_on_panic();
    if let Err(error) = signal.trap_on_sigint() {
        error!("{error}");
        return;
    }

    info!("Booting...");
    // nothing to do...

    info!("Registering market blackhole function workers...");
    let handlers = spawn_workers(&signal);

    info!("Ready");
    signal.wait_to_terminate().await;

    info!("Terminating...");
    for handler in handlers {
        handler.abort();
    }
    signal.exit().await
}

fn spawn_workers(signal: &FunctionSignal) -> Vec<JoinHandle<()>> {
    vec![spawn(crate::actix::loop_forever(signal.clone()))]
}
