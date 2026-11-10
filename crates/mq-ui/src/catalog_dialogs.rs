#![deny(clippy::await_holding_lock)]
//! Catalog overlays: project page, install / modpack confirmation dialog, manual download
//! dialog and the "Загрузки" queue. Rendering only: every action goes through `UiState` methods.

use egui::{Context, RichText, ScrollArea, Stroke, Ui, Vec2};
use mq_mods::ProjectKind;

use crate::catalog_flow::{format_size, InstallTarget, JobStatus, MdBlock};
use crate::catalog_view::{format_downloads, source_color, source_name};
use crate::state::{UiState, UiWorkerCommand};
use crate::theme::{
    ACCENT_COBALT, ACCENT_EMERALD, BORDER_SUBTLE, CARD_SURFACE, RADIUS_CARD, STATUS_DANGER,
    STATUS_WARN, TEXT_MUTED, TEXT_PRIMARY,
};
use crate::widgets::{
    danger_button, primary_emerald_button, secondary_glass_button, version_badge, VectorIcon,
};

fn window(title: &str) -> egui::Window<'_> {
    egui::Window::new(title)
        .collapsible(false)
        .resizable(true)
        .fade_in(false)
        .anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO)
        .default_width(560.0)
        .max_width(760.0)
        .frame(
            egui::Frame::window(&egui::Style::default())
                .fill(CARD_SURFACE)
                .stroke(Stroke::new(1.0_f32, BORDER_SUBTLE))
                .corner_radius(RADIUS_CARD)
                .inner_margin(egui::Margin::same(16)),
        )
}

/// Renders all catalog overlays. Call once per frame after the main panel.
pub fn show_overlays(ctx: &Context, state: &mut UiState) {
    project_window(ctx, state);
    install_window(ctx, state);
    manual_window(ctx, state);
    if state.jobs.active_count() > 0 {
        ctx.request_repaint_after(std::time::Duration::from_millis(200));
    }
}

fn project_window(ctx: &Context, state: &mut UiState) {
    let Some(hit) = state.project.open.clone() else {
        return;
    };
    let mut close = false;
    let mut install_hit = false;
    let mut modpack_version: Option<usize> = None;
    let mut open = true;

    window(&hit.title).open(&mut open).show(ctx, |ui| {
        ui.horizontal_wrapped(|ui| {
            version_badge(ui, source_name(hit.source), source_color(hit.source));
            ui.label(
                RichText::new(format!(
                    "{}  |  {} загрузок",
                    hit.author,
                    format_downloads(hit.downloads)
                ))
                .color(TEXT_MUTED)
                .small(),
            );
        });
        ui.add_space(8.0);

        if state.project.loading {
            for _ in 0..3 {
                crate::catalog_view::skeleton_card(ui);
                ui.add_space(4.0);
            }
            return;
        }
        if let Some(err) = &state.project.error {
            ui.label(RichText::new(err).color(STATUS_DANGER));
            return;
        }

        let is_pack = hit.project_kind == ProjectKind::Modpack;
        if !is_pack && primary_emerald_button(ui, Some(VectorIcon::Plus), "Установить").clicked()
        {
            install_hit = true;
        }
        ui.add_space(8.0);

        ScrollArea::vertical().max_height(420.0).show(ui, |ui| {
            for block in &state.project.blocks {
                render_block(ui, block);
            }
            ui.add_space(12.0);
            ui.label(RichText::new("Версии").color(TEXT_PRIMARY).strong());
            ui.horizontal(|ui| {
                ui.label(RichText::new("Версия MC:").color(TEXT_MUTED).small());
                ui.add(
                    egui::TextEdit::singleline(&mut state.project.filter_mc).desired_width(80.0),
                );
            });
            let indices = state.project.filtered_versions();
            if indices.is_empty() {
                ui.label(RichText::new("Нет подходящих версий").color(TEXT_MUTED));
            }
            if let Some(page) = &state.project.page {
                for i in indices.into_iter().take(50) {
                    let v = &page.versions[i];
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(&v.version_number).color(TEXT_PRIMARY));
                        ui.label(
                            RichText::new(v.game_versions.join(", "))
                                .color(TEXT_MUTED)
                                .small(),
                        );
                        if is_pack
                            && secondary_glass_button(ui, Some(VectorIcon::Plus), "Создать сборку")
                                .clicked()
                        {
                            modpack_version = Some(i);
                        }
                    });
                }
            }
        });
    });

    if !open {
        close = true;
    }
    if let Some(i) = modpack_version {
        if let Some(v) = state
            .project
            .page
            .as_ref()
            .and_then(|p| p.versions.get(i).cloned())
        {
            state.open_install_modpack(hit.clone(), v);
        }
    }
    if install_hit {
        state.open_install_content(hit);
    }
    if close {
        state.project.close();
    }
}

