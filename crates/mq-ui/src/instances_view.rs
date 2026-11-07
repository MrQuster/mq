#![deny(clippy::await_holding_lock)]
//! Instances screen: instance grid, creation dialog, instance details, mods table,
//! live game console with log filtering, and JVM settings. Zero emojis!

use egui::{Color32, RichText, ScrollArea, Stroke, Ui, Vec2};
use mq_core::{GameLogLevel, ModLoaderKind};
use mq_instances::{
    backup_world, delete_world, export_instance_to_zip, export_world_to_zip,
    import_instance_from_zip, list_backups, list_screenshots, list_worlds, load_servers_dat,
    ping_server, restore_world, save_servers_dat, ServerEntry,
};

use crate::state::{UiState, UiWorkerCommand};
use crate::theme::{
    ACCENT_COBALT, ACCENT_EMERALD, BORDER_SUBTLE, CARD_SURFACE, CARD_SURFACE_HOVER, RADIUS_CARD,
    STATUS_DANGER, STATUS_WARN, TEXT_MUTED, TEXT_PRIMARY,
};
use crate::widgets::{
    hero_action_button, loader_badge, paint_vector_icon, primary_emerald_button,
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
    "1.21.4", "1.21.3", "1.21.2", "1.21.1", "1.21", "1.20.6", "1.20.5", "1.20.4", "1.20.3",
    "1.20.2", "1.20.1", "1.20", "1.19.4", "1.19.3", "1.19.2", "1.19.1", "1.19", "1.18.2", "1.18.1",
    "1.18", "1.17.1", "1.17", "1.16.5", "1.16.4", "1.16.3", "1.16.2", "1.16.1", "1.16", "1.15.2",
    "1.15.1", "1.15", "1.14.4", "1.14.3", "1.14.2", "1.14.1", "1.14", "1.13.2", "1.13.1", "1.13",
    "1.12.2", "1.12.1", "1.12", "1.11.2", "1.11", "1.10.2", "1.10.1", "1.10", "1.9.4", "1.9.2",
    "1.9", "1.8.9", "1.8.8", "1.8", "1.7.10", "1.7.9", "1.7.2", "1.6.4", "1.6.2", "1.5.2", "1.4.7",
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
    "1.21.1-rc1",
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

/// Highlighted popular versions for quick 1-click selection chips.
pub const POPULAR_CHIPS: &[&str] = &["1.21.1", "1.20.1", "1.16.5", "1.12.2", "1.8.9", "b1.7.3"];

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
}

impl Default for InstancesView {
    fn default() -> Self {
        Self {
            current_tab: InstanceDetailTab::Console,
            view_mode: InstanceViewMode::List,
            sort_mode: InstanceSortMode::Playtime,
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
        }
    }
}

