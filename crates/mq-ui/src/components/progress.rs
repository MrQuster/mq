//! Minimal solid progress bar component (3px line or standard 8px bar) using single accent.

use egui::{CornerRadius, Rect, Ui, Vec2};

use crate::theme::Tokens;

pub fn render_progress_bar(ui: &mut Ui, tokens: &Tokens, progress: f32, height: f32) {
    let p = crate::state::safe_progress_clamp(progress);
    let desired_width = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(
        Vec2::new(desired_width, height.max(2.0)),
        egui::Sense::hover(),
    );

    let painter = ui.painter();
    let cr = CornerRadius::same((height / 2.0).round() as u8);
    painter.rect_filled(rect, cr, tokens.bg_input);

    if p > 0.001 {
        let fill_rect = Rect::from_min_size(rect.min, Vec2::new(rect.width() * p, rect.height()));
        painter.rect_filled(fill_rect, cr, tokens.accent);
    }
}
