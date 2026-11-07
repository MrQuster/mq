//! Filter chip component for logs and tags with colored level dots and counts.

use egui::{Color32, CursorIcon, Rect, Response, Sense, Stroke, Ui, Vec2};

use crate::theme::{Tokens, RADIUS_TAG};

/// Toggleable filter chip with optional colored level dot and count badge.
pub struct FilterChip<'a> {
    pub label: &'a str,
    pub active: bool,
    pub dot_color: Option<Color32>,
    pub count: Option<usize>,
}

impl<'a> FilterChip<'a> {
    pub fn new(label: &'a str, active: bool) -> Self {
        Self {
            label,
            active,
            dot_color: None,
            count: None,
        }
    }

    pub fn dot_color(mut self, color: Color32) -> Self {
        self.dot_color = Some(color);
        self
    }

    pub fn count(mut self, count: usize) -> Self {
        self.count = Some(count);
        self
    }

    pub fn show(self, ui: &mut Ui, tokens: &Tokens) -> Response {
        let mut width = 16.0; // Left and right padding
        if self.dot_color.is_some() {
            width += 14.0; // Dot + spacing
        }
        width += self.label.len() as f32 * 7.5;
        if let Some(cnt) = self.count {
            width += format!(" {cnt}").len() as f32 * 7.0 + 8.0;
        }
        width = width.max(48.0);

        let (rect, resp) = ui.allocate_exact_size(Vec2::new(width, 28.0), Sense::click());
        if resp.hovered() {
            ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
        }

        let painter = ui.painter();
        let bg_fill = if self.active {
            tokens.bg_surface_hover
        } else {
            tokens.bg_input
        };

        let stroke_color = if self.active {
            tokens.border_strong
        } else if resp.hovered() {
            tokens.border
        } else {
            tokens.border.gamma_multiply(0.6)
        };

        painter.rect(
            rect,
            RADIUS_TAG,
            bg_fill,
            Stroke::new(1.0_f32, stroke_color),
            egui::StrokeKind::Inside,
        );

        let mut current_x = rect.left() + 8.0;
        let center_y = rect.center().y;

        // Render level dot if present
        if let Some(dot_col) = self.dot_color {
            let dot_rect =
                Rect::from_center_size(egui::pos2(current_x + 3.0, center_y), Vec2::splat(6.0));
            painter.circle_filled(dot_rect.center(), 3.0, dot_col);
            current_x += 12.0;
        }

        // Render chip label text
        let text_color = if self.active {
            tokens.text
        } else {
            tokens.text_muted
        };

        let galley = painter.layout_no_wrap(
            self.label.to_string(),
            egui::FontId::proportional(12.0),
            text_color,
        );
        let text_w = galley.size().x;
        painter.galley(
            egui::pos2(current_x, center_y - galley.size().y * 0.5),
            galley,
            text_color,
        );
        current_x += text_w;

        // Render count badge if present
        if let Some(cnt) = self.count {
            current_x += 4.0;
            let count_str = format!("{cnt}");
            let count_galley =
                painter.layout_no_wrap(count_str, egui::FontId::monospace(10.5), tokens.text_muted);
            painter.galley(
                egui::pos2(current_x, center_y - count_galley.size().y * 0.5),
                count_galley,
                tokens.text_muted,
            );
        }

        resp
    }
}
