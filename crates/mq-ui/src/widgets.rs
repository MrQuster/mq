//! Zero-emoji geometric vector icons (`egui::Painter`), styled Dark Glass buttons,
//! 4px version badges, status indicators, and Nearest-Neighbor 2D skin head avatar rendering.

use std::collections::HashMap;

use egui::{
    Color32, ColorImage, Context, CornerRadius, FontId, Painter, Pos2, Rect, Response, Sense,
    Shape, Stroke, StrokeKind, TextureHandle, TextureOptions, Ui, Vec2,
};
use mq_auth::{generate_offline_geometric_avatar, SkinHeadAvatar};

use crate::theme::{
    ACCENT_EMERALD, ACCENT_EMERALD_HOVER, BG_DEEP_GRAPHITE, BORDER_HOVER, BORDER_SUBTLE,
    BUTTON_PADDING, CARD_SURFACE, CARD_SURFACE_HOVER, RADIUS_BUTTON, RADIUS_TAG, STATUS_DANGER,
    TEXT_PRIMARY,
};

/// Crisp vector icons drawn procedurally via [`egui::Painter`] (strict zero-emoji policy).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum VectorIcon {
    Play,
    Stop,
    Check,
    Verify,
    Folder,
    Plus,
    Trash,
    Search,
    Layers,
    Catalog,
    User,
    Cpu,
    Settings,
    Refresh,
    Copy,
    ChevronRight,
    ChevronDown,
    Warning,
    Shield,
}

#[inline]
fn rel_pos(rect: Rect, nx: f32, ny: f32) -> Pos2 {
    Pos2::new(
        rect.left() + rect.width() * nx,
        rect.top() + rect.height() * ny,
    )
}

