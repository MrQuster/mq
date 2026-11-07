//! Ephemeral notification toasts in bottom-right corner.

use egui::{Context, Pos2, Rect, Stroke, Vec2};
use std::time::{Duration, Instant};

use crate::theme::{Tokens, RADIUS_CARD};
use crate::widgets::{paint_vector_icon, VectorIcon};

#[derive(Debug, Clone)]
pub struct Toast {
    pub message: String,
    pub icon: VectorIcon,
    pub created_at: Instant,
    pub duration: Duration,
    pub is_error: bool,
}

#[derive(Default)]
pub struct ToastManager {
    toasts: Vec<Toast>,
}

impl ToastManager {
    pub fn push_info(&mut self, message: impl Into<String>) {
        self.toasts.push(Toast {
            message: message.into(),
            icon: VectorIcon::Info,
            created_at: Instant::now(),
            duration: Duration::from_secs(4),
            is_error: false,
        });
    }

    pub fn push_error(&mut self, message: impl Into<String>) {
        self.toasts.push(Toast {
            message: message.into(),
            icon: VectorIcon::Warning,
            created_at: Instant::now(),
            duration: Duration::from_secs(6),
            is_error: true,
        });
    }

    pub fn render(&mut self, ctx: &Context, tokens: &Tokens) {
        let now = Instant::now();
        self.toasts
            .retain(|t| now.duration_since(t.created_at) < t.duration);

        if self.toasts.is_empty() {
            return;
        }

        let screen_rect = ctx.screen_rect();
        let mut y_offset = screen_rect.bottom() - 48.0;

        for toast in self.toasts.iter().rev() {
            let width = 300.0;
            let height = 40.0;
            let pos = Pos2::new(screen_rect.right() - width - 16.0, y_offset - height);
            y_offset -= height + 8.0;

            let rect = Rect::from_min_size(pos, Vec2::new(width, height));
            let painter = ctx.layer_painter(egui::LayerId::new(
                egui::Order::Tooltip,
                egui::Id::new("toasts_layer"),
            ));

            let border_col = if toast.is_error {
                tokens.danger
            } else {
                tokens.border
            };
            painter.rect(
                rect,
                RADIUS_CARD,
                tokens.bg_surface,
                Stroke::new(1.0_f32, border_col),
                egui::StrokeKind::Inside,
            );

            let icon_rect = Rect::from_min_size(pos + Vec2::new(12.0, 12.0), Vec2::splat(16.0));
            let icon_col = if toast.is_error {
                tokens.danger
            } else {
                tokens.accent
            };
            paint_vector_icon(&painter, icon_rect, toast.icon, icon_col);

            let text_pos = pos + Vec2::new(36.0, 11.0);
            painter.text(
                text_pos,
                egui::Align2::LEFT_TOP,
                &toast.message,
                egui::FontId::proportional(13.0),
                tokens.text,
            );
        }
    }
}
