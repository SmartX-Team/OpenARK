use anyhow::Result;
use ark_core::signal::FunctionSignal;
use async_trait::async_trait;
use clap::Parser;
use kubegraph_api::component::NetworkComponent;
use serde::{Deserialize, Serialize};
use tokio::{spawn, task::JoinHandle};
use tracing::{instrument, Level};

#[derive(Clone)]
pub struct Agent {
    pub(crate) args: AgentArgs,
    pub(crate) signal: FunctionSignal,
}

#[async_trait]
impl NetworkComponent for Agent {
    type Args = AgentArgs;

    #[instrument(level = Level::INFO, skip(args, signal))]
    async fn try_new(
        args: <Self as NetworkComponent>::Args,
        signal: &FunctionSignal,
    ) -> Result<Self> {
        Ok(Self {
            args,
            signal: signal.clone(),
        })
    }
}

impl Agent {
    pub(crate) fn spawn_workers(&self) -> Vec<JoinHandle<()>> {
        vec![spawn(crate::actix::loop_forever(self.clone()))]
    }

    #[instrument(level = Level::INFO, skip(self))]
    pub(crate) async fn close(self) -> Result<()> {
        Ok(())
    }
}

#[derive(Clone, Debug, Parser, Serialize, Deserialize)]
#[clap(rename_all = "kebab-case")]
#[serde(rename_all = "camelCase")]
pub struct AgentArgs {}
