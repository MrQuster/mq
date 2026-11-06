#![deny(clippy::await_holding_lock)]
//! Instances screen: instance grid, creation dialog, instance details, mods table,
//! live game console with log filtering, and JVM settings. Zero emojis!

use egui::{Color32, RichText, ScrollArea, Stroke, Ui, Vec2};
use mq_core::{GameLogLevel, ModLoaderKind};

use crate::state::{UiState, UiWorkerCommand};
use crate::theme::{
    ACCENT_COBALT, ACCENT_EMERALD, BORDER_SUBTLE, CARD_SURFACE, CARD_SURFACE_HOVER, RADIUS_CARD,
    STATUS_DANGER, STATUS_WARN, TEXT_MUTED, TEXT_PRIMARY,
};
use crate::widgets::{
    hero_play_button, loader_badge, paint_vector_icon, primary_emerald_button,
    secondary_glass_button, status_pill, version_badge, VectorIcon,
};

/// All official release versions of Minecraft Java Edition in reverse chronological order.
pub const ALL_MC_VERSIONS: &[&str] = &[
    "1.21.4", "1.21.3", "1.21.2", "1.21.1", "1.21",
    "1.20.6", "1.20.5", "1.20.4", "1.20.3", "1.20.2", "1.20.1", "1.20",
    "1.19.4", "1.19.3", "1.19.2", "1.19.1", "1.19",
    "1.18.2", "1.18.1", "1.18",
    "1.17.1", "1.17",
    "1.16.5", "1.16.4", "1.16.3", "1.16.2", "1.16.1",
    "1.15.2", "1.14.4", "1.13.2",
    "1.12.2", "1.11.2", "1.10.2", "1.9.4", "1.8.9", "1.7.10",
];

