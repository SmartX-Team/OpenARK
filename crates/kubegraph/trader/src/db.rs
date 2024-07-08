use std::{collections::BTreeMap, sync::Arc};

use anyhow::Result;
use ark_core::signal::FunctionSignal;
use async_trait::async_trait;
use clap::Parser;
use kubegraph_api::{component::NetworkComponent, graph::GraphScope};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use tracing::{instrument, Level};

use crate::session::NetworkTraderSession;

#[derive(Clone)]
pub struct NetworkTraderDB {
    data: Arc<RwLock<BTreeMap<GraphScope, NetworkTraderSession>>>,
}

#[async_trait]
impl NetworkComponent for NetworkTraderDB {
    type Args = NetworkTraderDBArgs;

    async fn try_new(args: <Self as NetworkComponent>::Args, _: &FunctionSignal) -> Result<Self> {
        let NetworkTraderDBArgs {} = args;

        Ok(Self {
            data: Arc::default(),
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
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, JsonSchema, Parser)]
#[clap(rename_all = "kebab-case")]
#[serde(rename_all = "camelCase")]
pub struct NetworkTraderDBArgs {}
