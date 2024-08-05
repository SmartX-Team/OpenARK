use std::{
    collections::BTreeMap,
    net::{Ipv4Addr, SocketAddr, SocketAddrV4},
    sync::Arc,
};

use anyhow::Result;
use ark_core::signal::FunctionSignal;
use async_trait::async_trait;
use clap::Parser;
use kubegraph_api::{
    component::NetworkComponent, function::webhook::NetworkFunctionWebhookSpec, graph::GraphScope,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use tracing::{instrument, Level};

use crate::session::NetworkTraderSession;

#[derive(Clone)]
pub struct NetworkTraderDB {
    pub(crate) args: NetworkTraderDBArgs,
    data: Arc<RwLock<BTreeMap<GraphScope, NetworkTraderSession>>>,
    pub(crate) signal: FunctionSignal,
}

#[async_trait]
impl NetworkComponent for NetworkTraderDB {
    type Args = NetworkTraderDBArgs;

    async fn try_new(
        args: <Self as NetworkComponent>::Args,
        signal: &FunctionSignal,
    ) -> Result<Self> {
        Ok(Self {
            args,
            data: Arc::default(),
            signal: signal.clone(),
        })
    }
}

impl NetworkTraderDB {
    #[instrument(level = Level::INFO, skip(self, scope))]
    pub(crate) async fn is_locked(&self, scope: &GraphScope) -> Result<bool> {
        Ok(self.data.read().await.contains_key(scope))
    }

    #[instrument(level = Level::INFO, skip(self, session))]
    pub(crate) async fn register(&self, session: NetworkTraderSession) -> Result<()> {
        let scope = session.ctx.problem.scope.clone();
        self.data.write().await.insert(scope, session);
        Ok(())
    }

    #[instrument(level = Level::INFO, skip(self, scope))]
    pub(crate) async fn unregister(&self, scope: &GraphScope) -> Result<()> {
        self.data.write().await.remove(scope);
        Ok(())
    }

    pub(crate) const fn webhook_addr(&self) -> SocketAddr {
        self.args.webhook_addr
    }

    pub(crate) fn webhook_endpoint(&self) -> Result<NetworkFunctionWebhookSpec> {
        Ok(NetworkFunctionWebhookSpec {
            endpoint: self.args.webhook_endpoint.clone().parse()?,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema, Parser)]
#[clap(rename_all = "kebab-case")]
#[serde(rename_all = "camelCase")]
pub struct NetworkTraderDBArgs {
    #[arg(
        long,
        env = "KUBEGRAPH_MARKET_TRADER_WEBHOOK_ADDR",
        value_name = "ADDR",
        default_value_t = NetworkTraderDBArgs::default_webhook_addr(),
    )]
    #[serde(default = "NetworkTraderDBArgs::default_webhook_addr")]
    pub webhook_addr: SocketAddr,

    #[arg(
        long,
        env = "KUBEGRAPH_MARKET_TRADER_WEBHOOK_ENDPOINT",
        value_name = "ADDR",
        default_value_t = NetworkTraderDBArgs::default_webhook_endpoint(),
    )]
    #[serde(default = "NetworkTraderDBArgs::default_webhook_endpoint")]
    pub webhook_endpoint: String,
}

impl Default for NetworkTraderDBArgs {
    fn default() -> Self {
        Self {
            webhook_addr: Self::default_webhook_addr(),
            webhook_endpoint: Self::default_webhook_endpoint(),
        }
    }
}

impl NetworkTraderDBArgs {
    const fn default_webhook_addr() -> SocketAddr {
        SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::new(0, 0, 0, 0), 9090))
    }

    fn default_webhook_endpoint() -> String {
        "http://localhost:9090".into()
    }
}
