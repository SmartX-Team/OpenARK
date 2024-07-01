extern crate kubegraph_market_entity as entity;
extern crate kubegraph_market_migration as migration;

mod actix;
mod db;
mod routes;

use anyhow::anyhow;
use ark_core::signal::FunctionSignal;
use kubegraph_api::component::NetworkComponentExt;
use tokio::{spawn, task::JoinHandle};
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
    let db = match <self::db::Database as NetworkComponentExt>::try_default(&signal).await {
        Ok(db) => db,
        Err(error) => {
            signal
                .panic(anyhow!("failed to init kubegraph market db: {error}"))
                .await
        }
    };

    info!("Registering market db workers...");
    let handlers = spawn_workers(&db);

    info!("Ready");
    signal.wait_to_terminate().await;

    info!("Terminating...");
    for handler in handlers {
        handler.abort();
    }

    if let Err(error) = db.close().await {
        error!("{error}");
    };

    signal.exit().await
}

fn spawn_workers(db: &self::db::Database) -> Vec<JoinHandle<()>> {
    vec![spawn(crate::actix::loop_forever(db.clone()))]
}
