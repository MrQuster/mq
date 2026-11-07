//! Solid Card container with 8px corner radius and 1px border.

use egui::{Frame, Margin, Stroke, Ui};

use crate::theme::{Tokens, RADIUS_CARD};

pub fn card_panel<R>(
    ui: &mut Ui,
    tokens: &Tokens,
    add_contents: impl FnOnce(&mut Ui) -> R,
) -> egui::InnerResponse<R> {
    let frame = Frame::new()
        .fill(tokens.bg_surface)
        .stroke(Stroke::new(1.0_f32, tokens.border))
        .corner_radius(RADIUS_CARD)
        .inner_margin(Margin::same(12));

    frame.show(ui, add_contents)
}
