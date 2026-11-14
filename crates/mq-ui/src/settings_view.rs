#![deny(clippy::await_holding_lock)]
//! Global application settings & Customization Studio. Zero emojis!

use egui::{Color32, CornerRadius, Pos2, RichText, ScrollArea, Stroke, StrokeKind, Ui, Vec2};
use mq_core::{BackgroundStyle, GuiBackend, LaunchAction, ThemeConfig, ThemePreset, UiDensity};
use mq_meta::{find_mc_locale, search_mc_locales};

use crate::i18n::{tr, Language};
use crate::state::UiState;
use crate::theme::{
    apply_theme, blend_color, rgb_to_c32, solid_card_frame_with_config, Tokens, ACCENT_COBALT,
    CALM_ACCENT_PRESETS, RADIUS_TAG,
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
    Catalog,
    Updates,
    About,
}

#[derive(Default)]
pub struct SettingsView {
    pub active_section: Option<SettingsSection>,
    pub theme_export_path: String,
    pub catalog_key_input: String,
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
                .selectable_label(current_section == SettingsSection::Catalog, "Каталог")
                .clicked()
            {
                self.active_section = Some(SettingsSection::Catalog);
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
                    SettingsSection::Catalog => self.render_catalog_settings(ui, state),
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
        let card = solid_card_frame_with_config(&tokens, &state.config.theme)
            .inner_margin(egui::Margin::same(16));

        fn choice_chip(ui: &mut Ui, tokens: &Tokens, selected: bool, label: &str) -> bool {
            let fill = if selected {
                blend_color(
                    tokens.bg_surface,
                    tokens.accent,
                    if tokens.is_light() { 0.16 } else { 0.22 },
                )
            } else {
                tokens.bg_input
            };
            let stroke = if selected {
                tokens.accent
            } else {
                tokens.border
            };
            let fg = if selected {
                tokens.text
            } else {
                tokens.text_muted
            };
            ui.add(
                egui::Button::new(RichText::new(label).color(fg).size(12.5))
                    .fill(fill)
                    .stroke(Stroke::new(1.0_f32, stroke))
                    .corner_radius(RADIUS_TAG),
            )
            .clicked()
        }

        fn accent_swatch_chip(
            ui: &mut Ui,
            tokens: &Tokens,
            name: &str,
            primary: [u8; 3],
            selected: bool,
        ) -> bool {
            let swatch = rgb_to_c32(primary);
            let font_id = egui::FontId::proportional(12.5);
            let fg = if selected {
                tokens.text
            } else {
                tokens.text_muted
            };
            let galley = ui.painter().layout_no_wrap(name.to_owned(), font_id, fg);
            let desired = Vec2::new(10.0 + 12.0 + 6.0 + galley.size().x + 10.0, 28.0);
            let (rect, resp) = ui.allocate_exact_size(desired, egui::Sense::click());
            if resp.hovered() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            }
            if ui.is_rect_visible(rect) {
                let p = ui.painter();
                let fill = if selected {
                    blend_color(tokens.bg_surface, swatch, 0.18)
                } else if resp.hovered() {
                    tokens.bg_surface_hover
                } else {
                    tokens.bg_input
                };
                let stroke_col = if selected {
                    swatch
                } else if resp.hovered() {
                    tokens.border_strong
                } else {
                    tokens.border
                };
                p.rect_filled(rect, RADIUS_TAG, fill);
                p.rect_stroke(
                    rect,
                    RADIUS_TAG,
                    Stroke::new(if selected { 1.5_f32 } else { 1.0_f32 }, stroke_col),
                    StrokeKind::Inside,
                );
                let dot_rect = egui::Rect::from_center_size(
                    Pos2::new(rect.left() + 14.0, rect.center().y),
                    Vec2::splat(12.0),
                );
                p.rect_filled(dot_rect, CornerRadius::same(3), swatch);
                p.galley(
                    Pos2::new(
                        dot_rect.right() + 6.0,
                        rect.center().y - galley.size().y * 0.5,
                    ),
                    galley,
                    fg,
                );
            }
            resp.clicked()
        }

        card.show(ui, |ui| {
            ui.set_width(ui.available_width());

            let mut style_dirty = false;
            let mut should_save = false;

            // 1. Presets & Accent Palette
            ui.label(
                RichText::new("1. Тема и акцентный цвет")
                    .color(tokens.text)
                    .strong()
                    .size(14.0),
            );
            ui.add_space(6.0);

            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = Vec2::new(6.0, 6.0);
                for preset in ThemePreset::ALL {
                    let is_active = state.config.theme.preset == preset;
                    if choice_chip(ui, &tokens, is_active, preset.label()) {
                        state.config.theme.apply_preset(preset);
                        style_dirty = true;
                        should_save = true;
                    }
                }
            });

