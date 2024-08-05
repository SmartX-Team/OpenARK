mod webhook;

use anyhow::Result;
use ark_core::signal::FunctionSignal;
use async_trait::async_trait;
use clap::Parser;
use kubegraph_api::{
    component::NetworkComponent,
    market::{r#pub::PubSpec, sub::SubSpec, transaction::TransactionReceipt},
};
use serde::{Deserialize, Serialize};
use tracing::{instrument, Level};

#[async_trait]
pub trait MarketFunction<T> {
    async fn spawn(&self, receipt: TransactionReceipt, spec: T) -> Result<()>;
}

#[derive(Clone)]
pub struct MarketFunctionClient {
    pub(crate) session: ::reqwest::Client,
}

#[async_trait]
impl NetworkComponent for MarketFunctionClient {
    type Args = MarketFunctionClientArgs;

    async fn try_new(args: <Self as NetworkComponent>::Args, _: &FunctionSignal) -> Result<Self> {
        let MarketFunctionClientArgs {} = args;

        Ok(Self {
            session: ::reqwest::ClientBuilder::new().build()?,
        })
    }
}

#[async_trait]
impl MarketFunction<PubSpec> for MarketFunctionClient {
    #[instrument(level = Level::INFO, skip(self))]
    async fn spawn(&self, receipt: TransactionReceipt, spec: PubSpec) -> Result<()> {
        self.spawn(receipt, spec.function).await
    }
}

#[async_trait]
impl MarketFunction<SubSpec> for MarketFunctionClient {
    #[instrument(level = Level::INFO, skip(self))]
    async fn spawn(&self, receipt: TransactionReceipt, spec: SubSpec) -> Result<()> {
        self.spawn(receipt, spec.function).await
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, Parser)]
#[clap(rename_all = "kebab-case")]
#[serde(rename_all = "camelCase")]
pub struct MarketFunctionClientArgs {}