impl InstancesView {
    pub fn ui(&mut self, ui: &mut Ui, state: &mut UiState) {
        if state.selected_instance_id.is_none() && !state.instances.is_empty() {
            state.selected_instance_id = Some(state.instances[0].id.clone());
        }

        ui.horizontal(|ui| {
            ui.heading(
                RichText::new("Инстансы и Сборки")
                    .color(TEXT_PRIMARY)
                    .size(20.0)
                    .strong(),
            );

            ui.add_space(16.0);
            ui.add(
                egui::TextEdit::singleline(&mut self.search_query)
                    .hint_text("Поиск инстанса...")
                    .desired_width(160.0),
            );

            ui.add_space(8.0);
            ui.selectable_value(
                &mut self.sort_mode,
                InstanceSortMode::Playtime,
                "По времени",
            );
            ui.selectable_value(&mut self.sort_mode, InstanceSortMode::Name, "По имени");
            ui.selectable_value(&mut self.sort_mode, InstanceSortMode::Created, "По дате");

            ui.add_space(8.0);
            ui.selectable_value(&mut self.view_mode, InstanceViewMode::List, "Список");
            ui.selectable_value(&mut self.view_mode, InstanceViewMode::Grid, "Сетка");

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if primary_emerald_button(ui, Some(VectorIcon::Plus), "Создать инстанс").clicked()
                {
                    state.show_create_instance_dialog = true;
                }
                if secondary_glass_button(ui, Some(VectorIcon::Download), "Импорт").clicked()
                {
                    self.show_import_dialog = true;
                }
            });
        });

        ui.add_space(8.0);

        if state.instances.is_empty() && !state.show_create_instance_dialog {
            self.render_empty_onboarding_hub(ui, state);
            return;
        }

        // Instance creation modal
        if state.show_create_instance_dialog {
            // Trigger manifest fetch on first open if empty
            if state.remote_versions.is_empty() && !state.is_fetching_manifest {
                state.is_fetching_manifest = true;
                let _ = state.cmd_sender.send(UiWorkerCommand::FetchVersionManifest);
            }

            egui::Window::new("Создание нового инстанса Minecraft")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .show(ui.ctx(), |ui| {
                    ui.set_width(460.0);

                    // 1. Minecraft Version Selection
                    ui.label(
                        RichText::new("Версия Minecraft:")
                            .color(TEXT_PRIMARY)
                            .strong(),
                    );
                    ui.add_space(2.0);

                    // Quick-pick popular chips
                    ui.horizontal_wrapped(|ui| {
                        ui.spacing_mut().item_spacing = Vec2::new(6.0, 4.0);
                        for &v in POPULAR_CHIPS {
                            let is_selected = state.create_inst_mc_ver == v;
                            let label = if v == "1.21.1" {
                                format!("{v} (Рекомендуемая)")
                            } else {
                                v.to_string()
                            };
                            if ui.selectable_label(is_selected, label).clicked() {
                                state.create_inst_mc_ver = v.to_string();
                                state.create_inst_name =
                                    format!("{} {}", v, state.create_inst_loader.as_str());
                            }
                        }
                    });

                    ui.add_space(6.0);

                    // Category Filter Toggles & Manifest sync
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("Категории:").color(TEXT_MUTED).small());
                        ui.checkbox(&mut self.show_releases, "Релизы");
                        ui.checkbox(&mut self.show_snapshots, "Снапшоты");
                        ui.checkbox(&mut self.show_betas, "Beta");
                        ui.checkbox(&mut self.show_alphas, "Alpha");

                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if state.is_fetching_manifest {
                                ui.label(RichText::new("Загрузка...").color(ACCENT_COBALT).small());
                            } else if secondary_glass_button(
                                ui,
                                Some(VectorIcon::Refresh),
                                "Mojang",
                            )
                            .clicked()
                            {
                                state.is_fetching_manifest = true;
                                let _ =
                                    state.cmd_sender.send(UiWorkerCommand::FetchVersionManifest);
                            }
                        });
                    });

                    ui.add_space(4.0);

                    // Search input for versions
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("Поиск:").color(TEXT_MUTED).small());
                        ui.add(
                            egui::TextEdit::singleline(&mut self.version_search)
                                .hint_text("Фильтр (1.20, b1.7, 24w, rd)...")
                                .desired_width(ui.available_width() - 60.0),
                        );
                        if !self.version_search.is_empty() && ui.small_button("Сброс").clicked()
                        {
                            self.version_search.clear();
                        }
                    });

                    ui.add_space(6.0);

                    // Build filtered version list
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
                    } else {
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

                    // Scrollable version list
                    let picker_frame = egui::Frame::new()
                        .fill(CARD_SURFACE)
                        .stroke(Stroke::new(1.0_f32, BORDER_SUBTLE))
                        .corner_radius(RADIUS_CARD)
                        .inner_margin(egui::Margin::same(6));

                    picker_frame.show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ScrollArea::vertical()
                            .id_salt("version_select_scroll")
                            .max_height(130.0)
                            .show(ui, |ui| {
                                if filtered.is_empty() {
                                    ui.add_space(8.0);
                                    ui.label(
                                        RichText::new("Нет версий, соответствующих фильтрам")
                                            .color(TEXT_MUTED),
                                    );
                                } else {
                                    for (vid, cat) in filtered {
                                        let is_sel = state.create_inst_mc_ver == vid;
                                        ui.horizontal(|ui| {
                                            let text_col =
                                                if is_sel { ACCENT_COBALT } else { TEXT_PRIMARY };
                                            if ui
                                                .selectable_label(
                                                    is_sel,
                                                    RichText::new(vid).color(text_col).strong(),
                                                )
                                                .clicked()
                                            {
                                                state.create_inst_mc_ver = vid.to_string();
                                                state.create_inst_name = format!(
                                                    "{} {}",
                                                    vid,
                                                    state.create_inst_loader.as_str()
                                                );
                                            }
                                            version_badge(ui, cat.badge_label(), cat.badge_color());
                                        });
                                    }
                                }
                            });
                    });

                    ui.add_space(4.0);

                    // Selected version editable field
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("Выбранная версия:").color(TEXT_MUTED).small());
                        let mut ver_str = state.create_inst_mc_ver.clone();
                        if ui.text_edit_singleline(&mut ver_str).changed() {
                            state.create_inst_mc_ver = ver_str.clone();
                            state.create_inst_name =
                                format!("{} {}", ver_str, state.create_inst_loader.as_str());
                        }
                    });

                    ui.add_space(10.0);

                    // 2. Mod Loader Selection
                    ui.label(
                        RichText::new("Загрузчик модов:")
                            .color(TEXT_PRIMARY)
                            .strong(),
                    );
                    ui.add_space(2.0);
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
                                state.create_inst_name =
                                    format!("{} {}", state.create_inst_mc_ver, ldr.as_str());
                            }
                        }
                    });

                    ui.add_space(10.0);

                    // 3. Allocated Memory (RAM)
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new(format!(
                                "Выделенная память: {} МБ ({:.1} ГБ)",
                                state.create_inst_ram_mb,
                                state.create_inst_ram_mb as f32 / 1024.0
                            ))
                            .color(TEXT_PRIMARY)
                            .strong(),
                        );
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            for &(mb, lbl) in &[
                                (8192, "8 ГБ"),
                                (6144, "6 ГБ"),
                                (4096, "4 ГБ"),
                                (2048, "2 ГБ"),
                            ] {
                                if ui
                                    .selectable_label(state.create_inst_ram_mb == mb, lbl)
                                    .clicked()
                                {
                                    state.create_inst_ram_mb = mb;
                                }
                            }
                        });
                    });
                    ui.add(
                        egui::Slider::new(&mut state.create_inst_ram_mb, 1024..=16384)
                            .step_by(512.0)
                            .show_value(false),
                    );

                    ui.add_space(10.0);

                    // 4. Instance Name
                    ui.label(
                        RichText::new("Название инстанса:")
                            .color(TEXT_PRIMARY)
                            .strong(),
                    );
                    ui.text_edit_singleline(&mut state.create_inst_name);

                    ui.add_space(14.0);
                    ui.horizontal(|ui| {
                        if primary_emerald_button(ui, Some(VectorIcon::Check), "Создать инстанс")
                            .clicked()
                        {
                            if let Ok(mut manifest) = state.instance_mgr.create_instance(
                                &state.create_inst_name,
                                &state.create_inst_mc_ver,
                                state.create_inst_loader,
                                None,
                            ) {
                                manifest.memory_max_mb = state.create_inst_ram_mb;
                                let _ = state.instance_mgr.save_instance(&manifest);
                                state.selected_instance_id = Some(manifest.id.clone());
                                state.instances.push(manifest);
                            }
                            state.show_create_instance_dialog = false;
                        }
                        if secondary_glass_button(ui, None, "Отмена").clicked() {
                            state.show_create_instance_dialog = false;
                        }
                    });
                });
        }

        // Duplicate modal
        if self.show_duplicate_dialog {
            if let Some(selected_inst) = state.selected_instance().cloned() {
                egui::Window::new("Дублирование инстанса")
                    .collapsible(false)
                    .resizable(false)
                    .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                    .show(ui.ctx(), |ui| {
                        ui.set_width(340.0);
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
                    ui.set_width(400.0);
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

        // Layout: Left column (instances list) + Right column (selected instance details)
        ui.horizontal(|ui| {
            // Left list of instances
            ui.vertical(|ui| {
                ui.set_width(220.0);
                ui.label(RichText::new("Установленные версии").color(TEXT_MUTED));
                ui.add_space(4.0);

                ScrollArea::vertical()
                    .id_salt("instances_scroll")
                    .show(ui, |ui| {
                        if state.instances.is_empty() {
                            ui.label(
                                RichText::new("Нет инстансов. Создайте первый!").color(TEXT_MUTED),
                            );
                        }

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
                                        ui.label(
                                            RichText::new(&inst.name).color(TEXT_PRIMARY).strong(),
                                        );
                                        ui.horizontal(|ui| {
                                            version_badge(ui, &inst.mc_version, ACCENT_COBALT);
                                            loader_badge(ui, inst.loader);
                                            if is_downloading {
                                                version_badge(ui, "Загрузка", ACCENT_COBALT);
                                            } else if is_running {
                                                version_badge(ui, "Запущен", ACCENT_EMERALD);
                                            } else if !is_installed {
                                                version_badge(ui, "Не скачан", STATUS_WARN);
                                            }
                                        });
                                    });
                                });
                            });

                            if resp.response.interact(egui::Sense::click()).clicked() {
                                state.selected_instance_id = Some(inst.id.clone());
                                // Trigger scanning mods
                                let _ = state.cmd_sender.send(UiWorkerCommand::ScanInstanceMods {
                                    instance_id: inst.id.clone(),
                                });
                            }

                            ui.add_space(4.0);
                        }
                    });
            });

            ui.separator();

            // Right selected instance details
            ui.vertical(|ui| {
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

                    // 1. Error Banner (if any)
                    if let Some(err) = state.install_error.clone() {
                        let err_frame = egui::Frame::new()
                            .fill(Color32::from_rgba_unmultiplied(239, 68, 68, 28))
                            .stroke(Stroke::new(1.0_f32, STATUS_DANGER))
                            .corner_radius(RADIUS_CARD)
                            .inner_margin(egui::Margin::same(10));

                        err_frame.show(ui, |ui| {
                            ui.horizontal(|ui| {
                                let (rect, _) =
                                    ui.allocate_exact_size(Vec2::splat(18.0), egui::Sense::hover());
                                paint_vector_icon(
                                    ui.painter(),
                                    rect,
                                    VectorIcon::Warning,
                                    STATUS_DANGER,
                                );
                                ui.label(RichText::new(&err).color(TEXT_PRIMARY).size(13.0));
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        if secondary_glass_button(
                                            ui,
                                            Some(VectorIcon::Cross),
                                            "Закрыть",
                                        )
                                        .clicked()
                                        {
                                            state.install_error = None;
                                        }
                                    },
                                );
                            });
                        });
                        ui.add_space(8.0);
                    }

                    // 2. Active Download Progress Card (prominent real-time view)
                    if is_downloading {
                        let stage = state
                            .active_phase
                            .as_deref()
                            .unwrap_or("Загрузка данных игры...");
                        let (frac, speed, comp_bytes, tot_bytes, comp_items, tot_items) =
                            if let Some(prog) = &state.phase_progress {
                                let f = if prog.total_items > 0 {
                                    prog.completed_items as f32 / prog.total_items as f32
                                } else {
                                    0.0
                                };
                                (
                                    f,
                                    prog.mib_per_sec * 1024.0 * 1024.0,
                                    prog.bytes_done,
                                    prog.bytes_total,
                                    prog.completed_items,
                                    prog.total_items,
                                )
                            } else {
                                (0.0, 0.0, 0, 0, 0, 0)
                            };

                        crate::widgets::render_download_progress_card(
                            ui,
                            stage,
                            frac,
                            speed,
                            comp_bytes,
                            tot_bytes,
                            comp_items,
                            tot_items,
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
                        ui.horizontal(|ui| {
                            ui.vertical(|ui| {
                                ui.heading(
                                    RichText::new(&inst_name)
                                        .color(TEXT_PRIMARY)
                                        .size(20.0)
                                        .strong(),
                                );
                                ui.add_space(4.0);
                                ui.horizontal(|ui| {
                                    version_badge(ui, &mc_ver, ACCENT_COBALT);
                                    loader_badge(ui, loader);
                                    version_badge(
                                        ui,
                                        &format!("{} МБ RAM", memory_max_mb),
                                        TEXT_MUTED,
                                    );
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
                            });

                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    if is_running {
                                        if hero_action_button(
                                            ui,
                                            VectorIcon::Stop,
                                            "ОСТАНОВИТЬ",
                                            STATUS_DANGER,
                                        )
                                        .clicked()
                                        {
                                            let _ = state
                                                .cmd_sender
                                                .send(UiWorkerCommand::CancelDownload);
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
                                            let _ = state
                                                .cmd_sender
                                                .send(UiWorkerCommand::CancelDownload);
                                        }
                                    } else if is_installed {
                                        let btn_text = if state.is_launching {
                                            "ЗАПУСК..."
                                        } else {
                                            "ИГРАТЬ"
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
                                            let _ = state.cmd_sender.send(
                                                UiWorkerCommand::LaunchInstance {
                                                    instance_id: inst_id.clone(),
                                                    dry_run: false,
                                                },
                                            );
                                        }
                                    } else {
                                        let btn_text = if state.is_launching {
                                            "СКАЧИВАНИЕ И ЗАПУСК..."
                                        } else {
                                            "СКАЧАТЬ И ИГРАТЬ"
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
                                            let _ = state.cmd_sender.send(
                                                UiWorkerCommand::LaunchInstance {
                                                    instance_id: inst_id.clone(),
                                                    dry_run: false,
                                                },
                                            );
                                        }
                                    }

                                    ui.add_space(10.0);

                                    if !is_downloading {
                                        if is_installed {
                                            if secondary_glass_button(
                                                ui,
                                                Some(VectorIcon::Refresh),
                                                "Переустановить",
                                            )
                                            .clicked()
                                            {
                                                let _ = state.cmd_sender.send(
                                                    UiWorkerCommand::InstallInstance {
                                                        instance_id: inst_id.clone(),
                                                    },
                                                );
                                            }
                                            if secondary_glass_button(
                                                ui,
                                                Some(VectorIcon::Verify),
                                                "Проверить",
                                            )
                                            .clicked()
                                            {
                                                let _ = state.cmd_sender.send(
                                                    UiWorkerCommand::VerifyInstance {
                                                        instance_id: inst_id.clone(),
                                                    },
                                                );
                                            }
                                        } else if secondary_glass_button(
                                            ui,
                                            Some(VectorIcon::Download),
                                            "Установить",
                                        )
                                        .clicked()
                                        {
                                            let _ = state.cmd_sender.send(
                                                UiWorkerCommand::InstallInstance {
                                                    instance_id: inst_id.clone(),
                                                },
                                            );
                                        }
                                    }

                                    if secondary_glass_button(
                                        ui,
                                        Some(VectorIcon::Folder),
                                        "Папка игры",
                                    )
                                    .clicked()
                                    {
                                        let inst_dir = state.paths.instance_dir(inst_id.as_str());
                                        let _ = std::process::Command::new("xdg-open")
                                            .arg(inst_dir)
                                            .spawn();
                                    }
                                    if secondary_glass_button(
                                        ui,
                                        Some(VectorIcon::Folder),
                                        "Папка mods",
                                    )
                                    .clicked()
                                    {
                                        let mods_dir =
                                            state.paths.instance_dir(inst_id.as_str()).join("mods");
                                        let _ = std::fs::create_dir_all(&mods_dir);
                                        let _ = std::process::Command::new("xdg-open")
                                            .arg(mods_dir)
                                            .spawn();
                                    }
                                    if secondary_glass_button(
                                        ui,
                                        Some(VectorIcon::Download),
                                        "Экспорт .zip",
                                    )
                                    .clicked()
                                    {
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
                                    if secondary_glass_button(
                                        ui,
                                        Some(VectorIcon::Layers),
                                        "Дублировать",
                                    )
                                    .clicked()
                                    {
                                        self.show_duplicate_dialog = true;
                                        self.duplicate_name = format!("{} (Копия)", inst_name);
                                    }
                                },
                            );
                        });
                    });

                    ui.add_space(10.0);

                    // Tab selector (8 primary tabs)
                    ui.horizontal_wrapped(|ui| {
                        for (tab, label) in [
                            (InstanceDetailTab::Console, "Консоль"),
                            (InstanceDetailTab::Mods, "Моды"),
                            (InstanceDetailTab::ResourcePacks, "Ресурспаки"),
                            (InstanceDetailTab::ShaderPacks, "Шейдеры"),
                            (InstanceDetailTab::Worlds, "Миры"),
                            (InstanceDetailTab::Servers, "Серверы"),
                            (InstanceDetailTab::Screenshots, "Скриншоты"),
                            (InstanceDetailTab::Settings, "Параметры"),
                        ] {
                            if ui
                                .selectable_label(self.current_tab == tab, label)
                                .clicked()
                            {
                                self.current_tab = tab;
                                if tab == InstanceDetailTab::Mods {
                                    let _ =
                                        state.cmd_sender.send(UiWorkerCommand::ScanInstanceMods {
                                            instance_id: inst_id.clone(),
                                        });
                                }
                            }
                        }
                    });

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

    fn render_empty_onboarding_hub(&mut self, ui: &mut Ui, state: &mut UiState) {
        ui.vertical_centered(|ui| {
            ui.add_space(32.0);

            // Large geometric emblem
            let (rect, _) = ui.allocate_exact_size(Vec2::splat(64.0), egui::Sense::hover());
            let painter = ui.painter();
            painter.circle_filled(rect.center(), 32.0, ACCENT_EMERALD.gamma_multiply(0.12));
            painter.circle_stroke(
                rect.center(),
                32.0,
                Stroke::new(1.5_f32, ACCENT_EMERALD.gamma_multiply(0.5)),
            );
            let icon_rect = egui::Rect::from_center_size(rect.center(), Vec2::splat(32.0));
            paint_vector_icon(painter, icon_rect, VectorIcon::Layers, ACCENT_EMERALD);

            ui.add_space(16.0);
            ui.heading(
                RichText::new("Добро пожаловать в MQ")
                    .color(TEXT_PRIMARY)
                    .size(24.0)
                    .strong(),
            );
            ui.add_space(4.0);
            ui.label(
                RichText::new(
                    "Сверхбыстрый модульный лаунчер Minecraft на чистом Rust для Arch Linux",
                )
                .color(TEXT_MUTED)
                .size(14.0),
            );

            ui.add_space(28.0);

            // 3 Quick Action Cards in horizontal row
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 16.0;

                // Card 1: Vanilla 1.21.1
                let card1 = egui::Frame::new()
                    .fill(CARD_SURFACE)
                    .stroke(Stroke::new(1.0_f32, BORDER_SUBTLE))
                    .corner_radius(RADIUS_CARD)
                    .inner_margin(egui::Margin::same(18));
                card1.show(ui, |ui| {
                    ui.set_width(220.0);
                    ui.horizontal(|ui| {
                        let (r, _) =
                            ui.allocate_exact_size(Vec2::splat(20.0), egui::Sense::hover());
                        paint_vector_icon(ui.painter(), r, VectorIcon::Check, ACCENT_COBALT);
                        ui.label(RichText::new("Vanilla 1.21.1").color(TEXT_PRIMARY).strong());
                    });
                    ui.add_space(6.0);
                    ui.label(
                        RichText::new("Чистая официальная версия игры без сторонних модов.")
                            .color(TEXT_MUTED)
                            .small(),
                    );
                    ui.add_space(14.0);
                    if primary_emerald_button(ui, Some(VectorIcon::Plus), "Создать Vanilla")
                        .clicked()
                    {
                        if let Ok(mut manifest) = state.instance_mgr.create_instance(
                            "Vanilla 1.21.1",
                            "1.21.1",
                            ModLoaderKind::Vanilla,
                            None,
                        ) {
                            manifest.memory_max_mb = 4096;
                            let _ = state.instance_mgr.save_instance(&manifest);
                            state.selected_instance_id = Some(manifest.id.clone());
                            state.instances.push(manifest);
                        }
                    }
                });

                // Card 2: Fabric 1.21.1
                let card2 = egui::Frame::new()
                    .fill(CARD_SURFACE)
                    .stroke(Stroke::new(1.0_f32, BORDER_SUBTLE))
                    .corner_radius(RADIUS_CARD)
                    .inner_margin(egui::Margin::same(18));
                card2.show(ui, |ui| {
                    ui.set_width(220.0);
                    ui.horizontal(|ui| {
                        let (r, _) =
                            ui.allocate_exact_size(Vec2::splat(20.0), egui::Sense::hover());
                        paint_vector_icon(
                            ui.painter(),
                            r,
                            VectorIcon::Layers,
                            Color32::from_rgb(0xE2, 0xB3, 0x7E),
                        );
                        ui.label(RichText::new("Fabric 1.21.1").color(TEXT_PRIMARY).strong());
                    });
                    ui.add_space(6.0);
                    ui.label(
                        RichText::new("Оптимизированный легковесный загрузчик для Sodium и Iris.")
                            .color(TEXT_MUTED)
                            .small(),
                    );
                    ui.add_space(14.0);
                    if primary_emerald_button(ui, Some(VectorIcon::Plus), "Создать Fabric")
                        .clicked()
                    {
                        if let Ok(mut manifest) = state.instance_mgr.create_instance(
                            "Fabric 1.21.1",
                            "1.21.1",
                            ModLoaderKind::Fabric,
                            None,
                        ) {
                            manifest.memory_max_mb = 4096;
                            let _ = state.instance_mgr.save_instance(&manifest);
                            state.selected_instance_id = Some(manifest.id.clone());
                            state.instances.push(manifest);
                        }
                    }
                });

                // Card 3: Custom creation
                let card3 = egui::Frame::new()
                    .fill(CARD_SURFACE)
                    .stroke(Stroke::new(1.0_f32, BORDER_SUBTLE))
                    .corner_radius(RADIUS_CARD)
                    .inner_margin(egui::Margin::same(18));
                card3.show(ui, |ui| {
                    ui.set_width(220.0);
                    ui.horizontal(|ui| {
                        let (r, _) =
                            ui.allocate_exact_size(Vec2::splat(20.0), egui::Sense::hover());
                        paint_vector_icon(ui.painter(), r, VectorIcon::Settings, ACCENT_EMERALD);
                        ui.label(RichText::new("Свой инстанс").color(TEXT_PRIMARY).strong());
                    });
                    ui.add_space(6.0);
                    ui.label(
                        RichText::new(
                            "Выбор любой версии (1.7 - 1.21), Quilt, Forge или NeoForge.",
                        )
                        .color(TEXT_MUTED)
                        .small(),
                    );
                    ui.add_space(14.0);
                    if secondary_glass_button(ui, Some(VectorIcon::Plus), "Настроить...").clicked()
                    {
                        state.show_create_instance_dialog = true;
                    }
                });
            });

            ui.add_space(32.0);

            // Technical badges
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 12.0;
                status_pill(ui, "CAS Дедупликация O(1)", ACCENT_EMERALD);
                status_pill(ui, "Rayon Многопоточность", ACCENT_COBALT);
                status_pill(
                    ui,
                    "Zero-Lag UI Watchdog",
                    Color32::from_rgb(0xC0, 0x84, 0xFC),
                );
            });
        });
    }

    fn render_mods_tab(&mut self, ui: &mut Ui, state: &mut UiState, inst_id: &mq_core::InstanceId) {
        ui.horizontal(|ui| {
            ui.label(
                RichText::new(format!("Установлено модов: {}", state.installed_mods.len()))
                    .strong(),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if secondary_glass_button(ui, Some(VectorIcon::Refresh), "Пересканировать")
                    .clicked()
                {
                    let _ = state.cmd_sender.send(UiWorkerCommand::ScanInstanceMods {
                        instance_id: inst_id.clone(),
                    });
                }
                if secondary_glass_button(ui, Some(VectorIcon::Folder), "Открыть mods/").clicked()
                {
                    let mods_dir = state.paths.instance_dir(inst_id.as_str()).join("mods");
                    let _ = std::process::Command::new("xdg-open").arg(mods_dir).spawn();
                }
            });
        });

        ui.add_space(4.0);

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

        ScrollArea::vertical().id_salt("mods_table_scroll").show(ui, |ui| {
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

                            ui.vertical(|ui| {
                                ui.horizontal(|ui| {
                                    ui.label(RichText::new(&m.name).color(TEXT_PRIMARY).strong());
                                    version_badge(ui, &m.version, ACCENT_COBALT);
                                    if let Some(ld) = m.loader {
                                        version_badge(ui, ld.as_str(), ACCENT_EMERALD);
                                    }
                                });
                                ui.label(RichText::new(format!("Файл: {}", m.file_name)).color(TEXT_MUTED).small());
                            });

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

        ui.horizontal(|ui| {
            ui.label(
                RichText::new(format!("Строк в логе: {}", state.game_logs.len())).color(TEXT_MUTED),
            );
            ui.checkbox(&mut self.console_autoscroll, "Автоскролл");

            ui.add_space(8.0);
            ui.add(
                egui::TextEdit::singleline(&mut self.console_search)
                    .hint_text("Поиск в логе...")
                    .desired_width(160.0),
            );

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if secondary_glass_button(ui, None, "Очистить").clicked() {
                    state.game_logs.clear();
                }
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
                if ui
                    .selectable_label(state.log_filter == Some(GameLogLevel::Error), "ERROR")
                    .clicked()
                {
                    state.log_filter = if state.log_filter == Some(GameLogLevel::Error) {
                        None
                    } else {
                        Some(GameLogLevel::Error)
                    };
                }
                if ui
                    .selectable_label(state.log_filter == Some(GameLogLevel::Warn), "WARN")
                    .clicked()
                {
                    state.log_filter = if state.log_filter == Some(GameLogLevel::Warn) {
                        None
                    } else {
                        Some(GameLogLevel::Warn)
                    };
                }
                if ui
                    .selectable_label(state.log_filter == Some(GameLogLevel::Info), "INFO")
                    .clicked()
                {
                    state.log_filter = if state.log_filter == Some(GameLogLevel::Info) {
                        None
                    } else {
                        Some(GameLogLevel::Info)
                    };
                }
            });
        });

        ui.add_space(4.0);

        let console_frame = egui::Frame::new()
            .fill(Color32::from_rgb(0x08, 0x0A, 0x0E))
            .stroke(Stroke::new(1.0_f32, BORDER_SUBTLE))
            .corner_radius(RADIUS_CARD)
            .inner_margin(egui::Margin::same(8));

        console_frame.show(ui, |ui| {
            ui.set_width(ui.available_width());
            ScrollArea::vertical()
                .id_salt("console_scroll")
                .auto_shrink([false, false])
                .stick_to_bottom(self.console_autoscroll)
                .show(ui, |ui| {
                    if state.game_logs.is_empty() {
                        ui.label(
                            RichText::new("Логи игры появятся здесь при запуске Minecraft")
                                .color(TEXT_MUTED)
                                .monospace(),
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

                            ui.label(RichText::new(&entry.line).color(col).monospace().small());
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

        ui.horizontal(|ui| {
            ui.label(
                RichText::new(format!("Ресурспаки: {}", packs.len()))
                    .strong()
                    .size(14.0),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if secondary_glass_button(ui, Some(VectorIcon::Folder), "Открыть resourcepacks/")
                    .clicked()
                {
                    let _ = std::process::Command::new("xdg-open")
                        .arg(&packs_dir)
                        .spawn();
                }
            });
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
                        ui.label(RichText::new(&name).color(TEXT_PRIMARY).strong());
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

        ui.horizontal(|ui| {
            ui.label(
                RichText::new(format!("Шейдерпаки: {}", shaders.len()))
                    .strong()
                    .size(14.0),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if secondary_glass_button(ui, Some(VectorIcon::Folder), "Открыть shaderpacks/")
                    .clicked()
                {
                    let _ = std::process::Command::new("xdg-open")
                        .arg(&shaders_dir)
                        .spawn();
                }
            });
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
                        ui.label(RichText::new(&name).color(TEXT_PRIMARY).strong());
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

        ui.horizontal(|ui| {
            ui.label(
                RichText::new("Сохранения миров (Worlds)")
                    .strong()
                    .size(14.0),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if secondary_glass_button(ui, Some(VectorIcon::Folder), "Открыть saves/").clicked()
                {
                    let saves_dir = inst_dir.join("saves");
                    let _ = std::fs::create_dir_all(&saves_dir);
                    let _ = std::process::Command::new("xdg-open")
                        .arg(saves_dir)
                        .spawn();
                }
            });
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
                    ui.horizontal(|ui| {
                        let (r, _) =
                            ui.allocate_exact_size(Vec2::splat(20.0), egui::Sense::hover());
                        paint_vector_icon(ui.painter(), r, VectorIcon::Cube, ACCENT_EMERALD);

                        ui.vertical(|ui| {
                            ui.horizontal(|ui| {
                                ui.label(
                                    RichText::new(&w.display_name).color(TEXT_PRIMARY).strong(),
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
                        });

                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if secondary_glass_button(ui, Some(VectorIcon::Trash), "").clicked() {
                                let _ = delete_world(&inst_dir, &w.folder_name);
                                state.status_message =
                                    Some((format!("Мир '{}' удален", w.display_name), false));
                            }
                            if secondary_glass_button(ui, Some(VectorIcon::Download), "Экспорт")
                                .clicked()
                            {
                                let export_path =
                                    backups_root.join(format!("{}.zip", w.folder_name));
                                if let Ok(()) =
                                    export_world_to_zip(&inst_dir, &w.folder_name, &export_path)
                                {
                                    state.status_message = Some((
                                        format!("Мир экспортирован в {}", export_path.display()),
                                        false,
                                    ));
                                }
                            }
                            if secondary_glass_button(ui, Some(VectorIcon::Download), "Бэкап")
                                .clicked()
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
                                                bp.file_name()
                                                    .unwrap_or_default()
                                                    .to_string_lossy()
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
                        });
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

        ui.horizontal(|ui| {
            ui.label(
                RichText::new(format!("Серверы (servers.dat): {}", servers.len()))
                    .strong()
                    .size(14.0),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if primary_emerald_button(ui, Some(VectorIcon::Plus), "Добавить сервер").clicked()
                {
                    self.show_add_server_dialog = true;
                }
            });
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
                    ui.horizontal(|ui| {
                        let (r, _) =
                            ui.allocate_exact_size(Vec2::splat(20.0), egui::Sense::hover());
                        paint_vector_icon(ui.painter(), r, VectorIcon::Server, ACCENT_COBALT);

                        ui.vertical(|ui| {
                            ui.horizontal(|ui| {
                                ui.label(RichText::new(&s.name).color(TEXT_PRIMARY).strong());
                                ui.label(RichText::new(&s.ip).color(TEXT_MUTED));
                            });
                            if let Some(res) = state.server_pings.get(&s.ip) {
                                ui.horizontal(|ui| {
                                    version_badge(
                                        ui,
                                        &format!("{} ms", res.latency_ms),
                                        ACCENT_EMERALD,
                                    );
                                    version_badge(
                                        ui,
                                        &format!("{}/{}", res.players_online, res.players_max),
                                        ACCENT_COBALT,
                                    );
                                    ui.label(RichText::new(&res.motd).color(TEXT_PRIMARY).small());
                                });
                            }
                        });

                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if secondary_glass_button(ui, Some(VectorIcon::Trash), "").clicked() {
                                server_to_delete = Some(idx);
                            }
                            if secondary_glass_button(ui, Some(VectorIcon::Refresh), "Пинг")
                                .clicked()
                            {
                                let ip = s.ip.clone();
                                if let Ok(res) = ping_server(&ip, 3000) {
                                    state.server_pings.insert(ip, res);
                                }
                            }
                        });
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
                    ui.set_width(320.0);
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

        ui.horizontal(|ui| {
            ui.label(
                RichText::new(format!("Скриншоты: {}", screenshots.len()))
                    .strong()
                    .size(14.0),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if secondary_glass_button(ui, Some(VectorIcon::Folder), "Открыть screenshots/")
                    .clicked()
                {
                    let scr_dir = inst_dir.join("screenshots");
                    let _ = std::fs::create_dir_all(&scr_dir);
                    let _ = std::process::Command::new("xdg-open").arg(scr_dir).spawn();
                }
            });
        });

        ui.add_space(6.0);

        if screenshots.is_empty() {
            ui.add_space(20.0);
            ui.label(RichText::new("Скриншотов пока нет. Нажмите F2 в игре!").color(TEXT_MUTED));
        } else {
            ScrollArea::vertical()
                .id_salt("screenshots_scroll")
                .show(ui, |ui| {
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

                                ui.vertical(|ui| {
                                    ui.label(
                                        RichText::new(&s.file_name).color(TEXT_PRIMARY).strong(),
                                    );
                                    let mib = (s.size_bytes as f64) / (1024.0 * 1024.0);
                                    ui.label(
                                        RichText::new(format!(
                                            "{mib:.2} МБ • {}",
                                            s.created_at.format("%Y-%m-%d %H:%M")
                                        ))
                                        .color(TEXT_MUTED)
                                        .small(),
                                    );
                                });

                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        if secondary_glass_button(ui, Some(VectorIcon::Trash), "")
                                            .clicked()
                                        {
                                            let _ = std::fs::remove_file(&s.path);
                                        }
                                        if secondary_glass_button(
                                            ui,
                                            Some(VectorIcon::Eye),
                                            "Открыть",
                                        )
                                        .clicked()
                                        {
                                            let _ = std::process::Command::new("xdg-open")
                                                .arg(&s.path)
                                                .spawn();
                                        }
                                    },
                                );
                            });
                        });
                        ui.add_space(4.0);
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

            ui.horizontal(|ui| {
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
                        ui.set_width(380.0);
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

            ui.horizontal(|ui| {
                if primary_emerald_button(ui, Some(VectorIcon::Check), "Сохранить параметры")
                    .clicked()
                {
                    should_save = true;
                }

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if secondary_glass_button(ui, Some(VectorIcon::Trash), "Удалить инстанс")
                        .clicked()
                    {
                        should_delete = true;
                    }
                });
            });

            if should_save {
                let _ = state.instance_mgr.save_instance(inst);
                state.status_message = Some(("Параметры инстанса сохранены".to_string(), false));
            }

            if should_delete {
                let inst_dir = state.paths.instance_dir(inst_id.as_str());
                let _ = std::fs::remove_dir_all(inst_dir);
                let id_to_remove = inst_id.clone();
                state.instances.retain(|i| i.id != id_to_remove);
                state.selected_instance_id = state.instances.first().map(|i| i.id.clone());
                state.status_message = Some(("Инстанс успешно удален".to_string(), false));
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
        assert!(view.show_releases);
        assert!(!view.show_snapshots);
        assert!(!view.show_betas);
        assert!(!view.show_alphas);
        assert!(view.version_search.is_empty());
    }
}