/// Paints a geometric [`VectorIcon`] inside `rect` using `color` without any emoji glyphs.
pub fn paint_vector_icon(painter: &Painter, rect: Rect, icon: VectorIcon, color: Color32) {
    let r = rect.shrink((rect.width().min(rect.height()) * 0.10).max(1.0));
    let stroke_w = (r.width().min(r.height()) * 0.11).clamp(1.3, 2.2);
    let stroke = Stroke::new(stroke_w, color);

    match icon {
        VectorIcon::Play => {
            let pts = vec![
                rel_pos(r, 0.22, 0.14),
                rel_pos(r, 0.86, 0.50),
                rel_pos(r, 0.22, 0.86),
            ];
            painter.add(Shape::convex_polygon(pts, color, Stroke::NONE));
        }
        VectorIcon::Stop => {
            let sq = Rect::from_min_max(rel_pos(r, 0.20, 0.20), rel_pos(r, 0.80, 0.80));
            painter.rect_filled(sq, CornerRadius::same(2), color);
        }
        VectorIcon::Check => {
            let p1 = rel_pos(r, 0.16, 0.54);
            let p2 = rel_pos(r, 0.42, 0.78);
            let p3 = rel_pos(r, 0.86, 0.24);
            painter.line_segment([p1, p2], stroke);
            painter.line_segment([p2, p3], stroke);
        }
        VectorIcon::Verify => {
            let center = r.center();
            let rad = r.width().min(r.height()) * 0.44;
            painter.circle_stroke(center, rad, stroke);
            let p1 = rel_pos(r, 0.28, 0.52);
            let p2 = rel_pos(r, 0.45, 0.68);
            let p3 = rel_pos(r, 0.74, 0.34);
            painter.line_segment([p1, p2], stroke);
            painter.line_segment([p2, p3], stroke);
        }
        VectorIcon::Folder => {
            let tab_pts = vec![
                rel_pos(r, 0.10, 0.28),
                rel_pos(r, 0.42, 0.28),
                rel_pos(r, 0.52, 0.40),
                rel_pos(r, 0.10, 0.40),
            ];
            painter.add(Shape::convex_polygon(tab_pts, Color32::TRANSPARENT, stroke));
            let body = Rect::from_min_max(rel_pos(r, 0.10, 0.38), rel_pos(r, 0.90, 0.84));
            painter.rect_stroke(body, CornerRadius::same(2), stroke, StrokeKind::Inside);
        }
        VectorIcon::Plus => {
            painter.line_segment([rel_pos(r, 0.50, 0.16), rel_pos(r, 0.50, 0.84)], stroke);
            painter.line_segment([rel_pos(r, 0.16, 0.50), rel_pos(r, 0.84, 0.50)], stroke);
        }
        VectorIcon::Trash => {
            painter.line_segment([rel_pos(r, 0.18, 0.26), rel_pos(r, 0.82, 0.26)], stroke);
            painter.line_segment([rel_pos(r, 0.38, 0.16), rel_pos(r, 0.62, 0.16)], stroke);
            let can = Rect::from_min_max(rel_pos(r, 0.26, 0.28), rel_pos(r, 0.74, 0.86));
            painter.rect_stroke(can, CornerRadius::same(2), stroke, StrokeKind::Inside);
            painter.line_segment([rel_pos(r, 0.42, 0.42), rel_pos(r, 0.42, 0.72)], stroke);
            painter.line_segment([rel_pos(r, 0.58, 0.42), rel_pos(r, 0.58, 0.72)], stroke);
        }
        VectorIcon::Search => {
            let lens_center = rel_pos(r, 0.42, 0.42);
            let lens_rad = r.width().min(r.height()) * 0.28;
            painter.circle_stroke(lens_center, lens_rad, stroke);
            painter.line_segment([rel_pos(r, 0.62, 0.62), rel_pos(r, 0.88, 0.88)], stroke);
        }
        VectorIcon::Layers => {
            let top_diamond = vec![
                rel_pos(r, 0.50, 0.14),
                rel_pos(r, 0.88, 0.34),
                rel_pos(r, 0.50, 0.54),
                rel_pos(r, 0.12, 0.34),
            ];
            painter.add(Shape::closed_line(top_diamond, stroke));
            painter.add(Shape::line(
                vec![
                    rel_pos(r, 0.12, 0.52),
                    rel_pos(r, 0.50, 0.70),
                    rel_pos(r, 0.88, 0.52),
                ],
                stroke,
            ));
            painter.add(Shape::line(
                vec![
                    rel_pos(r, 0.12, 0.68),
                    rel_pos(r, 0.50, 0.86),
                    rel_pos(r, 0.88, 0.68),
                ],
                stroke,
            ));
        }
        VectorIcon::Catalog => {
            for (gx, gy) in [(0.14, 0.14), (0.56, 0.14), (0.14, 0.56), (0.56, 0.56)] {
                let cell = Rect::from_min_max(rel_pos(r, gx, gy), rel_pos(r, gx + 0.30, gy + 0.30));
                painter.rect_stroke(cell, CornerRadius::same(2), stroke, StrokeKind::Inside);
            }
        }
        VectorIcon::User => {
            let head_c = rel_pos(r, 0.50, 0.32);
            let head_r = r.width().min(r.height()) * 0.18;
            painter.circle_stroke(head_c, head_r, stroke);
            painter.add(Shape::line(
                vec![
                    rel_pos(r, 0.18, 0.86),
                    rel_pos(r, 0.26, 0.64),
                    rel_pos(r, 0.74, 0.64),
                    rel_pos(r, 0.82, 0.86),
                ],
                stroke,
            ));
        }
        VectorIcon::Cpu => {
            let body = Rect::from_min_max(rel_pos(r, 0.24, 0.24), rel_pos(r, 0.76, 0.76));
            painter.rect_stroke(body, CornerRadius::same(2), stroke, StrokeKind::Inside);
            let die = Rect::from_min_max(rel_pos(r, 0.38, 0.38), rel_pos(r, 0.62, 0.62));
            painter.rect_filled(die, CornerRadius::same(1), color);
            for t in [0.36_f32, 0.64_f32] {
                painter.line_segment([rel_pos(r, t, 0.08), rel_pos(r, t, 0.24)], stroke);
                painter.line_segment([rel_pos(r, t, 0.76), rel_pos(r, t, 0.92)], stroke);
                painter.line_segment([rel_pos(r, 0.08, t), rel_pos(r, 0.24, t)], stroke);
                painter.line_segment([rel_pos(r, 0.76, t), rel_pos(r, 0.92, t)], stroke);
            }
        }
        VectorIcon::Settings => {
            for (idx, y) in [0.24_f32, 0.50_f32, 0.76_f32].into_iter().enumerate() {
                painter.line_segment([rel_pos(r, 0.14, y), rel_pos(r, 0.86, y)], stroke);
                let knob_x = if idx == 1 { 0.66 } else { 0.34 };
                painter.circle_filled(rel_pos(r, knob_x, y), stroke_w * 1.35, color);
            }
        }
        VectorIcon::Refresh => {
            let center = r.center();
            let rad = r.width().min(r.height()) * 0.34;
            painter.circle_stroke(center, rad, stroke);
            let arrow = vec![
                rel_pos(r, 0.68, 0.14),
                rel_pos(r, 0.88, 0.22),
                rel_pos(r, 0.74, 0.38),
            ];
            painter.add(Shape::convex_polygon(arrow, color, Stroke::NONE));
        }
        VectorIcon::Copy => {
            let back = Rect::from_min_max(rel_pos(r, 0.16, 0.16), rel_pos(r, 0.66, 0.66));
            let front = Rect::from_min_max(rel_pos(r, 0.34, 0.34), rel_pos(r, 0.84, 0.84));
            painter.rect_stroke(back, CornerRadius::same(2), stroke, StrokeKind::Inside);
            painter.rect_filled(front, CornerRadius::same(2), CARD_SURFACE);
            painter.rect_stroke(front, CornerRadius::same(2), stroke, StrokeKind::Inside);
        }
        VectorIcon::ChevronRight => {
            painter.add(Shape::line(
                vec![
                    rel_pos(r, 0.34, 0.20),
                    rel_pos(r, 0.68, 0.50),
                    rel_pos(r, 0.34, 0.80),
                ],
                stroke,
            ));
        }
        VectorIcon::ChevronDown => {
            painter.add(Shape::line(
                vec![
                    rel_pos(r, 0.20, 0.36),
                    rel_pos(r, 0.50, 0.68),
                    rel_pos(r, 0.80, 0.36),
                ],
                stroke,
            ));
        }
        VectorIcon::Warning => {
            let tri = vec![
                rel_pos(r, 0.50, 0.14),
                rel_pos(r, 0.88, 0.84),
                rel_pos(r, 0.12, 0.84),
            ];
            painter.add(Shape::closed_line(tri, stroke));
            painter.line_segment([rel_pos(r, 0.50, 0.38), rel_pos(r, 0.50, 0.62)], stroke);
            painter.circle_filled(rel_pos(r, 0.50, 0.73), stroke_w * 0.7, color);
        }
        VectorIcon::Shield => {
            let shield = vec![
                rel_pos(r, 0.50, 0.12),
                rel_pos(r, 0.84, 0.26),
                rel_pos(r, 0.78, 0.62),
                rel_pos(r, 0.50, 0.88),
                rel_pos(r, 0.22, 0.62),
                rel_pos(r, 0.16, 0.26),
            ];
            painter.add(Shape::closed_line(shield, stroke));
            painter.line_segment([rel_pos(r, 0.36, 0.52), rel_pos(r, 0.47, 0.64)], stroke);
            painter.line_segment([rel_pos(r, 0.47, 0.64), rel_pos(r, 0.66, 0.40)], stroke);
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn render_styled_button(
    ui: &mut Ui,
    icon: Option<VectorIcon>,
    label: &str,
    bg_idle: Color32,
    bg_hover: Color32,
    border_idle: Color32,
    border_hover: Color32,
    fg_color: Color32,
) -> Response {
    let font_id = FontId::proportional(14.0);
    let galley = ui
        .painter()
        .layout_no_wrap(label.to_owned(), font_id, fg_color);
    let icon_side = if icon.is_some() { 15.0 } else { 0.0 };
    let icon_gap = if icon.is_some() && !label.is_empty() {
        8.0
    } else {
        0.0
    };

    let desired_width = BUTTON_PADDING.x * 2.0 + icon_side + icon_gap + galley.size().x;
    let desired_height = (BUTTON_PADDING.y * 2.0 + galley.size().y).max(32.0);
    let (rect, response) =
        ui.allocate_exact_size(Vec2::new(desired_width, desired_height), Sense::click());

    if ui.is_rect_visible(rect) {
        let hovered = response.hovered() && ui.is_enabled();
        let active = response.is_pointer_button_down_on() && ui.is_enabled();
        let fill = if active || hovered { bg_hover } else { bg_idle };
        let stroke_col = if hovered || active {
            border_hover
        } else {
            border_idle
        };

        let painter = ui.painter();
        painter.rect_filled(rect, RADIUS_BUTTON, fill);
        painter.rect_stroke(
            rect,
            RADIUS_BUTTON,
            Stroke::new(1.0_f32, stroke_col),
            StrokeKind::Inside,
        );

        let mut cursor_x = rect.left() + BUTTON_PADDING.x;
        if let Some(ic) = icon {
            let icon_rect = Rect::from_center_size(
                Pos2::new(cursor_x + icon_side * 0.5, rect.center().y),
                Vec2::splat(icon_side),
            );
            paint_vector_icon(painter, icon_rect, ic, fg_color);
            cursor_x += icon_side + icon_gap;
        }

        let text_pos = Pos2::new(cursor_x, rect.center().y - galley.size().y * 0.5);
        painter.galley(text_pos, galley, fg_color);
    }

    response
}

/// Primary emerald action button (`#10B981` fill, `#0D0F14` dark text/icon, `6px` rounding).
pub fn primary_emerald_button(ui: &mut Ui, icon: Option<VectorIcon>, label: &str) -> Response {
    render_styled_button(
        ui,
        icon,
        label,
        ACCENT_EMERALD,
        ACCENT_EMERALD_HOVER,
        ACCENT_EMERALD,
        Color32::WHITE,
        BG_DEEP_GRAPHITE,
    )
}

/// Secondary Dark Glass button (`#151821` fill, `1px` `#252A36` stroke, `#F3F4F6` text, `6px` rounding).
pub fn secondary_glass_button(ui: &mut Ui, icon: Option<VectorIcon>, label: &str) -> Response {
    render_styled_button(
        ui,
        icon,
        label,
        CARD_SURFACE,
        CARD_SURFACE_HOVER,
        BORDER_SUBTLE,
        BORDER_HOVER,
        TEXT_PRIMARY,
    )
}

/// Danger / destructive action button (`#EF4444` border/accent, `6px` rounding).
pub fn danger_button(ui: &mut Ui, icon: Option<VectorIcon>, label: &str) -> Response {
    render_styled_button(
        ui,
        icon,
        label,
        CARD_SURFACE,
        STATUS_DANGER.gamma_multiply(0.22),
        STATUS_DANGER.gamma_multiply(0.65),
        STATUS_DANGER,
        STATUS_DANGER,
    )
}

/// Renders a compact `4px` rounded badge (`RADIUS_TAG`) for versions, loaders, or account kinds.
pub fn version_badge(ui: &mut Ui, text: &str, color: Color32) {
    let font_id = FontId::monospace(11.5);
    let galley = ui.painter().layout_no_wrap(text.to_owned(), font_id, color);
    let pad = Vec2::new(7.0, 3.0);
    let size = galley.size() + pad * 2.0;
    let (rect, _) = ui.allocate_exact_size(size, Sense::hover());

    if ui.is_rect_visible(rect) {
        let painter = ui.painter();
        let bg = Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), 32);
        let border = Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), 110);
        painter.rect_filled(rect, RADIUS_TAG, bg);
        painter.rect_stroke(
            rect,
            RADIUS_TAG,
            Stroke::new(1.0_f32, border),
            StrokeKind::Inside,
        );
        painter.galley(rect.min + pad, galley, color);
    }
}

