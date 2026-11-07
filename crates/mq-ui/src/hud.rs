//! Bottom status bar and real-time per-core CPU telemetry HUD (`cpu0..cpuN` vertical bars,
//! live throughput metrics in `FontFamily::Monospace`, `UiWatchdog` alert counter, and safe
//! `0x0` Wayland/Hyprland viewport guard).

use egui::{
    Align, Color32, Context, CornerRadius, FontFamily, FontId, Layout, Pos2, Rect, RichText, Sense,
    Stroke, StrokeKind, TopBottomPanel, Ui, Vec2,
};
use mq_core::CpuSnapshot;

use crate::theme::{
    hud_frame, ACCENT_COBALT, ACCENT_EMERALD, BORDER_SUBTLE, CARD_SURFACE, RADIUS_TAG,
    STATUS_DANGER, STATUS_WARN, TEXT_MUTED, TEXT_PRIMARY,
};
use crate::widgets::{status_dot, version_badge};

/// Fixed height of the bottom status & telemetry HUD bar (`38px`).
pub const BOTTOM_HUD_HEIGHT: f32 = 38.0;

/// Data model for rendering the bottom status & CPU telemetry HUD bar.
#[derive(Debug, Clone, Default)]
pub struct BottomHudData {
    /// Name of the active installation/verification/launch phase, or `None` when idle.
    pub active_phase: Option<String>,
    /// Optional completion fraction (`0.0..=1.0`) for the active phase.
    pub progress_fraction: Option<f32>,
    /// Smoothed item throughput (`files/sec`).
    pub files_per_sec: f64,
    /// Smoothed byte throughput (`MiB/sec`).
    pub mib_per_sec: f64,
    /// Number of currently executing DAG / worker tasks.
    pub active_dag_tasks: usize,
    /// Latest `/proc/stat` per-core CPU load and `VmRSS` memory snapshot.
    pub cpu_snapshot: CpuSnapshot,
    /// Cumulative `UiWatchdog` (`>8ms` frame budget) violation count.
    pub watchdog_violations: u64,
}

/// Returns `true` when the window viewport has been minimized or resized to `0x0` (`<= 1.0px`)
/// on tiling Wayland compositors such as Hyprland or Sway, allowing the frame loop to safely
/// skip layout and avoid degenerate division-by-zero geometry.
#[inline]
pub fn is_zero_viewport(ctx: &Context) -> bool {
    let rect = ctx.screen_rect();
    !rect.width().is_finite()
        || !rect.height().is_finite()
        || rect.width() <= 1.0
        || rect.height() <= 1.0
}

fn core_bar_color(usage_pct: f32) -> Color32 {
    if usage_pct >= 90.0 {
        STATUS_WARN
    } else if usage_pct >= 55.0 {
        ACCENT_COBALT
    } else {
        ACCENT_EMERALD
    }
}

