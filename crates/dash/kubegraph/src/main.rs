use anyhow::Result;
use ark_core::signal::FunctionSignal;
use async_trait::async_trait;
use clap::Parser;
use kubegraph_api::{
    component::NetworkComponent,
    function::{call::FunctionCallRequest, service::NetworkFunctionServiceExt},
    vm::NetworkFallbackPolicy,
};
use tracing::{instrument, Level};

#[derive(Clone, Debug, PartialEq, Parser)]
#[clap(rename_all = "kebab-case")]
struct NetworkFunctionServiceArgs {
    #[arg(
        long,
        env = "KUBEGRAPH_FUNCTION_FALLBACK_POLICY",
        value_name = "POLICY",
        default_value_t = NetworkFallbackPolicy::default(),
    )]
    fallback_policy: NetworkFallbackPolicy,
}

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
    fn fallback_policy(&self) -> NetworkFallbackPolicy {
        self.args.fallback_policy
    }

    #[instrument(level = Level::INFO, skip(self, request))]
    async fn handle(&self, request: FunctionCallRequest) -> Result<()> {
        Ok(())
    }
}

#[::tokio::main]
async fn main() {
    NetworkFunctionService::main().await
}
