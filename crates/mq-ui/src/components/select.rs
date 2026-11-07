//! Dropdown select component for options and sort menus.

use egui::{CursorIcon, Response, Sense, Stroke, Ui, Vec2};

use crate::theme::{Tokens, RADIUS_BUTTON};
use crate::widgets::{paint_vector_icon, VectorIcon};

pub struct SelectOption<T: Clone + PartialEq> {
    pub value: T,
    pub label: String,
}

impl<T: Clone + PartialEq> SelectOption<T> {
    pub fn new(value: T, label: impl Into<String>) -> Self {
        Self {
            value,
            label: label.into(),
        }
    }
}

pub struct Select<'a, T: Clone + PartialEq> {
    pub id_source: &'a str,
    pub label_prefix: &'a str,
    pub selected: &'a mut T,
    pub options: &'a [SelectOption<T>],
}

impl<'a, T: Clone + PartialEq> Select<'a, T> {
    pub fn new(
        id_source: &'a str,
        label_prefix: &'a str,
        selected: &'a mut T,
        options: &'a [SelectOption<T>],
    ) -> Self {
        Self {
            id_source,
            label_prefix,
            selected,
            options,
        }
    }

    pub fn show(self, ui: &mut Ui, tokens: &Tokens) -> Response {
        let current_label = self
            .options
            .iter()
            .find(|opt| &opt.value == self.selected)
            .map(|opt| opt.label.as_str())
            .unwrap_or("");

        let display_text = if self.label_prefix.is_empty() {
            current_label.to_string()
        } else {
            format!("{}: {}", self.label_prefix, current_label)
        };

        let width = (display_text.len() as f32 * 7.5 + 40.0).max(120.0);
        let (rect, mut resp) = ui.allocate_exact_size(Vec2::new(width, 32.0), Sense::click());
        if resp.hovered() {
            ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
        }

        let painter = ui.painter();
        let bg = if resp.hovered() {
            tokens.bg_surface_hover
        } else {
            tokens.bg_surface
        };
        let border = if resp.hovered() {
            tokens.border_strong
        } else {
            tokens.border
        };

        painter.rect(
            rect,
            RADIUS_BUTTON,
            bg,
            Stroke::new(1.0_f32, border),
            egui::StrokeKind::Inside,
        );

        // Draw label
        let text_pos = egui::pos2(rect.left() + 10.0, rect.center().y - 7.0);
        painter.text(
            text_pos,
            egui::Align2::LEFT_TOP,
            &display_text,
            egui::FontId::proportional(12.5),
            tokens.text,
        );

        // Draw chevron
        let chev_rect = egui::Rect::from_center_size(
            egui::pos2(rect.right() - 14.0, rect.center().y),
            Vec2::splat(12.0),
        );
        paint_vector_icon(
            painter,
            chev_rect,
            VectorIcon::ChevronDown,
            tokens.text_muted,
        );

        // Popup combo menu on click
        let popup_id = ui.make_persistent_id(self.id_source);
        if resp.clicked() {
            ui.memory_mut(|mem| mem.toggle_popup(popup_id));
        }

        let mut changed = false;
        let selected_val = self.selected;
        let options = self.options;

        egui::popup::popup_below_widget(
            ui,
            popup_id,
            &resp,
            egui::PopupCloseBehavior::CloseOnClick,
            |ui| {
                ui.set_min_width(width);
                for opt in options {
                    let is_sel = &opt.value == selected_val;
                    let opt_text = egui::RichText::new(&opt.label)
                        .color(if is_sel { tokens.accent } else { tokens.text })
                        .size(12.5);
                    if ui.selectable_label(is_sel, opt_text).clicked() {
                        *selected_val = opt.value.clone();
                        changed = true;
                    }
                }
            },
        );

        if changed {
            resp.mark_changed();
        }

        resp
    }
}