fn render_per_core_bars(ui: &mut Ui, snap: &CpuSnapshot) {
    let core_count = snap.per_core.len().max(1);
    let bar_w = 5.0;
    let gap = 2.0;
    let chart_h = 20.0;
    let total_w = (core_count as f32) * bar_w + ((core_count.saturating_sub(1)) as f32) * gap + 6.0;

    let (rect, response) = ui.allocate_exact_size(Vec2::new(total_w, chart_h), Sense::hover());

    if ui.is_rect_visible(rect) {
        let painter = ui.painter();
        painter.rect_filled(rect, CornerRadius::same(3), CARD_SURFACE);
        painter.rect_stroke(
            rect,
            CornerRadius::same(3),
            Stroke::new(1.0_f32, BORDER_SUBTLE),
            StrokeKind::Inside,
        );

        let inner_top = rect.top() + 2.0;
        let inner_bottom = rect.bottom() - 2.0;
        let max_bar_h = (inner_bottom - inner_top).max(1.0);

        for idx in 0..core_count {
            let pct = snap
                .per_core
                .get(idx)
                .map(|c| c.usage_pct.clamp(0.0, 100.0))
                .unwrap_or(0.0);

            let x0 = rect.left() + 3.0 + (idx as f32) * (bar_w + gap);
            let track_rect = Rect::from_min_max(
                Pos2::new(x0, inner_top),
                Pos2::new(x0 + bar_w, inner_bottom),
            );
            painter.rect_filled(track_rect, CornerRadius::same(1), BORDER_SUBTLE);

            if pct > 0.5 {
                let fill_h = (max_bar_h * (pct / 100.0)).clamp(1.5, max_bar_h);
                let fill_rect = Rect::from_min_max(
                    Pos2::new(x0, inner_bottom - fill_h),
                    Pos2::new(x0 + bar_w, inner_bottom),
                );
                painter.rect_filled(fill_rect, CornerRadius::same(1), core_bar_color(pct));
            }
        }
    }

    if response.hovered() {
        response.on_hover_ui(|ui| {
            ui.label(
                RichText::new(format!(
                    "Процессор: {:>4.1}% | Память: {:>5.1} МБ",
                    snap.total_pct, snap.rss_mib
                ))
                .font(FontId::new(12.0, FontFamily::Monospace))
                .color(TEXT_PRIMARY),
            );
            if !snap.per_core.is_empty() {
                ui.separator();
                for core in &snap.per_core {
                    ui.label(
                        RichText::new(format!("cpu{}: {:>5.1}%", core.core_id, core.usage_pct))
                            .font(FontId::new(11.5, FontFamily::Monospace))
                            .color(TEXT_MUTED),
                    );
                }
            }
        });
    }
}

fn render_smooth_progress_bar(ui: &mut Ui, fraction: f32) {
    let clamped = fraction.clamp(0.0, 1.0);
    let desired = Vec2::new(130.0, 14.0);
    let (rect, _) = ui.allocate_exact_size(desired, Sense::hover());

    if ui.is_rect_visible(rect) {
        let painter = ui.painter();
        painter.rect_filled(rect, RADIUS_TAG, BORDER_SUBTLE);
        if clamped > 0.0 {
            let fill_w = (rect.width() * clamped).max(3.0);
            let fill_rect = Rect::from_min_size(rect.min, Vec2::new(fill_w, rect.height()));
            painter.rect_filled(fill_rect, RADIUS_TAG, ACCENT_EMERALD);
        }
        let pct_str = format!("{:>3.0}%", clamped * 100.0);
        let galley = painter.layout_no_wrap(
            pct_str,
            FontId::new(10.5, FontFamily::Monospace),
            TEXT_PRIMARY,
        );
        let pos = Pos2::new(
            rect.center().x - galley.size().x * 0.5,
            rect.center().y - galley.size().y * 0.5,
        );
        painter.galley(pos, galley, TEXT_PRIMARY);
    }
}

