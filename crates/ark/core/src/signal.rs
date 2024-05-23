use std::{
    panic,
    process::exit,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};

use anyhow::{anyhow, Error, Result};
use opentelemetry::global;
use tokio::time::sleep;
use tracing::{error, info, warn};

#[derive(Clone, Debug, Default)]
pub struct FunctionSignal {
    is_erroneous: Arc<AtomicBool>,
    is_terminating: Arc<AtomicBool>,
}

impl FunctionSignal {
    pub fn trap_on_panic(self) -> Self {
        info!("Registering panic hook...");

        let default_panic = panic::take_hook();
        panic::set_hook(Box::new({
            let signal = self.clone();

            move |info| {
                error!("Panicked!");

                // show Rust's native panic message
                default_panic(info);

                // trigger to gracefully terminate
                signal.terminate_on_panic()
            }
        }));

        self
    }

    pub fn trap_on_sigint(&self) -> Result<()> {
        let signal = self.clone();
        ::ctrlc::set_handler(move || signal.terminate())
            .map_err(|error| anyhow!("failed to set SIGINT handler: {error}"))
    }

    pub fn terminate(&self) {
        warn!("Gracefully shutting down...");
        self.is_terminating.store(true, Ordering::SeqCst)
    }

    pub fn terminate_on_panic(&self) {
        self.is_erroneous.store(true, Ordering::SeqCst);
        self.terminate()
    }

    pub fn is_terminating(&self) -> bool {
        self.is_terminating.load(Ordering::SeqCst)
    }

    pub async fn panic(&self, error: impl Into<Error>) -> ! {
        error!("{error}", error = error.into());

        self.terminate_on_panic();
        self.exit().await
    }

    pub async fn exit(&self) -> ! {
        self.wait_to_terminate().await;

        // postprocess
        info!("Terminated.");
        global::shutdown_tracer_provider();

        let code = self.is_erroneous.load(Ordering::SeqCst).into();
        exit(code)
    }

    pub async fn wait_to_terminate(&self) {
        while !self.is_terminating() {
            sleep(Duration::from_millis(100)).await;
        }
    }
}
