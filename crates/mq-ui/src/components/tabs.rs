//! Underline tab bar for detail screens (Console, Mods, Resourcepacks, Shaders, Worlds, etc.)

use egui::{CursorIcon, Sense, Stroke, Ui, Vec2};

use crate::theme::Tokens;

pub fn render_underline_tabs<T: Copy + PartialEq>(
    ui: &mut Ui,
    tokens: &Tokens,
    tabs: &[(T, &str)],
    current: &mut T,
) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing = Vec2::new(16.0, 0.0);
        for &(tab, label) in tabs {
            let is_selected = *current == tab;
            let text_color = if is_selected {
                tokens.text
            } else {
                tokens.text_muted
            };

            let (rect, response) = ui.allocate_exact_size(
                Vec2::new(label.len() as f32 * 8.0 + 8.0, 32.0),
                Sense::click(),
            );

            if response.hovered() {
                ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
            }

            if response.clicked() {
                *current = tab;
            }

            let painter = ui.painter();
            painter.text(
                egui::pos2(rect.center().x, rect.center().y - 2.0),
                egui::Align2::CENTER_CENTER,
                label,
                egui::FontId::proportional(13.5),
                text_color,
            );

            if is_selected {
                painter.line_segment(
                    [
                        egui::pos2(rect.left(), rect.bottom()),
                        egui::pos2(rect.right(), rect.bottom()),
                    ],
                    Stroke::new(2.0_f32, tokens.accent),
                );
            }
        }
    });
}
