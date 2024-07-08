mod actix;
mod db;
mod session;

use anyhow::Result;
use ark_core::signal::FunctionSignal;
use async_trait::async_trait;
use clap::Parser;
use kubegraph_api::{
    component::NetworkComponent,
    frame::LazyFrame,
    function::webhook::NetworkFunctionWebhookSpec,
    market::{product::ProductSpec, sub::SubSpec, BaseModel},
    problem::VirtualProblem,
    trader::NetworkTraderContext,
};
use kubegraph_market_client::{MarketClient, MarketClientArgs};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use tracing::{info, instrument, Level};

#[derive(Clone)]
pub struct NetworkTrader {
    client: MarketClient,
    db: crate::db::NetworkTraderDB,
}

#[async_trait]
impl NetworkComponent for NetworkTrader {
    type Args = NetworkTraderArgs;

    #[instrument(level = Level::INFO, skip(signal))]
    async fn try_new(
        args: <Self as NetworkComponent>::Args,
        signal: &FunctionSignal,
    ) -> Result<Self> {
        let NetworkTraderArgs { client, db } = args;

        Ok(Self {
            client: {
                info!("Initializing market trader...");
                MarketClient::try_new(client, signal).await?
            },
            db: crate::db::NetworkTraderDB::try_new(db, signal).await?,
        })
    }
}

#[async_trait]
impl ::kubegraph_api::trader::NetworkTrader<LazyFrame> for NetworkTrader {
    #[instrument(level = Level::INFO, skip(self, problem))]
    async fn is_locked(&self, problem: &VirtualProblem) -> Result<bool> {
        self.db.is_locked(&problem.scope).await
    }

    #[instrument(level = Level::INFO, skip(self, ctx))]
    async fn register(&self, ctx: NetworkTraderContext<LazyFrame>) -> Result<()> {
        let mut state = NetworkTraderState::default();
        match self.try_register(&mut state, ctx).await {
            Ok(()) => Ok(()),
            Err(error) => self
                .rollback_register(state)
                .await
                .map_err(|error_rollback| error.context(error_rollback)),
        }
    }
}

impl NetworkTrader {
    #[instrument(level = Level::INFO, skip(self, state, ctx))]
    async fn try_register(
        &self,
        state: &mut NetworkTraderState,
        ctx: NetworkTraderContext<LazyFrame>,
    ) -> Result<()> {
        // Step 1. Create a problem
        let prod_id = {
            let spec = ProductSpec {
                problem: ctx.problem.spec.clone(),
            };
            self.client.find_product(&spec).await?
        };
        state.prod_id.replace(prod_id);

        // Step 2. Create a webhook
        let function: NetworkFunctionWebhookSpec = todo!();
        state.function.replace(function.clone());

        // Step 3. Estimate the cost
        let cost = todo!();

        // Step 4. Create a subscriber
        {
            let spec = SubSpec {
                cost,
                count: 1,
                function,
            };
            self.client.insert_sub(prod_id, &spec).await?
        }

        // Step 5. Store it to the DB
        let session = crate::session::NetworkTraderSession { ctx };
        self.db.register(session).await
    }

    #[instrument(level = Level::INFO, skip(self, state))]
    async fn rollback_register(&self, state: NetworkTraderState) -> Result<()> {
        let NetworkTraderState {
            function,
            prod_id: _,
            sub_id,
        } = state;

        // Step -4. Rollback creating the subscriber
        todo!();

        // Step -3. Rollback estimating the cost
        // NOTE: nothing to do

        // Step -2. Rollback creating the webhook
        todo!();

        // Step -1. Rollback creating the problem
        // NOTE: nothing to do

        Ok(())
    }
}

#[derive(Default)]
struct NetworkTraderState {
    function: Option<NetworkFunctionWebhookSpec>,
    prod_id: Option<<ProductSpec as BaseModel>::Id>,
    sub_id: Option<<SubSpec as BaseModel>::Id>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, JsonSchema, Parser)]
#[clap(rename_all = "kebab-case")]
#[serde(rename_all = "camelCase")]
pub struct NetworkTraderArgs {
    #[command(flatten)]
    #[serde(default)]
    pub client: MarketClientArgs,

    #[command(flatten)]
    pub db: <self::db::NetworkTraderDB as NetworkComponent>::Args,
}
