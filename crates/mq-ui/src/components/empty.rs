//! Clean empty and error state layouts with clear Russian copy and action buttons.

use egui::{RichText, Ui, Vec2};

use crate::components::button::button_primary;
use crate::theme::Tokens;
use crate::widgets::{paint_vector_icon, VectorIcon};

pub fn render_empty_state(
    ui: &mut Ui,
    tokens: &Tokens,
    icon: VectorIcon,
    title: &str,
    description: &str,
    action_label: Option<&str>,
) -> bool {
    let mut action_clicked = false;
    ui.vertical_centered(|ui| {
        ui.add_space(32.0);
        let (rect, _) = ui.allocate_exact_size(Vec2::splat(48.0), egui::Sense::hover());
        paint_vector_icon(ui.painter(), rect, icon, tokens.text_muted);
        ui.add_space(12.0);

        ui.label(RichText::new(title).size(15.0).strong().color(tokens.text));
        ui.add_space(4.0);
        ui.label(
            RichText::new(description)
                .size(13.0)
                .color(tokens.text_muted),
        );

        if let Some(lbl) = action_label {
            ui.add_space(16.0);
            if button_primary(ui, tokens, None, lbl).clicked() {
                action_clicked = true;
            }
        }
        ui.add_space(32.0);
    });
    action_clicked
}

pub fn render_error_state(
    ui: &mut Ui,
    tokens: &Tokens,
    message: &str,
    details: Option<&str>,
    retry_label: Option<&str>,
) -> (bool, bool) {
    let mut retry_clicked = false;
    let mut copy_clicked = false;

    ui.vertical_centered(|ui| {
        ui.add_space(24.0);
        let (rect, _) = ui.allocate_exact_size(Vec2::splat(40.0), egui::Sense::hover());
        paint_vector_icon(ui.painter(), rect, VectorIcon::Warning, tokens.danger);
        ui.add_space(8.0);

        ui.label(
            RichText::new("Произошла ошибка")
                .size(15.0)
                .strong()
                .color(tokens.danger),
        );
        ui.add_space(4.0);
        ui.label(RichText::new(message).size(13.0).color(tokens.text));

        ui.add_space(12.0);
        ui.horizontal(|ui| {
            ui.add_space((ui.available_width() - 200.0).max(0.0) / 2.0);
            if let Some(retry) = retry_label {
                if button_primary(ui, tokens, Some(VectorIcon::Refresh), retry).clicked() {
                    retry_clicked = true;
                }
            }
            if let Some(det) = details {
                if crate::components::button::button_secondary(
                    ui,
                    tokens,
                    Some(VectorIcon::Copy),
                    "Подробнее",
                )
                .clicked()
                {
                    ui.ctx().copy_text(det.to_string());
                    copy_clicked = true;
                }
            }
        });
        ui.add_space(24.0);
    });

    (retry_clicked, copy_clicked)
}