fn render_block(ui: &mut Ui, block: &MdBlock) {
    match block {
        MdBlock::Heading { text, .. } => {
            ui.add_space(4.0);
            ui.label(RichText::new(text).color(TEXT_PRIMARY).strong());
        }
        MdBlock::Paragraph { text, links } => {
            ui.label(RichText::new(text).color(TEXT_PRIMARY));
            for (label, url) in links {
                ui.hyperlink_to(label, url);
            }
        }
        MdBlock::ListItem { text } => {
            ui.label(RichText::new(format!("- {text}")).color(TEXT_PRIMARY));
        }
        MdBlock::Quote { text } => {
            ui.label(RichText::new(text).color(TEXT_MUTED).italics());
        }
        MdBlock::Code { text } => {
            ui.label(RichText::new(text).monospace().color(TEXT_MUTED));
        }
        MdBlock::Rule => {
            ui.separator();
        }
        MdBlock::Image { alt, url } => {
            ui.hyperlink_to(
                if alt.is_empty() {
                    "Изображение"
                } else {
                    alt
                },
                url,
            );
        }
    }
}

fn install_window(ctx: &Context, state: &mut UiState) {
    if state.install.is_none() {
        return;
    }
    let mut cancel = false;
    let mut confirm = false;
    let mut new_target = None;
    let mut toggled: Option<String> = None;
    let instances: Vec<_> = state
        .instances
        .iter()
        .map(|i| (i.id.clone(), i.name.clone()))
        .collect();

    let title = state
        .install
        .as_ref()
        .map(|d| {
            if d.is_modpack() {
                "Создать инстанс из сборки"
            } else {
                "Установка"
            }
        })
        .unwrap_or("Установка");

    window(title).show(ctx, |ui| {
        let Some(d) = state.install.as_mut() else {
            return;
        };
        ui.label(RichText::new(&d.hit.title).color(TEXT_PRIMARY).strong());
        ui.add_space(8.0);

        if d.is_modpack() {
            if let InstallTarget::NewInstance { name } = &mut d.target {
                ui.label(RichText::new("Название инстанса").color(TEXT_MUTED).small());
                ui.text_edit_singleline(name);
            }
        } else if let InstallTarget::Existing(current) = d.target.clone() {
            ui.label(RichText::new("Инстанс").color(TEXT_MUTED).small());
            egui::ComboBox::from_id_salt("install_target")
                .selected_text(
                    instances
                        .iter()
                        .find(|(id, _)| *id == current)
                        .map_or_else(|| current.to_string(), |(_, n)| n.clone()),
                )
                .show_ui(ui, |ui| {
                    for (id, name) in &instances {
                        if ui.selectable_label(*id == current, name).clicked() && *id != current {
                            new_target = Some(id.clone());
                        }
                    }
                });
        }
        ui.add_space(8.0);

        if d.loading {
            crate::catalog_view::skeleton_card(ui);
        }
        if let Some(err) = &d.error {
            ui.label(RichText::new(err).color(STATUS_DANGER));
        }

        if let Some(p) = &d.preview {
            ui.label(
                RichText::new(format!(
                    "Minecraft {}  |  {}{}",
                    p.mc_version,
                    p.loader.as_str(),
                    p.loader_version
                        .as_deref()
                        .map(|v| format!(" {v}"))
                        .unwrap_or_default()
                ))
                .color(TEXT_PRIMARY),
            );
            ui.label(
                RichText::new(format!(
                    "Файлов: {}  |  Размер: {}",
                    p.file_count,
                    format_size(p.total_size)
                ))
                .color(TEXT_MUTED),
            );
            if !p.blocked.is_empty() {
                ui.label(
                    RichText::new(format!(
                        "{} файл(ов) нужно будет скачать вручную",
                        p.blocked.len()
                    ))
                    .color(STATUS_WARN),
                );
            }
            ui.label(
                RichText::new("Команды запуска из архива игнорируются.")
                    .color(TEXT_MUTED)
                    .small(),
            );
            ui.checkbox(
                &mut d.confirm_modpack,
                "Я хочу создать новый инстанс из этой сборки",
            );
        }

        if let Some(plan) = d.plan.clone() {
            ui.label(
                RichText::new("Будет установлено")
                    .color(TEXT_PRIMARY)
                    .strong(),
            );
            for dl in &plan.downloads {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(&dl.filename).color(TEXT_PRIMARY));
                    ui.label(
                        RichText::new(format_size(dl.size))
                            .color(TEXT_MUTED)
                            .small(),
                    );
                    if dl.is_dependency {
                        version_badge(ui, "зависимость", ACCENT_COBALT);
                    }
                    if dl.is_download_blocked() {
                        version_badge(ui, "вручную", STATUS_WARN);
                    }
                });
            }
            if !plan.optional_dependencies.is_empty() {
                ui.add_space(8.0);
                ui.label(RichText::new("Необязательные зависимости").color(TEXT_MUTED));
                for o in &plan.optional_dependencies {
                    if let Some(pid) = &o.project_id {
                        let mut on = d.selected_optionals.contains(pid);
                        let label = d.titles.get(pid).cloned().unwrap_or_else(|| pid.clone());
                        if ui.checkbox(&mut on, label).changed() {
                            toggled = Some(pid.clone());
                        }
                    }
                }
            }
            for b in &plan.blocked_downloads {
                ui.label(
                    RichText::new(format!("Вручную: {}", b.filename))
                        .color(STATUS_WARN)
                        .small(),
                );
            }
        }

        ui.add_space(12.0);
        ui.horizontal(|ui| {
            ui.add_enabled_ui(d.can_confirm(), |ui| {
                if primary_emerald_button(ui, Some(VectorIcon::Plus), "Установить").clicked()
                {
                    confirm = true;
                }
            });
            if secondary_glass_button(ui, None, "Отмена").clicked() {
                cancel = true;
            }
        });
    });

    if let Some(pid) = toggled {
        if let Some(d) = state.install.as_mut() {
            d.toggle_optional(&pid);
        }
        state.replan_install();
    }
    if let Some(id) = new_target {
        state.set_install_target(id);
    }
    if confirm {
        state.confirm_install();
    } else if cancel {
        state.install = None;
    }
}

