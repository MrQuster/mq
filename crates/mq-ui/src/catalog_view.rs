#![deny(clippy::await_holding_lock)]
//! Catalog screen: browse mods, resource packs, shaders and modpacks from Modrinth and
//! CurseForge. Opaque matte style, theme tokens only, 4/8 spacing grid, no emoji.

use std::time::Instant;

use egui::{Color32, RichText, ScrollArea, Sense, Stroke, TextureOptions, Ui, Vec2};
use mq_core::ModLoaderKind;
use mq_mods::{CatalogHit, CatalogSource, CatalogSourceFilter, ProjectKind, SearchSort};

use crate::catalog_state::{
    CatalogErrorKind, CatalogState, CatalogTab, IconSlot, CATALOG_PAGE_SIZE,
};
use crate::state::{UiState, UiWorkerCommand};
use crate::theme::{
    ACCENT_COBALT, ACCENT_EMERALD, BG_RECESSED, BORDER_STRONG, BORDER_SUBTLE, CARD_SURFACE,
    CARD_SURFACE_HOVER, RADIUS_BUTTON, RADIUS_CARD, STATUS_WARN, TEXT_MUTED, TEXT_PRIMARY,
};
use crate::widgets::{
    paint_vector_icon, primary_emerald_button, secondary_glass_button, version_badge, VectorIcon,
};

/// Page where CurseForge API keys are issued.
pub const CURSEFORGE_KEYS_URL: &str = "https://console.curseforge.com/";
const ICON_SIZE: f32 = 48.0;
const SKELETON_ROWS: usize = 6;

const LOADERS: [ModLoaderKind; 4] = [
    ModLoaderKind::Fabric,
    ModLoaderKind::Quilt,
    ModLoaderKind::Forge,
    ModLoaderKind::NeoForge,
];

const SORTS: [(SearchSort, &str); 5] = [
    (SearchSort::Downloads, "По загрузкам"),
    (SearchSort::Relevance, "По релевантности"),
    (SearchSort::Follows, "По популярности"),
    (SearchSort::Newest, "Новые"),
    (SearchSort::Updated, "Обновлённые"),
];

/// Category slugs offered as filter chips per tab.
pub const fn categories_for(tab: CatalogTab) -> &'static [(&'static str, &'static str)] {
    match tab {
        CatalogTab::Mods => &[
            ("optimization", "Оптимизация"),
            ("technology", "Технологии"),
            ("magic", "Магия"),
            ("adventure", "Приключения"),
            ("utility", "Утилиты"),
            ("library", "Библиотеки"),
        ],
        CatalogTab::ResourcePacks => &[
            ("realistic", "Реализм"),
            ("simplistic", "Минимализм"),
            ("vanilla-like", "Ванильные"),
        ],
        CatalogTab::Shaders => &[
            ("realistic", "Реализм"),
            ("fantasy", "Фэнтези"),
            ("vanilla-like", "Ванильные"),
        ],
        CatalogTab::Modpacks => &[
            ("optimization", "Оптимизация"),
            ("technology", "Технологии"),
            ("magic", "Магия"),
            ("adventure", "Приключения"),
        ],
    }
}

/// Human-readable text for a source error.
pub fn source_error_text(source: CatalogSource, kind: CatalogErrorKind) -> String {
    let name = source_name(source);
    match kind {
        CatalogErrorKind::Network => {
            format!("{name}: нет связи с сервером. Проверьте подключение к сети и повторите.")
        }
        CatalogErrorKind::RateLimited => {
            format!("{name}: слишком много запросов (429). Подождите немного и повторите.")
        }
        CatalogErrorKind::MissingKey => {
            format!("{name}: нужен действующий API-ключ.")
        }
        CatalogErrorKind::Other => format!("{name}: не удалось получить результаты."),
    }
}

pub const fn source_name(source: CatalogSource) -> &'static str {
    match source {
        CatalogSource::Modrinth => "Modrinth",
        CatalogSource::CurseForge => "CurseForge",
    }
}

pub const fn source_color(source: CatalogSource) -> Color32 {
    match source {
        CatalogSource::Modrinth => ACCENT_EMERALD,
        CatalogSource::CurseForge => Color32::from_rgb(0xF1, 0x64, 0x36),
    }
}