/// Renders the bottom status bar and per-core CPU telemetry HUD inside `ui`.
pub fn render_bottom_hud(ui: &mut Ui, data: &BottomHudData) {
    let avail_w = ui.available_width();
    ui.horizontal_centered(|ui| {
        ui.spacing_mut().item_spacing.x = 10.0;

        // Left: Phase status dot + label + optional progress bar + throughput metrics
        let (dot_color, phase_text) = match data.active_phase.as_deref() {
            Some(phase) if !phase.is_empty() => (ACCENT_EMERALD, phase),
            _ => (ACCENT_EMERALD.gamma_multiply(0.65), "Готов к работе"),
        };
        status_dot(ui, dot_color, phase_text);

        if let Some(frac) = data.progress_fraction {
            if avail_w >= 500.0 {
                render_smooth_progress_bar(ui, frac);
            }
        }

        let mono_font = FontId::new(12.0, FontFamily::Monospace);

        if avail_w >= 940.0 {
            ui.separator();
            let dag_label = if data.active_dag_tasks == 0 {
                "Загрузки: нет активных".to_string()
            } else {
                format!("Загрузки: {} активных", data.active_dag_tasks)
            };
            let metrics_text = format!(
                "{:>3.0} файлов/с | {:>4.1} МБ/с | {}",
                data.files_per_sec, data.mib_per_sec, dag_label
            );
            ui.label(
                RichText::new(metrics_text)
                    .font(mono_font.clone())
                    .color(TEXT_MUTED),
            );
        } else if avail_w >= 760.0 {
            ui.separator();
            let dag_label = if data.active_dag_tasks == 0 {
                "нет активных".to_string()
            } else {
                format!("{} акт.", data.active_dag_tasks)
            };
            let metrics_text = format!(
                "{:>3.0} ф/с | {:>4.1} МБ/с | Загрузки: {}",
                data.files_per_sec, data.mib_per_sec, dag_label
            );
            ui.label(
                RichText::new(metrics_text)
                    .font(mono_font.clone())
                    .color(TEXT_MUTED),
            );
        }

        // Right: Per-core CPU telemetry vertical bars + Memory + Watchdog indicator
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.spacing_mut().item_spacing.x = 8.0;

            if data.watchdog_violations > 0 {
                let badge_col = if data.watchdog_violations > 5 {
                    STATUS_DANGER
                } else {
                    STATUS_WARN
                };
                version_badge(ui, &format!("WD: {}", data.watchdog_violations), badge_col);
            }

            let rss_label = if avail_w < 550.0 {
                format!("Процессор: {:>3.0}%", data.cpu_snapshot.total_pct)
            } else {
                format!(
                    "Процессор: {:>3.0}% | Память: {:>5.1} МБ",
                    data.cpu_snapshot.total_pct, data.cpu_snapshot.rss_mib
                )
            };
            ui.label(RichText::new(rss_label).font(mono_font).color(TEXT_PRIMARY));

            render_per_core_bars(ui, &data.cpu_snapshot);
        });
    });
}

/// Renders the `38px` bottom [`TopBottomPanel`] HUD when the viewport is non-zero.
pub fn show_bottom_hud(ctx: &Context, data: &BottomHudData) {
    if is_zero_viewport(ctx) {
        return;
    }
    TopBottomPanel::bottom("mq_bottom_telemetry_hud")
        .exact_height(BOTTOM_HUD_HEIGHT)
        .frame(hud_frame())
        .show(ctx, |ui| {
            render_bottom_hud(ui, data);
        });
}

#[cfg(test)]
mod tests {
    use super::*;
    use mq_core::CoreCpuLoad;

    fn contains_emoji(text: &str) -> bool {
        text.chars().any(|ch| {
            let cp = ch as u32;
            (0x1F300..=0x1FAFF).contains(&cp)
                || (0x2600..=0x27BF).contains(&cp)
                || (0xFE00..=0xFE0F).contains(&cp)
        })
    }

    #[test]
    fn zero_emojis_exist_across_all_owned_ui_files() {
        let sources = [
            ("theme.rs", include_str!("theme.rs")),
            ("fonts.rs", include_str!("fonts.rs")),
            ("widgets.rs", include_str!("widgets.rs")),
            ("sidebar.rs", include_str!("sidebar.rs")),
            ("hud.rs", include_str!("hud.rs")),
        ];

        for (name, content) in sources {
            assert!(
                !contains_emoji(content),
                "strict zero-emoji policy violated in {name}"
            );
        }
    }

    #[test]
    fn bottom_hud_renders_and_handles_zero_viewport_safely() {
        let ctx = Context::default();
        let data = BottomHudData {
            active_phase: Some("Проверка SHA-1 (CAS)".to_owned()),
            progress_fraction: Some(0.64),
            files_per_sec: 420.0,
            mib_per_sec: 85.4,
            active_dag_tasks: 8,
            cpu_snapshot: CpuSnapshot {
                total_pct: 48.5,
                per_core: vec![
                    CoreCpuLoad {
                        core_id: 0,
                        usage_pct: 75.0,
                    },
                    CoreCpuLoad {
                        core_id: 1,
                        usage_pct: 22.0,
                    },
                ],
                rss_mib: 44.2,
            },
            watchdog_violations: 0,
        };

        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            show_bottom_hud(ctx, &data);
        });

        let zero_input = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::ZERO)),
            ..egui::RawInput::default()
        };
        let _ = ctx.run(zero_input, |ctx| {
            assert!(is_zero_viewport(ctx));
            show_bottom_hud(ctx, &data);
        });
    }
}
