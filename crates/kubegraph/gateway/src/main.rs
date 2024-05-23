mod actix;
mod routes;
mod vm;

use anyhow::anyhow;
use kubegraph_api::vm::NetworkVirtualMachineExt;
use tokio::spawn;
use tracing::{error, info};

#[tokio::main]
async fn main() {
    ::ark_core::tracer::init_once();
    info!("Welcome to kubegraph!");

    let signal = ::ark_core::signal::FunctionSignal::default().trap_on_panic();
    if let Err(error) = signal.trap_on_sigint() {
        error!("{error}");
        return;
    }

    info!("Booting...");
    let vm = match self::vm::try_init().await {
        Ok(vm) => vm,
        Err(error) => {
            signal
                .panic(anyhow!("failed to init network virtual machine: {error}"))
                .await
        }
    };

    info!("Registering side workers...");
    let handlers = vec![spawn(crate::actix::loop_forever(vm.clone()))];

    info!("Ready");
    signal.wait_to_terminate().await;

    info!("Terminating...");
    for handler in handlers {
        handler.abort();
    }

    if let Err(error) = vm.close().await {
        error!("{error}");
    };

    signal.exit().await
}
