//! Segmented switch component for toggling view modes (grid/list) and display options.

use egui::{CursorIcon, Response, Sense, Stroke, Ui, Vec2};

use crate::theme::{Tokens, RADIUS_BUTTON};
use crate::widgets::{paint_vector_icon, VectorIcon};

pub struct SegmentedOption<T: PartialEq + Clone> {
    pub value: T,
    pub label: String,
    pub icon: Option<VectorIcon>,
    pub tooltip: Option<String>,
}

impl<T: PartialEq + Clone> SegmentedOption<T> {
    pub fn new(value: T, label: impl Into<String>) -> Self {
        Self {
            value,
            label: label.into(),
            icon: None,
            tooltip: None,
        }
    }

    pub fn with_icon(mut self, icon: VectorIcon) -> Self {
        self.icon = Some(icon);
        self
    }

    pub fn with_tooltip(mut self, tooltip: impl Into<String>) -> Self {
        self.tooltip = Some(tooltip.into());
        self
    }
}

pub fn segmented_switch<T: PartialEq + Clone>(
    ui: &mut Ui,
    tokens: &Tokens,
    selected: &mut T,
    options: &[SegmentedOption<T>],
) -> Response {
    let height = 32.0;
    let mut total_width = 0.0;
    let mut widths = Vec::with_capacity(options.len());

    for opt in options {
        let mut w = 16.0;
        if opt.icon.is_some() {
            w += 18.0;
        }
        if !opt.label.is_empty() {
            w += opt.label.len() as f32 * 7.5 + 8.0;
        }
        w = w.max(34.0);
        widths.push(w);
        total_width += w;
    }

    let (outer_rect, mut main_resp) =
        ui.allocate_exact_size(Vec2::new(total_width, height), Sense::hover());
    let painter = ui.painter();

    painter.rect(
        outer_rect,
        RADIUS_BUTTON,
        tokens.bg_input,
        Stroke::new(1.0_f32, tokens.border),
        egui::StrokeKind::Inside,
    );

    let mut current_x = outer_rect.left();
    for (i, opt) in options.iter().enumerate() {
        let seg_w = widths[i];
        let seg_rect = egui::Rect::from_min_size(
            egui::pos2(current_x, outer_rect.top()),
            Vec2::new(seg_w, height),
        );
        let is_selected = opt.value == *selected;
        let item_id = ui.make_persistent_id(format!("seg_{i}_{}", opt.label));
        let item_resp = ui.interact(seg_rect, item_id, Sense::click());

        if item_resp.hovered() {
            ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
        }
        if item_resp.clicked() {
            *selected = opt.value.clone();
            main_resp.mark_changed();
        }

        if let Some(ref tt) = opt.tooltip {
            item_resp.clone().on_hover_text(tt);
        }

        if is_selected {
            painter.rect(
                seg_rect.shrink(1.5),
                RADIUS_BUTTON - 1_u8,
                tokens.bg_surface_hover,
                Stroke::new(1.0_f32, tokens.border_strong),
                egui::StrokeKind::Inside,
            );
        } else if item_resp.hovered() {
            painter.rect(
                seg_rect.shrink(2.0),
                RADIUS_BUTTON - 2_u8,
                tokens.bg_surface.gamma_multiply(0.5),
                Stroke::NONE,
                egui::StrokeKind::Inside,
            );
        }

        let fg = if is_selected || item_resp.hovered() {
            tokens.text
        } else {
            tokens.text_muted
        };

        if let Some(icon) = opt.icon {
            let icon_pos = if opt.label.is_empty() {
                seg_rect.center() - Vec2::splat(8.0)
            } else {
                egui::pos2(seg_rect.left() + 8.0, seg_rect.center().y - 8.0)
            };
            let icon_rect = egui::Rect::from_min_size(icon_pos, Vec2::splat(16.0));
            paint_vector_icon(painter, icon_rect, icon, fg);
        }

        if !opt.label.is_empty() {
            let text_pos = if opt.icon.is_some() {
                egui::pos2(seg_rect.left() + 28.0, seg_rect.center().y)
            } else {
                seg_rect.center()
            };
            painter.text(
                text_pos,
                if opt.icon.is_some() {
                    egui::Align2::LEFT_CENTER
                } else {
                    egui::Align2::CENTER_CENTER
                },
                &opt.label,
                egui::FontId::proportional(12.5),
                fg,
            );
        }

        current_x += seg_w;
    }

    main_resp
}
