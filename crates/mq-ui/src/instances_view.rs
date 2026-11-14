#![deny(clippy::await_holding_lock)]
//! Instances screen: instance grid, creation dialog, instance details, mods table,
//! live game console with log filtering, and JVM settings. Zero emojis!

use egui::{Color32, RichText, ScrollArea, Sense, Stroke, Ui, Vec2};
use mq_core::{GameLogLevel, ModLoaderKind};
use mq_instances::{
    backup_world, delete_world, export_instance_to_zip, export_world_to_zip,
    import_instance_from_zip, list_backups, list_screenshots, list_worlds, load_servers_dat,
    ping_server, restore_world, save_servers_dat, InstanceManifest, ServerEntry,
};

use crate::components::button::{button_primary, button_secondary};
use crate::components::{
    render_badge, render_underline_tabs, segmented_switch, BadgeKind, FilterChip, SearchField,
    SegmentedOption, Select, SelectOption,
};
use crate::state::{UiState, UiWorkerCommand};
use crate::theme::{
    Tokens, ACCENT_COBALT, ACCENT_EMERALD, BORDER_SUBTLE, CARD_SURFACE, CARD_SURFACE_HOVER,
    RADIUS_CARD, STATUS_DANGER, STATUS_WARN, TEXT_MUTED, TEXT_PRIMARY,
};
use crate::widgets::{
    danger_button, hero_action_button, loader_badge, paint_vector_icon, primary_emerald_button,
    secondary_glass_button, status_pill, version_badge, VectorIcon,
};

/// Minecraft version release category.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum McVersionCategory {
    Release,
    Snapshot,
    OldBeta,
    OldAlpha,
}

impl McVersionCategory {
    pub fn badge_label(&self) -> &'static str {
        match self {
            Self::Release => "Релиз",
            Self::Snapshot => "Снапшот",
            Self::OldBeta => "Beta",
            Self::OldAlpha => "Alpha",
        }
    }

    pub fn badge_color(&self) -> Color32 {
        match self {
            Self::Release => ACCENT_EMERALD,
            Self::Snapshot => ACCENT_COBALT,
            Self::OldBeta => Color32::from_rgb(0xF5, 0x9E, 0x0B),
            Self::OldAlpha => Color32::from_rgb(0xEC, 0x48, 0x99),
        }
    }
}

/// All official release versions of Minecraft Java Edition in reverse chronological order.
pub const ALL_MC_RELEASES: &[&str] = &[
    "1.21.4", "1.21.3", "1.21.2", "1.21", "1.20.6", "1.20.5", "1.20.4", "1.20.3", "1.20.2",
    "1.20.1", "1.20", "1.19.4", "1.19.3", "1.19.2", "1.19.1", "1.19", "1.18.2", "1.18.1", "1.18",
    "1.17.1", "1.17", "1.16.5", "1.16.4", "1.16.3", "1.16.2", "1.16.1", "1.16", "1.15.2", "1.15.1",
    "1.15", "1.14.4", "1.14.3", "1.14.2", "1.14.1", "1.14", "1.13.2", "1.13.1", "1.13", "1.12.2",
    "1.12.1", "1.12", "1.11.2", "1.11", "1.10.2", "1.10.1", "1.10", "1.9.4", "1.9.2", "1.9",
    "1.8.9", "1.8.8", "1.8", "1.7.10", "1.7.9", "1.7.2", "1.6.4", "1.6.2", "1.5.2", "1.4.7",
    "1.3.2", "1.2.5", "1.1", "1.0.0",
];

pub const ALL_MC_VERSIONS: &[&str] = ALL_MC_RELEASES;

/// Notable and recent snapshot versions.
pub const ALL_MC_SNAPSHOTS: &[&str] = &[
    "25w09b",
    "25w09a",
    "25w08a",
    "25w07a",
    "25w06a",
    "25w05a",
    "25w04a",
    "25w03a",
    "25w02a",
    "1.21.4-rc3",
    "1.21.4-rc2",
    "1.21.4-rc1",
    "1.21.4-pre2",
    "1.21.4-pre1",
    "24w46a",
    "24w45a",
    "24w44a",
    "24w40a",
    "24w39a",
    "24w38a",
    "24w37a",
    "24w36a",
    "1.21.2-rc2",
    "1.21.2-pre5",
    "1.21.2-pre1",
    "24w35a",
    "24w34a",
    "24w33a",
    "24w14a",
    "24w13a",
    "24w12a",
    "24w11a",
    "24w10a",
    "23w51b",
    "23w46a",
    "23w45a",
    "23w44a",
    "23w43a",
    "22w46a",
    "22w42a",
    "21w44a",
    "21w37a",
    "20w51a",
    "20w45a",
    "20w14infinite",
    "19w46b",
    "19w34a",
    "18w50a",
    "18w07a",
    "17w47b",
    "16w50a",
    "15w51b",
    "14w34d",
    "13w49a",
    "12w49a",
];

/// Classic Minecraft Beta versions.
pub const ALL_MC_BETAS: &[&str] = &[
    "b1.8.1", "b1.8", "b1.7.3", "b1.7.2", "b1.7", "b1.6.6", "b1.6.5", "b1.6.4", "b1.6", "b1.5_01",
    "b1.5", "b1.4_01", "b1.4", "b1.3_01", "b1.3", "b1.2_02", "b1.2_01", "b1.2", "b1.1_02",
    "b1.1_01", "b1.1", "b1.0_01", "b1.0",
];

