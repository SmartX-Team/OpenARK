mod agent;
mod solver;

use anyhow::anyhow;
use ark_core::signal::FunctionSignal;
use kubegraph_api::component::NetworkComponentExt;
use tokio::spawn;
use tracing::{error, info};

#[::tokio::main]
async fn main() {
    ::ark_core::tracer::init_once();
    info!("Welcome to kubegraph market solver!");

    let signal = FunctionSignal::default().trap_on_panic();
    if let Err(error) = signal.trap_on_sigint() {
        error!("{error}");
        return;
    }

    info!("Booting...");
    let solver = match <self::agent::MarketAgent as NetworkComponentExt>::try_default(&signal).await
    {
        Ok(solver) => solver,
        Err(error) => {
            signal
                .panic(anyhow!("failed to init kubegraph market solver: {error}"))
                .await
        }
    };

    info!("Registering market worker...");
    let handler = spawn(solver.loop_forever());

    info!("Ready");
    signal.wait_to_terminate().await;

    info!("Terminating...");
    handler.abort();
    signal.exit().await
}
