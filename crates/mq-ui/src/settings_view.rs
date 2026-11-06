#![deny(clippy::await_holding_lock)]
//! Global application settings screen. Zero emojis!

use egui::{RichText, Stroke, Ui};
use mq_core::GuiBackend;

use crate::state::UiState;
use crate::theme::{BORDER_SUBTLE, CARD_SURFACE, RADIUS_CARD, TEXT_PRIMARY};
use crate::widgets::{primary_emerald_button, VectorIcon};

#[derive(Default)]
pub struct SettingsView {
    pub curseforge_key_str: String,
    pub ms_client_id_str: String,
    pub threads_str: String,
}

impl SettingsView {
    pub fn ui(&mut self, ui: &mut Ui, state: &mut UiState) {
        ui.heading(RichText::new("Настройки MQ").color(TEXT_PRIMARY).strong());
        ui.add_space(8.0);

        let card = egui::Frame::new()
            .fill(CARD_SURFACE)
            .stroke(Stroke::new(1.0_f32, BORDER_SUBTLE))
            .corner_radius(RADIUS_CARD)
            .inner_margin(egui::Margin::same(12));

        card.show(ui, |ui| {
            ui.set_width(ui.available_width());

            ui.label(RichText::new("Потоки и соединения").strong());
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.label("Максимум параллельных HTTP-соединений:");
                ui.add(egui::Slider::new(
                    &mut state.config.max_connections,
                    4..=128,
                ));
            });

            ui.add_space(8.0);
            ui.separator();
            ui.add_space(8.0);

            ui.label(RichText::new("Интеграции и ключи API").strong());
            ui.add_space(4.0);
            ui.label("CurseForge Core API Key:");
            let mut cf_key = state
                .config
                .curseforge_api_key
                .as_ref()
                .map(|s| s.expose_secret().to_string())
                .unwrap_or_default();
            if ui
                .add(egui::TextEdit::singleline(&mut cf_key).password(true))
                .changed()
            {
                state.config.curseforge_api_key = if cf_key.trim().is_empty() {
                    None
                } else {
                    Some(mq_core::SecretString::new(cf_key.trim()))
                };
            }

            ui.add_space(6.0);
            ui.label("Microsoft OAuth Client ID:");
            let mut ms_id = state.config.ms_client_id.clone().unwrap_or_default();
            if ui.text_edit_singleline(&mut ms_id).changed() {
                state.config.ms_client_id = if ms_id.trim().is_empty() {
                    None
                } else {
                    Some(ms_id.trim().to_string())
                };
            }

            ui.add_space(8.0);
            ui.separator();
            ui.add_space(8.0);

            ui.label(RichText::new("Графический бэкенд (GUI)").strong());
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.selectable_value(
                    &mut state.config.ui_backend,
                    GuiBackend::Auto,
                    "Авто (Vulkan -> OpenGL)",
                );
                ui.selectable_value(
                    &mut state.config.ui_backend,
                    GuiBackend::Vulkan,
                    "Vulkan (WGPU)",
                );
                ui.selectable_value(
                    &mut state.config.ui_backend,
                    GuiBackend::Gl,
                    "OpenGL (Glow)",
                );
            });

            ui.add_space(14.0);
            if primary_emerald_button(ui, Some(VectorIcon::Check), "Сохранить настройки").clicked()
            {
                let _ = state.config.save(&state.paths);
                state.status_message = Some((
                    "Настройки успешно сохранены в config.toml".to_string(),
                    false,
                ));
            }
        });
    }
}
