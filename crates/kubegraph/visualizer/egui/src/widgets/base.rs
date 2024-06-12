use egui::Pos2;

pub(crate) enum Alignment {
    // Center,
    Top,
}

// Function to rotate a point around another point
pub(crate) fn rotate_point_around(center: Pos2, point: Pos2, angle: f32) -> Pos2 {
    let sin_angle = angle.sin();
    let cos_angle = angle.cos();

    // Translate point back to origin
    let translated_point = point - center;

    // Rotate point
    let rotated_x = translated_point.x * cos_angle - translated_point.y * sin_angle;
    let rotated_y = translated_point.x * sin_angle + translated_point.y * cos_angle;

    // Translate point back
    Pos2::new(rotated_x, rotated_y) + center.to_vec2()
}