/// Renders a crisp colored status circle followed by a text label.
pub fn status_dot(ui: &mut Ui, color: Color32, label: &str) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 6.0;
        let (dot_rect, _) = ui.allocate_exact_size(Vec2::splat(10.0), Sense::hover());
        if ui.is_rect_visible(dot_rect) {
            let center = dot_rect.center();
            ui.painter()
                .circle_filled(center, 4.5, color.gamma_multiply(0.28));
            ui.painter().circle_filled(center, 3.0, color);
        }
        ui.label(egui::RichText::new(label).color(TEXT_PRIMARY).size(13.0));
    });
}

/// Converts an `8x8` [`SkinHeadAvatar`] into an [`egui::ColorImage`] ready for [`TextureOptions::NEAREST`].
pub fn avatar_to_color_image(avatar: &SkinHeadAvatar) -> ColorImage {
    let w = avatar.width.max(1) as usize;
    let h = avatar.height.max(1) as usize;
    let expected = w * h * 4;
    if avatar.rgba.len() == expected {
        ColorImage::from_rgba_unmultiplied([w, h], &avatar.rgba)
    } else {
        let fallback = generate_offline_geometric_avatar("offline");
        ColorImage::from_rgba_unmultiplied(
            [fallback.width as usize, fallback.height as usize],
            &fallback.rgba,
        )
    }
}