/// Formats a download counter compactly (`1.2 млн`, `34 тыс.`).
pub fn format_downloads(n: u64) -> String {
    if n >= 1_000_000 {
        format!("{:.1} млн", n as f64 / 1_000_000.0)
    } else if n >= 1_000 {
        format!("{} тыс.", n / 1_000)
    } else {
        n.to_string()
    }
}

#[derive(Default)]
pub struct CatalogView {
    pub curseforge_api_key_input: String,
    import_path: String,
    textures: std::collections::HashMap<String, egui::TextureHandle>,
}

impl CatalogView {
    pub fn ui(&mut self, ui: &mut Ui, state: &mut UiState) {
        let now = Instant::now();
        let mut changed = false;

        ui.heading(RichText::new("Каталог").color(TEXT_PRIMARY).strong());
        ui.add_space(8.0);

        changed |= self.tabs_row(ui, &mut state.catalog);
        ui.add_space(8.0);
        changed |= self.source_row(ui, &mut state.catalog);
        ui.add_space(8.0);
        self.import_row(ui, state);

        let has_key = state.config.effective_cf_api_key().is_some();
        let cf_selected = state.catalog.source == CatalogSourceFilter::CurseForge;
        if !has_key && cf_selected {
            ui.add_space(8.0);
            self.key_panel(ui, state);
        }

        ui.add_space(8.0);
        changed |= self.search_row(ui, &mut state.catalog);
        ui.add_space(8.0);
        changed |= self.filters_row(ui, &mut state.catalog);
        ui.add_space(8.0);

        if changed {
            state.catalog.mark_dirty(now);
        }
        self.drive_search(ui, state, now);

        if state.catalog.source == CatalogSourceFilter::All && !has_key {
            self.inline_notice(
                ui,
                "CurseForge отключён: не задан API-ключ. Показаны результаты Modrinth.",
            );
        }
        for err in state.catalog.errors.clone() {
            if err.kind == CatalogErrorKind::MissingKey
                && err.source == CatalogSource::CurseForge
                && !has_key
            {
                continue;
            }
            let mut text = source_error_text(err.source, err.kind);
            if err.kind == CatalogErrorKind::Other && !err.message.is_empty() {
                text.push(' ');
                text.push_str(&err.message);
            }
            self.inline_notice(ui, &text);
        }

        ui.separator();
        self.results(ui, state);
        crate::catalog_dialogs::jobs_panel(ui, state);
        crate::catalog_dialogs::show_overlays(ui.ctx(), state);
    }

