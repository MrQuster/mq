#![deny(clippy::await_holding_lock)]
//! Global application settings & Customization Studio. Zero emojis!

use egui::{Color32, RichText, ScrollArea, Stroke, Ui, Vec2};
use mq_core::{GuiBackend, LaunchAction, ThemeConfig, ThemePreset, UiDensity};
use mq_meta::{find_mc_locale, search_mc_locales};

use crate::i18n::{tr, Language};
use crate::state::UiState;
use crate::theme::{
    apply_theme, rgb_to_c32, Tokens, ACCENT_COBALT, ACCENT_EMERALD, BORDER_SUBTLE, CARD_SURFACE,
    RADIUS_CARD, STATUS_WARN, TEXT_MUTED, TEXT_PRIMARY,
};
use crate::widgets::{
    accent_button, primary_emerald_button, secondary_glass_button, smooth_toggle, tag_pill,
    version_badge, VectorIcon,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsSection {
    Customization,
    Launch,
    Localization,
    NetworkAndApi,
    Updates,
    About,
}

#[derive(Default)]
pub struct SettingsView {
    pub active_section: Option<SettingsSection>,
    pub theme_export_path: String,
    pub theme_import_path: String,
    pub mc_locale_search: String,
    pub mc_locale_dropdown_open: bool,
    pub update_check_result: Option<String>,
    pub is_checking_update: bool,
    pub is_downloading_update: bool,
    pub update_progress: f32,
    pub available_update: Option<mq_update::ReleaseAssetInfo>,
}

impl SettingsView {
    pub fn ui(&mut self, ui: &mut Ui, state: &mut UiState) {
        let tokens = Tokens::from_config(&state.config.theme);
        let current_section = self
            .active_section
            .unwrap_or(SettingsSection::Customization);
        let lang = state.config.launcher_locale.clone();

        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = Vec2::new(12.0, 6.0);
            ui.heading(
                RichText::new(tr("tab_settings", &lang))
                    .color(tokens.text)
                    .strong(),
            );
            ui.add_space(8.0);

            // Sub-navigation tabs
            if ui
                .selectable_label(
                    current_section == SettingsSection::Customization,
                    tr("settings_customization", &lang),
                )
                .clicked()
            {
                self.active_section = Some(SettingsSection::Customization);
            }
            if ui
                .selectable_label(
                    current_section == SettingsSection::Launch,
                    tr("settings_launch", &lang),
                )
                .clicked()
            {
                self.active_section = Some(SettingsSection::Launch);
            }
            if ui
                .selectable_label(
                    current_section == SettingsSection::Localization,
                    tr("settings_language", &lang),
                )
                .clicked()
            {
                self.active_section = Some(SettingsSection::Localization);
            }
            if ui
                .selectable_label(
                    current_section == SettingsSection::NetworkAndApi,
                    "Сеть и API",
                )
                .clicked()
            {
                self.active_section = Some(SettingsSection::NetworkAndApi);
            }
            if ui
                .selectable_label(current_section == SettingsSection::Updates, "Обновления")
                .clicked()
            {
                self.active_section = Some(SettingsSection::Updates);
            }
            if ui
                .selectable_label(current_section == SettingsSection::About, "О программе")
                .clicked()
            {
                self.active_section = Some(SettingsSection::About);
            }
        });

        ui.add_space(8.0);
        ui.separator();
        ui.add_space(8.0);

        ScrollArea::vertical()
            .id_salt("settings_scroll_area")
            .show(ui, |ui| {
                match current_section {
                    SettingsSection::Customization => self.render_customization_studio(ui, state),
                    SettingsSection::Launch => self.render_launch_settings(ui, state),
                    SettingsSection::Localization => self.render_localization_settings(ui, state),
                    SettingsSection::NetworkAndApi => self.render_network_api_settings(ui, state),
                    SettingsSection::Updates => self.render_updates_settings(ui, state),
                    SettingsSection::About => self.render_about_settings(ui, state),
                }

                ui.add_space(16.0);
                ui.separator();
                ui.add_space(8.0);

                if primary_emerald_button(ui, Some(VectorIcon::Check), tr("btn_save", &lang))
                    .clicked()
                {
                    let _ = state.config.save(&state.paths);
                    apply_theme(ui.ctx(), &state.config.theme);
                    state.status_message = Some((
                        "Настройки успешно сохранены в config.toml и theme.toml".to_string(),
                        false,
                    ));
                }
            });
    }

    fn render_customization_studio(&mut self, ui: &mut Ui, state: &mut UiState) {
        let tokens = Tokens::from_config(&state.config.theme);
        let card = egui::Frame::new()
            .fill(tokens.bg_surface)
            .stroke(Stroke::new(1.0_f32, tokens.border))
            .corner_radius(RADIUS_CARD)
            .inner_margin(egui::Margin::same(14));

        card.show(ui, |ui| {
            ui.set_width(ui.available_width());

            // 1. Presets Row
            ui.label(
                RichText::new("1. Готовые пресеты тем")
                    .color(tokens.text)
                    .strong()
                    .size(14.0),
            );
            ui.add_space(4.0);

            let mut preset_changed = false;
            ui.horizontal_wrapped(|ui| {
                for preset in ThemePreset::ALL {
                    let is_active = state.config.theme.preset == preset;
                    if ui.selectable_label(is_active, preset.label()).clicked() {
                        state.config.theme.apply_preset(preset);
                        preset_changed = true;
                    }
                }
            });

            if preset_changed {
                apply_theme(ui.ctx(), &state.config.theme);
            }

            ui.add_space(10.0);
            ui.separator();
            ui.add_space(10.0);

            // 2. Live Preview Card
            ui.label(
                RichText::new("Живое превью стиля темы")
                    .color(tokens.text)
                    .strong()
                    .size(13.0),
            );
            ui.add_space(4.0);

            let preview_card = egui::Frame::new()
                .fill(Color32::from_rgba_unmultiplied(
                    state.config.theme.bg_card[0],
                    state.config.theme.bg_card[1],
                    state.config.theme.bg_card[2],
                    (state.config.theme.card_opacity * 255.0) as u8,
                ))
                .stroke(Stroke::new(
                    1.0_f32,
                    Color32::from_rgba_unmultiplied(
                        255,
                        255,
                        255,
                        (25.0 * state.config.theme.border_glow) as u8,
                    ),
                ))
                .corner_radius(egui::CornerRadius::same(
                    state.config.theme.corner_radius as u8,
                ))
                .inner_margin(egui::Margin::same(12));

            preview_card.show(ui, |ui| {
                ui.horizontal(|ui| {
                    let pri_col = rgb_to_c32(state.config.theme.accent_primary);
                    let sec_col = rgb_to_c32(state.config.theme.accent_secondary);
                    let _ = accent_button(ui, Some(VectorIcon::Play), "Кнопка действия", pri_col);
                    let _ = secondary_glass_button(ui, Some(VectorIcon::Folder), "Вторичная");
                    tag_pill(ui, "Fabric 1.21.1", sec_col);
                    let mut sample_toggle = true;
                    let _ = smooth_toggle(ui, &mut sample_toggle);
                });
            });

            ui.add_space(12.0);
            ui.separator();
            ui.add_space(10.0);

            // 3. Opacity & Glow Sliders
            ui.label(
                RichText::new("2. Прозрачность и стеклянный блик")
                    .color(tokens.text)
                    .strong()
                    .size(14.0),
            );
            ui.add_space(6.0);

            let slider_w = 180.0_f32.max((ui.available_width() - 20.0).min(360.0));
            let mut style_dirty = false;

            ui.vertical(|ui| {
                ui.label(RichText::new("Прозрачность окна (Window Opacity):").color(tokens.text));
                ui.add_space(2.0);
                if ui
                    .add_sized(
                        [slider_w, 20.0],
                        egui::Slider::new(&mut state.config.theme.window_opacity, 0.25..=1.00)
                            .step_by(0.01)
                            .trailing_fill(true),
                    )
                    .changed()
                {
                    state.config.theme.preset = ThemePreset::Custom;
                    style_dirty = true;
                }
            });

            ui.add_space(6.0);

            ui.vertical(|ui| {
                ui.label(RichText::new("Прозрачность карточек (Card Opacity):").color(tokens.text));
                ui.add_space(2.0);
                if ui
                    .add_sized(
                        [slider_w, 20.0],
                        egui::Slider::new(&mut state.config.theme.card_opacity, 0.35..=1.00)
                            .step_by(0.01),
                    )
                    .changed()
                {
                    state.config.theme.preset = ThemePreset::Custom;
                    style_dirty = true;
                }
            });

            ui.add_space(6.0);

            ui.vertical(|ui| {
                ui.label(
                    RichText::new("Стеклянный блик обводки (Glass Border Glow):")
                        .color(tokens.text),
                );
                ui.add_space(2.0);
                if ui
                    .add_sized(
                        [slider_w, 20.0],
                        egui::Slider::new(&mut state.config.theme.border_glow, 0.00..=1.00)
                            .step_by(0.05),
                    )
                    .changed()
                {
                    state.config.theme.preset = ThemePreset::Custom;
                    style_dirty = true;
                }
            });

            ui.add_space(10.0);
            ui.separator();
            ui.add_space(10.0);

            // 4. Geometry & Scale
            ui.label(
                RichText::new("3. Геометрия, масштаб и плотность")
                    .color(tokens.text)
                    .strong()
                    .size(14.0),
            );
            ui.add_space(6.0);

            ui.vertical(|ui| {
                ui.label(
                    RichText::new("Радиус скругления углов (Corner Radius):").color(tokens.text),
                );
                ui.add_space(2.0);
                if ui
                    .add_sized(
                        [slider_w, 20.0],
                        egui::Slider::new(&mut state.config.theme.corner_radius, 0.0..=20.0)
                            .suffix(" px"),
                    )
                    .changed()
                {
                    state.config.theme.preset = ThemePreset::Custom;
                    style_dirty = true;
                }
            });

            ui.add_space(6.0);

            ui.vertical(|ui| {
                ui.label(RichText::new("Масштаб интерфейса (UI Scale):").color(tokens.text));
                ui.add_space(2.0);
                if ui
                    .add_sized(
                        [slider_w, 20.0],
                        egui::Slider::new(&mut state.config.theme.ui_scale, 0.80..=1.50)
                            .step_by(0.05),
                    )
                    .changed()
                {
                    style_dirty = true;
                }
            });

            ui.add_space(6.0);

            ui.vertical(|ui| {
                ui.label(RichText::new("Размер шрифта:").color(tokens.text));
                ui.add_space(2.0);
                if ui
                    .add_sized(
                        [slider_w, 20.0],
                        egui::Slider::new(&mut state.config.theme.font_size, 12.0..=18.0)
                            .suffix(" px"),
                    )
                    .changed()
                {
                    style_dirty = true;
                }
            });

            ui.add_space(6.0);

            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = Vec2::new(8.0, 6.0);
                ui.label("Плотность компоновки (Density):");
                if ui
                    .selectable_label(
                        state.config.theme.density == UiDensity::Compact,
                        "Компактная",
                    )
                    .clicked()
                {
                    state.config.theme.density = UiDensity::Compact;
                    style_dirty = true;
                }
                if ui
                    .selectable_label(
                        state.config.theme.density == UiDensity::Comfortable,
                        "Комфортная",
                    )
                    .clicked()
                {
                    state.config.theme.density = UiDensity::Comfortable;
                    style_dirty = true;
                }
                if ui
                    .selectable_label(
                        state.config.theme.density == UiDensity::Spacious,
                        "Просторная",
                    )
                    .clicked()
                {
                    state.config.theme.density = UiDensity::Spacious;
                    style_dirty = true;
                }
            });

            ui.add_space(10.0);
            ui.separator();
            ui.add_space(10.0);

            // 5. Color Pickers
            ui.label(
                RichText::new("4. Палитра цветов (Color Pickers)")
                    .strong()
                    .size(14.0),
            );
            ui.add_space(6.0);

            ui.horizontal_wrapped(|ui| {
                ui.label("Акцентный цвет:");
                if ui
                    .color_edit_button_srgb(&mut state.config.theme.accent_primary)
                    .changed()
                {
                    state.config.theme.preset = ThemePreset::Custom;
                    style_dirty = true;
                }

                ui.add_space(10.0);
                ui.label("Вторичный акцент:");
                if ui
                    .color_edit_button_srgb(&mut state.config.theme.accent_secondary)
                    .changed()
                {
                    state.config.theme.preset = ThemePreset::Custom;
                    style_dirty = true;
                }

                ui.add_space(10.0);
                ui.label("Фон окна:");
                if ui
                    .color_edit_button_srgb(&mut state.config.theme.bg_window)
                    .changed()
                {
                    state.config.theme.preset = ThemePreset::Custom;
                    style_dirty = true;
                }

                ui.add_space(10.0);
                ui.label("Фон карточек:");
                if ui
                    .color_edit_button_srgb(&mut state.config.theme.bg_card)
                    .changed()
                {
                    state.config.theme.preset = ThemePreset::Custom;
                    style_dirty = true;
                }
            });

            ui.add_space(6.0);
            ui.horizontal_wrapped(|ui| {
                ui.label("Основной текст:");
                if ui
                    .color_edit_button_srgb(&mut state.config.theme.text_primary)
                    .changed()
                {
                    state.config.theme.preset = ThemePreset::Custom;
                    style_dirty = true;
                }

                ui.add_space(10.0);
                ui.label("Приглушённый текст:");
                if ui
                    .color_edit_button_srgb(&mut state.config.theme.text_muted)
                    .changed()
                {
                    state.config.theme.preset = ThemePreset::Custom;
                    style_dirty = true;
                }

                ui.add_space(10.0);
                ui.label("Успех:");
                if ui
                    .color_edit_button_srgb(&mut state.config.theme.status_success)
                    .changed()
                {
                    state.config.theme.preset = ThemePreset::Custom;
                    style_dirty = true;
                }

                ui.add_space(10.0);
                ui.label("Ошибка:");
                if ui
                    .color_edit_button_srgb(&mut state.config.theme.status_error)
                    .changed()
                {
                    state.config.theme.preset = ThemePreset::Custom;
                    style_dirty = true;
                }
            });

            ui.add_space(10.0);
            ui.separator();
            ui.add_space(10.0);

            // 6. Backdrop Image
            ui.label(
                RichText::new("5. Фоновое изображение (Backdrop)")
                    .strong()
                    .size(14.0),
            );
            ui.add_space(4.0);

            let mut bg_path = state.config.theme.backdrop_path.clone().unwrap_or_default();
            ui.horizontal(|ui| {
                ui.label("Путь к фоновой картинке (.png / .jpg):");
                if ui.text_edit_singleline(&mut bg_path).changed() {
                    state.config.theme.backdrop_path = if bg_path.trim().is_empty() {
                        None
                    } else {
                        Some(bg_path.trim().to_string())
                    };
                }
            });

            ui.horizontal(|ui| {
                ui.label("Затемнение подложки (Backdrop Dimming):");
                ui.add(
                    egui::Slider::new(&mut state.config.theme.backdrop_dim, 0.0..=1.0)
                        .step_by(0.05),
                );
            });

            ui.add_space(10.0);
            ui.separator();
            ui.add_space(10.0);

            // 7. Export / Import Theme
            ui.label(
                RichText::new("6. Экспорт и Импорт темы")
                    .strong()
                    .size(14.0),
            );
            ui.add_space(6.0);

            ui.horizontal(|ui| {
                ui.label("Файл для экспорта/импорта:");
                ui.text_edit_singleline(&mut self.theme_export_path);
            });

            ui.horizontal(|ui| {
                if secondary_glass_button(
                    ui,
                    Some(VectorIcon::Download),
                    "Экспортировать тему в файл",
                )
                .clicked()
                {
                    let target_path = if self.theme_export_path.trim().is_empty() {
                        state.paths.theme_file()
                    } else {
                        std::path::PathBuf::from(self.theme_export_path.trim())
                    };
                    if state.config.theme.save_to_file(&target_path).is_ok() {
                        state.status_message = Some((
                            format!("Тема успешно экспортирована в {}", target_path.display()),
                            false,
                        ));
                    }
                }

                if secondary_glass_button(
                    ui,
                    Some(VectorIcon::Folder),
                    "Импортировать тему из файла",
                )
                .clicked()
                {
                    let target_path = if self.theme_export_path.trim().is_empty() {
                        state.paths.theme_file()
                    } else {
                        std::path::PathBuf::from(self.theme_export_path.trim())
                    };
                    if let Ok(loaded) = ThemeConfig::load_from_file(&target_path) {
                        state.config.theme = loaded;
                        apply_theme(ui.ctx(), &state.config.theme);
                        state.status_message = Some((
                            format!("Тема успешно загружена из {}", target_path.display()),
                            false,
                        ));
                    } else {
                        state.status_message = Some((
                            format!("Не удалось прочитать файл темы {}", target_path.display()),
                            true,
                        ));
                    }
                }
            });

            if style_dirty {
                apply_theme(ui.ctx(), &state.config.theme);
            }
        });
    }

    fn render_launch_settings(&mut self, ui: &mut Ui, state: &mut UiState) {
        let card = egui::Frame::new()
            .fill(CARD_SURFACE)
            .stroke(Stroke::new(1.0_f32, BORDER_SUBTLE))
            .corner_radius(RADIUS_CARD)
            .inner_margin(egui::Margin::same(14));

        card.show(ui, |ui| {
            ui.set_width(ui.available_width());

            // 1. RAM Allocation
            ui.label(
                RichText::new("Выделение оперативной памяти (JVM RAM)")
                    .strong()
                    .size(14.0),
            );
            ui.add_space(4.0);

            ui.horizontal(|ui| {
                ui.label("Минимум RAM (-Xms):");
                ui.add(
                    egui::Slider::new(&mut state.config.default_memory_min_mb, 512..=8192)
                        .suffix(" MB")
                        .step_by(256.0),
                );
            });

            ui.horizontal(|ui| {
                ui.label("Максимум RAM (-Xmx):");
                ui.add(
                    egui::Slider::new(&mut state.config.default_memory_max_mb, 1024..=32768)
                        .suffix(" MB")
                        .step_by(512.0),
                );
            });

            ui.add_space(10.0);
            ui.separator();
            ui.add_space(10.0);

            // 2. Quick JVM Flag Presets
            ui.label(
                RichText::new("Быстрые пресеты аргументов JVM")
                    .strong()
                    .size(14.0),
            );
            ui.add_space(4.0);

            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = Vec2::new(8.0, 6.0);
                if secondary_glass_button(ui, Some(VectorIcon::Cpu), "Aikar's Flags (High Perf)")
                    .clicked()
                {
                    state.config.default_jvm_args = vec![
                        "-XX:+UseG1GC".to_string(),
                        "-XX:+ParallelRefProcEnabled".to_string(),
                        "-XX:MaxGCPauseMillis=200".to_string(),
                        "-XX:+UnlockExperimentalVMOptions".to_string(),
                        "-XX:+DisableExplicitGC".to_string(),
                        "-XX:+AlwaysPreTouch".to_string(),
                        "-XX:G1NewSizePercent=30".to_string(),
                        "-XX:G1MaxNewSizePercent=40".to_string(),
                        "-XX:G1ReservePercent=20".to_string(),
                        "-XX:G1HeapWastePercent=5".to_string(),
                        "-XX:G1MixedGCCountTarget=4".to_string(),
                        "-XX:InitiatingHeapOccupancyPercent=15".to_string(),
                        "-XX:G1MixedGCLiveThresholdPercent=90".to_string(),
                        "-XX:G1RSetUpdatingPauseTimePercent=5".to_string(),
                        "-XX:SurvivorRatio=32".to_string(),
                        "-XX:+PerfDisableSharedMem".to_string(),
                        "-XX:MaxTenuringThreshold=1".to_string(),
                    ];
                }

                if secondary_glass_button(ui, Some(VectorIcon::Cpu), "Java 21 Generational ZGC")
                    .clicked()
                {
                    state.config.default_jvm_args =
                        vec!["-XX:+UseZGC".to_string(), "-XX:+ZGenerational".to_string()];
                }

                if secondary_glass_button(ui, Some(VectorIcon::Cpu), "Low-End PC (Low Memory)")
                    .clicked()
                {
                    state.config.default_jvm_args =
                        vec!["-XX:+UseSerialGC".to_string(), "-Xnoclassgc".to_string()];
                }
            });

            ui.add_space(6.0);
            ui.label("Текущие аргументы JVM:");
            let mut jvm_args_str = state.config.default_jvm_args.join(" ");
            if ui.text_edit_singleline(&mut jvm_args_str).changed() {
                state.config.default_jvm_args = jvm_args_str
                    .split_whitespace()
                    .map(|s| s.to_string())
                    .collect();
            }

            ui.add_space(10.0);
            ui.separator();
            ui.add_space(10.0);

            // 3. Wrapper Command
            ui.label(
                RichText::new("Команда-обёртка запуска игры")
                    .strong()
                    .size(14.0),
            );
            ui.add_space(4.0);

            ui.horizontal(|ui| {
                if ui
                    .selectable_label(state.config.default_wrapper.is_none(), "Без обёртки")
                    .clicked()
                {
                    state.config.default_wrapper = None;
                }
                if ui
                    .selectable_label(
                        state.config.default_wrapper.as_deref() == Some("gamemoderun"),
                        "gamemoderun",
                    )
                    .clicked()
                {
                    state.config.default_wrapper = Some("gamemoderun".to_string());
                }
                if ui
                    .selectable_label(
                        state.config.default_wrapper.as_deref() == Some("mangohud"),
                        "mangohud",
                    )
                    .clicked()
                {
                    state.config.default_wrapper = Some("mangohud".to_string());
                }
                if ui
                    .selectable_label(
                        state.config.default_wrapper.as_deref() == Some("prime-run"),
                        "prime-run",
                    )
                    .clicked()
                {
                    state.config.default_wrapper = Some("prime-run".to_string());
                }
            });

            let mut custom_wrapper = state.config.default_wrapper.clone().unwrap_or_default();
            ui.horizontal(|ui| {
                ui.label("Или произвольная команда:");
                if ui.text_edit_singleline(&mut custom_wrapper).changed() {
                    state.config.default_wrapper = if custom_wrapper.trim().is_empty() {
                        None
                    } else {
                        Some(custom_wrapper.trim().to_string())
                    };
                }
            });

            ui.add_space(10.0);
            ui.separator();
            ui.add_space(10.0);

            // 4. Action upon Launch
            ui.label(
                RichText::new("Поведение лаунчера при запуске Minecraft")
                    .strong()
                    .size(14.0),
            );
            ui.add_space(4.0);

            ui.horizontal(|ui| {
                ui.selectable_value(
                    &mut state.config.theme.launch_action,
                    LaunchAction::KeepOpen,
                    "Оставить лаунчер открытым",
                );
                ui.selectable_value(
                    &mut state.config.theme.launch_action,
                    LaunchAction::Minimize,
                    "Свернуть",
                );
                ui.selectable_value(
                    &mut state.config.theme.launch_action,
                    LaunchAction::HideUntilExit,
                    "Скрыть до выхода из игры",
                );
            });

            ui.add_space(10.0);
            ui.separator();
            ui.add_space(10.0);

            // 5. Java Runtimes & 1-Click Adoptium Downloader
            ui.label(
                RichText::new("Управление Java (JRE / JDK)")
                    .strong()
                    .size(14.0),
            );
            ui.add_space(4.0);

            let runtimes = mq_launch::discover_java_runtimes();
            if runtimes.is_empty() {
                ui.label(
                    RichText::new("Установленные версии Java не найдены в системе")
                        .color(TEXT_MUTED),
                );
            } else {
                ui.label(
                    RichText::new(format!("Найдено JVM в системе: {}", runtimes.len()))
                        .color(TEXT_MUTED)
                        .small(),
                );
                for rt in runtimes.iter().take(5) {
                    ui.horizontal(|ui| {
                        version_badge(
                            ui,
                            &format!("Java {}", rt.major_version),
                            crate::theme::ACCENT_COBALT,
                        );
                        ui.label(RichText::new(&rt.vendor).color(TEXT_PRIMARY).strong());
                        ui.label(
                            RichText::new(rt.path.display().to_string())
                                .color(TEXT_MUTED)
                                .small(),
                        );
                    });
                }
            }

            ui.add_space(6.0);
            ui.label("Скачать проверенную Eclipse Temurin JRE напрямую из Adoptium:");
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = Vec2::new(8.0, 6.0);
                for &ver in &[8u32, 17u32, 21u32] {
                    let btn_label = match ver {
                        8 => "Скачать Java 8 (<= 1.16.5)",
                        17 => "Скачать Java 17 (1.17 - 1.20.4)",
                        21 => "Скачать Java 21 (1.20.5+)",
                        _ => "Скачать Java",
                    };
                    if secondary_glass_button(ui, Some(VectorIcon::Download), btn_label).clicked() {
                        let java_dir = state.paths.java_dir();
                        let bus = state.event_bus.clone();
                        tokio::spawn(async move {
                            bus.publish(mq_core::Event::PhaseStarted {
                                phase: format!("Скачивание Eclipse Temurin Java {ver}"),
                                total_items: 1,
                            });
                            let client = reqwest::Client::builder()
                                .timeout(std::time::Duration::from_secs(120))
                                .build()
                                .unwrap_or_default();
                            match mq_launch::download_adoptium_java(&client, ver, &java_dir).await {
                                Ok(rt) => {
                                    bus.publish(mq_core::Event::PhaseCompleted {
                                        phase: format!(
                                            "Java {ver} успешно установлена: {}",
                                            rt.full_version
                                        ),
                                        elapsed_ms: 0,
                                    });
                                }
                                Err(e) => {
                                    bus.publish(mq_core::Event::TaskError {
                                        context: format!("Загрузка Java {ver}"),
                                        message: e.to_string(),
                                    });
                                }
                            }
                        });
                    }
                }
            });
        });
    }

    fn render_localization_settings(&mut self, ui: &mut Ui, state: &mut UiState) {
        let card = egui::Frame::new()
            .fill(CARD_SURFACE)
            .stroke(Stroke::new(1.0_f32, BORDER_SUBTLE))
            .corner_radius(RADIUS_CARD)
            .inner_margin(egui::Margin::same(14));

        card.show(ui, |ui| {
            ui.set_width(ui.available_width());

            // 1. Launcher Interface Language
            ui.label(RichText::new("1. Язык интерфейса лаунчера (11 языков)").strong().size(14.0));
            ui.add_space(4.0);

            ui.horizontal_wrapped(|ui| {
                for lang in Language::ALL {
                    let is_active = state.config.launcher_locale == lang.code();
                    if ui.selectable_label(is_active, lang.native_name()).clicked() {
                        state.config.launcher_locale = lang.code().to_string();
                        // Automatically harmonize default Minecraft locale if not set
                        state.config.default_mc_locale = mq_meta::launcher_locale_to_mc_locale(lang.code()).to_string();
                    }
                }
            });

            ui.add_space(12.0);
            ui.separator();
            ui.add_space(12.0);

            // 2. Default Minecraft Game Locale (80+ locales)
            ui.label(RichText::new("2. Язык Minecraft по умолчанию (80+ языков)").strong().size(14.0));
            ui.label(RichText::new("Применяется автоматически ко всем новым инстансам и синхронизируется в options.txt перед запуском.").color(TEXT_MUTED).small());
            ui.add_space(6.0);

            let current_info = find_mc_locale(&state.config.default_mc_locale);
            let current_title = current_info
                .map(|i| i.display_label())
                .unwrap_or_else(|| state.config.default_mc_locale.clone());

            ui.horizontal(|ui| {
                ui.label("Выбранная локаль Minecraft:");
                version_badge(ui, &current_title, Color32::from_rgb(0x10, 0xB9, 0x81));
            });

            ui.add_space(6.0);
            ui.horizontal(|ui| {
                let (rect, _) = ui.allocate_exact_size(Vec2::splat(16.0), egui::Sense::hover());
                crate::widgets::paint_vector_icon(ui.painter(), rect, VectorIcon::Search, Color32::from_rgb(0x3B, 0x82, 0xF6));
                ui.add(egui::TextEdit::singleline(&mut self.mc_locale_search).hint_text("Поиск по всем 80+ языкам Minecraft (напр. українська, русский, polski)...").desired_width(340.0));
            });

            ui.add_space(6.0);
            let search_results = search_mc_locales(&self.mc_locale_search);
            ScrollArea::vertical()
                .max_height(160.0)
                .id_salt("mc_locales_scroll")
                .show(ui, |ui| {
                    for loc in search_results.iter().take(25) {
                        let is_active = state.config.default_mc_locale == loc.code;
                        if ui.selectable_label(is_active, loc.display_label()).clicked() {
                            state.config.default_mc_locale = loc.code.to_string();
                        }
                    }
                });
        });
    }

    fn render_network_api_settings(&mut self, ui: &mut Ui, state: &mut UiState) {
        let card = egui::Frame::new()
            .fill(CARD_SURFACE)
            .stroke(Stroke::new(1.0_f32, BORDER_SUBTLE))
            .corner_radius(RADIUS_CARD)
            .inner_margin(egui::Margin::same(14));

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
        });
    }

    fn render_updates_settings(&mut self, ui: &mut Ui, state: &mut UiState) {
        let max_w = ui.available_width().min(640.0);
        ui.allocate_ui_with_layout(
            Vec2::new(max_w, 0.0),
            egui::Layout::top_down(egui::Align::Min),
            |ui| {
                let card = egui::Frame::new()
                    .fill(CARD_SURFACE)
                    .stroke(Stroke::new(1.0_f32, BORDER_SUBTLE))
                    .corner_radius(RADIUS_CARD)
                    .inner_margin(egui::Margin::same(14));

                card.show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.label(RichText::new("Обновления лаунчера").strong().size(15.0));
                    ui.add_space(8.0);

                    let is_packaged = mq_update::UpdateManager::is_packaged_install();
                    if is_packaged {
                        ui.label(RichText::new("Лаунчер установлен через системный пакетный менеджер (/usr/). Для обновления используйте пакетный менеджер вашей системы.").color(STATUS_WARN));
                        ui.add_space(8.0);
                        if secondary_glass_button(ui, Some(VectorIcon::Globe), "Открыть страницу релизов на GitHub").clicked() {
                            let _ = crate::opener::open_that("https://github.com/MrQuster/mq/releases");
                        }
                    } else {
                        ui.label(RichText::new(format!("Текущая версия: v{}", env!("CARGO_PKG_VERSION"))).color(TEXT_MUTED));
                        ui.add_space(6.0);

                        if let Some(ref status) = self.update_check_result {
                            ui.label(RichText::new(status).color(TEXT_PRIMARY));
                            ui.add_space(6.0);
                        }

                        if self.is_checking_update {
                            ui.label(RichText::new("Проверка наличия обновлений...").color(ACCENT_COBALT));
                        } else if let Some(ref asset) = self.available_update.clone() {
                            ui.label(RichText::new(format!("Доступна новая версия: v{}", asset.version)).color(ACCENT_EMERALD).strong());
                            if !asset.release_notes.is_empty() {
                                ui.add_space(4.0);
                                ui.label(RichText::new("Список изменений:").small().color(TEXT_MUTED));
                                egui::Frame::new()
                                    .fill(Color32::from_rgb(0x10, 0x12, 0x16))
                                    .corner_radius(RADIUS_CARD)
                                    .inner_margin(egui::Margin::same(8))
                                    .show(ui, |ui| {
                                        ui.label(RichText::new(&asset.release_notes).color(TEXT_PRIMARY).small());
                                    });
                            }
                            ui.add_space(8.0);

                            if self.is_downloading_update {
                                ui.label(RichText::new("Загрузка и установка обновления...").color(ACCENT_COBALT));
                                crate::components::render_progress_bar(ui, &crate::theme::Tokens::matte_dark(), self.update_progress, 6.0_f32);
                            } else if primary_emerald_button(ui, Some(VectorIcon::Download), "Обновить лаунчер сейчас").clicked() {
                                self.is_downloading_update = true;
                                let mgr = mq_update::UpdateManager::new("MrQuster", "mq");
                                let asset_to_download = asset.clone();
                                let handle = tokio::runtime::Handle::current();
                                std::thread::spawn(move || {
                                    handle.block_on(async move {
                                        let res = mgr.download_and_install(&asset_to_download, |_d, _t| {}).await;
                                        match res {
                                            Ok(_path) => {
                                                let _ = mq_update::UpdateManager::restart_process(&[]);
                                            }
                                            Err(e) => {
                                                tracing::error!("Update failed: {e}");
                                            }
                                        }
                                    });
                                });
                            }
                        } else if secondary_glass_button(ui, Some(VectorIcon::Refresh), "Проверить сейчас").clicked() {
                            let mgr = mq_update::UpdateManager::new("MrQuster", "mq");
                            let handle = tokio::runtime::Handle::current();
                            let cur_ver = env!("CARGO_PKG_VERSION");
                            let res = std::thread::spawn(move || {
                                handle.block_on(async move {
                                    mgr.check_latest(cur_ver, false).await
                                })
                            }).join();

                            match res {
                                Ok(Ok(Some(info))) => {
                                    self.update_check_result = Some(format!("Найдена новая версия: v{}", info.version));
                                    self.available_update = Some(info);
                                }
                                Ok(Ok(None)) => {
                                    self.update_check_result = Some("У вас установлена самая актуальная версия лаунчера.".to_string());
                                }
                                Ok(Err(e)) => {
                                    self.update_check_result = Some(format!("Ошибка проверки обновлений: {e}"));
                                }
                                Err(_) => {
                                    self.update_check_result = Some("Ошибка потока проверки обновлений".to_string());
                                }
                            }
                        }

                        // Check if rollback is available (<exe>.old exists)
                        if let Ok(exe) = std::env::current_exe() {
                            let old = exe.with_extension("old");
                            if old.exists() {
                                ui.add_space(8.0);
                                ui.separator();
                                ui.add_space(8.0);
                                ui.label(RichText::new("Доступна предыдущая сохранённая версия.").color(TEXT_MUTED).small());
                                if secondary_glass_button(ui, Some(VectorIcon::Refresh), "Откатить до предыдущей версии").clicked() {
                                    if let Ok(()) = mq_update::UpdateManager::rollback() {
                                        state.status_message = Some(("Успешный откат к предыдущей версии. Перезапустите лаунчер.".to_string(), false));
                                    } else {
                                        state.status_message = Some(("Ошибка при откате к предыдущей версии".to_string(), true));
                                    }
                                }
                            }
                        }
                    }
                });
            },
        );
    }

    fn render_about_settings(&mut self, ui: &mut Ui, state: &mut UiState) {
        let max_w = ui.available_width().min(640.0);
        ui.allocate_ui_with_layout(
            Vec2::new(max_w, 0.0),
            egui::Layout::top_down(egui::Align::Min),
            |ui| {
                let card = egui::Frame::new()
                    .fill(CARD_SURFACE)
                    .stroke(Stroke::new(1.0_f32, BORDER_SUBTLE))
                    .corner_radius(RADIUS_CARD)
                    .inner_margin(egui::Margin::same(16));

                card.show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal(|ui| {
                        let (rect, _) = ui.allocate_exact_size(Vec2::splat(28.0), egui::Sense::hover());
                        crate::sidebar::paint_mq_emblem(ui.painter(), rect, &crate::theme::Tokens::matte_dark());
                        ui.add_space(8.0);
                        ui.vertical(|ui| {
                            ui.label(RichText::new("MQ Launcher").strong().size(18.0).color(TEXT_PRIMARY));
                            ui.label(RichText::new(format!("Версия {} Solid Edition", env!("CARGO_PKG_VERSION"))).color(TEXT_MUTED).small());
                        });
                    });

                    ui.add_space(12.0);
                    ui.separator();
                    ui.add_space(8.0);

                    ui.label(RichText::new("Автор: MrQuster").strong().color(TEXT_PRIMARY));
                    ui.label(RichText::new("Быстрый и модульный Minecraft-лаунчер нового поколения на Rust/egui.").color(TEXT_MUTED));
                    ui.label(RichText::new("Лицензия: MIT License (MrQuster)").color(TEXT_MUTED).small());

                    ui.add_space(12.0);
                    ui.separator();
                    ui.add_space(8.0);

                    ui.label(RichText::new("Ссылки и сообщество:").strong().color(TEXT_PRIMARY).small());
                    ui.add_space(6.0);

                    ui.horizontal_wrapped(|ui| {
                        ui.spacing_mut().item_spacing = Vec2::new(8.0, 8.0);

                        if secondary_glass_button(ui, Some(VectorIcon::User), "Профиль автора на GitHub").clicked() {
                            let _ = crate::opener::open_that("https://github.com/MrQuster");
                        }
                        if secondary_glass_button(ui, Some(VectorIcon::Globe), "Репозиторий").clicked() {
                            let _ = crate::opener::open_that("https://github.com/MrQuster/mq");
                        }
                        if secondary_glass_button(ui, Some(VectorIcon::Download), "Релизы").clicked() {
                            let _ = crate::opener::open_that("https://github.com/MrQuster/mq/releases");
                        }
                        if secondary_glass_button(ui, Some(VectorIcon::Warning), "Сообщить об ошибке").clicked() {
                            let _ = crate::opener::open_that("https://github.com/MrQuster/mq/issues");
                        }
                        if secondary_glass_button(ui, Some(VectorIcon::Copy), "Скопировать диагностику").clicked() {
                            let diag = format!(
                                "MQ Launcher v{}\nOS: {} ({})\nGUI Backend: {:?}\nPaths: Data={:?}, Instances={:?}",
                                env!("CARGO_PKG_VERSION"),
                                std::env::consts::OS,
                                std::env::consts::ARCH,
                                state.config.ui_backend,
                                state.paths.data_dir,
                                state.paths.instances_dir(),
                            );
                            ui.ctx().copy_text(diag);
                            state.status_message = Some(("Диагностическая информация скопирована в буфер обмена".to_string(), false));
                        }
                    });
                });
            },
        );
    }
}
