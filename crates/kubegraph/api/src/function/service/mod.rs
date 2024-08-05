mod actix;

use std::sync::Arc;

use anyhow::{anyhow, Result};
use ark_core::signal::FunctionSignal;
use async_trait::async_trait;
use clap::{Args, Parser};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
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
    <Self as NetworkComponent>::Args: Send + Args + Parser,
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
        let NetworkFunctionServiceAgentArgs {
            fallback_policy,
            service: args,
        } = match NetworkFunctionServiceAgentArgs::try_parse() {
            Ok(args) => args,
            Err(error) => signal.panic(error).await,
        };
        let function = match <Self as NetworkComponent>::try_new(args, &signal).await {
            Ok(function) => Arc::new(function),
            Err(error) => {
                signal
                    .panic(anyhow!("failed to init function service: {error}"))
                    .await
            }
        };

        info!("Creating http server...");
        let handler_http_server = spawn(self::actix::loop_forever(
            signal.clone(),
            function.clone(),
            fallback_policy,
        ));

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

#[derive(Copy, Clone, Debug, Default, PartialEq, Serialize, Deserialize, JsonSchema, Parser)]
#[clap(rename_all = "kebab-case")]
#[serde(rename_all = "camelCase")]
struct NetworkFunctionServiceAgentArgs<S>
where
    S: Args,
{
    #[arg(
        long,
        env = "KUBEGRAPH_FUNCTION_FALLBACK_POLICY",
        value_name = "POLICY",
        default_value_t = NetworkFallbackPolicy::default(),
    )]
    #[serde(default)]
    fallback_policy: NetworkFallbackPolicy,

    #[command(flatten)]
    service: S,
}

#[async_trait]
impl<T> NetworkFunctionServiceExt for T
where
    Self: NetworkComponentExt + NetworkFunctionService,
    <Self as NetworkComponent>::Args: Send + Args + Parser,
{
}

#[async_trait]
pub trait NetworkFunctionService
where
    Self: Send + Sync,
{
    async fn handle(&self, request: FunctionCallRequest) -> Result<()>;

    #[instrument(level = Level::INFO, skip(self))]
    fn close(&self) -> Result<()> {
        Ok(())
    }
}
