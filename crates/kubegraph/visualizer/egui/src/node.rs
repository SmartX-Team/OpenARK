use egui::{
    emath::Rot2, Color32, FontFamily, FontId, Pos2, Rect, Shape as EguiShape, Stroke, Vec2,
};
use egui_graphs::{DrawContext, NodeProps};
use kubegraph_api::graph::GraphEntry;

use crate::widgets::{
    base::{rotate_point_around, Alignment},
    shape::{DisplayShape, Shape},
    text::TextWidget,
};

pub type NodeShape = Shape<NodeProps<GraphEntry>>;

impl DisplayShape for NodeShape {
    fn shapes(&mut self, ctx: &DrawContext) -> Vec<EguiShape> {
        // lets draw a rect with label in the center for every node
        // which rotates when the node is dragged

        // find node center location on the screen coordinates
        let center = ctx.meta.canvas_to_screen_pos(self.props.location);
        let size = ctx.meta.canvas_to_screen_size(self.spec.size);
        let rect_default = Rect::from_center_size(center, Vec2::new(size, size));
        let color = ctx.ctx.style().visuals.weak_text_color();

        let diff = match self.props.dragged {
            true => self.get_rotation_increment(),
            false => {
                if self.last_time_update.is_some() {
                    self.last_time_update = None;
                }
                0.
            }
        };

        if diff.abs() > 0. {
            let curr_angle = self.angle_rad + diff;
            let rot = Rot2::from_angle(curr_angle).normalized();
            self.angle_rad = rot.angle();
        };

        let points = rect_to_points(rect_default)
            .into_iter()
            .map(|p| rotate_point_around(center, p, self.angle_rad))
            .collect::<Vec<_>>();

        let shape_rect =
            EguiShape::convex_polygon(points, Color32::default(), Stroke::new(1., color));

        let widget = TextWidget {
            alignment: Alignment::Top,
            color: None,
            font: FontId::new(ctx.meta.canvas_to_screen_size(10.), FontFamily::Monospace),
            text: self.name(),
        };
        let shape_label = widget.build(self, ctx);

        vec![shape_rect, shape_label.into()]
    }
}

fn rect_to_points(rect: Rect) -> Vec<Pos2> {
    let top_left = rect.min;
    let bottom_right = rect.max;

    // Calculate the other two corners
    let top_right = Pos2::new(bottom_right.x, top_left.y);
    let bottom_left = Pos2::new(top_left.x, bottom_right.y);

    vec![top_left, top_right, bottom_right, bottom_left]
}
