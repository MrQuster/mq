//! Table and list row component with 44px fixed height, 1px divider, and hover highlight.

use egui::{CursorIcon, Response, Sense, Stroke, Ui, Vec2};

use crate::theme::Tokens;

pub fn list_row<R>(
    ui: &mut Ui,
    tokens: &Tokens,
    height: f32,
    add_contents: impl FnOnce(&mut Ui) -> R,
) -> (Response, R) {
    let desired_size = Vec2::new(ui.available_width(), height.max(40.0));
    let (rect, response) = ui.allocate_exact_size(desired_size, Sense::click());

    if response.hovered() {
        ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
    }

    let bg_color = if response.is_pointer_button_down_on() || response.hovered() {
        tokens.bg_surface_hover
    } else {
        tokens.bg_surface
    };

    let painter = ui.painter();
    painter.rect_filled(rect, 0.0, bg_color);
    painter.line_segment(
        [
            egui::pos2(rect.left(), rect.bottom()),
            egui::pos2(rect.right(), rect.bottom()),
        ],
        Stroke::new(1.0_f32, tokens.border),
    );

    let content_rect = rect.shrink2(Vec2::new(12.0, 4.0));
    let mut row_ui = ui.new_child(egui::UiBuilder::new().max_rect(content_rect));
    let res = add_contents(&mut row_ui);

    (response, res)
}
