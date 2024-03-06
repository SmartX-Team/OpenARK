use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};

use anyhow::{anyhow, Result};
use tokio::time::sleep;
use tracing::info;

#[derive(Clone, Debug, Default)]
pub struct FunctionSignal {
    is_terminating: Arc<AtomicBool>,
}

impl FunctionSignal {
    pub fn trap_on_sigint(&self) -> Result<()> {
        let signal = self.clone();
        ::ctrlc::set_handler(move || signal.terminate())
            .map_err(|error| anyhow!("failed to set SIGINT handler: {error}"))
    }

    pub fn terminate(&self) {
        info!("Gracefully shutting down...");
        self.is_terminating.store(true, Ordering::SeqCst)
    }

    pub fn is_terminating(&self) -> bool {
        self.is_terminating.load(Ordering::SeqCst)
    }

    pub async fn wait_to_terminate(&self) {
        while !self.is_terminating() {
            sleep(Duration::from_millis(100)).await;
        }
    }
}
