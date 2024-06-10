use egui::{Pos2, Rect, Shape as EguiShape, Vec2};
use egui_graphs::{DisplayNode, DrawContext, NodeProps};
use kubegraph_api::graph::GraphEntry;
use petgraph::{stable_graph::IndexType, EdgeType};

use super::{base::rotate_point_around, props::Props};

pub(crate) trait DisplayShape {
    fn shapes(&mut self, ctx: &DrawContext) -> Vec<EguiShape>;
}

#[derive(Clone)]
pub(crate) struct Shape<P> {
    pub(crate) props: P,
    pub(crate) spec: ShapeSpec,

    pub(crate) clockwise: bool,
    pub(crate) angle_rad: f32,
    pub(crate) speed_per_second: f32,
    /// None means animation is not in progress
    pub(crate) last_time_update: Option<std::time::Instant>,
}

impl<P> From<P> for Shape<P> {
    fn from(props: P) -> Self {
        Self {
            props,
            spec: ShapeSpec::default(),

            clockwise: true,
            angle_rad: Default::default(),
            last_time_update: Default::default(),
            speed_per_second: 1.,
        }
    }
}

impl<P, Ty, Ix> DisplayNode<GraphEntry, GraphEntry, Ty, Ix> for Shape<P>
where
    Self: From<NodeProps<GraphEntry>> + DisplayShape,
    P: Clone + From<NodeProps<GraphEntry>> + Props,
    Ty: EdgeType,
    Ix: IndexType,
{
    fn is_inside(&self, pos: Pos2) -> bool {
        let rotated_pos = rotate_point_around(self.props.location(), pos, -self.angle_rad);
        let rect = Rect::from_center_size(
            self.props.location(),
            Vec2::new(self.spec.size, self.spec.size),
        );

        rect.contains(rotated_pos)
    }

    fn closest_boundary_point(&self, dir: Vec2) -> Pos2 {
        let rotated_dir = rotate_vector(dir, -self.angle_rad);
        let intersection_point =
            find_intersection(self.props.location(), self.spec.size, rotated_dir);
        rotate_point_around(self.props.location(), intersection_point, self.angle_rad)
    }

    fn shapes(&mut self, ctx: &DrawContext) -> Vec<EguiShape> {
        <Self as DisplayShape>::shapes(self, ctx)
    }

    fn update(&mut self, state: &NodeProps<GraphEntry>) {
        self.props = state.clone().into();
    }
}

impl<P> Shape<P> {
    pub(crate) fn get_rotation_increment(&mut self) -> f32 {
        let now = std::time::Instant::now();
        let mult = match self.clockwise {
            true => 1.,
            false => -1.,
        };
        match self.last_time_update {
            Some(last_time) => {
                self.last_time_update = Some(now);
                let seconds_passed = now.duration_since(last_time);
                seconds_passed.as_secs_f32() * self.speed_per_second * mult
            }
            None => {
                self.last_time_update = Some(now);
                0.
            }
        }
    }

    pub(crate) fn name(&self) -> String
    where
        P: Props,
    {
        self.props.name()
    }
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub(crate) struct ShapeSpec {
    pub(crate) size: f32,
}

impl Default for ShapeSpec {
    fn default() -> Self {
        Self { size: 10. }
    }
}

fn find_intersection(center: Pos2, size: f32, direction: Vec2) -> Pos2 {
    // Determine the intersection side based on the direction
    if direction.x.abs() > direction.y.abs() {
        // Intersects left or right side
        let x = if direction.x > 0.0 {
            center.x + size / 2.0
        } else {
            center.x - size / 2.0
        };
        let y = center.y + direction.y / direction.x * (x - center.x);
        Pos2::new(x, y)
    } else {
        // Intersects top or bottom side
        let y = if direction.y > 0.0 {
            center.y + size / 2.0
        } else {
            center.y - size / 2.0
        };
        let x = center.x + direction.x / direction.y * (y - center.y);
        Pos2::new(x, y)
    }
}

/// rotates vector by angle
fn rotate_vector(vec: Vec2, angle: f32) -> Vec2 {
    let cos = angle.cos();
    let sin = angle.sin();
    Vec2::new(cos * vec.x - sin * vec.y, sin * vec.x + cos * vec.y)
}
