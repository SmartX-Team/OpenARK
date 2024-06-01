use anyhow::Result;
use ark_core::signal::FunctionSignal;
use async_trait::async_trait;
use clap::{Parser, ValueEnum};
use kubegraph_api::{
    component::NetworkComponent,
    frame::LazyFrame,
    graph::{GraphData, GraphMetadataStandard, ScopedNetworkGraphDB},
    problem::ProblemSpec,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use tracing::{instrument, Level};

#[derive(
    Copy,
    Clone,
    Debug,
    Default,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    JsonSchema,
    Parser,
)]
#[clap(rename_all = "kebab-case")]
#[serde(rename_all = "camelCase")]
pub struct NetworkRunnerArgs {
    #[arg(
        long,
        env = "KUBEGRAPH_RUNNER",
        value_enum,
        value_name = "IMPL",
        default_value_t = NetworkRunnerType::default(),
    )]
    #[serde(default)]
    pub runner: NetworkRunnerType,

    #[cfg(feature = "runner-simulator")]
    #[command(flatten)]
    #[serde(default)]
    pub simulator: <::kubegraph_runner_simulator::NetworkRunner as NetworkComponent>::Args,
}

#[derive(
    Copy,
    Clone,
    Debug,
    Default,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    JsonSchema,
    ValueEnum,
)]
#[clap(rename_all = "kebab-case")]
#[serde(rename_all = "kebab-case")]
pub enum NetworkRunnerType {
    #[cfg_attr(not(feature = "runner-simulator"), default)]
    Disabled,
    #[cfg(feature = "runner-simulator")]
    #[default]
    Simulator,
}

#[derive(Clone)]
pub enum NetworkRunner {
    Disabled,
    #[cfg(feature = "runner-simulator")]
    Simulator(::kubegraph_runner_simulator::NetworkRunner),
}

#[async_trait]
impl NetworkComponent for NetworkRunner {
    type Args = NetworkRunnerArgs;

    #[instrument(level = Level::INFO)]
    async fn try_new(
        args: <Self as NetworkComponent>::Args,
        signal: &FunctionSignal,
    ) -> Result<Self> {
        let NetworkRunnerArgs {
            runner,
            #[cfg(feature = "runner-simulator")]
            simulator,
        } = args;

        match runner {
            NetworkRunnerType::Disabled => {
                let _ = signal;
                Ok(Self::Disabled)
            }
            #[cfg(feature = "runner-simulator")]
            NetworkRunnerType::Simulator => Ok(Self::Simulator(
                ::kubegraph_runner_simulator::NetworkRunner::try_new(simulator, signal).await?,
            )),
        }
    }
}

#[async_trait]
impl ::kubegraph_api::runner::NetworkRunner<GraphData<LazyFrame>> for NetworkRunner {
    #[instrument(level = Level::INFO, skip(self, graph_db, graph, problem))]
    async fn execute(
        &self,
        graph_db: &dyn ScopedNetworkGraphDB,
        graph: GraphData<LazyFrame>,
        problem: &ProblemSpec<GraphMetadataStandard>,
    ) -> Result<()> {
        match self {
            Self::Disabled => {
                let _ = graph_db;
                let _ = graph;
                let _ = problem;
                Ok(())
            }
            #[cfg(feature = "runner-simulator")]
            Self::Simulator(runtime) => runtime.execute(graph_db, graph, problem).await,
        }
    }
}