fn manual_window(ctx: &Context, state: &mut UiState) {
    if !state.manual.open {
        return;
    }
    let mut close = false;
    let mut open_url: Option<String> = None;
    window("Эти файлы нужно скачать вручную").show(ctx, |ui| {
        ui.label(
            RichText::new(
                "Автор проекта запретил автоматическую загрузку. Скачайте файлы в браузере: \
                 они будут найдены в папке загрузок и добавлены автоматически.",
            )
            .color(TEXT_MUTED),
        );
        ui.label(
            RichText::new(format!("Папка: {}", state.manual.watch_dir_label))
                .color(TEXT_MUTED)
                .small(),
        );
        ui.horizontal(|ui| {
            ui.label(
                RichText::new(format!("Осталось: {}", state.manual.remaining()))
                    .color(TEXT_PRIMARY)
                    .strong(),
            );
            ui.label(
                RichText::new("Перетащите скачанный файл в окно лаунчера")
                    .color(TEXT_MUTED)
                    .small(),
            );
        });
        ui.add_space(8.0);
        for b in &state.manual.pending {
            ui.horizontal(|ui| {
                ui.label(RichText::new(&b.filename).color(TEXT_PRIMARY));
                if secondary_glass_button(ui, None, "Открыть страницу").clicked() {
                    open_url = Some(b.webpage_url.clone());
                }
            });
        }
        for name in &state.manual.completed {
            ui.label(RichText::new(format!("Получено: {name}")).color(ACCENT_EMERALD));
        }
        if let Some(m) = &state.manual.message {
            ui.label(RichText::new(m).color(ACCENT_EMERALD));
        }
        ui.add_space(8.0);
        if secondary_glass_button(ui, None, "Закрыть").clicked() {
            close = true;
        }
    });
    if let Some(url) = open_url {
        if crate::catalog_flow::is_safe_http_url(&url) {
            ctx.open_url(egui::OpenUrl::new_tab(url));
        }
    }
    if close {
        state.manual.open = false;
    }
    // Files dragged into the window are handed to the watcher immediately.
    let dropped: Vec<std::path::PathBuf> = ctx.input(|i| {
        i.raw
            .dropped_files
            .iter()
            .filter_map(|f| f.path.clone())
            .collect()
    });
    // Poll the downloads folder at most every 2 seconds.
    let due = state
        .manual
        .last_poll
        .is_none_or(|t| t.elapsed() >= std::time::Duration::from_secs(2));
    if state.manual.open && (due || !dropped.is_empty()) {
        state.manual.last_poll = Some(std::time::Instant::now());
        let _ = state
            .cmd_sender
            .send(UiWorkerCommand::PollManual { dropped });
    }
    ctx.request_repaint_after(std::time::Duration::from_secs(2));
}

