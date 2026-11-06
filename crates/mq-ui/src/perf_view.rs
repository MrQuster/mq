#![deny(clippy::await_holding_lock)]
//! Performance & Task telemetry view: per-core CPU meters from `/proc/stat`,
//! memory RSS monitoring, and UI Watchdog statistics. Zero emojis!

use egui::{ProgressBar, RichText, Stroke, Ui, Vec2};

use crate::state::UiState;
use crate::theme::{
    ACCENT_EMERALD, BORDER_SUBTLE, CARD_SURFACE, RADIUS_CARD, STATUS_WARN, TEXT_MUTED, TEXT_PRIMARY,
};
use crate::widgets::{paint_vector_icon, VectorIcon};

#[derive(Default)]
pub struct PerfView;

impl PerfView {
    pub fn ui(&self, ui: &mut Ui, state: &UiState) {
        ui.heading(
            RichText::new("Производительность и Задачи")
                .color(TEXT_PRIMARY)
                .strong(),
        );
        ui.add_space(8.0);

        let card = egui::Frame::new()
            .fill(CARD_SURFACE)
            .stroke(Stroke::new(1.0_f32, BORDER_SUBTLE))
            .corner_radius(RADIUS_CARD)
            .inner_margin(egui::Margin::same(12));

        card.show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                let (rect, _) = ui.allocate_exact_size(Vec2::splat(20.0), egui::Sense::hover());
                paint_vector_icon(ui.painter(), rect, VectorIcon::Cpu, ACCENT_EMERALD);
                ui.label(RichText::new("Загрузка ядер процессора (/proc/stat)").strong());
            });

            ui.add_space(8.0);
            ui.label(format!(
                "Общая загрузка CPU: {:.1}% | Память процесса (RSS): {:.1} МиБ",
                state.latest_cpu.total_pct, state.latest_cpu.rss_mib
            ));

            ui.add_space(10.0);
            for core in &state.latest_cpu.per_core {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(format!("Ядро {:02}:", core.core_id)).color(TEXT_MUTED).monospace());
                    let frac = (core.usage_pct / 100.0).clamp(0.0, 1.0);
                    ui.add(ProgressBar::new(frac).desired_width(260.0).text(format!("{:.1}%", core.usage_pct)));
                });
                ui.add_space(2.0);
            }

            ui.add_space(12.0);
            ui.separator();
            ui.add_space(8.0);

            ui.label(RichText::new("Сторожевой таймер UI (Watchdog > 8 мс)").strong());
            let violations = state.watchdog.violation_count();
            let col = if violations == 0 { ACCENT_EMERALD } else { STATUS_WARN };
            ui.label(RichText::new(format!("Зафиксировано фризов кадра > 8 мс: {violations}")).color(col));

            ui.add_space(6.0);
            ui.label(RichText::new(
                "Лаунчер MQ использует shared-nothing архитектуру и параллельный пул Rayon для всех тяжёлых вычислений (хэширование SHA-1/SHA-512, распаковка модпаков, сканирование модов), гарантируя плавность интерфейса."
            ).color(TEXT_MUTED).small());
        });
    }
}