/// Highlighted popular versions for quick 1-click selection chips.
pub const POPULAR_CHIPS: &[&str] = &[
    "1.21.1", "1.20.1", "1.16.5", "1.12.2", "1.8.9"
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InstanceDetailTab {
    Mods,
    Console,
    Settings,
}

pub struct InstancesView {
    pub current_tab: InstanceDetailTab,
    pub console_autoscroll: bool,
}

impl Default for InstancesView {
    fn default() -> Self {
        Self {
            current_tab: InstanceDetailTab::Mods,
            console_autoscroll: true,
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
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if primary_emerald_button(ui, Some(VectorIcon::Plus), "Создать инстанс").clicked()
                {
                    state.show_create_instance_dialog = true;
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
            egui::Window::new("Создание нового инстанса Minecraft")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .show(ui.ctx(), |ui| {
                    ui.set_width(420.0);

                    // 1. Minecraft Version Selection
                    ui.label(RichText::new("Версия Minecraft:").color(TEXT_PRIMARY).strong());
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
                                state.create_inst_name = format!("{} {}", v, state.create_inst_loader.as_str());
                            }
                        }
                    });

                    ui.add_space(4.0);

                    // Full version dropdown selector
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("Все релизы:").color(TEXT_MUTED).small());
                        egui::ComboBox::from_id_salt("all_mc_versions_dropdown")
                            .width(280.0)
                            .selected_text(
                                RichText::new(&state.create_inst_mc_ver)
                                    .color(ACCENT_COBALT)
                                    .strong(),
                            )
                            .show_ui(ui, |ui| {
                                for &v in ALL_MC_VERSIONS {
                                    if ui
                                        .selectable_label(state.create_inst_mc_ver == v, v)
                                        .clicked()
                                    {
                                        state.create_inst_mc_ver = v.to_string();
                                        state.create_inst_name =
                                            format!("{} {}", v, state.create_inst_loader.as_str());
                                    }
                                }
                            });
                    });

                    ui.add_space(10.0);

                    // 2. Mod Loader Selection
                    ui.label(RichText::new("Загрузчик модов:").color(TEXT_PRIMARY).strong());
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
                    ui.label(RichText::new("Название инстанса:").color(TEXT_PRIMARY).strong());
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

                            let frame = egui::Frame::new()
                                .fill(if is_selected {
                                    CARD_SURFACE_HOVER
                                } else {
                                    CARD_SURFACE
                                })
                                .stroke(Stroke::new(
                                    1.0_f32,
                                    if is_selected {
                                        ACCENT_EMERALD
                                    } else {
                                        BORDER_SUBTLE
                                    },
                                ))
                                .corner_radius(RADIUS_CARD)
                                .inner_margin(egui::Margin::same(8));

                            let resp = frame.show(ui, |ui| {
                                ui.set_width(ui.available_width());
                                ui.horizontal(|ui| {
                                    let (rect, _) = ui.allocate_exact_size(
                                        Vec2::splat(16.0),
                                        egui::Sense::hover(),
                                    );
                                    paint_vector_icon(
                                        ui.painter(),
                                        rect,
                                        VectorIcon::Layers,
                                        if is_selected {
                                            ACCENT_EMERALD
                                        } else {
                                            ACCENT_COBALT
                                        },
                                    );
                                    ui.vertical(|ui| {
                                        ui.label(
                                            RichText::new(&inst.name).color(TEXT_PRIMARY).strong(),
                                        );
                                        ui.horizontal(|ui| {
                                            version_badge(ui, &inst.mc_version, ACCENT_COBALT);
                                            loader_badge(ui, inst.loader);
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
                if let Some(inst) = state.selected_instance() {
                    let inst_id = inst.id.clone();
                    let inst_name = inst.name.clone();
                    let mc_ver = inst.mc_version.clone();
                    let loader = inst.loader;

                    // Detail hero header card
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
                                    version_badge(ui, &format!("{} МБ RAM", inst.memory_max_mb), TEXT_MUTED);
                                    status_pill(ui, "Готов к запуску", ACCENT_EMERALD);
                                });
                            });

                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                if hero_play_button(ui, "ИГРАТЬ").clicked() {
                                    let _ = state.cmd_sender.send(UiWorkerCommand::LaunchInstance {
                                        instance_id: inst_id.clone(),
                                        dry_run: false,
                                    });
                                }
                                ui.add_space(10.0);
                                if secondary_glass_button(ui, Some(VectorIcon::Verify), "Проверить").clicked() {
                                    let _ = state.cmd_sender.send(UiWorkerCommand::VerifyInstance {
                                        instance_id: inst_id.clone(),
                                    });
                                }
                                if secondary_glass_button(ui, Some(VectorIcon::Folder), "Папка игры").clicked() {
                                    let inst_dir = state.paths.instance_dir(inst_id.as_str());
                                    let _ = std::process::Command::new("xdg-open").arg(inst_dir).spawn();
                                }
                            });
                        });
                    });

                    ui.add_space(10.0);

                    // Tab selector
                    ui.horizontal(|ui| {
                        if ui
                            .selectable_label(
                                self.current_tab == InstanceDetailTab::Mods,
                                "Моды инстанса",
                            )
                            .clicked()
                        {
                            self.current_tab = InstanceDetailTab::Mods;
                            let _ = state.cmd_sender.send(UiWorkerCommand::ScanInstanceMods {
                                instance_id: inst_id.clone(),
                            });
                        }
                        if ui
                            .selectable_label(
                                self.current_tab == InstanceDetailTab::Console,
                                "Консоль игры",
                            )
                            .clicked()
                        {
                            self.current_tab = InstanceDetailTab::Console;
                        }
                        if ui
                            .selectable_label(
                                self.current_tab == InstanceDetailTab::Settings,
                                "Параметры инстанса",
                            )
                            .clicked()
                        {
                            self.current_tab = InstanceDetailTab::Settings;
                        }
                    });

                    ui.separator();

                    // Tab contents
                    match self.current_tab {
                        InstanceDetailTab::Mods => {
                            self.render_mods_tab(ui, state, &inst_id);
                        }
                        InstanceDetailTab::Console => {
                            self.render_console_tab(ui, state);
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
            painter.circle_stroke(rect.center(), 32.0, Stroke::new(1.5_f32, ACCENT_EMERALD.gamma_multiply(0.5)));
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
                RichText::new("Сверхбыстрый модульный лаунчер Minecraft на чистом Rust для Arch Linux")
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
                        let (r, _) = ui.allocate_exact_size(Vec2::splat(20.0), egui::Sense::hover());
                        paint_vector_icon(ui.painter(), r, VectorIcon::Check, ACCENT_COBALT);
                        ui.label(RichText::new("Vanilla 1.21.1").color(TEXT_PRIMARY).strong());
                    });
                    ui.add_space(6.0);
                    ui.label(RichText::new("Чистая официальная версия игры без сторонних модов.").color(TEXT_MUTED).small());
                    ui.add_space(14.0);
                    if primary_emerald_button(ui, Some(VectorIcon::Plus), "Создать Vanilla").clicked() {
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
                        let (r, _) = ui.allocate_exact_size(Vec2::splat(20.0), egui::Sense::hover());
                        paint_vector_icon(ui.painter(), r, VectorIcon::Layers, Color32::from_rgb(0xE2, 0xB3, 0x7E));
                        ui.label(RichText::new("Fabric 1.21.1").color(TEXT_PRIMARY).strong());
                    });
                    ui.add_space(6.0);
                    ui.label(RichText::new("Оптимизированный легковесный загрузчик для Sodium и Iris.").color(TEXT_MUTED).small());
                    ui.add_space(14.0);
                    if primary_emerald_button(ui, Some(VectorIcon::Plus), "Создать Fabric").clicked() {
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
                        let (r, _) = ui.allocate_exact_size(Vec2::splat(20.0), egui::Sense::hover());
                        paint_vector_icon(ui.painter(), r, VectorIcon::Settings, ACCENT_EMERALD);
                        ui.label(RichText::new("Свой инстанс").color(TEXT_PRIMARY).strong());
                    });
                    ui.add_space(6.0);
                    ui.label(RichText::new("Выбор любой версии (1.7 - 1.21), Quilt, Forge или NeoForge.").color(TEXT_MUTED).small());
                    ui.add_space(14.0);
                    if secondary_glass_button(ui, Some(VectorIcon::Plus), "Настроить...").clicked() {
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
                status_pill(ui, "Zero-Lag UI Watchdog", Color32::from_rgb(0xC0, 0x84, 0xFC));
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

    fn render_console_tab(&mut self, ui: &mut Ui, state: &mut UiState) {
        ui.horizontal(|ui| {
            ui.label(
                RichText::new(format!("Строк в логе: {}", state.game_logs.len())).color(TEXT_MUTED),
            );
            ui.checkbox(&mut self.console_autoscroll, "Автоскролл");

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
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
                        for entry in &state.game_logs {
                            if let Some(flt) = state.log_filter {
                                if entry.level != flt {
                                    continue;
                                }
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

            ui.add_space(14.0);
            if primary_emerald_button(ui, Some(VectorIcon::Check), "Сохранить параметры").clicked()
            {
                let _ = state.instance_mgr.save_instance(inst);
                state.status_message = Some(("Параметры инстанса сохранены".to_string(), false));
            }
        }
    }
}
