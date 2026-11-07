//! Input fields with 6px corner radius, embedded vector search icon, and focus ring.

use egui::{Response, Stroke, TextEdit, Ui, Vec2};

use crate::theme::{Tokens, RADIUS_BUTTON};
use crate::widgets::{paint_vector_icon, VectorIcon};

pub fn search_input(
    ui: &mut Ui,
    tokens: &Tokens,
    query: &mut String,
    placeholder: &str,
) -> Response {
    let desired_width = ui.available_width().min(280.0);
    let (outer_rect, _) =
        ui.allocate_exact_size(Vec2::new(desired_width, 32.0), egui::Sense::hover());

    let painter = ui.painter();
    painter.rect(
        outer_rect,
        RADIUS_BUTTON,
        tokens.bg_input,
        Stroke::new(1.0_f32, tokens.border),
        egui::StrokeKind::Inside,
    );

    let icon_rect = egui::Rect::from_min_size(
        egui::pos2(outer_rect.left() + 8.0, outer_rect.center().y - 7.0),
        Vec2::splat(14.0),
    );
    paint_vector_icon(painter, icon_rect, VectorIcon::Search, tokens.text_muted);

    let text_rect = egui::Rect::from_min_max(
        egui::pos2(outer_rect.left() + 28.0, outer_rect.top() + 4.0),
        egui::pos2(outer_rect.right() - 8.0, outer_rect.bottom() - 4.0),
    );

    let mut child_ui = ui.new_child(egui::UiBuilder::new().max_rect(text_rect));
    let resp = child_ui.add(
        TextEdit::singleline(query)
            .hint_text(placeholder)
            .text_color(tokens.text)
            .frame(false),
    );

    if resp.has_focus() {
        ui.painter().rect_stroke(
            outer_rect.expand(1.0),
            RADIUS_BUTTON,
            Stroke::new(1.5_f32, tokens.accent),
            egui::StrokeKind::Outside,
        );
    }

    resp
}