/// GPU texture cache for `8x8` skin head avatars using [`TextureOptions::NEAREST`] filtering.
#[derive(Default)]
pub struct SkinAvatarCache {
    textures: HashMap<String, TextureHandle>,
}

impl SkinAvatarCache {
    /// Creates an empty [`SkinAvatarCache`].
    pub fn new() -> Self {
        Self::default()
    }

    /// Retrieves or uploads a pixel-sharp [`TextureOptions::NEAREST`] texture for `username`.
    pub fn get_or_upload(
        &mut self,
        ctx: &Context,
        username: &str,
        avatar: Option<&SkinHeadAvatar>,
    ) -> TextureHandle {
        if let Some(handle) = self.textures.get(username) {
            return handle.clone();
        }
        let resolved = avatar
            .cloned()
            .unwrap_or_else(|| generate_offline_geometric_avatar(username));
        let image = avatar_to_color_image(&resolved);
        let handle = ctx.load_texture(
            format!("mq_skin_head_{username}"),
            image,
            TextureOptions::NEAREST,
        );
        self.textures.insert(username.to_owned(), handle.clone());
        handle
    }

    /// Invalidates a cached avatar texture by username.
    pub fn invalidate(&mut self, username: &str) {
        self.textures.remove(username);
    }

    /// Clears all cached avatar textures.
    pub fn clear(&mut self) {
        self.textures.clear();
    }
}

