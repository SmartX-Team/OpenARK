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
    market::{product::ProductSpec, sub::SubSpec, BaseModel},
    problem::VirtualProblem,
    trader::NetworkTraderContext,
};
use kubegraph_market_client::{MarketClient, MarketClientArgs};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use tokio::spawn;
use tracing::{debug, info, instrument, Level};

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

        let db = crate::db::NetworkTraderDB::try_new(db, signal).await?;
        spawn(crate::actix::loop_forever(db.clone()));

        Ok(Self {
            client: {
                info!("Initializing market trader...");
                MarketClient::try_new(client, signal).await?
            },
            db,
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
            Err(error) => match self.rollback_register(state).await {
                Ok(()) => Err(error),
                Err(error_rollback) => Err(error.context(error_rollback)),
            },
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
        debug!("Creating a problem");
        let prod_id = {
            let spec = ProductSpec {
                problem: ctx.problem.spec.clone(),
            };
            self.client.find_product(&spec).await?
        };
        state.prod_id.replace(prod_id);

        // Step 2. Estimate the cost
        debug!("Estimating the cost");
        let cost = todo!();

        // Step 3. Create a subscriber
        debug!("Creating a subscriber");
        let sub_id = {
            let spec = SubSpec {
                cost,
                count: 1,
                function: self.db.webhook_endpoint()?,
            };
            self.client.insert_sub(prod_id, &spec).await?
        };
        state.sub_id.replace(sub_id);

        // Step 4. Store it to the DB
        debug!("Storing the session to the DB");
        let session = crate::session::NetworkTraderSession { ctx };
        self.db.register(session).await
    }

    #[instrument(level = Level::INFO, skip(self, state))]
    async fn rollback_register(&self, state: NetworkTraderState) -> Result<()> {
        let NetworkTraderState { prod_id, sub_id } = state;

        let mut error = None;

        // Step -3. Rollback creating the subscriber
        debug!("Rollback creating the subscriber");
        if let Some((prod_id, sub_id)) = prod_id.zip(sub_id) {
            if let Err(e) = self.client.remove_sub(prod_id, sub_id).await {
                error.replace(e);
            }
        }

        // Step -2. Rollback estimating the cost
        // NOTE: nothing to do

        // Step -1. Rollback creating the problem
        // NOTE: nothing to do

        Ok(())
    }
}

#[derive(Default)]
struct NetworkTraderState {
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
