//! Centered modal dialog container with 60% black backdrop, 8px panel, and Esc dismiss.

use egui::{Color32, Context, Frame, Margin, Stroke, Ui};

use crate::theme::{Tokens, RADIUS_CARD};

pub fn render_modal_container<R>(
    ctx: &Context,
    tokens: &Tokens,
    title: &str,
    max_width: f32,
    add_contents: impl FnOnce(&mut Ui) -> R,
) -> Option<R> {
    let screen_rect = ctx.screen_rect();
    let painter = ctx.layer_painter(egui::LayerId::new(
        egui::Order::Foreground,
        egui::Id::new("modal_backdrop"),
    ));

    // 60% Black backdrop
    painter.rect_filled(screen_rect, 0.0, Color32::from_black_alpha(153));

    let modal_width = max_width.min(screen_rect.width() - 32.0).max(360.0);
    let modal_pos = egui::pos2(
        (screen_rect.width() - modal_width) / 2.0,
        (screen_rect.height() * 0.20).max(32.0),
    );

    let frame = Frame::new()
        .fill(tokens.bg_surface)
        .stroke(Stroke::new(1.0_f32, tokens.border_strong))
        .corner_radius(RADIUS_CARD)
        .inner_margin(Margin::same(20));

    let area = egui::Area::new(egui::Id::new("modal_window"))
        .order(egui::Order::Foreground)
        .fixed_pos(modal_pos);

    let inner = area.show(ctx, |ui| {
        ui.set_width(modal_width);
        frame
            .show(ui, |ui| {
                ui.label(
                    egui::RichText::new(title)
                        .size(17.0)
                        .strong()
                        .color(tokens.text),
                );
                ui.add_space(16.0);
                add_contents(ui)
            })
            .inner
    });

    Some(inner.inner)
}