    fn tabs_row(&self, ui: &mut Ui, cat: &mut CatalogState) -> bool {
        let mut changed = false;
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing = Vec2::new(8.0, 8.0);
            for tab in CatalogTab::ALL {
                let selected = cat.tab == tab;
                let text = RichText::new(tab.label()).color(if selected {
                    TEXT_PRIMARY
                } else {
                    TEXT_MUTED
                });
                let btn = egui::Button::new(if selected { text.strong() } else { text })
                    .fill(if selected {
                        CARD_SURFACE_HOVER
                    } else {
                        CARD_SURFACE
                    })
                    .stroke(Stroke::new(
                        1.0_f32,
                        if selected {
                            ACCENT_EMERALD
                        } else {
                            BORDER_SUBTLE
                        },
                    ))
                    .corner_radius(RADIUS_BUTTON)
                    .min_size(Vec2::new(96.0, 32.0));
                if ui.add(btn).clicked() && !selected {
                    cat.tab = tab;
                    cat.filters.categories.clear();
                    changed = true;
                }
            }
        });
        changed
    }

    /// Local import: path field plus drag-and-drop of `.mrpack` / `.zip` onto the window.
    fn import_row(&mut self, ui: &mut Ui, state: &mut UiState) {
        let dropped: Vec<std::path::PathBuf> = ui.ctx().input(|i| {
            i.raw
                .dropped_files
                .iter()
                .filter_map(|f| f.path.clone())
                .collect()
        });
        if let Some(p) = dropped.into_iter().next() {
            state.import_local_pack(p);
        }
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing = Vec2::new(8.0, 8.0);
            ui.label(RichText::new("Импорт").color(TEXT_MUTED));
            ui.add(
                egui::TextEdit::singleline(&mut self.import_path)
                    .hint_text("Путь к .mrpack / .zip или перетащите файл в окно")
                    .desired_width(360.0),
            );
            let can = !self.import_path.trim().is_empty();
            if secondary_glass_button(ui, None, "Импортировать").clicked() && can {
                let p = std::path::PathBuf::from(self.import_path.trim());
                self.import_path.clear();
                state.import_local_pack(p);
            }
        });
    }

    fn source_row(&self, ui: &mut Ui, cat: &mut CatalogState) -> bool {
        let mut changed = false;
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing = Vec2::new(8.0, 8.0);
            ui.label(RichText::new("Источник").color(TEXT_MUTED));
            for (filter, label) in [
                (CatalogSourceFilter::All, "Все"),
                (CatalogSourceFilter::Modrinth, "Modrinth"),
                (CatalogSourceFilter::CurseForge, "CurseForge"),
            ] {
                if ui.selectable_label(cat.source == filter, label).clicked()
                    && cat.source != filter
                {
                    cat.source = filter;
                    changed = true;
                }
            }
        });
        changed
    }

    fn search_row(&self, ui: &mut Ui, cat: &mut CatalogState) -> bool {
        let mut changed = false;
        ui.horizontal(|ui| {
            let (rect, _) = ui.allocate_exact_size(Vec2::splat(16.0), Sense::hover());
            paint_vector_icon(ui.painter(), rect, VectorIcon::Search, ACCENT_COBALT);
            let resp = ui.add(
                egui::TextEdit::singleline(&mut cat.query)
                    .hint_text("Поиск по названию")
                    .desired_width((ui.available_width() - 8.0).max(160.0)),
            );
            changed |= resp.changed();
        });
        changed
    }

    fn filters_row(&self, ui: &mut Ui, cat: &mut CatalogState) -> bool {
        let mut changed = false;
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = Vec2::new(8.0, 8.0);
            ui.label(RichText::new("Версия MC").color(TEXT_MUTED));
            changed |= ui
                .add(
                    egui::TextEdit::singleline(&mut cat.filters.mc_version)
                        .hint_text("любая")
                        .desired_width(72.0),
                )
                .changed();

            if matches!(cat.tab, CatalogTab::Mods | CatalogTab::Modpacks) {
                ui.label(RichText::new("Загрузчик").color(TEXT_MUTED));
                egui::ComboBox::from_id_salt("cat_loader_combo")
                    .selected_text(cat.filters.loader.map_or("Любой", |l| l.as_str()))
                    .show_ui(ui, |ui| {
                        changed |= ui
                            .selectable_value(&mut cat.filters.loader, None, "Любой")
                            .changed();
                        for l in LOADERS {
                            changed |= ui
                                .selectable_value(&mut cat.filters.loader, Some(l), l.as_str())
                                .changed();
                        }
                    });
            }

            ui.label(RichText::new("Сортировка").color(TEXT_MUTED));
            let current = SORTS
                .iter()
                .find(|(s, _)| *s == cat.filters.sort)
                .map_or("По загрузкам", |(_, l)| *l);
            egui::ComboBox::from_id_salt("cat_sort_combo")
                .selected_text(current)
                .show_ui(ui, |ui| {
                    for (sort, label) in SORTS {
                        changed |= ui
                            .selectable_value(&mut cat.filters.sort, sort, label)
                            .changed();
                    }
                });
        });

        ui.add_space(4.0);
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = Vec2::new(8.0, 8.0);
            ui.label(RichText::new("Категории").color(TEXT_MUTED));
            for (slug, label) in categories_for(cat.tab) {
                let on = cat.filters.categories.iter().any(|c| c == slug);
                if ui.selectable_label(on, *label).clicked() {
                    if on {
                        cat.filters.categories.retain(|c| c != slug);
                    } else {
                        cat.filters.categories.push((*slug).to_string());
                    }
                    changed = true;
                }
            }
        });
        changed
    }

    /// Fires the debounced search and schedules the repaint for the remaining wait.
    fn drive_search(&self, ui: &Ui, state: &mut UiState, now: Instant) {
        if state.catalog.is_due(now) {
            let (request_id, query) = state.catalog.begin_request();
            let _ = state.cmd_sender.send(UiWorkerCommand::SearchCatalog {
                request_id,
                latest: state.catalog.latest_request.clone(),
                source: state.catalog.source,
                query,
            });
        } else if let Some(left) = state.catalog.debounce_remaining(now) {
            ui.ctx().request_repaint_after(left);
        } else if !state.catalog.searched && !state.catalog.loading {
            // First open of the screen: show popular projects without waiting for typing.
            state.catalog.mark_page_changed(now, false);
            ui.ctx().request_repaint();
        }
    }

    fn inline_notice(&self, ui: &mut Ui, text: &str) {
        egui::Frame::new()
            .fill(CARD_SURFACE)
            .stroke(Stroke::new(1.0_f32, STATUS_WARN))
            .corner_radius(RADIUS_CARD)
            .inner_margin(egui::Margin::same(8))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.label(RichText::new(text).color(STATUS_WARN));
            });
        ui.add_space(4.0);
    }

    /// Panel shown when CurseForge is selected without a key: instruction, link and input.
    fn key_panel(&mut self, ui: &mut Ui, state: &mut UiState) {
        egui::Frame::new()
            .fill(CARD_SURFACE)
            .stroke(Stroke::new(1.0_f32, STATUS_WARN))
            .corner_radius(RADIUS_CARD)
            .inner_margin(egui::Margin::same(12))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.label(
                    RichText::new("Для CurseForge нужен API-ключ")
                        .color(TEXT_PRIMARY)
                        .strong(),
                );
                ui.add_space(4.0);
                ui.label(
                    RichText::new(
                        "1. Откройте страницу ключей и войдите в аккаунт.\n\
                         2. Создайте ключ и вставьте его ниже.\n\
                         Ключ можно также задать переменной окружения MQ_CURSEFORGE_API_KEY.\n\
                         Modrinth работает без ключа.",
                    )
                    .color(TEXT_MUTED),
                );
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if secondary_glass_button(ui, None, "Открыть страницу ключей").clicked()
                    {
                        let _ = crate::opener::open_that(CURSEFORGE_KEYS_URL);
                    }
                });
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    ui.add(
                        egui::TextEdit::singleline(&mut self.curseforge_api_key_input)
                            .password(true)
                            .hint_text("API-ключ CurseForge")
                            .desired_width(280.0),
                    );
                    let can_save = !self.curseforge_api_key_input.trim().is_empty();
                    if primary_emerald_button(ui, None, "Сохранить ключ").clicked() && can_save
                    {
                        state.config.curseforge_api_key = Some(mq_core::SecretString::new(
                            self.curseforge_api_key_input.trim(),
                        ));
                        let _ = state.config.save(&state.paths);
                        self.curseforge_api_key_input.clear();
                        state.catalog.mark_page_changed(Instant::now(), false);
                    }
                });
            });
    }

    fn results(&mut self, ui: &mut Ui, state: &mut UiState) {
        let mut go_prev = false;
        let mut go_next = false;

        ScrollArea::vertical()
            .id_salt("catalog_results_scroll")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                let cat = &state.catalog;
                if cat.loading && cat.hits.is_empty() {
                    for _ in 0..SKELETON_ROWS {
                        skeleton_card(ui);
                        ui.add_space(8.0);
                    }
                    return;
                }
                if cat.hits.is_empty() {
                    if cat.searched {
                        if cat.all_sources_failed() {
                            empty_state(
                                ui,
                                "Не удалось загрузить каталог",
                                "Проверьте сообщение выше и повторите поиск.",
                            );
                        } else {
                            empty_state(
                                ui,
                                "Ничего не найдено",
                                "Измените запрос или сбросьте фильтры.",
                            );
                        }
                    }
                    return;
                }

                let hits = cat.hits.clone();
                let urls = state.catalog.icons_to_fetch(hits.iter());
                for url in urls {
                    let _ = state.cmd_sender.send(UiWorkerCommand::FetchIcon {
                        url,
                        edge: crate::catalog_worker::ICON_EDGE,
                    });
                }
                for hit in &hits {
                    self.hit_card(ui, state, hit);
                    ui.add_space(8.0);
                }

                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing = Vec2::new(8.0, 8.0);
                    let page = state.catalog.page;
                    ui.add_enabled_ui(page > 0 && !state.catalog.loading, |ui| {
                        if secondary_glass_button(ui, None, "Назад").clicked() {
                            go_prev = true;
                        }
                    });
                    ui.label(RichText::new(format!("Страница {}", page + 1)).color(TEXT_MUTED));
                    ui.add_enabled_ui(state.catalog.has_next && !state.catalog.loading, |ui| {
                        if secondary_glass_button(ui, None, "Вперёд").clicked() {
                            go_next = true;
                        }
                    });
                    if state.catalog.loading {
                        ui.label(RichText::new("Загрузка").color(TEXT_MUTED));
                    }
                    ui.label(
                        RichText::new(format!("до {CATALOG_PAGE_SIZE} на источник"))
                            .color(TEXT_MUTED)
                            .small(),
                    );
                });
            });

        if go_prev && state.catalog.page > 0 {
            state.catalog.page -= 1;
            state.catalog.mark_page_changed(Instant::now(), false);
        }
        if go_next {
            state.catalog.page += 1;
            state.catalog.mark_page_changed(Instant::now(), false);
        }
    }

    /// Returns the GPU texture for an icon URL, uploading a freshly downloaded image first.
    fn icon_texture(
        &mut self,
        ui: &Ui,
        cat: &mut CatalogState,
        url: &str,
    ) -> Option<egui::TextureHandle> {
        if let Some(IconSlot::Ready(_)) = cat.icons.get(url) {
            if let Some(IconSlot::Ready(img)) =
                cat.icons.insert(url.to_string(), IconSlot::Uploaded)
            {
                let tex = ui.ctx().load_texture(
                    format!("catalog-icon:{url}"),
                    img,
                    TextureOptions::LINEAR,
                );
                self.textures.insert(url.to_string(), tex);
            }
        }
        self.textures.get(url).cloned()
    }

    fn hit_card(&mut self, ui: &mut Ui, state: &mut UiState, hit: &CatalogHit) {
        let installed = state.installed_mods.iter().any(|m| {
            let slug = hit.slug.to_lowercase();
            let title = hit.title.to_lowercase();
            let file = m.file_name.to_lowercase();
            let name = m.name.to_lowercase();
            file.contains(&slug)
                || file.contains(&title)
                || name.contains(&title)
                || name.contains(&slug)
        });
        let texture = hit
            .icon_url
            .as_deref()
            .and_then(|u| self.icon_texture(ui, &mut state.catalog, u));

        egui::Frame::new()
            .fill(CARD_SURFACE)
            .stroke(Stroke::new(1.0_f32, BORDER_SUBTLE))
            .corner_radius(RADIUS_CARD)
            .inner_margin(egui::Margin::same(12))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing = Vec2::new(12.0, 4.0);
                    let (rect, _) = ui.allocate_exact_size(Vec2::splat(ICON_SIZE), Sense::hover());
                    match &texture {
                        Some(tex) => {
                            egui::Image::new(tex)
                                .corner_radius(RADIUS_BUTTON)
                                .fit_to_exact_size(Vec2::splat(ICON_SIZE))
                                .paint_at(ui, rect);
                        }
                        None => {
                            ui.painter().rect_filled(rect, RADIUS_BUTTON, BG_RECESSED);
                            let icon = match hit.project_kind {
                                ProjectKind::Mod => VectorIcon::Layers,
                                ProjectKind::Modpack => VectorIcon::Catalog,
                                ProjectKind::Shader => VectorIcon::Shield,
                                ProjectKind::ResourcePack => VectorIcon::Folder,
                            };
                            paint_vector_icon(ui.painter(), rect.shrink(12.0), icon, TEXT_MUTED);
                        }
                    }

                    let right_w = 148.0;
                    let mid_w = (ui.available_width() - right_w - 12.0).max(120.0);
                    ui.allocate_ui_with_layout(
                        Vec2::new(mid_w, ICON_SIZE),
                        egui::Layout::top_down(egui::Align::Min),
                        |ui| {
                            ui.horizontal(|ui| {
                                ui.spacing_mut().item_spacing = Vec2::new(8.0, 4.0);
                                ui.add(
                                    egui::Label::new(
                                        RichText::new(&hit.title).color(TEXT_PRIMARY).strong(),
                                    )
                                    .truncate(),
                                );
                            });
                            ui.add(
                                egui::Label::new(
                                    RichText::new(format!(
                                        "{}  |  {} загрузок",
                                        hit.author,
                                        format_downloads(hit.downloads)
                                    ))
                                    .color(TEXT_MUTED)
                                    .small(),
                                )
                                .truncate(),
                            );
                            ui.add(
                                egui::Label::new(
                                    RichText::new(&hit.description).color(TEXT_MUTED).small(),
                                )
                                .truncate(),
                            );
                        },
                    );

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::TOP), |ui| {
                        ui.spacing_mut().item_spacing = Vec2::new(8.0, 8.0);
                        self.card_actions(ui, state, hit, installed);
                    });
                });
                ui.add_space(4.0);
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing = Vec2::new(4.0, 4.0);
                    version_badge(ui, source_name(hit.source), source_color(hit.source));
                    for l in hit.loaders.iter().take(4) {
                        version_badge(ui, l, ACCENT_COBALT);
                    }
                    if installed {
                        version_badge(ui, "Установлен", ACCENT_EMERALD);
                    }
                });
            });
    }

    fn card_actions(&self, ui: &mut Ui, state: &mut UiState, hit: &CatalogHit, installed: bool) {
        if secondary_glass_button(ui, None, "Подробнее").clicked() {
            state.open_project(hit.clone());
        }
        if hit.project_kind == ProjectKind::Modpack {
            if primary_emerald_button(ui, Some(VectorIcon::Plus), "Создать сборку").clicked()
            {
                state.open_project(hit.clone());
            }
        } else if installed {
            version_badge(ui, "Уже добавлен", ACCENT_EMERALD);
        } else if state.selected_instance().is_some() {
            if primary_emerald_button(ui, Some(VectorIcon::Plus), "Установить").clicked()
            {
                state.open_install_content(hit.clone());
            }
        } else {
            ui.label(RichText::new("Выберите инстанс").color(TEXT_MUTED).small());
        }
    }
}