            ui.add_space(8.0);
            ui.label(
                RichText::new("Спокойные акценты:")
                    .color(tokens.text_muted)
                    .small(),
            );
            ui.add_space(4.0);
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = Vec2::new(6.0, 6.0);
                for swatch in CALM_ACCENT_PRESETS {
                    let is_sel = state.config.theme.accent_primary == swatch.primary;
                    if accent_swatch_chip(ui, &tokens, swatch.name, swatch.primary, is_sel) {
                        state.config.theme.accent_primary = swatch.primary;
                        state.config.theme.accent_secondary = swatch.secondary;
                        state.config.theme.status_success = swatch.primary;
                        style_dirty = true;
                        should_save = true;
                    }
                }
            });

            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.label(RichText::new("Свой цвет акцента:").color(tokens.text));
                if ui
                    .color_edit_button_srgb(&mut state.config.theme.accent_primary)
                    .changed()
                {
                    let pri = state.config.theme.accent_primary;
                    let sec = blend_color(
                        rgb_to_c32(pri),
                        Color32::BLACK,
                        if tokens.is_light() { 0.18 } else { 0.14 },
                    );
                    state.config.theme.accent_secondary = [sec.r(), sec.g(), sec.b()];
                    state.config.theme.status_success = pri;
                    style_dirty = true;
                    should_save = true;
                }
            });

            ui.add_space(10.0);
            ui.separator();
            ui.add_space(10.0);

            // 2. Background Style & Custom Backdrop
            ui.label(
                RichText::new("2. Фон лаунчера")
                    .color(tokens.text)
                    .strong()
                    .size(14.0),
            );
            ui.add_space(6.0);

            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = Vec2::new(6.0, 6.0);
                for mode in BackgroundStyle::ALL {
                    let is_sel = state.config.theme.background_style == mode;
                    if choice_chip(ui, &tokens, is_sel, mode.label()) {
                        state.config.theme.background_style = mode;
                        should_save = true;
                    }
                }
            });

            if state.config.theme.background_style == BackgroundStyle::CustomImage {
                ui.add_space(6.0);
                let mut bg_path = state.config.theme.backdrop_path.clone().unwrap_or_default();
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Изображение:").color(tokens.text));
                    if ui
                        .add(
                            egui::TextEdit::singleline(&mut bg_path)
                                .hint_text("Путь к .png / .jpg / .webp")
                                .desired_width((ui.available_width() - 100.0).max(160.0)),
                        )
                        .changed()
                    {
                        state.config.theme.backdrop_path = if bg_path.trim().is_empty() {
                            None
                        } else {
                            Some(bg_path.trim().to_string())
                        };
                        should_save = true;
                    }
                    if state.config.theme.backdrop_path.is_some()
                        && secondary_glass_button(ui, None, "Очистить").clicked()
                    {
                        state.config.theme.backdrop_path = None;
                        should_save = true;
                    }
                });

                let dim_resp = ui.horizontal(|ui| {
                    ui.label(RichText::new("Затемнение подложки:").color(tokens.text));
                    ui.add(
                        egui::Slider::new(&mut state.config.theme.backdrop_dim, 0.0..=0.90)
                            .step_by(0.05),
                    )
                });
                if dim_resp.inner.drag_stopped() {
                    should_save = true;
                }
            }

            ui.add_space(10.0);
            ui.separator();
            ui.add_space(10.0);

            // 3. Live Preview Card
            ui.label(
                RichText::new("Живое превью")
                    .color(tokens.text)
                    .strong()
                    .size(13.0),
            );
            ui.add_space(4.0);

            let live_tokens = Tokens::from_config(&state.config.theme);
            let preview_card = solid_card_frame_with_config(&live_tokens, &state.config.theme)
                .inner_margin(egui::Margin::same(12));

            preview_card.show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    let pri_col = rgb_to_c32(state.config.theme.accent_primary);
                    let sec_col = rgb_to_c32(state.config.theme.accent_secondary);
                    let _ = accent_button(ui, Some(VectorIcon::Play), "Основная кнопка", pri_col);
                    let _ = secondary_glass_button(ui, Some(VectorIcon::Folder), "Вторичная");
                    tag_pill(ui, "Fabric", sec_col);
                    let mut sample_toggle = true;
                    let _ = smooth_toggle(ui, &mut sample_toggle);
                });
            });

            ui.add_space(10.0);
            ui.separator();
            ui.add_space(10.0);

            // 4. Surfaces, Radius, Density & Scale
            ui.label(
                RichText::new("3. Карточки, скругления и масштаб")
                    .color(tokens.text)
                    .strong()
                    .size(14.0),
            );
            ui.add_space(6.0);

            ui.add_space(4.0);
            egui::Grid::new("customization_sliders_grid")
                .num_columns(2)
                .spacing(Vec2::new(20.0, 8.0))
                .show(ui, |ui| {
                    let mut slider_row = |label: &str, slider: egui::Slider<'_>| {
                        ui.label(RichText::new(label).color(tokens.text));
                        ui.scope(|ui| {
                            ui.spacing_mut().slider_width = 210.0;
                            let r = ui.add(slider);
                            if r.changed() {
                                style_dirty = true;
                            }
                            if r.drag_stopped() {
                                should_save = true;
                            }
                        });
                        ui.end_row();
                    };

                    slider_row(
                        "Прозрачность карточек:",
                        egui::Slider::new(&mut state.config.theme.card_opacity, 0.45..=1.00)
                            .step_by(0.01),
                    );
                    slider_row(
                        "Акцентная обводка карточек:",
                        egui::Slider::new(&mut state.config.theme.border_glow, 0.00..=1.00)
                            .step_by(0.05),
                    );
                    slider_row(
                        "Скругление углов:",
                        egui::Slider::new(&mut state.config.theme.corner_radius, 0.0..=16.0)
                            .step_by(1.0)
                            .suffix(" px"),
                    );
                    slider_row(
                        "Масштаб интерфейса:",
                        egui::Slider::new(&mut state.config.theme.ui_scale, 0.80..=1.50)
                            .step_by(0.05),
                    );
                    slider_row(
                        "Размер шрифта:",
                        egui::Slider::new(&mut state.config.theme.font_size, 12.0..=18.0)
                            .step_by(0.5)
                            .suffix(" px"),
                    );
                });

            ui.add_space(6.0);
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = Vec2::new(8.0, 6.0);
                ui.label(RichText::new("Плотность:").color(tokens.text));
                for (density, label) in [
                    (UiDensity::Compact, "Компактная"),
                    (UiDensity::Comfortable, "Комфортная"),
                    (UiDensity::Spacious, "Просторная"),
                ] {
                    if choice_chip(ui, &tokens, state.config.theme.density == density, label) {
                        state.config.theme.density = density;
                        style_dirty = true;
                        should_save = true;
                    }
                }
            });

            ui.add_space(10.0);
            ui.separator();
            ui.add_space(8.0);

            egui::CollapsingHeader::new(
                RichText::new("Тонкая настройка цветов и экспорт").color(tokens.text),
            )
            .default_open(false)
            .show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.label(RichText::new("Фон окна:").color(tokens.text));
                    if ui
                        .color_edit_button_srgb(&mut state.config.theme.bg_window)
                        .changed()
                    {
                        state.config.theme.preset = ThemePreset::Custom;
                        style_dirty = true;
                        should_save = true;
                    }
                    ui.add_space(8.0);
                    ui.label(RichText::new("Фон карточек:").color(tokens.text));
                    if ui
                        .color_edit_button_srgb(&mut state.config.theme.bg_card)
                        .changed()
                    {
                        state.config.theme.preset = ThemePreset::Custom;
                        style_dirty = true;
                        should_save = true;
                    }
                    ui.add_space(8.0);
                    ui.label(RichText::new("Текст:").color(tokens.text));
                    if ui
                        .color_edit_button_srgb(&mut state.config.theme.text_primary)
                        .changed()
                    {
                        state.config.theme.preset = ThemePreset::Custom;
                        style_dirty = true;
                        should_save = true;
                    }
                    ui.add_space(8.0);
                    ui.label(RichText::new("Приглушённый:").color(tokens.text));
                    if ui
                        .color_edit_button_srgb(&mut state.config.theme.text_muted)
                        .changed()
                    {
                        state.config.theme.preset = ThemePreset::Custom;
                        style_dirty = true;
                        should_save = true;
                    }
                });

                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Файл темы:").color(tokens.text));
                    ui.text_edit_singleline(&mut self.theme_export_path);
                });
                ui.add_space(4.0);
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
                            let _ = state.config.save(&state.paths);
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
            });

            if style_dirty {
                apply_theme(ui.ctx(), &state.config.theme);
            }
            if should_save {
                let _ = state.config.save(&state.paths);
            }
        });
    }

    fn render_launch_settings(&mut self, ui: &mut Ui, state: &mut UiState) {
        let tokens = Tokens::from_config(&state.config.theme);
        let card = solid_card_frame_with_config(&tokens, &state.config.theme)
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

            let tokens = Tokens::from_config(&state.config.theme);
            let runtimes = mq_launch::discover_java_runtimes();
            if runtimes.is_empty() {
                ui.label(
                    RichText::new("Установленные версии Java не найдены в системе")
                        .color(tokens.text_muted),
                );
            } else {
                ui.label(
                    RichText::new(format!("Найдено JVM в системе: {}", runtimes.len()))
                        .color(tokens.text_muted)
                        .small(),
                );
                for rt in runtimes.iter().take(5) {
                    ui.horizontal(|ui| {
                        version_badge(
                            ui,
                            &format!("Java {}", rt.major_version),
                            crate::theme::ACCENT_COBALT,
                        );
                        ui.label(RichText::new(&rt.vendor).color(tokens.text).strong());
                        ui.label(
                            RichText::new(rt.path.display().to_string())
                                .color(tokens.text_muted)
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
        let tokens = Tokens::from_config(&state.config.theme);
        let card = solid_card_frame_with_config(&tokens, &state.config.theme)
            .inner_margin(egui::Margin::same(14));

        card.show(ui, |ui| {
            ui.set_width(ui.available_width());

            // 1. Launcher Interface Language
            ui.label(RichText::new("1. Язык интерфейса лаунчера (11 языков)").color(tokens.text).strong().size(14.0));
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
            ui.label(RichText::new("2. Язык Minecraft по умолчанию (80+ языков)").color(tokens.text).strong().size(14.0));
            ui.label(RichText::new("Применяется автоматически ко всем новым инстансам и синхронизируется в options.txt перед запуском.").color(tokens.text_muted).small());
            ui.add_space(6.0);

            let current_info = find_mc_locale(&state.config.default_mc_locale);
            let current_title = current_info
                .map(|i| i.display_label())
                .unwrap_or_else(|| state.config.default_mc_locale.clone());

            ui.horizontal(|ui| {
                ui.label(RichText::new("Выбранная локаль Minecraft:").color(tokens.text));
                version_badge(ui, &current_title, tokens.accent);
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

    fn render_catalog_settings(&mut self, ui: &mut Ui, state: &mut UiState) {
        let tokens = Tokens::from_config(&state.config.theme);
        ui.label(RichText::new("Ключ CurseForge").color(tokens.text).strong());
        ui.add_space(4.0);
        let from_env = std::env::var("MQ_CURSEFORGE_API_KEY").is_ok_and(|v| !v.trim().is_empty());
        let saved = state.config.curseforge_api_key.is_some();
        let status = if saved {
            "Ключ сохранён (файл с правами 0600)."
        } else if from_env {
            "Используется ключ из переменной MQ_CURSEFORGE_API_KEY."
        } else {
            "Ключ не задан: CurseForge отключён, Modrinth работает полностью."
        };
        ui.label(RichText::new(status).color(tokens.text_muted));
        ui.add_space(4.0);
        ui.label(
            RichText::new(
                "Получите бесплатный личный ключ в консоли CurseForge и вставьте его ниже. \
                 Общий ключ в лаунчер не вшит.",
            )
            .color(tokens.text_muted),
        );
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            if secondary_glass_button(ui, None, "Открыть страницу ключей").clicked()
            {
                let _ = crate::opener::open_that(crate::catalog_view::CURSEFORGE_KEYS_URL);
            }
        });
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            ui.add(
                egui::TextEdit::singleline(&mut self.catalog_key_input)
                    .password(true)
                    .hint_text("API-ключ CurseForge")
                    .desired_width(320.0),
            );
            let can_save = !self.catalog_key_input.trim().is_empty();
            if primary_emerald_button(ui, Some(VectorIcon::Check), "Сохранить ключ").clicked()
                && can_save
            {
                let key = mq_core::SecretString::new(self.catalog_key_input.trim());
                state.config.curseforge_api_key = Some(key);
                let _ = state.config.save(&state.paths);
                self.catalog_key_input.clear();
                state
                    .catalog
                    .mark_page_changed(std::time::Instant::now(), false);
            }
            if saved && secondary_glass_button(ui, None, "Удалить ключ").clicked() {
                state.config.curseforge_api_key = None;
                let _ = state.config.save(&state.paths);
                let _ = mq_core::AppConfig::save_cf_api_key_secret_file(&state.paths, None);
            }
        });
    }

    fn render_network_api_settings(&mut self, ui: &mut Ui, state: &mut UiState) {
        let tokens = Tokens::from_config(&state.config.theme);
        let card = solid_card_frame_with_config(&tokens, &state.config.theme)
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
        let tokens = Tokens::from_config(&state.config.theme);
        let max_w = ui.available_width().min(640.0);
        ui.allocate_ui_with_layout(
            Vec2::new(max_w, 0.0),
            egui::Layout::top_down(egui::Align::Min),
            |ui| {
                let card = solid_card_frame_with_config(&tokens, &state.config.theme)
                    .inner_margin(egui::Margin::same(14));

                card.show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.label(RichText::new("Обновления лаунчера").color(tokens.text).strong().size(15.0));
                    ui.add_space(8.0);

                    let is_packaged = mq_update::UpdateManager::is_packaged_install();
                    if is_packaged {
                        ui.label(RichText::new("Лаунчер установлен через системный пакетный менеджер (/usr/). Для обновления используйте пакетный менеджер вашей системы.").color(tokens.warning));
                        ui.add_space(8.0);
                        if secondary_glass_button(ui, Some(VectorIcon::Globe), "Открыть страницу релизов на GitHub").clicked() {
                            let _ = crate::opener::open_that("https://github.com/MrQuster/mq/releases");
                        }
                    } else {
                        ui.label(RichText::new(format!("Текущая версия: v{}", env!("CARGO_PKG_VERSION"))).color(tokens.text_muted));
                        ui.add_space(6.0);

                        if let Some(ref status) = self.update_check_result {
                            ui.label(RichText::new(status).color(tokens.text));
                            ui.add_space(6.0);
                        }

                        if self.is_checking_update {
                            ui.label(RichText::new("Проверка наличия обновлений...").color(ACCENT_COBALT));
                        } else if let Some(ref asset) = self.available_update.clone() {
                            ui.label(RichText::new(format!("Доступна новая версия: v{}", asset.version)).color(tokens.accent).strong());
                            if !asset.release_notes.is_empty() {
                                ui.add_space(4.0);
                                ui.label(RichText::new("Список изменений:").small().color(tokens.text_muted));
                                egui::Frame::new()
                                    .fill(tokens.bg_input)
                                    .stroke(Stroke::new(1.0_f32, tokens.border))
                                    .corner_radius(crate::theme::RADIUS_CARD)
                                    .inner_margin(egui::Margin::same(8))
                                    .show(ui, |ui| {
                                        ui.label(RichText::new(&asset.release_notes).color(tokens.text).small());
                                    });
                            }
                            ui.add_space(8.0);

                            if self.is_downloading_update {
                                ui.label(RichText::new("Загрузка и установка обновления...").color(ACCENT_COBALT));
                                crate::components::render_progress_bar(ui, &tokens, self.update_progress, 6.0_f32);
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
                                ui.label(RichText::new("Доступна предыдущая сохранённая версия.").color(tokens.text_muted).small());
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
        let tokens = Tokens::from_config(&state.config.theme);
        let max_w = ui.available_width().min(640.0);
        ui.allocate_ui_with_layout(
            Vec2::new(max_w, 0.0),
            egui::Layout::top_down(egui::Align::Min),
            |ui| {
                let card = solid_card_frame_with_config(&tokens, &state.config.theme)
                    .inner_margin(egui::Margin::same(16));

                card.show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal(|ui| {
                        let (rect, _) = ui.allocate_exact_size(Vec2::splat(28.0), egui::Sense::hover());
                        crate::sidebar::paint_mq_emblem(ui.painter(), rect, &tokens);
                        ui.add_space(8.0);
                        ui.vertical(|ui| {
                            ui.label(RichText::new("MQ Launcher").strong().size(18.0).color(tokens.text));
                            ui.label(RichText::new(format!("Версия {} Solid Edition", env!("CARGO_PKG_VERSION"))).color(tokens.text_muted).small());
                        });
                    });

                    ui.add_space(12.0);
                    ui.separator();
                    ui.add_space(8.0);

                    ui.label(RichText::new("Автор: MrQuster").strong().color(tokens.text));
                    ui.label(RichText::new("Быстрый и модульный Minecraft-лаунчер нового поколения на Rust/egui.").color(tokens.text_muted));
                    ui.label(RichText::new("Лицензия: MIT License (MrQuster)").color(tokens.text_muted).small());

                    ui.add_space(12.0);
                    ui.separator();
                    ui.add_space(8.0);

                    ui.label(RichText::new("Ссылки и сообщество:").strong().color(tokens.text).small());
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