/// Classic Minecraft Alpha / Infdev / Classic versions.
pub const ALL_MC_ALPHAS: &[&str] = &[
    "a1.2.6",
    "a1.2.5",
    "a1.2.4_01",
    "a1.2.3_04",
    "a1.2.2",
    "a1.2.1_01",
    "a1.2.0_02",
    "a1.1.2_01",
    "a1.1.0",
    "a1.0.17_04",
    "a1.0.16",
    "a1.0.15",
    "a1.0.14",
    "a1.0.5_01",
    "a1.0.4",
    "inf-20100618",
    "inf-20100415",
    "c0.30_01c",
    "c0.0.13a_03",
    "c0.0.11a",
    "rd-161348",
    "rd-132211",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoaderFilter {
    All,
    Vanilla,
    Fabric,
    Quilt,
    Forge,
    NeoForge,
}

impl LoaderFilter {
    pub fn matches(self, kind: ModLoaderKind) -> bool {
        match self {
            Self::All => true,
            Self::Vanilla => kind == ModLoaderKind::Vanilla,
            Self::Fabric => kind == ModLoaderKind::Fabric,
            Self::Quilt => kind == ModLoaderKind::Quilt,
            Self::Forge => kind == ModLoaderKind::Forge,
            Self::NeoForge => kind == ModLoaderKind::NeoForge,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InstanceDetailTab {
    Console,
    Mods,
    ResourcePacks,
    ShaderPacks,
    Worlds,
    Servers,
    Screenshots,
    Settings,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum InstanceViewMode {
    #[default]
    List,
    Grid,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum InstanceSortMode {
    #[default]
    Playtime,
    Name,
    Created,
}

pub struct InstancesView {
    pub current_tab: InstanceDetailTab,
    pub view_mode: InstanceViewMode,
    pub sort_mode: InstanceSortMode,
    pub loader_filter: LoaderFilter,
    pub search_query: String,
    pub console_autoscroll: bool,
    pub console_search: String,
    pub show_releases: bool,
    pub show_snapshots: bool,
    pub show_betas: bool,
    pub show_alphas: bool,
    pub version_search: String,
    pub show_locale_dialog: bool,
    pub locale_search_query: String,
    pub show_import_dialog: bool,
    pub import_file_path: String,
    pub import_name: String,
    pub show_duplicate_dialog: bool,
    pub duplicate_name: String,
    pub show_add_server_dialog: bool,
    pub new_server_name: String,
    pub new_server_ip: String,
    pub mod_update_check_done: bool,
    pub show_delete_dialog: bool,
    pub instance_to_delete: Option<(mq_core::InstanceId, String)>,
}

impl Default for InstancesView {
    fn default() -> Self {
        Self {
            current_tab: InstanceDetailTab::Console,
            view_mode: InstanceViewMode::Grid,
            sort_mode: InstanceSortMode::Playtime,
            loader_filter: LoaderFilter::All,
            search_query: String::new(),
            console_autoscroll: true,
            console_search: String::new(),
            show_releases: true,
            show_snapshots: false,
            show_betas: false,
            show_alphas: false,
            version_search: String::new(),
            show_locale_dialog: false,
            locale_search_query: String::new(),
            show_import_dialog: false,
            import_file_path: String::new(),
            import_name: String::new(),
            show_duplicate_dialog: false,
            duplicate_name: String::new(),
            show_add_server_dialog: false,
            new_server_name: String::new(),
            new_server_ip: String::new(),
            mod_update_check_done: false,
            show_delete_dialog: false,
            instance_to_delete: None,
        }
    }
}

impl InstancesView {
    pub fn ui(&mut self, ui: &mut Ui, state: &mut UiState) {
        if state.selected_instance_id.is_none() && !state.instances.is_empty() {
            state.selected_instance_id = Some(state.instances[0].id.clone());
        }

        let tokens = Tokens::from_config(&state.config.theme);

        if !state.show_create_instance_dialog
            && !self.show_import_dialog
            && ui.input_mut(|i| {
                i.consume_key(egui::Modifiers::CTRL, egui::Key::N)
                    || i.consume_key(egui::Modifiers::COMMAND, egui::Key::N)
            })
        {
            state.show_create_instance_dialog = true;
        }

        if !state.instances.is_empty() {
            // Row 1: Header + primary action buttons
            ui.horizontal(|ui| {
                ui.heading(
                    RichText::new("Инстансы")
                        .color(tokens.text)
                        .size(20.0)
                        .strong(),
                );

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if button_primary(ui, &tokens, Some(VectorIcon::Plus), "Создать инстанс")
                        .clicked()
                    {
                        state.show_create_instance_dialog = true;
                    }
                    ui.add_space(6.0);
                    if button_secondary(ui, &tokens, Some(VectorIcon::Download), "Импорт").clicked()
                    {
                        self.show_import_dialog = true;
                    }
                });
            });

            ui.add_space(8.0);

            // Row 2: SearchField + loader filter + sorting dropdown + view mode segmented switch
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = Vec2::new(10.0, 6.0);

                SearchField::new(&mut self.search_query, "Поиск инстанса...")
                    .width(180.0)
                    .show(ui, &tokens);

                let loader_opts = [
                    SelectOption::new(LoaderFilter::All, "Все загрузчики"),
                    SelectOption::new(LoaderFilter::Vanilla, "Vanilla"),
                    SelectOption::new(LoaderFilter::Fabric, "Fabric"),
                    SelectOption::new(LoaderFilter::Quilt, "Quilt"),
                    SelectOption::new(LoaderFilter::Forge, "Forge"),
                    SelectOption::new(LoaderFilter::NeoForge, "NeoForge"),
                ];
                Select::new(
                    "instance_loader_filter",
                    "Загрузчик",
                    &mut self.loader_filter,
                    &loader_opts,
                )
                .show(ui, &tokens);

                let sort_opts = [
                    SelectOption::new(InstanceSortMode::Playtime, "По времени"),
                    SelectOption::new(InstanceSortMode::Name, "По имени"),
                    SelectOption::new(InstanceSortMode::Created, "По дате"),
                ];
                Select::new(
                    "instance_sort_select",
                    "Сортировка",
                    &mut self.sort_mode,
                    &sort_opts,
                )
                .show(ui, &tokens);

                let view_opts = [
                    SegmentedOption::new(InstanceViewMode::Grid, "Сетка")
                        .with_icon(VectorIcon::Layers)
                        .with_tooltip("Отображение плитками"),
                    SegmentedOption::new(InstanceViewMode::List, "Список")
                        .with_icon(VectorIcon::Terminal)
                        .with_tooltip("Отображение списком"),
                ];
                segmented_switch(ui, &tokens, &mut self.view_mode, &view_opts);
            });

            ui.add_space(8.0);
        }

        // Instance creation modal
        if state.show_create_instance_dialog {
            self.render_create_instance_modal(ui, state, &tokens);
        }

        // Duplicate modal
        if self.show_duplicate_dialog {
            if let Some(selected_inst) = state.selected_instance().cloned() {
                egui::Window::new("Дублирование инстанса")
                    .collapsible(false)
                    .resizable(false)
                    .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                    .show(ui.ctx(), |ui| {
                        let modal_w = 340.0_f32.min(ui.ctx().screen_rect().width() - 32.0);
                        ui.set_width(modal_w);
                        ui.label("Имя нового инстанса:");
                        ui.text_edit_singleline(&mut self.duplicate_name);
                        ui.add_space(10.0);
                        ui.horizontal(|ui| {
                            if primary_emerald_button(ui, Some(VectorIcon::Check), "Дублировать")
                                .clicked()
                                && !self.duplicate_name.trim().is_empty()
                            {
                                match state
                                    .instance_mgr
                                    .clone_instance(&selected_inst.name, &self.duplicate_name)
                                {
                                    Ok(cloned) => {
                                        state.instances.push(cloned.clone());
                                        state.selected_instance_id = Some(cloned.id);
                                        state.status_message = Some((
                                            format!(
                                                "Инстанс успешно продублирован как '{}'",
                                                self.duplicate_name
                                            ),
                                            false,
                                        ));
                                    }
                                    Err(e) => {
                                        state.status_message =
                                            Some((format!("Ошибка дублирования: {e}"), true));
                                    }
                                }
                                self.show_duplicate_dialog = false;
                            }
                            if secondary_glass_button(ui, None, "Отмена").clicked() {
                                self.show_duplicate_dialog = false;
                            }
                        });
                    });
            }
        }

        // Import modal
        if self.show_import_dialog {
            egui::Window::new("Импорт инстанса (.mrpack или .zip)")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .show(ui.ctx(), |ui| {
                    let modal_w = 400.0_f32.min(ui.ctx().screen_rect().width() - 32.0);
                    ui.set_width(modal_w);
                    ui.label("Путь к архиву (.mrpack или .zip):");
                    ui.text_edit_singleline(&mut self.import_file_path);
                    ui.add_space(4.0);
                    ui.label("Имя инстанса (опционально):");
                    ui.text_edit_singleline(&mut self.import_name);
                    ui.add_space(10.0);
                    ui.horizontal(|ui| {
                        if primary_emerald_button(ui, Some(VectorIcon::Check), "Импортировать")
                            .clicked()
                        {
                            let path = std::path::Path::new(self.import_file_path.trim());
                            let opt_name = if self.import_name.trim().is_empty() {
                                None
                            } else {
                                Some(self.import_name.trim())
                            };
                            match import_instance_from_zip(
                                path,
                                &state.paths.instances_dir(),
                                opt_name,
                            ) {
                                Ok(m) => {
                                    state.instances.push(m.clone());
                                    state.selected_instance_id = Some(m.id);
                                    state.status_message =
                                        Some(("Инстанс успешно импортирован".to_string(), false));
                                    self.show_import_dialog = false;
                                }
                                Err(e) => {
                                    state.status_message =
                                        Some((format!("Ошибка импорта: {e}"), true));
                                }
                            }
                        }
                        if secondary_glass_button(ui, None, "Отмена").clicked() {
                            self.show_import_dialog = false;
                        }
                    });
                });
        }

        // Delete confirmation modal
        if self.show_delete_dialog {
            if let Some((target_id, target_name)) = self.instance_to_delete.clone() {
                let is_running =
                    state.game_running && state.running_instance.as_ref() == Some(&target_id);
                let is_downloading = state.installing_instance.as_ref() == Some(&target_id);

                egui::Window::new("Удаление инстанса")
                    .collapsible(false)
                    .resizable(false)
                    .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                    .show(ui.ctx(), |ui| {
                        let modal_w = 420.0_f32.min(ui.ctx().screen_rect().width() - 32.0);
                        ui.set_width(modal_w);

                        ui.horizontal(|ui| {
                            let (rect, _) =
                                ui.allocate_exact_size(Vec2::splat(22.0), egui::Sense::hover());
                            paint_vector_icon(
                                ui.painter(),
                                rect,
                                VectorIcon::Warning,
                                STATUS_DANGER,
                            );
                            ui.add_space(4.0);
                            ui.label(
                                RichText::new(format!("Удалить «{}»?", target_name))
                                    .color(TEXT_PRIMARY)
                                    .strong()
                                    .size(15.0),
                            );
                        });

                        ui.add_space(8.0);
                        ui.label(
                            RichText::new(
                                "Вы уверены, что хотите удалить этот инстанс? Это действие безвозвратно удалит директорию инстанса, включая все установленные моды, миры, скриншоты и настройки.",
                            )
                            .color(TEXT_MUTED)
                            .size(12.5),
                        );

                        if is_running {
                            ui.add_space(6.0);
                            ui.label(
                                RichText::new("Инстанс сейчас запущен! Закройте игру перед удалением.")
                                    .color(STATUS_DANGER)
                                    .strong(),
                            );
                        } else if is_downloading {
                            ui.add_space(6.0);
                            ui.label(
                                RichText::new("Инстанс сейчас загружается/устанавливается. Дождитесь окончания или отмените загрузку.")
                                    .color(STATUS_DANGER)
                                    .strong(),
                            );
                        }

                        ui.add_space(14.0);
                        ui.horizontal(|ui| {
                            let can_delete = !is_running && !is_downloading;
                            ui.add_enabled_ui(can_delete, |ui| {
                                if danger_button(ui, Some(VectorIcon::Trash), "Удалить").clicked() {
                                    let inst_dir = state.paths.instance_dir(target_id.as_str());
                                    if inst_dir.exists() {
                                        let _ = std::fs::remove_dir_all(&inst_dir);
                                    }
                                    let _ = state.instance_mgr.delete_instance(target_id.as_str());
                                    let id_to_remove = target_id.clone();
                                    state.instances.retain(|i| i.id != id_to_remove);
                                    state.favorite_instances.remove(target_id.as_str());
                                    if state.selected_instance_id.as_ref() == Some(&id_to_remove) {
                                        state.selected_instance_id =
                                            state.instances.first().map(|i| i.id.clone());
                                    }
                                    state.status_message = Some((
                                        format!("Инстанс «{}» успешно удален", target_name),
                                        false,
                                    ));
                                    self.show_delete_dialog = false;
                                    self.instance_to_delete = None;
                                }
                            });
                            if secondary_glass_button(ui, None, "Отмена").clicked() {
                                self.show_delete_dialog = false;
                                self.instance_to_delete = None;
                            }
                        });
                    });
            } else {
                self.show_delete_dialog = false;
            }
        }

        if state.instances.is_empty() {
            self.render_empty_onboarding_hub(ui, state, &tokens);
            return;
        }

        if self.view_mode == InstanceViewMode::Grid {
            self.render_grid_view(ui, state, &tokens);
            return;
        }

        let avail_h = ui.available_height();

        // Layout: Left column (instances list) + Right column (selected instance details)
        ui.horizontal(|ui| {
            ui.set_min_height(avail_h);

            // Left list of instances
            ui.vertical(|ui| {
                ui.set_width(220.0);
                ui.set_min_height(avail_h);
                ui.label(RichText::new("Инстансы").color(tokens.text_muted).strong());
                ui.add_space(4.0);

                ScrollArea::vertical()
                    .id_salt("instances_scroll")
                    .auto_shrink([false, false])
                    .max_height(avail_h - 40.0)
                    .show(ui, |ui| {
                        if state.instances.is_empty() {
                            ui.label(RichText::new("Нет инстансов").color(TEXT_MUTED));
                            if primary_emerald_button(ui, Some(VectorIcon::Plus), "Создать инстанс")
                                .clicked()
                            {
                                state.show_create_instance_dialog = true;
                            }
                        }

                        let mut action_delete = None;
                        let mut action_open_dir = None;
                        let mut action_duplicate = None;
                        let mut action_export = None;

                        for inst in &state.instances {
                            let is_selected = state.selected_instance_id.as_ref() == Some(&inst.id);
                            let is_installed = state.is_instance_installed(&inst.id);
                            let is_downloading =
                                state.installing_instance.as_ref() == Some(&inst.id);
                            let is_running = state.game_running
                                && state.running_instance.as_ref() == Some(&inst.id);

                            let stroke_color = if is_downloading {
                                ACCENT_COBALT
                            } else if is_running || is_selected {
                                ACCENT_EMERALD
                            } else {
                                BORDER_SUBTLE
                            };

                            let frame = egui::Frame::new()
                                .fill(if is_selected {
                                    CARD_SURFACE_HOVER
                                } else {
                                    CARD_SURFACE
                                })
                                .stroke(Stroke::new(1.0_f32, stroke_color))
                                .corner_radius(RADIUS_CARD)
                                .inner_margin(egui::Margin::same(8));

                            let resp = frame.show(ui, |ui| {
                                ui.set_width(ui.available_width());
                                ui.horizontal(|ui| {
                                    let (rect, _) = ui.allocate_exact_size(
                                        Vec2::splat(16.0),
                                        egui::Sense::hover(),
                                    );
                                    let (icon, icon_col) = if is_downloading {
                                        (VectorIcon::Download, ACCENT_COBALT)
                                    } else if is_running {
                                        (VectorIcon::Play, ACCENT_EMERALD)
                                    } else if is_installed {
                                        (
                                            VectorIcon::Layers,
                                            if is_selected {
                                                ACCENT_EMERALD
                                            } else {
                                                ACCENT_COBALT
                                            },
                                        )
                                    } else {
                                        (VectorIcon::Download, TEXT_MUTED)
                                    };
                                    paint_vector_icon(ui.painter(), rect, icon, icon_col);

                                    ui.vertical(|ui| {
                                        ui.add(
                                            egui::Label::new(
                                                RichText::new(&inst.name)
                                                    .color(TEXT_PRIMARY)
                                                    .strong(),
                                            )
                                            .truncate(),
                                        );
                                        ui.horizontal(|ui| {
                                            version_badge(ui, &inst.mc_version, ACCENT_COBALT);
                                            loader_badge(ui, inst.loader);

                                            let (dot_col, tip) = if is_downloading {
                                                (ACCENT_COBALT, "Загрузка")
                                            } else if is_running {
                                                (ACCENT_EMERALD, "Запущен")
                                            } else if is_installed {
                                                (ACCENT_EMERALD.gamma_multiply(0.7), "Установлен")
                                            } else {
                                                (STATUS_WARN, "Не скачан")
                                            };
                                            let (d_rect, d_resp) = ui.allocate_exact_size(
                                                Vec2::splat(10.0),
                                                egui::Sense::hover(),
                                            );
                                            if ui.is_rect_visible(d_rect) {
                                                ui.painter().circle_filled(
                                                    d_rect.center(),
                                                    3.5,
                                                    dot_col,
                                                );
                                            }
                                            d_resp.on_hover_text(tip);
                                        });
                                    });
                                });
                            });

                            let click_resp = resp.response.interact(egui::Sense::click());
                            if click_resp.clicked() {
                                state.selected_instance_id = Some(inst.id.clone());
                                // Trigger scanning mods
                                let _ = state.cmd_sender.send(UiWorkerCommand::ScanInstanceMods {
                                    instance_id: inst.id.clone(),
                                });
                            }

                            click_resp.context_menu(|ui| {
                                ui.set_min_width(150.0);
                                if ui.button("Открыть папку").clicked() {
                                    action_open_dir = Some(inst.id.clone());
                                    ui.close_menu();
                                }
                                if ui.button("Дублировать").clicked() {
                                    action_duplicate = Some((inst.id.clone(), inst.name.clone()));
                                    ui.close_menu();
                                }
                                if ui.button("Экспорт (.zip)").clicked() {
                                    action_export = Some(inst.id.clone());
                                    ui.close_menu();
                                }
                                ui.separator();
                                if ui
                                    .button(RichText::new("Удалить инстанс").color(STATUS_DANGER))
                                    .clicked()
                                {
                                    action_delete = Some((inst.id.clone(), inst.name.clone()));
                                    ui.close_menu();
                                }
                            });

                            ui.add_space(4.0);
                        }

                        if let Some((id, name)) = action_delete {
                            self.show_delete_dialog = true;
                            self.instance_to_delete = Some((id, name));
                        }
                        if let Some((id, name)) = action_duplicate {
                            state.selected_instance_id = Some(id);
                            self.show_duplicate_dialog = true;
                            self.duplicate_name = format!("{} (Копия)", name);
                        }
                        if let Some(id) = action_open_dir {
                            let inst_dir = state.paths.instance_dir(id.as_str());
                            let _ = crate::opener::open_that(inst_dir);
                        }
                        if let Some(id) = action_export {
                            let inst_dir = state.paths.instance_dir(id.as_str());
                            let out_path = state
                                .paths
                                .backups_dir()
                                .join(format!("{}.zip", id.as_str()));
                            if let Err(e) = export_instance_to_zip(&inst_dir, &out_path) {
                                state.status_message =
                                    Some((format!("Ошибка экспорта: {e}"), true));
                            } else {
                                state.status_message = Some((
                                    format!("Экспортировано в {}", out_path.display()),
                                    false,
                                ));
                            }
                        }
                    });
            });

            ui.separator();

            // Right selected instance details
            ui.vertical(|ui| {
                ui.set_min_height(avail_h);
                ui.set_width(ui.available_width());
                let selected_info = state.selected_instance().map(|i| {
                    (
                        i.id.clone(),
                        i.name.clone(),
                        i.mc_version.clone(),
                        i.loader,
                        i.memory_max_mb,
                    )
                });

                if let Some((inst_id, inst_name, mc_ver, loader, memory_max_mb)) = selected_info {
                    let is_installed = state.is_instance_installed(&inst_id);
                    let is_downloading = state.installing_instance.as_ref() == Some(&inst_id);
                    let is_running =
                        state.game_running && state.running_instance.as_ref() == Some(&inst_id);

                    // 1. Error Card (if any installation/verification error occurred)
                    let error_opt = if let Some(detailed) = state.detailed_install_error.clone() {
                        Some(detailed)
                    } else if let Some(err_str) = state.install_error.clone() {
                        Some(crate::install_types::InstallError::from_error_str(
                            state.current_install_stage,
                            &err_str,
                        ))
                    } else {
                        None
                    };

                    if let Some(err) = error_opt {
                        if let Some(action) = crate::widgets::render_detailed_install_error_card(
                            ui,
                            &tokens,
                            &err,
                        ) {
                            match action {
                                crate::widgets::InstallErrorCardAction::Retry => {
                                    state.install_error = None;
                                    state.detailed_install_error = None;
                                    state.installing_instance = Some(inst_id.clone());
                                    let _ = state.cmd_sender.send(UiWorkerCommand::InstallInstance {
                                        instance_id: inst_id.clone(),
                                    });
                                }
                                crate::widgets::InstallErrorCardAction::Verify => {
                                    state.install_error = None;
                                    state.detailed_install_error = None;
                                    let _ = state.cmd_sender.send(
                                        UiWorkerCommand::VerifyAndRepairInstance {
                                            instance_id: inst_id.clone(),
                                        },
                                    );
                                }
                                crate::widgets::InstallErrorCardAction::CopyDetails => {
                                    ui.ctx().copy_text(err.format_for_clipboard());
                                    state.status_message = Some((
                                        "Подробности ошибки скопированы в буфер обмена".to_string(),
                                        false,
                                    ));
                                }
                                crate::widgets::InstallErrorCardAction::OpenLog => {
                                    let logs_dir = state.paths.logs_dir();
                                    let crash_latest = logs_dir.join("crash-latest.log");
                                    let target = if crash_latest.is_file() {
                                        crash_latest
                                    } else if logs_dir.is_dir() {
                                        logs_dir
                                    } else {
                                        state.paths.instance_dir(inst_id.as_str())
                                    };
                                    let _ = crate::opener::open_that(target);
                                }
                                crate::widgets::InstallErrorCardAction::Dismiss => {
                                    state.install_error = None;
                                    state.detailed_install_error = None;
                                }
                            }
                        }
                        ui.add_space(8.0);
                    }

                    // 2. Active Download Progress Card (prominent real-time view)
                    if is_downloading {
                        let (frac, speed, comp_bytes, tot_bytes, comp_items, tot_items) =
                            if let Some(prog) = &state.phase_progress {
                                let f = crate::state::safe_progress_ratio(
                                    prog.completed_items,
                                    prog.total_items,
                                );
                                let speed = if prog.mib_per_sec.is_finite() && prog.mib_per_sec > 0.0 {
                                    prog.mib_per_sec * 1024.0 * 1024.0
                                } else {
                                    0.0
                                };
                                (
                                    f,
                                    speed,
                                    prog.bytes_done,
                                    prog.bytes_total,
                                    prog.completed_items,
                                    prog.total_items,
                                )
                            } else {
                                (0.0, 0.0, 0, 0, 0, 0)
                            };

                        crate::widgets::render_detailed_install_progress_card(
                            ui,
                            &tokens,
                            state.current_install_stage,
                            frac,
                            speed,
                            comp_bytes,
                            tot_bytes,
                            comp_items,
                            tot_items,
                            state.install_eta_secs,
                            state.install_retries_count,
                            || {
                                let _ = state.cmd_sender.send(UiWorkerCommand::CancelDownload);
                            },
                        );
                        ui.add_space(10.0);
                    }

                    // 3. Detail hero header card
                    let header_frame = egui::Frame::new()
                        .fill(CARD_SURFACE)
                        .stroke(Stroke::new(1.0_f32, BORDER_SUBTLE))
                        .corner_radius(RADIUS_CARD)
                        .inner_margin(egui::Margin::same(14));

                    header_frame.show(ui, |ui| {
                        let render_hero_button = |ui: &mut egui::Ui, state: &mut UiState| {
                            if is_running {
                                if hero_action_button(
                                    ui,
                                    VectorIcon::Stop,
                                    "ОСТАНОВИТЬ",
                                    STATUS_DANGER,
                                )
                                .clicked()
                                {
                                    let _ = state.cmd_sender.send(UiWorkerCommand::CancelDownload);
                                }
                            } else if is_downloading {
                                if hero_action_button(
                                    ui,
                                    VectorIcon::Download,
                                    "СКАЧИВАНИЕ...",
                                    ACCENT_COBALT,
                                )
                                .clicked()
                                {
                                    let _ = state.cmd_sender.send(UiWorkerCommand::CancelDownload);
                                }
                            } else if is_installed {
                                let btn_text = if state.is_launching {
                                    "Запуск..."
                                } else {
                                    "Играть"
                                };
                                if hero_action_button(
                                    ui,
                                    VectorIcon::Play,
                                    btn_text,
                                    ACCENT_EMERALD,
                                )
                                .clicked()
                                    && !state.is_launching
                                    && !is_running
                                {
                                    state.is_launching = true;
                                    let _ =
                                        state.cmd_sender.send(UiWorkerCommand::LaunchInstance {
                                            instance_id: inst_id.clone(),
                                            dry_run: false,
                                        });
                                }
                            } else {
                                let btn_text = if state.is_launching {
                                    "Скачивание и запуск..."
                                } else {
                                    "Скачать и играть"
                                };
                                if hero_action_button(
                                    ui,
                                    VectorIcon::Download,
                                    btn_text,
                                    ACCENT_EMERALD,
                                )
                                .clicked()
                                    && !state.is_launching
                                    && !is_running
                                {
                                    state.is_launching = true;
                                    let _ =
                                        state.cmd_sender.send(UiWorkerCommand::LaunchInstance {
                                            instance_id: inst_id.clone(),
                                            dry_run: false,
                                        });
                                }
                            }
                        };

                        // Row 1: Instance Title on the left, Hero Action Button on the right
                        let avail_w = ui.available_width();
                        let hero_w = if is_downloading || !is_installed {
                            210.0
                        } else {
                            160.0
                        };

                        if avail_w < 400.0 {
                            ui.add(
                                egui::Label::new(
                                    RichText::new(&inst_name)
                                        .color(TEXT_PRIMARY)
                                        .size(19.0)
                                        .strong(),
                                )
                                .truncate(),
                            );
                            ui.add_space(6.0);
                            render_hero_button(ui, state);
                        } else {
                            ui.horizontal(|ui| {
                                let title_w = (ui.available_width() - hero_w - 12.0).max(100.0);
                                ui.allocate_ui_with_layout(
                                    Vec2::new(title_w, 40.0),
                                    egui::Layout::left_to_right(egui::Align::Center),
                                    |ui| {
                                        ui.add(
                                            egui::Label::new(
                                                RichText::new(&inst_name)
                                                    .color(TEXT_PRIMARY)
                                                    .size(20.0)
                                                    .strong(),
                                            )
                                            .truncate(),
                                        );
                                    },
                                );
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        render_hero_button(ui, state);
                                    },
                                );
                            });
                        }

                        ui.add_space(8.0);

                        // Row 2: Loader & version text + neutral RAM badge + status (B8)
                        ui.horizontal_wrapped(|ui| {
                            ui.spacing_mut().item_spacing = Vec2::new(8.0, 6.0);
                            let loader_str = match loader {
                                ModLoaderKind::Vanilla => "Vanilla",
                                ModLoaderKind::Fabric => "Fabric",
                                ModLoaderKind::Quilt => "Quilt",
                                ModLoaderKind::Forge => "Forge",
                                ModLoaderKind::NeoForge => "NeoForge",
                            };
                            ui.label(
                                RichText::new(format!("{loader_str} · {mc_ver}"))
                                    .font(egui::FontId::proportional(13.0))
                                    .color(TEXT_PRIMARY),
                            );

                            let ram_str = if memory_max_mb >= 1024 {
                                format!("{:.0} ГБ ОЗУ", memory_max_mb as f32 / 1024.0)
                            } else {
                                format!("{memory_max_mb} МБ ОЗУ")
                            };
                            render_badge(ui, &tokens, &ram_str, BadgeKind::Neutral);

                            if is_running {
                                status_pill(ui, "Игра запущена", ACCENT_EMERALD);
                            } else if is_downloading {
                                status_pill(ui, "Скачивание файлов...", ACCENT_COBALT);
                            } else if is_installed {
                                status_pill(ui, "Готов к запуску", ACCENT_EMERALD);
                            } else {
                                status_pill(ui, "Требуется установка", STATUS_WARN);
                            }
                        });

                        ui.add_space(8.0);

                        // Row 3: Actions toolbar (B10, B9)
                        ui.horizontal_wrapped(|ui| {
                            ui.spacing_mut().item_spacing = Vec2::new(8.0, 6.0);

                            if secondary_glass_button(ui, Some(VectorIcon::Folder), "Папка игры")
                                .on_hover_text("Открыть директорию инстанса в файловом менеджере")
                                .clicked()
                            {
                                let inst_dir = state.paths.instance_dir(inst_id.as_str());
                                let _ = crate::opener::open_that(inst_dir);
                            }

                            if is_installed {
                                if secondary_glass_button(ui, Some(VectorIcon::Verify), "Проверить")
                                    .on_hover_text("Проверить целостность и починить повреждённые файлы")
                                    .clicked()
                                {
                                    let _ =
                                        state.cmd_sender.send(UiWorkerCommand::VerifyAndRepairInstance {
                                            instance_id: inst_id.clone(),
                                        });
                                }
                                if secondary_glass_button(ui, Some(VectorIcon::Refresh), "Переустановить")
                                    .on_hover_text("Переустановить версию без потери миров (saves) и настроек (options.txt)")
                                    .clicked()
                                {
                                    let _ = state.cmd_sender.send(
                                        UiWorkerCommand::ReinstallInstance {
                                            instance_id: inst_id.clone(),
                                        },
                                    );
                                }
                            } else if !is_downloading
                                && secondary_glass_button(
                                    ui,
                                    Some(VectorIcon::Download),
                                    "Установить",
                                )
                                .clicked()
                            {
                                let _ = state.cmd_sender.send(UiWorkerCommand::InstallInstance {
                                    instance_id: inst_id.clone(),
                                });
                            }

                            // Secondary actions menu
                            let menu_id =
                                ui.make_persistent_id(format!("inst_menu_{}", inst_id.as_str()));
                            let more_btn = secondary_glass_button(ui, None, "•••");
                            if more_btn.clicked() {
                                ui.memory_mut(|mem| mem.toggle_popup(menu_id));
                            }

                            egui::popup_below_widget(
                                ui,
                                menu_id,
                                &more_btn,
                                egui::PopupCloseBehavior::CloseOnClick,
                                |ui| {
                                    ui.set_min_width(160.0);
                                    if ui.button("Папка модов").clicked() {
                                        let mods_dir =
                                            state.paths.instance_dir(inst_id.as_str()).join("mods");
                                        let _ = std::fs::create_dir_all(&mods_dir);
                                        let _ = crate::opener::open_that(mods_dir);
                                    }
                                    if is_installed && ui.button("Переустановить").clicked()
                                    {
                                        let _ = state.cmd_sender.send(
                                            UiWorkerCommand::ReinstallInstance {
                                                instance_id: inst_id.clone(),
                                            },
                                        );
                                    }
                                    if ui.button("Экспорт").clicked() {
                                        let inst_dir = state.paths.instance_dir(inst_id.as_str());
                                        let out_path = state
                                            .paths
                                            .backups_dir()
                                            .join(format!("{}.zip", inst_id.as_str()));
                                        if let Err(e) = export_instance_to_zip(&inst_dir, &out_path)
                                        {
                                            state.status_message =
                                                Some((format!("Ошибка экспорта: {e}"), true));
                                        } else {
                                            state.status_message = Some((
                                                format!("Экспортировано в {}", out_path.display()),
                                                false,
                                            ));
                                        }
                                    }
                                    if ui.button("Дублировать").clicked() {
                                        self.show_duplicate_dialog = true;
                                        self.duplicate_name = format!("{} (Копия)", inst_name);
                                    }
                                    ui.separator();
                                    if ui
                                        .button(
                                            RichText::new("Удалить инстанс").color(STATUS_DANGER),
                                        )
                                        .clicked()
                                    {
                                        self.show_delete_dialog = true;
                                        self.instance_to_delete =
                                            Some((inst_id.clone(), inst_name.clone()));
                                    }
                                },
                            );

                            if danger_button(ui, Some(VectorIcon::Trash), "Удалить")
                                .on_hover_text("Удалить этот инстанс")
                                .clicked()
                            {
                                self.show_delete_dialog = true;
                                self.instance_to_delete =
                                    Some((inst_id.clone(), inst_name.clone()));
                            }
                        });
                    });

                    ui.add_space(8.0);

                    // Tab selector with underline tabs (B5)
                    let detail_tabs = [
                        (InstanceDetailTab::Console, "Консоль"),
                        (InstanceDetailTab::Mods, "Моды"),
                        (InstanceDetailTab::ResourcePacks, "Ресурспаки"),
                        (InstanceDetailTab::ShaderPacks, "Шейдеры"),
                        (InstanceDetailTab::Worlds, "Миры"),
                        (InstanceDetailTab::Servers, "Серверы"),
                        (InstanceDetailTab::Screenshots, "Скриншоты"),
                        (InstanceDetailTab::Settings, "Параметры"),
                    ];
                    let prev_tab = self.current_tab;
                    render_underline_tabs(ui, &tokens, &detail_tabs, &mut self.current_tab);
                    if self.current_tab != prev_tab && self.current_tab == InstanceDetailTab::Mods {
                        let _ = state.cmd_sender.send(UiWorkerCommand::ScanInstanceMods {
                            instance_id: inst_id.clone(),
                        });
                    }

                    ui.separator();

                    // Tab contents
                    match self.current_tab {
                        InstanceDetailTab::Console => {
                            self.render_console_tab(ui, state, &inst_id);
                        }
                        InstanceDetailTab::Mods => {
                            self.render_mods_tab(ui, state, &inst_id);
                        }
                        InstanceDetailTab::ResourcePacks => {
                            self.render_resourcepacks_tab(ui, state, &inst_id);
                        }
                        InstanceDetailTab::ShaderPacks => {
                            self.render_shaderpacks_tab(ui, state, &inst_id);
                        }
                        InstanceDetailTab::Worlds => {
                            self.render_worlds_tab(ui, state, &inst_id);
                        }
                        InstanceDetailTab::Servers => {
                            self.render_servers_tab(ui, state, &inst_id);
                        }
                        InstanceDetailTab::Screenshots => {
                            self.render_screenshots_tab(ui, state, &inst_id);
                        }
                        InstanceDetailTab::Settings => {
                            self.render_settings_tab(ui, state, &inst_id);
                        }
                    }
                } else {
                    ui.centered_and_justified(|ui| {
                        ui.label(
                            RichText::new("Выберите инстанс из списка слева").color(TEXT_MUTED),
                        );
                    });
                }
            });
        });
    }

    fn filtered_instances(&self, state: &UiState) -> Vec<InstanceManifest> {
        let q = self.search_query.trim().to_lowercase();
        let mut list: Vec<InstanceManifest> = state
            .instances
            .iter()
            .filter(|inst| self.loader_filter.matches(inst.loader))
            .filter(|inst| {
                q.is_empty()
                    || inst.name.to_lowercase().contains(&q)
                    || inst.mc_version.to_lowercase().contains(&q)
                    || inst.loader.as_str().to_lowercase().contains(&q)
            })
            .cloned()
            .collect();

        match self.sort_mode {
            InstanceSortMode::Playtime => {
                list.sort_by(|a, b| b.playtime_secs.cmp(&a.playtime_secs));
            }
            InstanceSortMode::Name => {
                list.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
            }
            InstanceSortMode::Created => {
                list.sort_by(|a, b| b.created_at.cmp(&a.created_at));
            }
        }
        list
    }

    fn render_grid_view(&mut self, ui: &mut Ui, state: &mut UiState, tokens: &Tokens) {
        let instances = self.filtered_instances(state);
        if instances.is_empty() {
            ui.vertical_centered(|ui| {
                ui.add_space(40.0);
                ui.label(
                    RichText::new("Инстансы не найдены")
                        .size(15.0)
                        .color(tokens.text_muted),
                );
                ui.add_space(8.0);
                if button_secondary(ui, tokens, None, "Сбросить фильтры").clicked() {
                    self.search_query.clear();
                    self.loader_filter = LoaderFilter::All;
                }
            });
            return;
        }

        ScrollArea::vertical()
            .id_salt("instances_grid_scroll")
            .show(ui, |ui| {
                let avail_w = ui.available_width();
                let card_w = 300.0_f32;
                let spacing = 16.0_f32;
                let cols = ((avail_w + spacing) / (card_w + spacing)).floor().max(1.0) as usize;

                for chunk in instances.chunks(cols) {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing = Vec2::new(spacing, spacing);
                        for inst in chunk {
                            self.render_instance_grid_card(ui, state, inst, tokens, card_w);
                        }
                    });
                    ui.add_space(spacing);
                }
            });
    }

    fn render_instance_grid_card(
        &mut self,
        ui: &mut Ui,
        state: &mut UiState,
        inst: &InstanceManifest,
        tokens: &Tokens,
        card_w: f32,
    ) {
        let is_selected = state.selected_instance_id.as_ref() == Some(&inst.id);
        let is_installed = state.is_instance_installed(&inst.id);
        let is_downloading = state.installing_instance.as_ref() == Some(&inst.id);
        let is_running = state.game_running && state.running_instance.as_ref() == Some(&inst.id);

        let border_stroke = if is_running || is_selected {
            Stroke::new(1.5_f32, tokens.accent)
        } else if is_downloading {
            Stroke::new(1.5_f32, ACCENT_COBALT)
        } else {
            Stroke::new(1.0_f32, tokens.border)
        };

        let bg_color = if is_selected {
            tokens.bg_surface_hover
        } else {
            tokens.bg_surface
        };

        let card_frame = egui::Frame::new()
            .fill(bg_color)
            .stroke(border_stroke)
            .corner_radius(RADIUS_CARD)
            .inner_margin(egui::Margin::same(14));

        let resp = card_frame.show(ui, |ui| {
            ui.set_width(card_w);
            ui.vertical(|ui| {
                // Top row: Vector Icon + Name & Version/Loader Badges
                ui.horizontal(|ui| {
                    let (icon_rect, _) = ui.allocate_exact_size(Vec2::splat(32.0), Sense::hover());
                    let (icon, icon_col) = match inst.loader {
                        ModLoaderKind::Vanilla => (VectorIcon::Cube, tokens.text),
                        ModLoaderKind::Fabric | ModLoaderKind::Quilt => {
                            (VectorIcon::Layers, tokens.accent)
                        }
                        ModLoaderKind::Forge | ModLoaderKind::NeoForge => {
                            (VectorIcon::Shield, ACCENT_COBALT)
                        }
                    };
                    paint_vector_icon(ui.painter(), icon_rect, icon, icon_col);

                    ui.vertical(|ui| {
                        ui.add(
                            egui::Label::new(
                                RichText::new(&inst.name)
                                    .color(tokens.text)
                                    .size(15.0)
                                    .strong(),
                            )
                            .truncate(),
                        );
                        ui.add_space(2.0);
                        ui.horizontal(|ui| {
                            version_badge(ui, &inst.mc_version, ACCENT_COBALT);
                            loader_badge(ui, inst.loader);
                        });
                    });
                });

                ui.add_space(10.0);

                // Status row: status dot + status label + playtime on right
                let (status_text, status_col) = if is_running {
                    ("Запущен", tokens.accent)
                } else if is_downloading {
                    ("Загрузка...", ACCENT_COBALT)
                } else if is_installed {
                    ("Готов", tokens.accent)
                } else {
                    ("Не скачан", tokens.warning)
                };

                ui.horizontal(|ui| {
                    let (dot_rect, _) = ui.allocate_exact_size(Vec2::splat(10.0), Sense::hover());
                    ui.painter()
                        .circle_filled(dot_rect.center(), 3.5, status_col);
                    ui.label(
                        RichText::new(status_text)
                            .color(status_col)
                            .small()
                            .strong(),
                    );

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let hours = (inst.playtime_secs as f32) / 3600.0;
                        if hours >= 0.1 {
                            ui.label(
                                RichText::new(format!("Время: {:.1} ч", hours))
                                    .color(tokens.text_muted)
                                    .small(),
                            );
                        } else {
                            ui.label(RichText::new("Новый").color(tokens.text_muted).small());
                        }
                    });
                });

                ui.add_space(10.0);
                ui.separator();
                ui.add_space(6.0);

                // Bottom row: Actions
                ui.horizontal(|ui| {
                    let play_label = if is_running {
                        "Стоп"
                    } else {
                        "Играть"
                    };
                    if primary_emerald_button(ui, Some(VectorIcon::Play), play_label).clicked() {
                        state.selected_instance_id = Some(inst.id.clone());
                    }

                    if secondary_glass_button(ui, Some(VectorIcon::Settings), "Детали").clicked()
                    {
                        state.selected_instance_id = Some(inst.id.clone());
                        self.view_mode = InstanceViewMode::List;
                    }

                    if secondary_glass_button(ui, Some(VectorIcon::Folder), "Папка").clicked()
                    {
                        let inst_dir = state.paths.instance_dir(inst.id.as_str());
                        let _ = crate::opener::open_that(inst_dir);
                    }
                });
            });
        });

        if resp.response.interact(Sense::click()).clicked() {
            state.selected_instance_id = Some(inst.id.clone());
        }
    }

    fn render_create_instance_modal(&mut self, ui: &mut Ui, state: &mut UiState, tokens: &Tokens) {
        if state.remote_versions.is_empty() && !state.is_fetching_manifest {
            state.is_fetching_manifest = true;
            let _ = state.cmd_sender.send(UiWorkerCommand::FetchVersionManifest);
        }

        if state.create_inst_mc_ver.is_empty() {
            if let Some(latest) = &state.latest_release {
                state.create_inst_mc_ver = latest.clone();
            }
        }

        let effective_ver = if !state.create_inst_mc_ver.is_empty() {
            state.create_inst_mc_ver.clone()
        } else {
            state
                .latest_release
                .clone()
                .unwrap_or_else(|| ALL_MC_RELEASES[0].to_string())
        };
        let auto_name = format!("{} {}", state.create_inst_loader.as_str(), effective_ver);

        egui::Window::new("Создать инстанс")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ui.ctx(), |ui| {
                let modal_w = 460.0_f32.min(ui.ctx().screen_rect().width() - 32.0);
                ui.set_width(modal_w);

                // 1. Instance Name with auto-placeholder
                ui.label(RichText::new("Название").color(tokens.text).strong());
                ui.add_space(4.0);
                ui.add(
                    egui::TextEdit::singleline(&mut state.create_inst_name)
                        .hint_text(&auto_name)
                        .desired_width(ui.available_width()),
                );

                ui.add_space(10.0);

                // 2. Mod Loader Selection
                ui.label(RichText::new("Загрузчик").color(tokens.text).strong());
                ui.add_space(4.0);
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing = Vec2::new(6.0, 4.0);
                    for &ldr in &[
                        ModLoaderKind::Vanilla,
                        ModLoaderKind::Fabric,
                        ModLoaderKind::Quilt,
                        ModLoaderKind::Forge,
                        ModLoaderKind::NeoForge,
                    ] {
                        let is_sel = state.create_inst_loader == ldr;
                        if ui.selectable_label(is_sel, ldr.as_str()).clicked() {
                            state.create_inst_loader = ldr;
                        }
                    }
                });

                ui.add_space(10.0);

                // 3. Minecraft Version Selection
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new("Версия Minecraft")
                            .color(tokens.text)
                            .strong(),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if state.is_fetching_manifest {
                            ui.label(
                                RichText::new("Загрузка...")
                                    .color(tokens.text_muted)
                                    .small(),
                            );
                        } else if button_secondary(
                            ui,
                            tokens,
                            Some(VectorIcon::Refresh),
                            "Обновить",
                        )
                        .clicked()
                        {
                            state.is_fetching_manifest = true;
                            let _ = state.cmd_sender.send(UiWorkerCommand::FetchVersionManifest);
                        }
                    });
                });
                ui.add_space(4.0);

                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing = Vec2::new(8.0, 4.0);
                    ui.checkbox(&mut self.show_releases, "Релизы");
                    ui.checkbox(&mut self.show_snapshots, "Снапшоты");
                    ui.checkbox(&mut self.show_betas, "Beta");
                    ui.checkbox(&mut self.show_alphas, "Alpha");
                });

                ui.add_space(4.0);

                ui.horizontal(|ui| {
                    ui.add(
                        egui::TextEdit::singleline(&mut self.version_search)
                            .hint_text("Поиск версии (26.1, 1.21.4, 25w09b)...")
                            .desired_width(ui.available_width() - 64.0),
                    );
                    if !self.version_search.is_empty() && ui.small_button("Сброс").clicked() {
                        self.version_search.clear();
                    }
                });

                ui.add_space(6.0);

                let query = self.version_search.trim().to_lowercase();
                let mut filtered: Vec<(&str, McVersionCategory)> = Vec::new();

                if !state.remote_versions.is_empty() {
                    for (id, rel_type) in &state.remote_versions {
                        let cat = match rel_type.as_str() {
                            "release" => McVersionCategory::Release,
                            "snapshot" => McVersionCategory::Snapshot,
                            "old_beta" => McVersionCategory::OldBeta,
                            "old_alpha" => McVersionCategory::OldAlpha,
                            _ => continue,
                        };
                        let matches_cat = match cat {
                            McVersionCategory::Release => self.show_releases,
                            McVersionCategory::Snapshot => self.show_snapshots,
                            McVersionCategory::OldBeta => self.show_betas,
                            McVersionCategory::OldAlpha => self.show_alphas,
                        };
                        if !matches_cat {
                            continue;
                        }
                        if !query.is_empty() && !id.to_lowercase().contains(&query) {
                            continue;
                        }
                        filtered.push((id.as_str(), cat));
                    }
                } else if !state.is_fetching_manifest {
                    if self.show_releases {
                        for &v in ALL_MC_RELEASES {
                            if query.is_empty() || v.to_lowercase().contains(&query) {
                                filtered.push((v, McVersionCategory::Release));
                            }
                        }
                    }
                    if self.show_snapshots {
                        for &v in ALL_MC_SNAPSHOTS {
                            if query.is_empty() || v.to_lowercase().contains(&query) {
                                filtered.push((v, McVersionCategory::Snapshot));
                            }
                        }
                    }
                    if self.show_betas {
                        for &v in ALL_MC_BETAS {
                            if query.is_empty() || v.to_lowercase().contains(&query) {
                                filtered.push((v, McVersionCategory::OldBeta));
                            }
                        }
                    }
                    if self.show_alphas {
                        for &v in ALL_MC_ALPHAS {
                            if query.is_empty() || v.to_lowercase().contains(&query) {
                                filtered.push((v, McVersionCategory::OldAlpha));
                            }
                        }
                    }
                }

                let picker_frame = egui::Frame::new()
                    .fill(tokens.bg_input)
                    .stroke(Stroke::new(1.0_f32, tokens.border))
                    .corner_radius(RADIUS_CARD)
                    .inner_margin(egui::Margin::same(6));

                picker_frame.show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ScrollArea::vertical()
                        .id_salt("version_select_scroll")
                        .max_height(130.0)
                        .show(ui, |ui| {
                            if state.is_fetching_manifest && state.remote_versions.is_empty() {
                                for width_frac in [0.55_f32, 0.42, 0.62, 0.48] {
                                    let avail_w = ui.available_width();
                                    let (rect, _) = ui.allocate_exact_size(
                                        Vec2::new(avail_w * width_frac, 18.0),
                                        Sense::hover(),
                                    );
                                    ui.painter().rect_filled(rect, 4.0, tokens.bg_surface_hover);
                                    ui.add_space(4.0);
                                }
                            } else if filtered.is_empty() {
                                ui.add_space(8.0);
                                ui.label(
                                    RichText::new("Нет версий, соответствующих фильтрам")
                                        .color(tokens.text_muted),
                                );
                            } else {
                                for (vid, cat) in filtered {
                                    let is_sel = effective_ver == vid;
                                    ui.horizontal(|ui| {
                                        let text_col =
                                            if is_sel { tokens.accent } else { tokens.text };
                                        if ui
                                            .selectable_label(
                                                is_sel,
                                                RichText::new(vid).color(text_col).strong(),
                                            )
                                            .clicked()
                                        {
                                            state.create_inst_mc_ver = vid.to_string();
                                        }
                                        version_badge(ui, cat.badge_label(), cat.badge_color());
                                    });
                                }
                            }
                        });
                });

                ui.add_space(10.0);

                // 4. Collapsible Advanced Block (Memory + Java)
                egui::CollapsingHeader::new(RichText::new("Дополнительно").color(tokens.text))
                    .default_open(false)
                    .show(ui, |ui| {
                        ui.label(
                            RichText::new(format!(
                                "Память: {} МБ ({:.1} ГБ)",
                                state.create_inst_ram_mb,
                                state.create_inst_ram_mb as f32 / 1024.0
                            ))
                            .color(tokens.text)
                            .small(),
                        );
                        ui.add_space(4.0);
                        ui.horizontal_wrapped(|ui| {
                            ui.spacing_mut().item_spacing = Vec2::new(6.0, 4.0);
                            for &(mb, lbl) in &[
                                (2048, "2 ГБ"),
                                (4096, "4 ГБ"),
                                (6144, "6 ГБ"),
                                (8192, "8 ГБ"),
                                (12288, "12 ГБ"),
                                (16384, "16 ГБ"),
                            ] {
                                if ui
                                    .selectable_label(state.create_inst_ram_mb == mb, lbl)
                                    .clicked()
                                {
                                    state.create_inst_ram_mb = mb;
                                }
                            }
                        });
                        ui.add_space(4.0);
                        ui.add(
                            egui::Slider::new(&mut state.create_inst_ram_mb, 1024..=16384)
                                .step_by(512.0)
                                .show_value(false),
                        );

                        ui.add_space(8.0);
                        ui.label(RichText::new("Путь к Java").color(tokens.text).small());
                        ui.add(
                            egui::TextEdit::singleline(&mut state.create_inst_java_path)
                                .hint_text("Авто (встроенная или системная Java)")
                                .desired_width(ui.available_width()),
                        );
                    });

                ui.add_space(14.0);
                ui.horizontal(|ui| {
                    if button_primary(ui, tokens, Some(VectorIcon::Check), "Создать инстанс")
                        .clicked()
                    {
                        let final_name = if state.create_inst_name.trim().is_empty() {
                            auto_name.clone()
                        } else {
                            state.create_inst_name.trim().to_string()
                        };
                        match state.instance_mgr.create_instance(
                            &final_name,
                            &effective_ver,
                            state.create_inst_loader,
                            None,
                        ) {
                            Ok(mut manifest) => {
                                manifest.memory_max_mb = state.create_inst_ram_mb;
                                let custom_java = state.create_inst_java_path.trim();
                                if !custom_java.is_empty() {
                                    manifest.java_path =
                                        Some(std::path::PathBuf::from(custom_java));
                                }
                                let _ = state.instance_mgr.save_instance(&manifest);
                                let inst_id = manifest.id.clone();
                                state.selected_instance_id = Some(inst_id.clone());
                                state.instances.push(manifest.clone());

                                state.installing_instance = Some(inst_id.clone());
                                state.install_error = None;
                                state.detailed_install_error = None;
                                state.current_install_stage =
                                    crate::install_types::InstallStage::Manifest;
                                state.install_retries_count = 0;
                                state.install_eta_secs = None;
                                state.status_message = Some((
                                    format!(
                                        "Инстанс '{}' создан. Запущена установка...",
                                        manifest.name
                                    ),
                                    false,
                                ));
                                let _ = state.cmd_sender.send(UiWorkerCommand::InstallInstance {
                                    instance_id: inst_id,
                                });
                            }
                            Err(e) => {
                                state.status_message =
                                    Some((format!("Ошибка создания инстанса: {e}"), true));
                            }
                        }
                        state.create_inst_name.clear();
                        state.create_inst_java_path.clear();
                        state.show_create_instance_dialog = false;
                    }
                    ui.add_space(8.0);
                    if button_secondary(ui, tokens, None, "Отмена").clicked() {
                        state.show_create_instance_dialog = false;
                    }
                });
            });
    }

    fn render_empty_onboarding_hub(&mut self, ui: &mut Ui, state: &mut UiState, tokens: &Tokens) {
        let avail = ui.available_size();
        let card_max_w = 420.0_f32.min((avail.x - 32.0).max(240.0));
        let top_pad = ((avail.y - 240.0) * 0.4).max(24.0);

        ui.vertical_centered(|ui| {
            ui.add_space(top_pad);

            let card = egui::Frame::new()
                .fill(tokens.bg_surface)
                .stroke(Stroke::new(1.0_f32, tokens.border))
                .corner_radius(RADIUS_CARD)
                .inner_margin(egui::Margin::same(32));

            card.show(ui, |ui| {
                ui.set_max_width(card_max_w);
                ui.vertical_centered(|ui| {
                    let (rect, _) = ui.allocate_exact_size(Vec2::splat(48.0), egui::Sense::hover());
                    paint_vector_icon(ui.painter(), rect, VectorIcon::Cube, tokens.text_muted);

                    ui.add_space(16.0);
                    ui.label(
                        RichText::new("Инстансов пока нет")
                            .color(tokens.text)
                            .size(18.0)
                            .strong(),
                    );
                    ui.add_space(6.0);
                    ui.label(
                        RichText::new("Создайте первый инстанс или импортируйте сборку")
                            .color(tokens.text_muted)
                            .size(13.0),
                    );

                    ui.add_space(20.0);
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 10.0;
                        let total_btn_w = 300.0_f32.min(ui.available_width());
                        let side_pad = ((ui.available_width() - total_btn_w) * 0.5).max(0.0);
                        if side_pad > 0.0 {
                            ui.add_space(side_pad);
                        }
                        if button_primary(ui, tokens, Some(VectorIcon::Plus), "Создать инстанс")
                            .clicked()
                        {
                            state.show_create_instance_dialog = true;
                        }
                        if button_secondary(ui, tokens, Some(VectorIcon::Download), "Импорт")
                            .clicked()
                        {
                            self.show_import_dialog = true;
                        }
                    });
                });
            });
        });
    }

    /// Lock file of an instance, re-read at most every 2 seconds.
    fn cached_lock(
        ctx: &egui::Context,
        dir: &std::path::Path,
    ) -> std::sync::Arc<mq_mods::ModLockFile> {
        type Cache = (std::time::Instant, std::sync::Arc<mq_mods::ModLockFile>);
        let id = egui::Id::new(("mq_lock_cache", dir));
        if let Some((at, lock)) = ctx.data(|d| d.get_temp::<Cache>(id)) {
            if at.elapsed() < std::time::Duration::from_secs(2) {
                return lock;
            }
        }
        let lock = std::sync::Arc::new(mq_mods::ModLockFile::load_for_instance(dir));
        ctx.data_mut(|d| d.insert_temp::<Cache>(id, (std::time::Instant::now(), lock.clone())));
        lock
    }

    /// "Проверить обновления": newer versions at the source, one-click update with rollback.
    fn updates_panel(ui: &mut Ui, state: &mut UiState, inst_id: &mq_core::InstanceId) {
        ui.horizontal(|ui| {
            if secondary_glass_button(ui, Some(VectorIcon::Refresh), "Проверить обновления")
                .clicked()
                && !state.updates.checking
            {
                state.check_updates(inst_id.clone());
            }
            if state.updates.checking {
                ui.label(RichText::new("Проверка...").color(TEXT_MUTED));
            }
        });
        if state.updates.instance.as_ref() != Some(inst_id) {
            return;
        }
        if let Some(e) = &state.updates.error {
            ui.label(RichText::new(e).color(STATUS_DANGER));
        }
        if state.updates.checked && state.updates.items.is_empty() && state.updates.error.is_none()
        {
            ui.label(RichText::new("Все установленные файлы актуальны").color(TEXT_MUTED));
        }
        let mut apply = None;
        for (i, c) in state.updates.items.iter().enumerate() {
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(format!("{} -> {}", c.old_filename, c.new.filename))
                        .color(TEXT_PRIMARY),
                );
                version_badge(
                    ui,
                    match c.source {
                        mq_mods::CatalogSource::Modrinth => "Modrinth",
                        mq_mods::CatalogSource::CurseForge => "CurseForge",
                    },
                    ACCENT_COBALT,
                );
                if secondary_glass_button(ui, None, "Обновить").clicked() {
                    apply = Some(i);
                }
            });
        }
        if let Some(i) = apply {
            state.apply_update(i);
        }
        ui.add_space(4.0);
    }

    fn render_mods_tab(&mut self, ui: &mut Ui, state: &mut UiState, inst_id: &mq_core::InstanceId) {
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = Vec2::new(8.0, 6.0);
            ui.label(
                RichText::new(format!("Установлено модов: {}", state.installed_mods.len()))
                    .strong(),
            );
            if secondary_glass_button(ui, Some(VectorIcon::Refresh), "Пересканировать").clicked()
            {
                let _ = state.cmd_sender.send(UiWorkerCommand::ScanInstanceMods {
                    instance_id: inst_id.clone(),
                });
            }
            if secondary_glass_button(ui, Some(VectorIcon::Trash), "Удалить неиспользуемые")
                .clicked()
            {
                let _ = state.cmd_sender.send(UiWorkerCommand::CleanupUnusedDeps {
                    instance_id: inst_id.clone(),
                });
            }
            if secondary_glass_button(ui, Some(VectorIcon::Folder), "Открыть mods/").clicked()
            {
                let mods_dir = state.paths.instance_dir(inst_id.as_str()).join("mods");
                let _ = crate::opener::open_that(mods_dir);
            }
        });

        ui.add_space(4.0);
        Self::updates_panel(ui, state, inst_id);

        if !state.mod_conflicts.is_empty() {
            let warn_frame = egui::Frame::new()
                .fill(CARD_SURFACE)
                .stroke(Stroke::new(1.0_f32, STATUS_WARN))
                .corner_radius(RADIUS_CARD)
                .inner_margin(egui::Margin::same(8));

            warn_frame.show(ui, |ui| {
                ui.horizontal(|ui| {
                    let (rect, _) = ui.allocate_exact_size(Vec2::splat(16.0), egui::Sense::hover());
                    paint_vector_icon(ui.painter(), rect, VectorIcon::Warning, STATUS_WARN);
                    ui.label(RichText::new(format!("Обнаружено конфликтов: {}", state.mod_conflicts.len())).color(STATUS_WARN).strong());
                });
                for c in &state.mod_conflicts {
                    match c {
                        mq_mods::ModConflict::DuplicateModId { mod_id, files } => {
                            ui.label(RichText::new(format!("Дубликат ID '{mod_id}' в файлах: {}", files.join(", "))).color(STATUS_DANGER));
                        }
                        mq_mods::ModConflict::MissingDependency { mod_id, required_dep, .. } => {
                            ui.label(RichText::new(format!("Мод '{mod_id}' требует отсутствующую зависимость '{required_dep}'")).color(STATUS_WARN));
                        }
                    }
                }
            });
            ui.add_space(6.0);
        }

        let lock = Self::cached_lock(ui.ctx(), &state.paths.instance_dir(inst_id.as_str()));
        ScrollArea::vertical()
            .id_salt("mods_table_scroll")
            .auto_shrink([false, false])
            .show(ui, |ui| {
            if state.installed_mods.is_empty() {
                ui.add_space(20.0);
                ui.label(RichText::new("В этом инстансе пока нет модов. Найдите их во вкладке 'Каталог' или положите .jar файлы в папку mods/!").color(TEXT_MUTED));
            } else {
                for m in &state.installed_mods {
                    let card = egui::Frame::new()
                        .fill(CARD_SURFACE)
                        .stroke(Stroke::new(1.0_f32, BORDER_SUBTLE))
                        .corner_radius(RADIUS_CARD)
                        .inner_margin(egui::Margin::same(8));

                    card.show(ui, |ui| {
                        ui.horizontal(|ui| {
                            // Toggle button
                            let toggle_text = if m.enabled { "Вкл" } else { "Выкл" };
                            let toggle_col = if m.enabled { ACCENT_EMERALD } else { TEXT_MUTED };
                            if ui.button(RichText::new(toggle_text).color(toggle_col).strong()).clicked() {
                                let _ = state.cmd_sender.send(UiWorkerCommand::ToggleMod {
                                    instance_id: inst_id.clone(),
                                    file_name: m.file_name.clone(),
                                });
                            }

                            let right_w = 40.0;
                            let middle_w = (ui.available_width() - right_w - 8.0).max(60.0);

                            ui.allocate_ui_with_layout(
                                Vec2::new(middle_w, 36.0),
                                egui::Layout::top_down(egui::Align::Min),
                                |ui| {
                                    ui.horizontal_wrapped(|ui| {
                                        ui.spacing_mut().item_spacing = Vec2::new(6.0, 4.0);
                                        let name_w = (ui.available_width() - 110.0).max(60.0);
                                        ui.add_sized(
                                            [name_w.min(180.0), 18.0],
                                            egui::Label::new(RichText::new(&m.name).color(TEXT_PRIMARY).strong()).truncate(),
                                        );
                                        version_badge(ui, &m.version, ACCENT_COBALT);
                                        if let Some(ld) = m.loader {
                                            version_badge(ui, ld.as_str(), ACCENT_EMERALD);
                                        }
                                        match lock.find_by_filename(&m.file_name) {
                                            Some((_, e)) => {
                                                let (txt, col) = match e.source {
                                                    mq_mods::LockSource::Modrinth => ("Modrinth", ACCENT_EMERALD),
                                                    mq_mods::LockSource::Manual => ("Вручную", TEXT_MUTED),
                                                    mq_mods::LockSource::CurseForge => ("CurseForge", Color32::from_rgb(0xF1, 0x64, 0x36)),
                                                };
                                                version_badge(ui, txt, col);
                                            }
                                            None => version_badge(ui, "Локальный", TEXT_MUTED),
                                        }
                                    });
                                    let file_w = (ui.available_width() - 8.0).max(40.0);
                                    ui.add_sized(
                                        [file_w, 16.0],
                                        egui::Label::new(RichText::new(format!("Файл: {}", m.file_name)).color(TEXT_MUTED).small()).truncate(),
                                    );
                                },
                            );

                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                if secondary_glass_button(ui, Some(VectorIcon::Trash), "").clicked() {
                                    let _ = state.cmd_sender.send(UiWorkerCommand::DeleteMod {
                                        instance_id: inst_id.clone(),
                                        file_name: m.file_name.clone(),
                                    });
                                }
                            });
                        });
                    });
                    ui.add_space(4.0);
                }
            }
        });
    }

    fn render_console_tab(
        &mut self,
        ui: &mut Ui,
        state: &mut UiState,
        _inst_id: &mq_core::InstanceId,
    ) {
        // 1. Crash Analysis Banner (if any failure detected)
        if let Some(ref analysis) = state.crash_analysis {
            let err_frame = egui::Frame::new()
                .fill(Color32::from_rgb(0x28, 0x14, 0x16))
                .stroke(Stroke::new(1.0_f32, STATUS_DANGER))
                .corner_radius(RADIUS_CARD)
                .inner_margin(egui::Margin::same(12));

            err_frame.show(ui, |ui| {
                ui.horizontal(|ui| {
                    let (r, _) = ui.allocate_exact_size(Vec2::splat(18.0), egui::Sense::hover());
                    paint_vector_icon(ui.painter(), r, VectorIcon::Warning, STATUS_DANGER);
                    ui.label(
                        RichText::new(&analysis.summary)
                            .color(STATUS_DANGER)
                            .strong()
                            .size(14.0),
                    );
                });
                ui.add_space(4.0);
                ui.label(RichText::new(&analysis.recommendation).color(TEXT_PRIMARY));
                if !analysis.suspected_mods.is_empty() {
                    ui.add_space(4.0);
                    ui.horizontal_wrapped(|ui| {
                        ui.label(
                            RichText::new("Подозрительные моды:")
                                .color(TEXT_MUTED)
                                .small(),
                        );
                        for sm in &analysis.suspected_mods {
                            version_badge(ui, sm, STATUS_WARN);
                        }
                    });
                }
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if state.is_uploading_mclogs {
                        ui.label(RichText::new("Загрузка на mclogs...").color(ACCENT_COBALT));
                    } else if let Some(ref url) = state.mclogs_url {
                        ui.label(
                            RichText::new(format!("Ссылка на лог: {url}")).color(ACCENT_EMERALD),
                        );
                        if secondary_glass_button(ui, Some(VectorIcon::Copy), "Скопировать ссылку")
                            .clicked()
                        {
                            ui.ctx().copy_text(url.clone());
                        }
                    } else if secondary_glass_button(
                        ui,
                        Some(VectorIcon::Download),
                        "Поделиться логом на mclogs",
                    )
                    .clicked()
                    {
                        state.is_uploading_mclogs = true;
                        let full_log: String = state
                            .game_logs
                            .iter()
                            .map(|l| l.line.as_str())
                            .collect::<Vec<_>>()
                            .join("\n");
                        let bus = state.event_bus.clone();
                        tokio::spawn(async move {
                            let client = reqwest::Client::new();
                            if let Ok(url) = mq_launch::upload_to_mclogs(&client, &full_log).await {
                                bus.publish(mq_core::Event::PhaseCompleted {
                                    phase: format!("mclogs:{url}"),
                                    elapsed_ms: 0,
                                });
                            }
                        });
                    }
                });
            });
            ui.add_space(8.0);
        }

        // Row 1: Line counter, autoscroll checkbox, action buttons
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = Vec2::new(8.0, 6.0);
            ui.label(
                RichText::new(format!("Строк в логе: {}", state.game_logs.len())).color(TEXT_MUTED),
            );
            ui.checkbox(&mut self.console_autoscroll, "Автоскролл");
            if secondary_glass_button(ui, Some(VectorIcon::Copy), "Копировать лог").clicked()
            {
                let full_log: String = state
                    .game_logs
                    .iter()
                    .map(|l| l.line.as_str())
                    .collect::<Vec<_>>()
                    .join("\n");
                ui.ctx().copy_text(full_log);
            }
            if secondary_glass_button(ui, None, "Очистить").clicked() {
                state.game_logs.clear();
            }
        });

        ui.add_space(4.0);

        // Row 2: Search input and level filter chips (B6, B7)
        let tokens = Tokens::from_config(&state.config.theme);
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing = Vec2::new(8.0, 0.0);
            SearchField::new(&mut self.console_search, "Поиск в логе...")
                .width(200.0)
                .show(ui, &tokens);

            let info_count = state
                .game_logs
                .iter()
                .filter(|l| l.level == GameLogLevel::Info)
                .count();
            let warn_count = state
                .game_logs
                .iter()
                .filter(|l| l.level == GameLogLevel::Warn)
                .count();
            let error_count = state
                .game_logs
                .iter()
                .filter(|l| l.level == GameLogLevel::Error)
                .count();

            if FilterChip::new("INFO", state.log_filter == Some(GameLogLevel::Info))
                .dot_color(tokens.text_muted)
                .count(info_count)
                .show(ui, &tokens)
                .clicked()
            {
                state.log_filter = if state.log_filter == Some(GameLogLevel::Info) {
                    None
                } else {
                    Some(GameLogLevel::Info)
                };
            }

            if FilterChip::new("WARN", state.log_filter == Some(GameLogLevel::Warn))
                .dot_color(tokens.warning)
                .count(warn_count)
                .show(ui, &tokens)
                .clicked()
            {
                state.log_filter = if state.log_filter == Some(GameLogLevel::Warn) {
                    None
                } else {
                    Some(GameLogLevel::Warn)
                };
            }

            if FilterChip::new("ERROR", state.log_filter == Some(GameLogLevel::Error))
                .dot_color(tokens.danger)
                .count(error_count)
                .show(ui, &tokens)
                .clicked()
            {
                state.log_filter = if state.log_filter == Some(GameLogLevel::Error) {
                    None
                } else {
                    Some(GameLogLevel::Error)
                };
            }
        });

        ui.add_space(8.0);

        // B3: Console occupies full remaining height, min 240px, monospace 12 font
        let avail_h = (ui.available_height() - 12.0).max(240.0);
        let console_frame = egui::Frame::new()
            .fill(Color32::from_rgb(0x08, 0x0A, 0x0E))
            .stroke(Stroke::new(1.0_f32, BORDER_SUBTLE))
            .corner_radius(RADIUS_CARD)
            .inner_margin(egui::Margin::same(8));

        console_frame.show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.set_height(avail_h);
            ScrollArea::vertical()
                .id_salt("console_scroll")
                .auto_shrink([false, false])
                .stick_to_bottom(self.console_autoscroll)
                .show(ui, |ui| {
                    if state.game_logs.is_empty() {
                        ui.label(
                            RichText::new("Логи игры появятся здесь при запуске Minecraft")
                                .color(TEXT_MUTED)
                                .font(egui::FontId::monospace(12.0)),
                        );
                    } else {
                        let query = self.console_search.to_lowercase();
                        for entry in &state.game_logs {
                            if let Some(flt) = state.log_filter {
                                if entry.level != flt {
                                    continue;
                                }
                            }
                            if !query.is_empty() && !entry.line.to_lowercase().contains(&query) {
                                continue;
                            }

                            let col = match entry.level {
                                GameLogLevel::Info => TEXT_PRIMARY,
                                GameLogLevel::Warn => STATUS_WARN,
                                GameLogLevel::Error => STATUS_DANGER,
                                GameLogLevel::Debug => TEXT_MUTED,
                            };

                            ui.label(
                                RichText::new(&entry.line)
                                    .color(col)
                                    .font(egui::FontId::monospace(12.0)),
                            );
                        }
                    }
                });
        });
    }

    fn render_resourcepacks_tab(
        &mut self,
        ui: &mut Ui,
        state: &mut UiState,
        inst_id: &mq_core::InstanceId,
    ) {
        let packs_dir = state
            .paths
            .instance_dir(inst_id.as_str())
            .join("resourcepacks");
        let _ = std::fs::create_dir_all(&packs_dir);
        let packs = list_packs_in_dir(&packs_dir);

        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = Vec2::new(8.0, 6.0);
            ui.label(
                RichText::new(format!("Ресурспаки: {}", packs.len()))
                    .strong()
                    .size(14.0),
            );
            if secondary_glass_button(ui, Some(VectorIcon::Folder), "Открыть resourcepacks/")
                .clicked()
            {
                let _ = crate::opener::open_that(&packs_dir);
            }
        });

        ui.add_space(6.0);
        if packs.is_empty() {
            ui.add_space(20.0);
            ui.label(RichText::new("Ресурспаки не установлены. Поместите .zip файлы в папку resourcepacks/ или установите из Каталога!").color(TEXT_MUTED));
        } else {
            for (name, enabled, size) in packs {
                let card = egui::Frame::new()
                    .fill(CARD_SURFACE)
                    .stroke(Stroke::new(1.0_f32, BORDER_SUBTLE))
                    .corner_radius(RADIUS_CARD)
                    .inner_margin(egui::Margin::same(8));

                card.show(ui, |ui| {
                    ui.horizontal(|ui| {
                        let toggle_lbl = if enabled { "Вкл" } else { "Выкл" };
                        let toggle_col = if enabled { ACCENT_EMERALD } else { TEXT_MUTED };
                        if ui
                            .button(RichText::new(toggle_lbl).color(toggle_col).strong())
                            .clicked()
                        {
                            toggle_pack_file(&packs_dir, &name);
                        }
                        let name_w = (ui.available_width() - 80.0).max(60.0);
                        ui.add_sized(
                            [name_w, 18.0],
                            egui::Label::new(RichText::new(&name).color(TEXT_PRIMARY).strong())
                                .truncate(),
                        );
                        if size > 0 {
                            let mib = (size as f64) / (1024.0 * 1024.0);
                            ui.label(
                                RichText::new(format!("{mib:.1} МБ"))
                                    .color(TEXT_MUTED)
                                    .small(),
                            );
                        }
                    });
                });
                ui.add_space(4.0);
            }
        }
    }

    fn render_shaderpacks_tab(
        &mut self,
        ui: &mut Ui,
        state: &mut UiState,
        inst_id: &mq_core::InstanceId,
    ) {
        let shaders_dir = state
            .paths
            .instance_dir(inst_id.as_str())
            .join("shaderpacks");
        let _ = std::fs::create_dir_all(&shaders_dir);
        let shaders = list_packs_in_dir(&shaders_dir);

        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = Vec2::new(8.0, 6.0);
            ui.label(
                RichText::new(format!("Шейдерпаки: {}", shaders.len()))
                    .strong()
                    .size(14.0),
            );
            if secondary_glass_button(ui, Some(VectorIcon::Folder), "Открыть shaderpacks/")
                .clicked()
            {
                let _ = crate::opener::open_that(&shaders_dir);
            }
        });

        ui.add_space(6.0);
        if shaders.is_empty() {
            ui.add_space(20.0);
            ui.label(RichText::new("Шейдеры не установлены. Поместите шейдерпаки в папку shaderpacks/! Для их работы требуется Iris или OptiFine.").color(TEXT_MUTED));
        } else {
            for (name, enabled, size) in shaders {
                let card = egui::Frame::new()
                    .fill(CARD_SURFACE)
                    .stroke(Stroke::new(1.0_f32, BORDER_SUBTLE))
                    .corner_radius(RADIUS_CARD)
                    .inner_margin(egui::Margin::same(8));

                card.show(ui, |ui| {
                    ui.horizontal(|ui| {
                        let toggle_lbl = if enabled { "Вкл" } else { "Выкл" };
                        let toggle_col = if enabled { ACCENT_EMERALD } else { TEXT_MUTED };
                        if ui
                            .button(RichText::new(toggle_lbl).color(toggle_col).strong())
                            .clicked()
                        {
                            toggle_pack_file(&shaders_dir, &name);
                        }
                        let name_w = (ui.available_width() - 80.0).max(60.0);
                        ui.add_sized(
                            [name_w, 18.0],
                            egui::Label::new(RichText::new(&name).color(TEXT_PRIMARY).strong())
                                .truncate(),
                        );
                        if size > 0 {
                            let mib = (size as f64) / (1024.0 * 1024.0);
                            ui.label(
                                RichText::new(format!("{mib:.1} МБ"))
                                    .color(TEXT_MUTED)
                                    .small(),
                            );
                        }
                    });
                });
                ui.add_space(4.0);
            }
        }
    }

    fn render_worlds_tab(
        &mut self,
        ui: &mut Ui,
        state: &mut UiState,
        inst_id: &mq_core::InstanceId,
    ) {
        let inst_dir = state.paths.instance_dir(inst_id.as_str());
        let backups_root = state.paths.backups_dir();

        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = Vec2::new(8.0, 6.0);
            ui.label(
                RichText::new("Сохранения миров (Worlds)")
                    .strong()
                    .size(14.0),
            );
            if secondary_glass_button(ui, Some(VectorIcon::Folder), "Открыть saves/").clicked()
            {
                let saves_dir = inst_dir.join("saves");
                let _ = std::fs::create_dir_all(&saves_dir);
                let _ = crate::opener::open_that(saves_dir);
            }
        });

        ui.add_space(6.0);

        let worlds = list_worlds(&inst_dir).unwrap_or_default();
        if worlds.is_empty() {
            ui.add_space(20.0);
            ui.label(
                RichText::new("В этом инстансе пока нет сохранений. Создайте мир в игре!")
                    .color(TEXT_MUTED),
            );
        } else {
            for w in &worlds {
                let card = egui::Frame::new()
                    .fill(CARD_SURFACE)
                    .stroke(Stroke::new(1.0_f32, BORDER_SUBTLE))
                    .corner_radius(RADIUS_CARD)
                    .inner_margin(egui::Margin::same(10));

                card.show(ui, |ui| {
                    ui.horizontal_wrapped(|ui| {
                        ui.spacing_mut().item_spacing = Vec2::new(6.0, 4.0);
                        let (r, _) =
                            ui.allocate_exact_size(Vec2::splat(18.0), egui::Sense::hover());
                        paint_vector_icon(ui.painter(), r, VectorIcon::Cube, ACCENT_EMERALD);
                        ui.add(
                            egui::Label::new(
                                RichText::new(&w.display_name).color(TEXT_PRIMARY).strong(),
                            )
                            .truncate(),
                        );
                        version_badge(ui, &w.game_mode, ACCENT_COBALT);
                        let mib = (w.size_bytes as f64) / (1024.0 * 1024.0);
                        ui.label(
                            RichText::new(format!("{mib:.1} МБ"))
                                .color(TEXT_MUTED)
                                .small(),
                        );
                    });

                    ui.label(
                        RichText::new(format!("Папка: saves/{}", w.folder_name))
                            .color(TEXT_MUTED)
                            .small(),
                    );

                    ui.add_space(4.0);

                    ui.horizontal_wrapped(|ui| {
                        ui.spacing_mut().item_spacing = Vec2::new(6.0, 4.0);
                        if secondary_glass_button(ui, Some(VectorIcon::Download), "Бэкап").clicked()
                        {
                            match backup_world(
                                &inst_dir,
                                &backups_root,
                                inst_id.as_str(),
                                &w.folder_name,
                            ) {
                                Ok(bp) => {
                                    state.status_message = Some((
                                        format!(
                                            "Бэкап создан: {}",
                                            bp.file_name().unwrap_or_default().to_string_lossy()
                                        ),
                                        false,
                                    ));
                                }
                                Err(e) => {
                                    state.status_message =
                                        Some((format!("Ошибка бэкапа: {e}"), true));
                                }
                            }
                        }
                        if secondary_glass_button(ui, Some(VectorIcon::Download), "Экспорт")
                            .clicked()
                        {
                            let export_path = backups_root.join(format!("{}.zip", w.folder_name));
                            if let Ok(()) =
                                export_world_to_zip(&inst_dir, &w.folder_name, &export_path)
                            {
                                state.status_message = Some((
                                    format!("Мир экспортирован в {}", export_path.display()),
                                    false,
                                ));
                            }
                        }
                        if secondary_glass_button(ui, Some(VectorIcon::Trash), "Удалить").clicked()
                        {
                            let _ = delete_world(&inst_dir, &w.folder_name);
                            state.status_message =
                                Some((format!("Мир '{}' удален", w.display_name), false));
                        }
                    });
                });
                ui.add_space(4.0);
            }
        }

        // Backups list section
        ui.add_space(14.0);
        ui.separator();
        ui.add_space(8.0);
        ui.label(
            RichText::new("Резервные копии (Бэкапы)")
                .strong()
                .size(13.0),
        );
        let backups = list_backups(&backups_root, inst_id.as_str()).unwrap_or_default();
        if backups.is_empty() {
            ui.label(
                RichText::new("Резервных копий пока нет")
                    .color(TEXT_MUTED)
                    .small(),
            );
        } else {
            for b in backups {
                ui.horizontal(|ui| {
                    let mib = (b.size_bytes as f64) / (1024.0 * 1024.0);
                    ui.label(RichText::new(&b.file_name).color(TEXT_PRIMARY).small());
                    ui.label(
                        RichText::new(format!("{mib:.1} МБ"))
                            .color(TEXT_MUTED)
                            .small(),
                    );
                    if ui.button("Восстановить").clicked() {
                        match restore_world(
                            &backups_root,
                            inst_id.as_str(),
                            &b.file_name,
                            &inst_dir,
                            None,
                        ) {
                            Ok(_) => {
                                state.status_message = Some((
                                    format!("Мир успешно восстановлен из {}", b.file_name),
                                    false,
                                ));
                            }
                            Err(e) => {
                                state.status_message =
                                    Some((format!("Ошибка восстановления: {e}"), true));
                            }
                        }
                    }
                });
            }
        }
    }

    fn render_servers_tab(
        &mut self,
        ui: &mut Ui,
        state: &mut UiState,
        inst_id: &mq_core::InstanceId,
    ) {
        let inst_dir = state.paths.instance_dir(inst_id.as_str());
        let mut servers = load_servers_dat(&inst_dir).unwrap_or_default();

        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = Vec2::new(8.0, 6.0);
            ui.label(
                RichText::new(format!("Серверы (servers.dat): {}", servers.len()))
                    .strong()
                    .size(14.0),
            );
            if primary_emerald_button(ui, Some(VectorIcon::Plus), "Добавить сервер").clicked()
            {
                self.show_add_server_dialog = true;
            }
        });

        ui.add_space(6.0);

        if servers.is_empty() {
            ui.add_space(20.0);
            ui.label(
                RichText::new("Список серверов пуст. Добавьте сервер кнопкой выше!")
                    .color(TEXT_MUTED),
            );
        } else {
            let mut server_to_delete = None;
            for (idx, s) in servers.iter().enumerate() {
                let card = egui::Frame::new()
                    .fill(CARD_SURFACE)
                    .stroke(Stroke::new(1.0_f32, BORDER_SUBTLE))
                    .corner_radius(RADIUS_CARD)
                    .inner_margin(egui::Margin::same(10));

                card.show(ui, |ui| {
                    ui.horizontal_wrapped(|ui| {
                        ui.spacing_mut().item_spacing = Vec2::new(6.0, 4.0);
                        let (r, _) =
                            ui.allocate_exact_size(Vec2::splat(18.0), egui::Sense::hover());
                        paint_vector_icon(ui.painter(), r, VectorIcon::Server, ACCENT_COBALT);
                        ui.add(
                            egui::Label::new(RichText::new(&s.name).color(TEXT_PRIMARY).strong())
                                .truncate(),
                        );
                        ui.label(RichText::new(&s.ip).color(TEXT_MUTED));
                    });

                    if let Some(res) = state.server_pings.get(&s.ip) {
                        ui.horizontal_wrapped(|ui| {
                            ui.spacing_mut().item_spacing = Vec2::new(6.0, 4.0);
                            version_badge(ui, &format!("{} ms", res.latency_ms), ACCENT_EMERALD);
                            version_badge(
                                ui,
                                &format!("{}/{}", res.players_online, res.players_max),
                                ACCENT_COBALT,
                            );
                            ui.label(RichText::new(&res.motd).color(TEXT_PRIMARY).small());
                        });
                    }

                    ui.add_space(4.0);

                    ui.horizontal_wrapped(|ui| {
                        ui.spacing_mut().item_spacing = Vec2::new(6.0, 4.0);
                        if secondary_glass_button(ui, Some(VectorIcon::Refresh), "Пинг").clicked()
                        {
                            let ip = s.ip.clone();
                            if let Ok(res) = ping_server(&ip, 3000) {
                                state.server_pings.insert(ip, res);
                            }
                        }
                        if secondary_glass_button(ui, Some(VectorIcon::Trash), "Удалить").clicked()
                        {
                            server_to_delete = Some(idx);
                        }
                    });
                });
                ui.add_space(4.0);
            }

            if let Some(del_idx) = server_to_delete {
                servers.remove(del_idx);
                let _ = save_servers_dat(&inst_dir, &servers);
            }
        }

        if self.show_add_server_dialog {
            egui::Window::new("Добавление сервера")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .show(ui.ctx(), |ui| {
                    let modal_w = 320.0_f32.min(ui.ctx().screen_rect().width() - 32.0);
                    ui.set_width(modal_w);
                    ui.label("Название сервера:");
                    ui.text_edit_singleline(&mut self.new_server_name);
                    ui.add_space(4.0);
                    ui.label("Адрес сервера (IP / Host):");
                    ui.text_edit_singleline(&mut self.new_server_ip);
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        if primary_emerald_button(ui, Some(VectorIcon::Check), "Добавить").clicked()
                            && !self.new_server_ip.trim().is_empty()
                        {
                            let name = if self.new_server_name.trim().is_empty() {
                                self.new_server_ip.trim().to_string()
                            } else {
                                self.new_server_name.trim().to_string()
                            };
                            servers.push(ServerEntry {
                                name,
                                ip: self.new_server_ip.trim().to_string(),
                                icon: None,
                                accept_textures: 1,
                            });
                            let _ = save_servers_dat(&inst_dir, &servers);
                            self.new_server_name.clear();
                            self.new_server_ip.clear();
                            self.show_add_server_dialog = false;
                        }
                        if secondary_glass_button(ui, None, "Отмена").clicked() {
                            self.show_add_server_dialog = false;
                        }
                    });
                });
        }
    }

    fn render_screenshots_tab(
        &mut self,
        ui: &mut Ui,
        state: &mut UiState,
        inst_id: &mq_core::InstanceId,
    ) {
        let inst_dir = state.paths.instance_dir(inst_id.as_str());
        let screenshots = list_screenshots(&inst_dir).unwrap_or_default();

        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = Vec2::new(8.0, 6.0);
            ui.label(
                RichText::new(format!("Скриншоты: {}", screenshots.len()))
                    .strong()
                    .size(14.0),
            );
            if secondary_glass_button(ui, Some(VectorIcon::Folder), "Открыть screenshots/")
                .clicked()
            {
                let scr_dir = inst_dir.join("screenshots");
                let _ = std::fs::create_dir_all(&scr_dir);
                let _ = crate::opener::open_that(scr_dir);
            }
        });

        ui.add_space(6.0);

        if screenshots.is_empty() {
            ui.add_space(20.0);
            ui.label(RichText::new("Скриншотов пока нет. Нажмите F2 в игре!").color(TEXT_MUTED));
        } else {
            ScrollArea::vertical()
                .id_salt("screenshots_scroll")
                .show(ui, |ui| {
                    let mut file_to_delete = None;
                    for s in screenshots {
                        let card = egui::Frame::new()
                            .fill(CARD_SURFACE)
                            .stroke(Stroke::new(1.0_f32, BORDER_SUBTLE))
                            .corner_radius(RADIUS_CARD)
                            .inner_margin(egui::Margin::same(8));

                        card.show(ui, |ui| {
                            ui.horizontal(|ui| {
                                let (r, _) =
                                    ui.allocate_exact_size(Vec2::splat(18.0), egui::Sense::hover());
                                paint_vector_icon(ui.painter(), r, VectorIcon::Eye, ACCENT_EMERALD);

                                let right_w = 120.0;
                                let middle_w = (ui.available_width() - right_w - 8.0).max(60.0);

                                ui.allocate_ui_with_layout(
                                    Vec2::new(middle_w, 38.0),
                                    egui::Layout::top_down(egui::Align::Min),
                                    |ui| {
                                        let text_w = (ui.available_width() - 8.0).max(40.0);
                                        ui.add_sized(
                                            [text_w, 18.0],
                                            egui::Label::new(
                                                RichText::new(&s.file_name)
                                                    .color(TEXT_PRIMARY)
                                                    .strong(),
                                            )
                                            .truncate(),
                                        );
                                        let mib = (s.size_bytes as f64) / (1024.0 * 1024.0);
                                        ui.add_sized(
                                            [text_w, 16.0],
                                            egui::Label::new(
                                                RichText::new(format!(
                                                    "{mib:.2} МБ • {}",
                                                    s.created_at.format("%Y-%m-%d %H:%M")
                                                ))
                                                .color(TEXT_MUTED)
                                                .small(),
                                            )
                                            .truncate(),
                                        );
                                    },
                                );

                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        if secondary_glass_button(ui, Some(VectorIcon::Trash), "")
                                            .clicked()
                                        {
                                            file_to_delete = Some(s.path.clone());
                                        }
                                        if secondary_glass_button(
                                            ui,
                                            Some(VectorIcon::Eye),
                                            "Открыть",
                                        )
                                        .clicked()
                                        {
                                            let _ = crate::opener::open_that(&s.path);
                                        }
                                    },
                                );
                            });
                        });
                        ui.add_space(4.0);
                    }
                    if let Some(path) = file_to_delete {
                        let _ = std::fs::remove_file(path);
                    }
                });
        }
    }

    fn render_settings_tab(
        &mut self,
        ui: &mut Ui,
        state: &mut UiState,
        inst_id: &mq_core::InstanceId,
    ) {
        if let Some(inst) = state.instances.iter_mut().find(|i| i.id == *inst_id) {
            ui.label(RichText::new("Конфигурация инстанса").strong());
            ui.add_space(6.0);

            ui.label(format!(
                "Выделенная оперативная память (RAM): {} МБ",
                inst.memory_max_mb
            ));
            ui.add(egui::Slider::new(&mut inst.memory_max_mb, 1024..=16384).step_by(512.0));

            ui.add_space(8.0);
            ui.label("Команда-враппер запуска (например, gamemoderun, mangohud, prime-run):");
            let mut wrapper_str = inst.wrapper_cmd.clone().unwrap_or_default();
            if ui.text_edit_singleline(&mut wrapper_str).changed() {
                inst.wrapper_cmd = if wrapper_str.trim().is_empty() {
                    None
                } else {
                    Some(wrapper_str.trim().to_string())
                };
            }

            ui.add_space(8.0);
            ui.label("Дополнительные аргументы JVM (через пробел):");
            let mut jvm_args_str = inst.jvm_args.join(" ");
            if ui.text_edit_singleline(&mut jvm_args_str).changed() {
                inst.jvm_args = jvm_args_str
                    .split_whitespace()
                    .map(|s| s.to_string())
                    .collect();
            }

            ui.add_space(8.0);
            ui.label("Язык Minecraft для этого инстанса (80+ языков):");
            let effective_loc = inst
                .mc_locale
                .as_deref()
                .unwrap_or(state.config.default_mc_locale.as_str());
            let current_loc_info = mq_meta::find_mc_locale(effective_loc);
            let display_name = current_loc_info
                .map(|i| i.display_label())
                .unwrap_or_else(|| effective_loc.to_string());

            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = Vec2::new(8.0, 6.0);
                version_badge(ui, &display_name, ACCENT_EMERALD);
                if secondary_glass_button(ui, Some(VectorIcon::Globe), "Выбрать другой язык...")
                    .clicked()
                {
                    self.show_locale_dialog = true;
                }
                if inst.mc_locale.is_some()
                    && secondary_glass_button(
                        ui,
                        Some(VectorIcon::Cross),
                        "Сбросить на язык по умолчанию",
                    )
                    .clicked()
                {
                    inst.mc_locale = None;
                }
            });

            if self.show_locale_dialog {
                egui::Window::new("Выбор языка Minecraft")
                    .collapsible(false)
                    .resizable(true)
                    .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                    .show(ui.ctx(), |ui| {
                        let modal_w = 380.0_f32.min(ui.ctx().screen_rect().width() - 32.0);
                        ui.set_width(modal_w);
                        ui.horizontal(|ui| {
                            let (r, _) =
                                ui.allocate_exact_size(Vec2::splat(16.0), egui::Sense::hover());
                            paint_vector_icon(ui.painter(), r, VectorIcon::Search, ACCENT_COBALT);
                            ui.add(
                                egui::TextEdit::singleline(&mut self.locale_search_query)
                                    .hint_text("Поиск языка..."),
                            );
                        });
                        ui.add_space(6.0);
                        ScrollArea::vertical()
                            .max_height(200.0)
                            .id_salt("inst_locale_modal_scroll")
                            .show(ui, |ui| {
                                for loc in mq_meta::search_mc_locales(&self.locale_search_query)
                                    .iter()
                                    .take(30)
                                {
                                    let is_active = inst.mc_locale.as_deref() == Some(loc.code);
                                    if ui
                                        .selectable_label(is_active, loc.display_label())
                                        .clicked()
                                    {
                                        inst.mc_locale = Some(loc.code.to_string());
                                        self.show_locale_dialog = false;
                                    }
                                }
                            });
                        ui.add_space(8.0);
                        if secondary_glass_button(ui, None, "Закрыть").clicked() {
                            self.show_locale_dialog = false;
                        }
                    });
            }

            ui.add_space(14.0);
            let mut should_delete = false;
            let mut should_save = false;

            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = Vec2::new(10.0, 8.0);
                if primary_emerald_button(ui, Some(VectorIcon::Check), "Сохранить параметры")
                    .clicked()
                {
                    should_save = true;
                }

                if danger_button(ui, Some(VectorIcon::Trash), "Удалить инстанс").clicked()
                {
                    should_delete = true;
                }
            });

            if should_save {
                let _ = state.instance_mgr.save_instance(inst);
                state.status_message = Some(("Параметры инстанса сохранены".to_string(), false));
            }

            if should_delete {
                self.show_delete_dialog = true;
                self.instance_to_delete = Some((inst_id.clone(), inst.name.clone()));
            }
        }
    }
}

