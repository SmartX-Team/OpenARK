mod actix;
mod reloader;
mod routes;
mod vm;

use std::process::exit;

use kubegraph_api::vm::NetworkVirtualMachine;
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

    let vm = match self::vm::try_init().await {
        Ok(vm) => vm,
        Err(error) => {
            error!("failed to init network virtual machine: {error}");
            exit(1);
        }
    };

    let handlers = vec![
        spawn(crate::actix::loop_forever(vm.clone())),
        spawn(crate::reloader::loop_forever(vm.clone())),
        spawn({
            let vm = vm.clone();
            async move { vm.loop_forever().await }
        }),
    ];

    info!("Ready");
    signal.wait_to_terminate().await;

    info!("Terminating...");
    for handler in handlers {
        handler.abort();
    }

    if let Err(error) = vm.close().await {
        error!("{error}");
    };

    info!("Terminated.");
    global::shutdown_tracer_provider();
}
