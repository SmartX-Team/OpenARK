use anyhow::Result;
use async_trait::async_trait;
use kubegraph_api::{
    frame::LazyFrame,
    graph::{Graph, GraphMetadataExt},
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
    async fn try_default() -> Result<Self> {
        Ok(Self {
            #[cfg(feature = "visualizer-egui")]
            egui: ::kubegraph_visualizer_egui::NetworkVisualizer::try_default().await?,
        })
    }

    #[instrument(level = Level::INFO, skip(self, graph))]
    async fn register<M>(&self, graph: Graph<LazyFrame, M>) -> Result<()>
    where
        M: Send + Clone + GraphMetadataExt,
    {
        #[cfg(feature = "visualizer-egui")]
        {
            self.egui.register(graph.clone()).await?;
        }
        let _ = graph;
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
