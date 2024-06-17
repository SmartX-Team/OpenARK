mod actix;

use std::sync::Arc;

use anyhow::{anyhow, Result};
use ark_core::signal::FunctionSignal;
use async_trait::async_trait;
use clap::Parser;
use tokio::spawn;
use tracing::{error, info, instrument, warn, Level};

use crate::{
    component::{NetworkComponent, NetworkComponentExt},
    vm::NetworkFallbackPolicy,
};

use super::call::FunctionCallRequest;

#[async_trait]
pub trait NetworkFunctionServiceExt
where
    Self: NetworkComponentExt + NetworkFunctionService,
    <Self as NetworkComponent>::Args: Parser,
{
    async fn main()
    where
        Self: 'static + Sized,
    {
        <Self as NetworkFunctionServiceExt>::main_with_handlers(|_, _| vec![]).await
    }

    async fn main_with_handlers<F>(handlers: F)
    where
        Self: 'static + Sized,
        F: Send + FnOnce(&FunctionSignal, &Arc<Self>) -> Vec<::tokio::task::JoinHandle<()>>,
    {
        ::ark_core::tracer::init_once();
        info!("Welcome to kubegraph function service!");

        let signal = FunctionSignal::default().trap_on_panic();
        if let Err(error) = signal.trap_on_sigint() {
            error!("{error}");
            return;
        }

        info!("Booting...");
        let function = match <Self as NetworkComponentExt>::try_default(&signal).await {
            Ok(function) => Arc::new(function),
            Err(error) => {
                signal
                    .panic(anyhow!("failed to init function service: {error}"))
                    .await
            }
        };

        info!("Creating http server...");
        let handler_http_server =
            spawn(self::actix::loop_forever(signal.clone(), function.clone()));

        info!("Registering side workers...");
        let mut handlers = handlers(&signal, &function);
        handlers.push(handler_http_server);

        info!("Ready");
        signal.wait_to_terminate().await;

        info!("Terminating...");
        for handler in handlers {
            handler.abort();
        }

        if let Err(error) = function.close() {
            error!("{error}");
        };

        signal.exit().await
    }
}

#[async_trait]
impl<T> NetworkFunctionServiceExt for T
where
    Self: NetworkComponentExt + NetworkFunctionService,
    <Self as NetworkComponent>::Args: Parser,
{
}

#[async_trait]
pub trait NetworkFunctionService
where
    Self: Send + Sync,
{
    fn fallback_policy(&self) -> NetworkFallbackPolicy;

    async fn handle(&self, request: FunctionCallRequest) -> Result<()>;

    #[instrument(level = Level::INFO, skip(self))]
    fn close(&self) -> Result<()> {
        Ok(())
    }
}
