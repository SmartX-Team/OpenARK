use egui::{epaint::TextShape, Color32, FontId, Vec2};
use egui_graphs::DrawContext;

use super::{base::Alignment, props::Props, shape::Shape};

pub(crate) struct TextWidget {
    pub(crate) alignment: Alignment,
    pub(crate) color: Option<Color32>,
    pub(crate) font: FontId,
    pub(crate) text: String,
}

impl TextWidget {
    pub(crate) fn build<P>(
        self,
        shape: &Shape<P>,
        ctx: &DrawContext,
    ) -> impl Into<::egui::epaint::Shape>
    where
        P: Props,
    {
        let Self {
            alignment,
            color,
            font,
            text,
        } = self;

        // find node center location on the screen coordinates
        let center = ctx.meta.canvas_to_screen_pos(shape.props.location());
        let size = ctx.meta.canvas_to_screen_size(shape.spec.size);

        // create label
        let color = color.unwrap_or(ctx.ctx.style().visuals.text_color());
        let fallback_color = ctx.ctx.style().visuals.weak_text_color();
        let galley = ctx.ctx.fonts(|f| f.layout_no_wrap(text, font, color));

        // we need to offset label by half its size to place it in the center of the rect
        let offset = match alignment {
            // Alignment::Center => {
            //     let x = -galley.size().x / 2.;
            //     let y = -galley.size().y / 2.;
            //     Vec2::new(x, y)
            // }
            Alignment::Top => {
                let x = -galley.size().x / 2.;
                let y = -galley.size().y - size / 2.;
                Vec2::new(x, y)
            }
        };

        // create the shape and add it to the layers
        let pos = center + offset;
        TextShape::new(pos, galley, fallback_color)
    }
}
