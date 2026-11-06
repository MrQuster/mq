#![deny(clippy::await_holding_lock)]
//! Catalog screen: browse and install mods, modpacks, shaders, and resourcepacks
//! from Modrinth and CurseForge. Zero emojis!

use egui::{RichText, ScrollArea, Stroke, Ui, Vec2};
use mq_core::ModLoaderKind;
use mq_mods::{CatalogHit, CatalogSource, ProjectKind, SearchQuery};

use crate::state::{UiState, UiWorkerCommand};
use crate::theme::{
    ACCENT_COBALT, ACCENT_EMERALD, BORDER_SUBTLE, CARD_SURFACE, RADIUS_CARD, STATUS_WARN,
    TEXT_MUTED, TEXT_PRIMARY,
};
use crate::widgets::{paint_vector_icon, primary_emerald_button, version_badge, VectorIcon};

#[derive(Default)]
pub struct CatalogView {
    pub curseforge_api_key_input: String,
    pub selected_loader: Option<ModLoaderKind>,
}

impl CatalogView {
    pub fn ui(&mut self, ui: &mut Ui, state: &mut UiState) {
        ui.heading(
            RichText::new("Каталог модов и сборок")
                .color(TEXT_PRIMARY)
                .strong(),
        );
        ui.add_space(8.0);

        // Source and Content type selector
        ui.horizontal(|ui| {
            ui.label(RichText::new("Источник:").strong());
            if ui
                .selectable_label(state.catalog_source == CatalogSource::Modrinth, "Modrinth")
                .clicked()
            {
                state.catalog_source = CatalogSource::Modrinth;
            }
            if ui
                .selectable_label(
                    state.catalog_source == CatalogSource::CurseForge,
                    "CurseForge",
                )
                .clicked()
            {
                state.catalog_source = CatalogSource::CurseForge;
            }

            ui.separator();

            ui.label(RichText::new("Тип:").strong());
            if ui
                .selectable_label(state.catalog_kind == ProjectKind::Mod, "Моды")
                .clicked()
            {
                state.catalog_kind = ProjectKind::Mod;
            }
            if ui
                .selectable_label(
                    state.catalog_kind == ProjectKind::Modpack,
                    "Сборки (Modpacks)",
                )
                .clicked()
            {
                state.catalog_kind = ProjectKind::Modpack;
            }
            if ui
                .selectable_label(state.catalog_kind == ProjectKind::Shader, "Шейдеры")
                .clicked()
            {
                state.catalog_kind = ProjectKind::Shader;
            }
            if ui
                .selectable_label(
                    state.catalog_kind == ProjectKind::ResourcePack,
                    "Ресурспаки",
                )
                .clicked()
            {
                state.catalog_kind = ProjectKind::ResourcePack;
            }
        });

        // CurseForge API key prompt if needed
        if state.catalog_source == CatalogSource::CurseForge
            && state.config.effective_cf_api_key().is_none()
        {
            ui.add_space(6.0);
            let warn_frame = egui::Frame::new()
                .fill(CARD_SURFACE)
                .stroke(Stroke::new(1.0_f32, STATUS_WARN))
                .corner_radius(RADIUS_CARD)
                .inner_margin(egui::Margin::same(8));

            warn_frame.show(ui, |ui| {
                ui.label(
                    RichText::new(
                        "Для поиска в CurseForge укажите API-ключ (или переменную MQ_CF_API_KEY):",
                    )
                    .color(STATUS_WARN),
                );
                ui.horizontal(|ui| {
                    ui.text_edit_singleline(&mut self.curseforge_api_key_input);
                    if primary_emerald_button(ui, None, "Сохранить ключ").clicked()
                        && !self.curseforge_api_key_input.trim().is_empty()
                    {
                        state.config.curseforge_api_key = Some(mq_core::SecretString::new(
                            self.curseforge_api_key_input.trim(),
                        ));
                        let _ = state.config.save(&state.paths);
                    }
                });
            });
        }

        ui.add_space(8.0);

        // Search bar + version/loader filters
        ui.horizontal(|ui| {
            let (rect, _) = ui.allocate_exact_size(Vec2::splat(18.0), egui::Sense::hover());
            paint_vector_icon(ui.painter(), rect, VectorIcon::Search, ACCENT_COBALT);

            let search_resp = ui.add(
                egui::TextEdit::singleline(&mut state.catalog_query_text)
                    .hint_text("Поиск модов (например, Sodium, Lithium, Fabric API)...")
                    .desired_width(320.0),
            );

            ui.label("Версия MC:");
            ui.text_edit_singleline(&mut state.catalog_filter_version);

            ui.label("Лоадер:");
            egui::ComboBox::from_id_salt("cat_loader_combo")
                .selected_text(self.selected_loader.map(|l| l.as_str()).unwrap_or("Все"))
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut self.selected_loader, None, "Все");
                    ui.selectable_value(
                        &mut self.selected_loader,
                        Some(ModLoaderKind::Fabric),
                        "Fabric",
                    );
                    ui.selectable_value(
                        &mut self.selected_loader,
                        Some(ModLoaderKind::Quilt),
                        "Quilt",
                    );
                    ui.selectable_value(
                        &mut self.selected_loader,
                        Some(ModLoaderKind::Forge),
                        "Forge",
                    );
                    ui.selectable_value(
                        &mut self.selected_loader,
                        Some(ModLoaderKind::NeoForge),
                        "NeoForge",
                    );
                });

