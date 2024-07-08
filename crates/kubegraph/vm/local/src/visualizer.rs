use anyhow::Result;
use ark_core::signal::FunctionSignal;
use async_trait::async_trait;
use clap::{Parser, ValueEnum};
use kubegraph_api::{
    component::NetworkComponent,
    frame::LazyFrame,
    graph::{Graph, GraphData, GraphMetadataExt},
    visualizer::NetworkVisualizerEvent,
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
pub struct NetworkVisualizerArgs {
    #[arg(
        long,
        env = "KUBEGRAPH_VISUALIZER",
        value_enum,
        value_name = "IMPL",
        default_value_t = NetworkVisualizerType::default(),
    )]
    #[serde(default)]
    pub visualizer: NetworkVisualizerType,

    #[cfg(feature = "visualizer-egui")]
    #[command(flatten)]
    #[serde(default)]
    pub egui: <::kubegraph_visualizer_egui::NetworkVisualizer as NetworkComponent>::Args,
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
pub enum NetworkVisualizerType {
    #[cfg_attr(not(feature = "visualizer-egui"), default)]
    Disabled,
    #[cfg(feature = "visualizer-egui")]
    #[default]
    Egui,
}

#[derive(Clone)]
pub enum NetworkVisualizer {
    Disabled,
    #[cfg(feature = "visualizer-egui")]
    Egui(::kubegraph_visualizer_egui::NetworkVisualizer),
}

#[async_trait]
impl NetworkComponent for NetworkVisualizer {
    type Args = NetworkVisualizerArgs;

    #[instrument(level = Level::INFO, skip(signal))]
    async fn try_new(
        args: <Self as NetworkComponent>::Args,
        signal: &FunctionSignal,
    ) -> Result<Self> {
        let NetworkVisualizerArgs {
            visualizer,
            #[cfg(feature = "visualizer-egui")]
            egui,
        } = args;

        match visualizer {
            NetworkVisualizerType::Disabled => {
                let _ = signal;
                Ok(Self::Disabled)
            }
            #[cfg(feature = "visualizer-egui")]
            NetworkVisualizerType::Egui => Ok(Self::Egui(
                ::kubegraph_visualizer_egui::NetworkVisualizer::try_new(egui, signal).await?,
            )),
        }
    }
}

#[async_trait]
impl ::kubegraph_api::visualizer::NetworkVisualizer for NetworkVisualizer {
    #[instrument(level = Level::INFO, skip(self, graph))]
    async fn replace_graph<M>(&self, graph: Graph<GraphData<LazyFrame>, M>) -> Result<()>
    where
        M: Send + Clone + GraphMetadataExt,
    {
        match self {
            Self::Disabled => {
                let _ = graph;
                Ok(())
            }
            #[cfg(feature = "visualizer-egui")]
            Self::Egui(runtime) => runtime.replace_graph(graph).await,
        }
    }

    #[instrument(level = Level::INFO, skip(self))]
    async fn call(&self, event: NetworkVisualizerEvent) -> Result<()> {
        match self {
            Self::Disabled => {
                let _ = event;
                Ok(())
            }
            #[cfg(feature = "visualizer-egui")]
            Self::Egui(runtime) => runtime.call(event).await,
        }
    }

    #[instrument(level = Level::INFO, skip(self))]
    #[instrument(level = Level::INFO, skip(self))]
    async fn close(&self) -> Result<()> {
        match self {
            Self::Disabled => Ok(()),
            #[cfg(feature = "visualizer-egui")]
            Self::Egui(runtime) => runtime.close().await,
        }
    }
}