/// "Загрузки" queue of catalog jobs, shown at the bottom of the catalog screen.
pub fn jobs_panel(ui: &mut Ui, state: &mut UiState) {
    if state.jobs.jobs.is_empty() {
        return;
    }
    ui.add_space(8.0);
    ui.separator();
    ui.horizontal(|ui| {
        ui.label(RichText::new("Загрузки").color(TEXT_PRIMARY).strong());
        if secondary_glass_button(ui, None, "Очистить завершённые").clicked() {
            state.jobs.remove_finished();
        }
    });
    let mut cancel = None;
    let mut retry = None;
    for j in &state.jobs.jobs {
        ui.horizontal(|ui| {
            ui.label(RichText::new(&j.title).color(TEXT_PRIMARY));
            ui.label(RichText::new(&j.target_label).color(TEXT_MUTED).small());
            match &j.status {
                JobStatus::Running => {
                    ui.add(
                        egui::ProgressBar::new(j.progress())
                            .desired_width(160.0)
                            .text(format!("{}/{}", j.done_files, j.total_files)),
                    );
                    if danger_button(ui, None, "Отмена").clicked() {
                        cancel = Some(j.id);
                    }
                }
                JobStatus::Done => version_badge(ui, "Готово", ACCENT_EMERALD),
                JobStatus::Cancelled => {
                    version_badge(ui, "Отменено", STATUS_WARN);
                    if secondary_glass_button(ui, None, "Повторить").clicked() {
                        retry = Some(j.id);
                    }
                }
                JobStatus::Failed(e) => {
                    ui.label(RichText::new(e).color(STATUS_DANGER).small());
                    if secondary_glass_button(ui, None, "Повторить").clicked() {
                        retry = Some(j.id);
                    }
                }
            }
        });
    }
    if let Some(id) = cancel {
        state.cancel_job(id);
    }
    if let Some(id) = retry {
        state.retry_job(id);
    }
}
