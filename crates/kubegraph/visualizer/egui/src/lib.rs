use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use eframe::{run_native, App, AppCreator, NativeOptions};
use egui::Context;
use egui_graphs::{
    DefaultEdgeShape, DefaultNodeShape, Graph as EguiGraph, GraphView, SettingsInteraction,
    SettingsStyle,
};
use kubegraph_api::{
    frame::LazyFrame,
    graph::{Graph, GraphEntry, GraphMetadataExt},
};
use tokio::{
    sync::Mutex,
    task::{spawn_blocking, JoinHandle},
};
use tracing::error;
use winit::platform::wayland::EventLoopBuilderExtWayland;

#[derive(Clone)]
pub struct NetworkVisualizer {
    graph: Arc<Mutex<Option<EguiGraph<GraphEntry, GraphEntry>>>>,
    session: Arc<Mutex<Option<JoinHandle<()>>>>,
}

#[async_trait]
impl ::kubegraph_api::visualizer::NetworkVisualizer for NetworkVisualizer {
    async fn try_default() -> Result<Self> {
        let data = Self {
            graph: Arc::default(),
            session: Arc::default(),
        };

        data.session.lock().await.replace(spawn_blocking({
            let data = data.clone();
            || data.loop_forever()
        }));

        Ok(data)
    }

    async fn register<M>(&self, graph: Graph<LazyFrame, M>) -> Result<()>
    where
        M: Send + Clone + GraphMetadataExt,
    {
        self.graph
            .lock()
            .await
            .replace(EguiGraph::from(&graph.try_into()?));
        Ok(())
    }

    async fn close(&self) -> Result<()> {
        if let Some(session) = self.session.lock().await.take() {
            session.abort();
        }
        Ok(())
    }
}

impl NetworkVisualizer {
    fn loop_forever(self) {
        let app = NetworkVisualizerApp { data: self };

        let app_name = "kubegraph_visualizer";
        let native_options = NativeOptions {
            event_loop_builder: Some(Box::new(|event_loop_builder| {
                event_loop_builder.with_any_thread(true);
            })),
            ..Default::default()
        };
        let app_creator: AppCreator = Box::new(|_| Box::new(app));

        if let Err(error) = run_native(app_name, native_options, app_creator) {
            error!("failed to operate visualizer: {error}");
        }
    }
}

struct NetworkVisualizerApp {
    data: NetworkVisualizer,
}

impl App for NetworkVisualizerApp {
    fn update(&mut self, ctx: &Context, _: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            if let Some(graph) = self.data.graph.blocking_lock().as_mut() {
                let interaction_settings = &SettingsInteraction::new()
                    .with_dragging_enabled(true)
                    .with_node_clicking_enabled(true)
                    .with_node_selection_enabled(true)
                    .with_node_selection_multi_enabled(true)
                    .with_edge_clicking_enabled(true)
                    .with_edge_selection_enabled(true)
                    .with_edge_selection_multi_enabled(true);
                let style_settings = &SettingsStyle::new().with_labels_always(true);
                ui.add(
                    &mut GraphView::<_, _, _, _, DefaultNodeShape, DefaultEdgeShape>::new(graph)
                        .with_styles(style_settings)
                        .with_interactions(interaction_settings),
                );
            }
        });
    }
}