fn list_packs_in_dir(dir: &std::path::Path) -> Vec<(String, bool, u64)> {
    let mut packs = Vec::new();
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().into_owned();
            let is_disabled = name.ends_with(".disabled");
            let size = if path.is_file() {
                entry.metadata().map(|m| m.len()).unwrap_or(0)
            } else {
                0
            };
            packs.push((name, !is_disabled, size));
        }
    }
    packs.sort_by(|a, b| a.0.cmp(&b.0));
    packs
}

fn toggle_pack_file(dir: &std::path::Path, filename: &str) {
    let src = dir.join(filename);
    let dst = if filename.ends_with(".disabled") {
        dir.join(filename.trim_end_matches(".disabled"))
    } else {
        dir.join(format!("{filename}.disabled"))
    };
    let _ = std::fs::rename(src, dst);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_catalogs_contain_expected_categories_and_versions() {
        assert!(ALL_MC_RELEASES.contains(&"1.21.4"));
        assert!(ALL_MC_RELEASES.contains(&"1.7.10"));
        assert!(ALL_MC_SNAPSHOTS.contains(&"25w09b"));
        assert!(ALL_MC_BETAS.contains(&"b1.7.3"));
        assert!(ALL_MC_ALPHAS.contains(&"a1.2.6"));
        assert!(ALL_MC_ALPHAS.contains(&"c0.30_01c"));
    }

    #[test]
    fn version_categories_have_clean_labels_and_colors() {
        assert_eq!(McVersionCategory::Release.badge_label(), "Релиз");
        assert_eq!(McVersionCategory::Snapshot.badge_label(), "Снапшот");
        assert_eq!(McVersionCategory::OldBeta.badge_label(), "Beta");
        assert_eq!(McVersionCategory::OldAlpha.badge_label(), "Alpha");
    }

    #[test]
    fn instances_view_default_configuration() {
        let view = InstancesView::default();
        assert_eq!(view.view_mode, InstanceViewMode::Grid);
        assert_eq!(view.loader_filter, LoaderFilter::All);
        assert!(view.show_releases);
        assert!(!view.show_snapshots);
        assert!(!view.show_betas);
        assert!(!view.show_alphas);
        assert!(view.version_search.is_empty());
        assert!(!view.show_delete_dialog);
        assert!(view.instance_to_delete.is_none());
    }

    fn test_ui_state() -> (tempfile::TempDir, UiState) {
        let tmp = tempfile::tempdir().expect("tempdir");
        let paths = mq_core::MqPaths::with_root(tmp.path().to_path_buf());
        let config = mq_core::AppConfig::default();
        let pool = std::sync::Arc::new(mq_sched::ComputePool::new(2));
        let state = UiState::new(paths, config, pool).expect("ui state");
        (tmp, state)
    }

    #[test]
    fn empty_state_renders_cleanly_without_panic() {
        let (_tmp, mut state) = test_ui_state();
        assert!(state.instances.is_empty());

        let mut view = InstancesView::default();
        let ctx = egui::Context::default();
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                Vec2::new(1024.0, 720.0),
            )),
            ..Default::default()
        };

        let _ = ctx.run(input, |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                view.ui(ui, &mut state);
            });
        });
    }

    #[test]
    fn create_dialog_renders_skeleton_and_manifest_with_26x_support() {
        let (_tmp, mut state) = test_ui_state();
        state.show_create_instance_dialog = true;
        state.is_fetching_manifest = true;
        assert!(state.create_inst_name.is_empty());
        assert!(state.create_inst_mc_ver.is_empty());

        let mut view = InstancesView::default();
        let ctx = egui::Context::default();
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                Vec2::new(1024.0, 720.0),
            )),
            ..Default::default()
        };

        // 1. Loading skeleton state
        let _ = ctx.run(input.clone(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                view.ui(ui, &mut state);
            });
        });

        // 2. Manifest fetched with 26.x release
        state.latest_release = Some("26.1".to_string());
        state.remote_versions = vec![
            ("26.1".to_string(), "release".to_string()),
            ("26.1-snapshot-1".to_string(), "snapshot".to_string()),
            ("1.21.4".to_string(), "release".to_string()),
        ];
        state.is_fetching_manifest = false;

        let _ = ctx.run(input, |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                view.ui(ui, &mut state);
            });
        });

        assert_eq!(state.create_inst_mc_ver, "26.1");
    }

    #[test]
    fn filtered_instances_filters_by_query_and_loader() {
        let (_tmp, mut state) = test_ui_state();
        let fab = state
            .instance_mgr
            .create_instance("My Fabric", "26.1", ModLoaderKind::Fabric, None)
            .expect("fabric");
        let van = state
            .instance_mgr
            .create_instance("Classic Vanilla", "1.21.4", ModLoaderKind::Vanilla, None)
            .expect("vanilla");
        state.instances.push(fab);
        state.instances.push(van);

        let mut view = InstancesView::default();
        assert_eq!(view.filtered_instances(&state).len(), 2);

        view.loader_filter = LoaderFilter::Fabric;
        let only_fab = view.filtered_instances(&state);
        assert_eq!(only_fab.len(), 1);
        assert_eq!(only_fab[0].name, "My Fabric");

        view.loader_filter = LoaderFilter::All;
        view.search_query = "26.1".to_string();
        let by_ver = view.filtered_instances(&state);
        assert_eq!(by_ver.len(), 1);
        assert_eq!(by_ver[0].mc_version, "26.1");
    }
}
