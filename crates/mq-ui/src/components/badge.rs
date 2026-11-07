//! Compact 4px badge component for versions, loaders, and status tags.

use egui::{Stroke, Ui, Vec2};

use crate::theme::{Tokens, RADIUS_TAG};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BadgeKind {
    Neutral,
    Accent,
    Success,
    Warning,
    Danger,
}

pub fn render_badge(ui: &mut Ui, tokens: &Tokens, text: &str, kind: BadgeKind) {
    let (bg, border, fg) = match kind {
        BadgeKind::Neutral => (tokens.bg_input, tokens.border, tokens.text_muted),
        BadgeKind::Accent => (tokens.bg_surface_hover, tokens.accent, tokens.accent),
        BadgeKind::Success => (tokens.bg_input, tokens.success, tokens.success),
        BadgeKind::Warning => (tokens.bg_input, tokens.warning, tokens.warning),
        BadgeKind::Danger => (tokens.bg_input, tokens.danger, tokens.danger),
    };

    let padding = Vec2::new(6.0, 2.0);
    let galley = ui
        .painter()
        .layout_no_wrap(text.to_owned(), egui::FontId::proportional(11.0), fg);
    let size = galley.size() + padding * 2.0;

    let (rect, _response) = ui.allocate_exact_size(size, egui::Sense::hover());
    let painter = ui.painter();
    painter.rect(
        rect,
        RADIUS_TAG,
        bg,
        Stroke::new(1.0_f32, border),
        egui::StrokeKind::Inside,
    );
    painter.galley(rect.min + padding, galley, fg);
}
