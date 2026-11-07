//! Clean 36x20px toggle switch component.

use egui::{CursorIcon, Response, Sense, Stroke, Ui, Vec2};

use crate::theme::Tokens;

pub fn toggle_switch(ui: &mut Ui, tokens: &Tokens, on: &mut bool) -> Response {
    let desired_size = Vec2::new(36.0, 20.0);
    let (rect, mut response) = ui.allocate_exact_size(desired_size, Sense::click());

    if response.hovered() {
        ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
    }

    if response.clicked() {
        *on = !*on;
        response.mark_changed();
    }

    let bg_color = if *on { tokens.accent } else { tokens.bg_input };
    let border_color = if *on { tokens.accent } else { tokens.border };

    let painter = ui.painter();
    painter.rect(
        rect,
        10.0,
        bg_color,
        Stroke::new(1.0_f32, border_color),
        egui::StrokeKind::Inside,
    );

    let circle_x = if *on {
        rect.right() - 10.0
    } else {
        rect.left() + 10.0
    };
    let knob_color = if *on {
        tokens.accent_text
    } else {
        tokens.text_muted
    };
    painter.circle_filled(egui::pos2(circle_x, rect.center().y), 7.0, knob_color);

    response
}
