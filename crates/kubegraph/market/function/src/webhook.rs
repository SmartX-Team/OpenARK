use anyhow::Result;
use async_trait::async_trait;
use kubegraph_api::{
    function::webhook::NetworkFunctionWebhookSpec, market::function::MarketFunctionContext,
};
use tokio::spawn;
use tracing::{error, info, instrument, Level};

#[async_trait]
impl super::MarketFunction<NetworkFunctionWebhookSpec> for super::MarketFunctionClient {
    #[instrument(level = Level::INFO, skip(self))]
    async fn spawn(
        &self,
        ctx: MarketFunctionContext,
        spec: NetworkFunctionWebhookSpec,
    ) -> Result<()> {
        spawn(call(self.session.clone(), ctx, spec));
        Ok(())
    }
}

#[instrument(level = Level::INFO, skip(session))]
async fn call(
    session: ::reqwest::Client,
    ctx: MarketFunctionContext,
    spec: NetworkFunctionWebhookSpec,
) {
    let NetworkFunctionWebhookSpec { endpoint } = spec;

    let response = match session.post(endpoint.0).json(&ctx).send().await {
        Ok(response) => response,
        Err(error) => {
            error!("failed to call market function: {error}");
            return;
        }
    };

    let json = match response.json().await {
        Ok(json) => json,
        Err(error) => {
            error!("failed to get a response from market function: {error}");
            return;
        }
    };

    match json {
        ::ark_core::result::Result::Ok(()) => {
            info!("completed calling market function");
        }
        ::ark_core::result::Result::Err(error) => {
            error!("failed to complete market function: {error}")
        }
    }
}