            if primary_emerald_button(ui, Some(VectorIcon::Search), "Искать").clicked()
                || (search_resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)))
            {
                state.catalog_searching = true;
                let q = SearchQuery {
                    query: state.catalog_query_text.clone(),
                    project_kind: state.catalog_kind,
                    mc_version: if state.catalog_filter_version.trim().is_empty() {
                        None
                    } else {
                        Some(state.catalog_filter_version.trim().to_string())
                    },
                    loader: self.selected_loader,
                    limit: 30,
                    offset: 0,
                };
                let _ = state.cmd_sender.send(UiWorkerCommand::SearchCatalog {
                    source: state.catalog_source,
                    query: q,
                });
            }
        });

        ui.separator();

        // Results list
        ScrollArea::vertical()
            .id_salt("catalog_results_scroll")
            .show(ui, |ui| {
                if state.catalog_searching {
                    ui.add_space(20.0);
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label(RichText::new("Поиск в каталоге...").color(TEXT_MUTED));
                    });
                } else if state.catalog_hits.is_empty() {
                    ui.add_space(20.0);
                    ui.label(RichText::new("Введите запрос и нажмите 'Искать'").color(TEXT_MUTED));
                } else {
                    let hits = state.catalog_hits.clone();
                    for hit in &hits {
                        self.render_hit_card(ui, state, hit);
                        ui.add_space(6.0);
                    }
                }
            });
    }

    fn render_hit_card(&self, ui: &mut Ui, state: &mut UiState, hit: &CatalogHit) {
        let card = egui::Frame::new()
            .fill(CARD_SURFACE)
            .stroke(Stroke::new(1.0_f32, BORDER_SUBTLE))
            .corner_radius(RADIUS_CARD)
            .inner_margin(egui::Margin::same(10));

        card.show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                // Vector icon for project type
                let (rect, _) = ui.allocate_exact_size(Vec2::splat(24.0), egui::Sense::hover());
                let icon = match hit.project_kind {
                    ProjectKind::Mod => VectorIcon::Layers,
                    ProjectKind::Modpack => VectorIcon::Catalog,
                    ProjectKind::Shader => VectorIcon::Shield,
                    ProjectKind::ResourcePack => VectorIcon::Folder,
                };
                paint_vector_icon(ui.painter(), rect, icon, ACCENT_EMERALD);

                ui.vertical(|ui| {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(&hit.title).color(TEXT_PRIMARY).strong());
                        ui.label(
                            RichText::new(format!("от {}", hit.author))
                                .color(TEXT_MUTED)
                                .small(),
                        );
                        version_badge(ui, &format!("{} скачиваний", hit.downloads), ACCENT_COBALT);
                    });

                    ui.label(RichText::new(&hit.description).color(TEXT_MUTED).small());

                    ui.horizontal(|ui| {
                        for l in &hit.loaders {
                            version_badge(ui, l, ACCENT_EMERALD);
                        }
                    });
                });

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if hit.project_kind == ProjectKind::Modpack {
                        if primary_emerald_button(ui, Some(VectorIcon::Plus), "Создать сборку")
                            .clicked()
                        {
                            let _ = state.cmd_sender.send(UiWorkerCommand::InstallModpack {
                                path_or_slug: hit.slug.clone(),
                                instance_name: Some(hit.title.clone()),
                            });
                        }
                    } else if let Some(inst) = state.selected_instance() {
                        let btn_label = format!("В '{}'", inst.name);
                        if primary_emerald_button(ui, Some(VectorIcon::Plus), &btn_label).clicked()
                        {
                            let _ = state.cmd_sender.send(UiWorkerCommand::InstallMod {
                                instance_id: inst.id.clone(),
                                project_id_or_slug: hit.slug.clone(),
                                mc_version: inst.mc_version.clone(),
                                loader: inst.loader,
                            });
                        }
                    } else {
                        ui.label(
                            RichText::new("Выберите инстанс для установки")
                                .color(TEXT_MUTED)
                                .small(),
                        );
                    }
                });
            });
        });
    }
}
