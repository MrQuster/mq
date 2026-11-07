//! Input fields and search fields with standard 32px control height,
//! embedded vector search icon, clear button, focus ring, and Ctrl+F hotkey.

use egui::{CursorIcon, Key, Modifiers, Response, Stroke, TextEdit, Ui, Vec2};

use crate::theme::{Tokens, RADIUS_BUTTON};
use crate::widgets::{paint_vector_icon, VectorIcon};

pub fn search_input(
    ui: &mut Ui,
    tokens: &Tokens,
    query: &mut String,
    placeholder: &str,
) -> Response {
    SearchField::new(query, placeholder).show(ui, tokens)
}

/// Search field component satisfying specification B6 and CONTRACTS.md.
pub struct SearchField<'a> {
    pub query: &'a mut String,
    pub placeholder: &'a str,
    pub width: Option<f32>,
    pub focus_on_ctrl_f: bool,
}

impl<'a> SearchField<'a> {
    pub fn new(query: &'a mut String, placeholder: &'a str) -> Self {
        Self {
            query,
            placeholder,
            width: None,
            focus_on_ctrl_f: true,
        }
    }

    pub fn width(mut self, width: f32) -> Self {
        self.width = Some(width);
        self
    }

    pub fn focus_on_ctrl_f(mut self, focus: bool) -> Self {
        self.focus_on_ctrl_f = focus;
        self
    }

    pub fn show(self, ui: &mut Ui, tokens: &Tokens) -> Response {
        let desired_width = self
            .width
            .unwrap_or_else(|| ui.available_width().min(280.0));
        let (outer_rect, _) =
            ui.allocate_exact_size(Vec2::new(desired_width, 32.0), egui::Sense::hover());

        ui.painter().rect(
            outer_rect,
            RADIUS_BUTTON,
            tokens.bg_input,
            Stroke::new(1.0_f32, tokens.border),
            egui::StrokeKind::Inside,
        );

        // Magnifying glass icon on left
        let icon_rect = egui::Rect::from_min_size(
            egui::pos2(outer_rect.left() + 8.0, outer_rect.center().y - 7.0),
            Vec2::splat(14.0),
        );
        paint_vector_icon(
            ui.painter(),
            icon_rect,
            VectorIcon::Search,
            tokens.text_muted,
        );

        let has_clear_btn = !self.query.is_empty();
        let right_pad = if has_clear_btn { 26.0 } else { 8.0 };

        let text_rect = egui::Rect::from_min_max(
            egui::pos2(outer_rect.left() + 28.0, outer_rect.top() + 4.0),
            egui::pos2(outer_rect.right() - right_pad, outer_rect.bottom() - 4.0),
        );

        let mut child_ui = ui.new_child(egui::UiBuilder::new().max_rect(text_rect));
        let mut text_edit = TextEdit::singleline(self.query)
            .hint_text(self.placeholder)
            .text_color(tokens.text)
            .frame(false);

        let edit_id = ui.make_persistent_id("search_field_edit");
        text_edit = text_edit.id(edit_id);
        let resp = child_ui.add(text_edit);

        // Ctrl+F shortcut focus
        if self.focus_on_ctrl_f
            && ui.input_mut(|i| {
                i.consume_key(Modifiers::COMMAND, Key::F) || i.consume_key(Modifiers::CTRL, Key::F)
            })
        {
            resp.request_focus();
        }

        // Draw clear button if query is non-empty
        if has_clear_btn {
            let clear_rect = egui::Rect::from_center_size(
                egui::pos2(outer_rect.right() - 14.0, outer_rect.center().y),
                Vec2::splat(16.0),
            );
            let clear_id = ui.make_persistent_id("search_clear_btn");
            let clear_resp = ui.interact(clear_rect, clear_id, egui::Sense::click());
            if clear_resp.hovered() {
                ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
            }
            if clear_resp.clicked() {
                self.query.clear();
            }

            let clear_color = if clear_resp.hovered() {
                tokens.text
            } else {
                tokens.text_muted
            };
            paint_vector_icon(ui.painter(), clear_rect, VectorIcon::Cross, clear_color);
        }

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
}
