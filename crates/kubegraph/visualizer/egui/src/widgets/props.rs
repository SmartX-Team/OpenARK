use egui::Pos2;
use egui_graphs::NodeProps;
use kubegraph_api::graph::GraphEntry;

pub(crate) trait Props {
    fn location(&self) -> Pos2;

    fn name(&self) -> String;
}

impl Props for NodeProps<GraphEntry> {
    fn location(&self) -> Pos2 {
        self.location
    }

    fn name(&self) -> String {
        self.payload
            .name()
            .cloned()
            .unwrap_or_else(|| self.label.clone())
    }
}
