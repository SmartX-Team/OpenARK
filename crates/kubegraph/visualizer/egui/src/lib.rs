mod node;
mod widgets;

use std::sync::Arc;

use anyhow::Result;
use ark_core::signal::FunctionSignal;
use async_trait::async_trait;
use clap::Parser;
use eframe::{run_native, App, AppCreator, Frame, NativeOptions};
use egui::{Button, Context, Ui};
use egui_graphs::{
    DefaultEdgeShape, Graph as EguiGraph, GraphView, SettingsInteraction, SettingsStyle,
};
use kubegraph_api::{
    component::NetworkComponent,
    frame::LazyFrame,
    graph::{Graph, GraphData, GraphEntry, GraphMetadataExt},
    visualizer::NetworkVisualizerEvent,
};
use petgraph::{csr::DefaultIx, Directed};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use tokio::{
    runtime::Handle,
    sync::{mpsc, oneshot, Mutex},
    task::{spawn_blocking, JoinHandle},
};
use tracing::{error, info, instrument, Level};
use winit::platform::wayland::EventLoopBuilderExtWayland;

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
pub struct NetworkVisualizerArgs {}

#[derive(Clone)]
pub struct NetworkVisualizer {
    data: Arc<NetworkVisualizerData>,
    task: Arc<Mutex<Option<JoinHandle<()>>>>,
}

#[async_trait]
impl NetworkComponent for NetworkVisualizer {
    type Args = NetworkVisualizerArgs;

    #[instrument(level = Level::INFO)]
    async fn try_new(
        args: <Self as NetworkComponent>::Args,
        signal: &FunctionSignal,
    ) -> Result<Self> {
        let NetworkVisualizerArgs {} = args;

        let (event_channel, event_collectors) = mpsc::channel(Self::MAX_EVENT_CHANNEL);

        let ctx = NetworkVisualizerContext::new(event_collectors);
        let this = Self {
            data: Arc::new(NetworkVisualizerData::new(event_channel)),
            task: Arc::default(),
        };

        this.task.lock().await.replace(spawn_blocking({
            let this = this.clone();
            let signal = signal.clone();
            || this.loop_forever(signal, ctx)
        }));

        Ok(this)
    }
}

#[async_trait]
impl ::kubegraph_api::visualizer::NetworkVisualizer for NetworkVisualizer {
    #[instrument(level = Level::INFO, skip(self, graph))]
    async fn replace_graph<M>(&self, graph: Graph<GraphData<LazyFrame>, M>) -> Result<()>
    where
        M: Send + Clone + GraphMetadataExt,
    {
        self.data
            .graph
            .lock()
            .await
            .replace(EguiGraph::from(&graph.try_into()?));
        Ok(())
    }

    #[instrument(level = Level::INFO, skip(self))]
    async fn call(&self, event: NetworkVisualizerEvent) -> Result<()> {
        self.data.call(event).await
    }

    #[instrument(level = Level::INFO, skip(self))]
    async fn close(&self) -> Result<()> {
        if let Some(session) = self.task.lock().await.take() {
            session.abort();
        }
        Ok(())
    }
}

impl NetworkVisualizer {
    const MAX_EVENT_CHANNEL: usize = 32;

    fn loop_forever(self, signal: FunctionSignal, ctx: NetworkVisualizerContext) {
        info!("Starting egui visualizer...");

        let app = NetworkVisualizerApp::new(ctx, self.data.clone());

        let app_name = "KubeGraph - Visualizer";
        let native_options = NativeOptions {
            event_loop_builder: Some(Box::new(|event_loop_builder| {
                event_loop_builder.with_any_thread(true);
            })),
            ..Default::default()
        };
        let app_creator: AppCreator = Box::new(|_| Box::new(app));

        match run_native(app_name, native_options, app_creator) {
            Ok(()) => {
                info!("Completed egui visualizer");
                signal.terminate()
            }
            Err(error) => {
                error!("failed to operate egui visualizer: {error}");
                signal.terminate_on_panic()
            }
        }
    }
}

struct NetworkVisualizerApp {
    ctx: NetworkVisualizerContext,
    data: Arc<NetworkVisualizerData>,
}

impl App for NetworkVisualizerApp {
    fn update(&mut self, ctx: &Context, _: &mut Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            Handle::current().block_on(async move {
                self.ctx.collect_events().await;

                if ui.add(Button::new("Next")).clicked() {
                    self.ctx.activate(NetworkVisualizerEvent::Next).await;
                }

                self.update_graph(ui).await
            })
        });
    }
}

impl NetworkVisualizerApp {
    fn new(ctx: NetworkVisualizerContext, data: Arc<NetworkVisualizerData>) -> Self {
        Self { ctx, data }
    }

    async fn update_graph(&mut self, ui: &mut Ui) {
        if let Some(graph) = self.data.graph.lock().await.as_mut() {
            let settings_interaction = &SettingsInteraction::new()
                .with_dragging_enabled(true)
                .with_node_clicking_enabled(true)
                .with_node_selection_enabled(true)
                .with_node_selection_multi_enabled(true)
                .with_edge_clicking_enabled(true)
                .with_edge_selection_enabled(true)
                .with_edge_selection_multi_enabled(true);
            let settings_style = &SettingsStyle::new().with_labels_always(true);
            ui.add(
                &mut GraphView::<_, _, _, _, self::node::NodeShape, DefaultEdgeShape>::new(graph)
                    .with_styles(settings_style)
                    .with_interactions(settings_interaction),
            );
        }
    }
}

struct NetworkVisualizerContext {
    event_collectors: mpsc::Receiver<NetworkVisualizerEventContext>,
    events: Vec<NetworkVisualizerEventContext>,
}

impl NetworkVisualizerContext {
    fn new(event_collectors: mpsc::Receiver<NetworkVisualizerEventContext>) -> Self {
        Self {
            event_collectors,
            events: Vec::default(),
        }
    }

    async fn collect_events(&mut self) {
        while let Ok(event) = self.event_collectors.try_recv() {
            self.events.push(event);
        }
    }

    async fn activate(&mut self, event: NetworkVisualizerEvent) {
        for index in (0..self.events.len()).rev() {
            let ctx = &self.events[index];
            if ctx.event == event {
                let ctx = self.events.remove(index);
                ctx.sender.send(()).ok();
            }
        }
    }
}

struct NetworkVisualizerData {
    event_channel: mpsc::Sender<NetworkVisualizerEventContext>,
    graph: Mutex<
        Option<
            EguiGraph<
                GraphEntry,
                GraphEntry,
                Directed,
                DefaultIx,
                self::node::NodeShape,
                DefaultEdgeShape,
            >,
        >,
    >,
}

impl NetworkVisualizerData {
    fn new(event_channel: mpsc::Sender<NetworkVisualizerEventContext>) -> Self {
        Self {
            event_channel,
            graph: Mutex::default(),
        }
    }

    async fn call(&self, event: NetworkVisualizerEvent) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        let ctx = NetworkVisualizerEventContext { event, sender: tx };

        self.event_channel.send(ctx).await?;
        rx.await.map_err(Into::into)
    }
}

struct NetworkVisualizerEventContext {
    event: NetworkVisualizerEvent,
    sender: oneshot::Sender<()>,
}
