mod actix;
mod agent;
mod db;
mod histogram;
mod routes;

use anyhow::anyhow;
use ark_core::signal::FunctionSignal;
use kubegraph_api::component::NetworkComponentExt;
use tracing::{error, info};

#[::tokio::main]
async fn main() {
    ::ark_core::tracer::init_once();
    info!("Welcome to kubegraph market!");

    let signal = FunctionSignal::default().trap_on_panic();
    if let Err(error) = signal.trap_on_sigint() {
        error!("{error}");
        return;
    }

    info!("Booting...");
    let agent = match <self::agent::Agent as NetworkComponentExt>::try_default(&signal).await {
        Ok(agent) => agent,
        Err(error) => {
            signal
                .panic(anyhow!("failed to init kubegraph market agent: {error}"))
                .await
        }
    };

    info!("Registering market agent workers...");
    let handlers = agent.spawn_workers();

    info!("Ready");
    signal.wait_to_terminate().await;

    info!("Terminating...");
    for handler in handlers {
        handler.abort();
    }

    if let Err(error) = agent.close().await {
        error!("{error}");
    };

    signal.exit().await
}