/// Paints an `8x8` [`SkinHeadAvatar`] with exact Nearest-Neighbor pixel bounds inside `rect`.
pub fn paint_skin_head_in_rect(painter: &Painter, rect: Rect, avatar: &SkinHeadAvatar) {
    let cols = avatar.width.max(1) as usize;
    let rows = avatar.height.max(1) as usize;
    let expected = cols * rows * 4;
    if avatar.rgba.len() < expected {
        return;
    }

    let cell_w = rect.width() / cols as f32;
    let cell_h = rect.height() / rows as f32;

    for row in 0..rows {
        let y0 = (rect.top() + row as f32 * cell_h).floor();
        let y1 = (rect.top() + (row + 1) as f32 * cell_h)
            .floor()
            .max(y0 + 1.0);
        for col in 0..cols {
            let idx = (row * cols + col) * 4;
            let r = avatar.rgba[idx];
            let g = avatar.rgba[idx + 1];
            let b = avatar.rgba[idx + 2];
            let a = avatar.rgba[idx + 3];
            let color = Color32::from_rgba_unmultiplied(r, g, b, a);

            let x0 = (rect.left() + col as f32 * cell_w).floor();
            let x1 = (rect.left() + (col + 1) as f32 * cell_w)
                .floor()
                .max(x0 + 1.0);
            let px_rect = Rect::from_min_max(Pos2::new(x0, y0), Pos2::new(x1, y1));
            painter.rect_filled(px_rect, CornerRadius::ZERO, color);
        }
    }

    painter.rect_stroke(
        rect,
        CornerRadius::same(2),
        Stroke::new(1.0_f32, BORDER_SUBTLE),
        StrokeKind::Inside,
    );
}

/// Allocates a `size x size` square and renders the `8x8` skin head avatar (or deterministic
/// offline geometric avatar for `username` when `avatar` is `None`) with Nearest-Neighbor crispness.
pub fn paint_skin_head_avatar(
    ui: &mut Ui,
    size: f32,
    username: &str,
    avatar: Option<&SkinHeadAvatar>,
) -> Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(size), Sense::click());
    if ui.is_rect_visible(rect) {
        let fallback;
        let resolved = match avatar {
            Some(av) if av.rgba.len() == SkinHeadAvatar::BYTE_LEN => av,
            _ => {
                fallback = generate_offline_geometric_avatar(username);
                &fallback
            }
        };
        paint_skin_head_in_rect(ui.painter(), rect, resolved);
    }
    response
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn avatar_to_color_image_produces_8x8_nearest_neighbor_source() {
        let avatar = generate_offline_geometric_avatar("PlayerOne");
        let img = avatar_to_color_image(&avatar);
        assert_eq!(img.size, [8, 8]);
        assert_eq!(img.pixels.len(), 64);
    }

    #[test]
    fn vector_icons_render_without_panicking_in_headless_context() {
        let ctx = Context::default();
        let all_icons = [
            VectorIcon::Play,
            VectorIcon::Stop,
            VectorIcon::Check,
            VectorIcon::Verify,
            VectorIcon::Folder,
            VectorIcon::Plus,
            VectorIcon::Trash,
            VectorIcon::Search,
            VectorIcon::Layers,
            VectorIcon::Catalog,
            VectorIcon::User,
            VectorIcon::Cpu,
            VectorIcon::Settings,
            VectorIcon::Refresh,
            VectorIcon::Copy,
            VectorIcon::ChevronRight,
            VectorIcon::ChevronDown,
            VectorIcon::Warning,
            VectorIcon::Shield,
        ];

        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                for icon in all_icons {
                    let _ = primary_emerald_button(ui, Some(icon), "Action");
                    let _ = secondary_glass_button(ui, Some(icon), "Secondary");
                }
                version_badge(ui, "v0.1.0", ACCENT_EMERALD);
                status_dot(ui, ACCENT_EMERALD, "Ready");
                let _ = paint_skin_head_avatar(ui, 36.0, "OfflineTester", None);
            });
        });
    }
}