/// Placeholder card (matte rectangles, no spinner) shown while the first page loads.
pub fn skeleton_card(ui: &mut Ui) {
    egui::Frame::new()
        .fill(CARD_SURFACE)
        .stroke(Stroke::new(1.0_f32, BORDER_SUBTLE))
        .corner_radius(RADIUS_CARD)
        .inner_margin(egui::Margin::same(12))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            let (rect, _) =
                ui.allocate_exact_size(Vec2::new(ui.available_width(), ICON_SIZE), Sense::hover());
            let p = ui.painter();
            let icon = egui::Rect::from_min_size(rect.min, Vec2::splat(ICON_SIZE));
            p.rect_filled(icon, RADIUS_BUTTON, BG_RECESSED);
            let x = icon.max.x + 12.0;
            let w = (rect.max.x - x).max(40.0);
            for (dy, frac, h) in [(4.0, 0.35, 12.0), (22.0, 0.55, 8.0), (36.0, 0.8, 8.0)] {
                let r = egui::Rect::from_min_size(
                    egui::pos2(x, rect.min.y + dy),
                    Vec2::new(w * frac, h),
                );
                p.rect_filled(r, 4.0, BG_RECESSED);
            }
        });
}

fn empty_state(ui: &mut Ui, title: &str, hint: &str) {
    ui.add_space(32.0);
    ui.vertical_centered(|ui| {
        egui::Frame::new()
            .fill(CARD_SURFACE)
            .stroke(Stroke::new(1.0_f32, BORDER_STRONG))
            .corner_radius(RADIUS_CARD)
            .inner_margin(egui::Margin::same(24))
            .show(ui, |ui| {
                ui.label(RichText::new(title).color(TEXT_PRIMARY).strong());
                ui.add_space(4.0);
                ui.label(RichText::new(hint).color(TEXT_MUTED));
            });
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn downloads_are_formatted_compactly() {
        assert_eq!(format_downloads(999), "999");
        assert_eq!(format_downloads(34_820), "34 тыс.");
        assert_eq!(format_downloads(34_820_000), "34.8 млн");
    }

    #[test]
    fn error_texts_cover_all_kinds_without_emoji() {
        for kind in [
            CatalogErrorKind::Network,
            CatalogErrorKind::RateLimited,
            CatalogErrorKind::MissingKey,
            CatalogErrorKind::Other,
        ] {
            let t = source_error_text(CatalogSource::CurseForge, kind);
            assert!(t.starts_with("CurseForge:"));
            assert!(t.chars().all(|c| (c as u32) < 0x1F000), "no emoji: {t}");
        }
        assert!(
            source_error_text(CatalogSource::Modrinth, CatalogErrorKind::RateLimited)
                .contains("429")
        );
    }

    #[test]
    fn every_tab_has_categories_and_maps_back() {
        for tab in CatalogTab::ALL {
            assert!(!categories_for(tab).is_empty());
            assert_eq!(CatalogTab::from_kind(tab.kind()), tab);
        }
    }

    #[test]
    fn keys_page_is_official_console() {
        assert_eq!(CURSEFORGE_KEYS_URL, "https://console.curseforge.com/");
    }
}
