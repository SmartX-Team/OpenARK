use anyhow::Result;
use ark_core::signal::FunctionSignal;
use async_trait::async_trait;
use kubegraph_api::{
    frame::LazyFrame,
    graph::{Graph, GraphMetadataExt},
    visualizer::NetworkVisualizerEvent,
};
use tracing::{instrument, Level};

#[derive(Clone)]
pub struct NetworkVisualizer {
    #[cfg(feature = "visualizer-egui")]
    egui: ::kubegraph_visualizer_egui::NetworkVisualizer,
}

#[async_trait]
impl ::kubegraph_api::visualizer::NetworkVisualizer for NetworkVisualizer {
    #[instrument(level = Level::INFO)]
    async fn try_new(signal: &FunctionSignal) -> Result<Self> {
        Ok(Self {
            #[cfg(feature = "visualizer-egui")]
            egui: ::kubegraph_visualizer_egui::NetworkVisualizer::try_new(signal).await?,
        })
    }

    #[instrument(level = Level::INFO, skip(self, graph))]
    async fn replace_graph<M>(&self, graph: Graph<LazyFrame, M>) -> Result<()>
    where
        M: Send + Clone + GraphMetadataExt,
    {
        #[cfg(feature = "visualizer-egui")]
        {
            self.egui.replace_graph(graph.clone()).await?;
        }
        let _ = graph;
        Ok(())
    }

    async fn call(&self, event: NetworkVisualizerEvent) -> Result<()> {
        #[cfg(feature = "visualizer-egui")]
        {
            self.egui.call(event).await?;
        }
        let _ = event;
        Ok(())
    }

    #[instrument(level = Level::INFO, skip(self))]
    async fn close(&self) -> Result<()> {
        #[cfg(feature = "visualizer-egui")]
        {
            self.egui.close().await?;
        }
        Ok(())
    }
}
