//! Solid Button and IconButton components with 6px corner radius,
//! clear visual hover/pressed/focus states, and vector icon support.

use egui::{CursorIcon, Response, Stroke, Ui, Vec2};

use crate::theme::{Tokens, RADIUS_BUTTON};
use crate::widgets::{paint_vector_icon, VectorIcon};

/// Renders a primary action button (e.g. "Играть", "Создать", "Сохранить")
/// using the single accent color (`#4C9A82` in Matte Dark) with high contrast text.
pub fn button_primary(
    ui: &mut Ui,
    tokens: &Tokens,
    icon: Option<VectorIcon>,
    text: &str,
) -> Response {
    let (rect, response) = ui.allocate_exact_size(
        Vec2::new(text.len() as f32 * 8.5 + 36.0, 34.0),
        egui::Sense::click(),
    );

    if response.hovered() {
        ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
    }

    let bg_color = if response.is_pointer_button_down_on() {
        tokens.accent
    } else if response.hovered() {
        tokens.accent_hover
    } else {
        tokens.accent
    };

    let painter = ui.painter();
    painter.rect_filled(rect, RADIUS_BUTTON, bg_color);

    if response.has_focus() {
        painter.rect_stroke(
            rect.expand(2.0),
            RADIUS_BUTTON,
            Stroke::new(2.0_f32, tokens.accent_hover),
            egui::StrokeKind::Outside,
        );
    }

    let mut content_left = rect.left() + 12.0;
    if let Some(ic) = icon {
        let icon_rect = egui::Rect::from_min_size(
            egui::pos2(content_left, rect.center().y - 8.0),
            Vec2::splat(16.0),
        );
        paint_vector_icon(painter, icon_rect, ic, tokens.accent_text);
        content_left += 22.0;
    }

    let text_pos = egui::pos2(content_left, rect.center().y - 7.0);
    painter.text(
        text_pos,
        egui::Align2::LEFT_TOP,
        text,
        egui::FontId::proportional(13.5),
        tokens.accent_text,
    );

    response
}

/// Renders a secondary / neutral button with surface background and 1px border.
pub fn button_secondary(
    ui: &mut Ui,
    tokens: &Tokens,
    icon: Option<VectorIcon>,
    text: &str,
) -> Response {
    let (rect, response) = ui.allocate_exact_size(
        Vec2::new(text.len() as f32 * 8.0 + 32.0, 32.0),
        egui::Sense::click(),
    );

    if response.hovered() {
        ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
    }

    let (bg_color, stroke_color) = if response.is_pointer_button_down_on() {
        (tokens.bg_surface_hover, tokens.accent)
    } else if response.hovered() {
        (tokens.bg_surface_hover, tokens.border_strong)
    } else {
        (tokens.bg_surface, tokens.border)
    };

    let painter = ui.painter();
    painter.rect(
        rect,
        RADIUS_BUTTON,
        bg_color,
        Stroke::new(1.0_f32, stroke_color),
        egui::StrokeKind::Inside,
    );

    if response.has_focus() {
        painter.rect_stroke(
            rect.expand(2.0),
            RADIUS_BUTTON,
            Stroke::new(2.0_f32, tokens.accent),
            egui::StrokeKind::Outside,
        );
    }

    let mut content_left = rect.left() + 10.0;
    if let Some(ic) = icon {
        let icon_rect = egui::Rect::from_min_size(
            egui::pos2(content_left, rect.center().y - 7.5),
            Vec2::splat(15.0),
        );
        paint_vector_icon(painter, icon_rect, ic, tokens.text);
        content_left += 20.0;
    }

    let text_pos = egui::pos2(content_left, rect.center().y - 6.5);
    painter.text(
        text_pos,
        egui::Align2::LEFT_TOP,
        text,
        egui::FontId::proportional(13.0),
        tokens.text,
    );

    response
}

/// Compact square icon button (32x32px) for toolbar actions (e.g. folder, refresh, menu).
pub fn icon_button(ui: &mut Ui, tokens: &Tokens, icon: VectorIcon, tooltip: &str) -> Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(32.0), egui::Sense::click());

    if response.hovered() {
        ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
    }

    let (bg, border) = if response.is_pointer_button_down_on() {
        (tokens.bg_surface_hover, tokens.accent)
    } else if response.hovered() {
        (tokens.bg_surface_hover, tokens.border_strong)
    } else {
        (tokens.bg_surface, tokens.border)
    };

    let painter = ui.painter();
    painter.rect(
        rect,
        RADIUS_BUTTON,
        bg,
        Stroke::new(1.0_f32, border),
        egui::StrokeKind::Inside,
    );

    let icon_rect = rect.shrink(8.0);
    let icon_col = if response.hovered() {
        tokens.text
    } else {
        tokens.text_muted
    };
    paint_vector_icon(painter, icon_rect, icon, icon_col);

    if !tooltip.is_empty() {
        response.on_hover_text(tooltip)
    } else {
        response
    }
}
