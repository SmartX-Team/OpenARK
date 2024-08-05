use anyhow::Result;
use ark_core::signal::FunctionSignal;
use async_trait::async_trait;
use clap::Parser;
use kubegraph_api::{
    component::NetworkComponent,
    function::{call::FunctionCallRequest, service::NetworkFunctionServiceExt},
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use tracing::{instrument, Level};

#[derive(Copy, Clone, Debug, Default, PartialEq, Serialize, Deserialize, JsonSchema, Parser)]
#[clap(rename_all = "kebab-case")]
#[serde(rename_all = "camelCase")]
struct NetworkFunctionServiceArgs {}

struct NetworkFunctionService {
    args: NetworkFunctionServiceArgs,
}

#[async_trait]
impl NetworkComponent for NetworkFunctionService {
    type Args = NetworkFunctionServiceArgs;

    async fn try_new(args: <Self as NetworkComponent>::Args, _: &FunctionSignal) -> Result<Self> {
        Ok(Self { args })
    }
}

#[async_trait]
impl ::kubegraph_api::function::service::NetworkFunctionService for NetworkFunctionService {
    #[instrument(level = Level::INFO, skip(self, request))]
    async fn handle(&self, request: FunctionCallRequest) -> Result<()> {
        dbg!(request);
        Ok(())
    }
}

#[::tokio::main]
async fn main() {
    NetworkFunctionService::main().await
}
