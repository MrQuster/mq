//! Liquid Glass visual theme engine (`#0D0F14` deep graphite / configurable translucent glass,
//! `#10B981` emerald primary accent, `#3B82F6` cobalt secondary accent) and dynamic `ThemeConfig` application.

use egui::style::{Selection, WidgetVisuals, Widgets};
use egui::{Color32, Context, CornerRadius, Frame, Margin, Stroke, Style, Vec2, Visuals};
use mq_core::{ThemeConfig, UiDensity};

/// Main window background (`#0D0F14` — Deep Obsidian Graphite).
pub const BG_DEEP_GRAPHITE: Color32 = Color32::from_rgb(0x0D, 0x0F, 0x14);

/// Elevated panel / sidebar / modal surface (`#151821` with subtle glass translucency).
pub const PANEL_DARK_GLASS: Color32 = Color32::from_rgba_premultiplied(0x15, 0x18, 0x21, 242);

/// Card surface background (`#151821`).
pub const CARD_SURFACE: Color32 = Color32::from_rgb(0x15, 0x18, 0x21);

/// Hovered card or interactive surface (`#1C202C`).
pub const CARD_SURFACE_HOVER: Color32 = Color32::from_rgb(0x1C, 0x20, 0x2C);

/// Recessed input / log viewer background (`#090B0E`).
pub const BG_RECESSED: Color32 = Color32::from_rgb(0x09, 0x0B, 0x0E);

/// Subtle 1px structural border (`#252A36`).
pub const BORDER_SUBTLE: Color32 = Color32::from_rgb(0x25, 0x2A, 0x36);

/// Hovered 1px border (`#384152`).
pub const BORDER_HOVER: Color32 = Color32::from_rgb(0x38, 0x41, 0x52);

/// Primary emerald action accent (`#10B981` — Play / Install / Active indicator).
pub const ACCENT_EMERALD: Color32 = Color32::from_rgb(0x10, 0xB9, 0x81);

/// Hovered emerald action accent (`#34D399`).
pub const ACCENT_EMERALD_HOVER: Color32 = Color32::from_rgb(0x34, 0xD3, 0x99);

/// Secondary cobalt accent (`#3B82F6` — links, version badges, secondary progress).
pub const ACCENT_COBALT: Color32 = Color32::from_rgb(0x3B, 0x82, 0xF6);

/// Primary high-contrast text (`#F3F4F6`).
pub const TEXT_PRIMARY: Color32 = Color32::from_rgb(0xF3, 0xF4, 0xF6);

/// Secondary / muted slate text (`#9CA3AF`).
pub const TEXT_MUTED: Color32 = Color32::from_rgb(0x9C, 0xA3, 0xAF);

/// Warning status amber (`#F59E0B`).
pub const STATUS_WARN: Color32 = Color32::from_rgb(0xF5, 0x9E, 0x0B);

/// Danger / error status crimson (`#EF4444`).
pub const STATUS_DANGER: Color32 = Color32::from_rgb(0xEF, 0x44, 0x44);

/// 8px corner radius for cards, panels, and dialogs.
pub const RADIUS_CARD: CornerRadius = CornerRadius::same(8);

/// 6px corner radius for buttons, inputs, and selectable rows.
pub const RADIUS_BUTTON: CornerRadius = CornerRadius::same(6);

/// 4px corner radius for version badges and compact tags.
pub const RADIUS_TAG: CornerRadius = CornerRadius::same(4);

/// Standard item spacing (`[10.0, 10.0]`).
pub const ITEM_SPACING: Vec2 = egui::vec2(10.0, 10.0);

/// Standard window margin (`16px`).
pub const WINDOW_MARGIN: Margin = Margin::same(16);

/// Standard button padding (`[14.0, 8.0]`).
pub const BUTTON_PADDING: Vec2 = egui::vec2(14.0, 8.0);

#[inline]
pub fn rgb_to_c32(rgb: [u8; 3]) -> Color32 {
    Color32::from_rgb(rgb[0], rgb[1], rgb[2])
}

#[inline]
pub fn rgb_to_glass_c32(rgb: [u8; 3], alpha_fraction: f32) -> Color32 {
    let a = (alpha_fraction.clamp(0.0, 1.0) * 255.0).round() as u8;
    Color32::from_rgba_unmultiplied(rgb[0], rgb[1], rgb[2], a)
}

/// Builds dynamic visuals from the active [`ThemeConfig`].
pub fn build_visuals_from_config(cfg: &ThemeConfig) -> Visuals {
    let mut visuals = Visuals::dark();

    let bg_window = rgb_to_glass_c32(cfg.bg_window, cfg.window_opacity);
    let bg_card = rgb_to_glass_c32(cfg.bg_card, cfg.card_opacity);
    let accent_pri = rgb_to_c32(cfg.accent_primary);
    let accent_sec = rgb_to_c32(cfg.accent_secondary);
    let text_pri = rgb_to_c32(cfg.text_primary);
    let text_mut = rgb_to_c32(cfg.text_muted);
    let stat_warn = rgb_to_c32(cfg.status_success);
    let stat_err = rgb_to_c32(cfg.status_error);

    let border_glow_alpha = (25.0 * cfg.border_glow.clamp(0.0, 1.0)).round() as u8;
    let border_stroke_color = if border_glow_alpha > 0 {
        Color32::from_rgba_unmultiplied(255, 255, 255, border_glow_alpha.max(20))
    } else {
        BORDER_SUBTLE
    };

    let corner_card = CornerRadius::same(cfg.corner_radius.round() as u8);
    let corner_btn = CornerRadius::same(((cfg.corner_radius * 0.75).round() as u8).max(3));

    visuals.dark_mode = true;
    visuals.override_text_color = Some(text_pri);
    visuals.panel_fill = bg_window;
    visuals.window_fill = bg_card;
    visuals.faint_bg_color = bg_card;
    visuals.extreme_bg_color = BG_RECESSED;
    visuals.code_bg_color = BG_RECESSED;

    visuals.window_corner_radius = corner_card;
    visuals.menu_corner_radius = corner_card;
    visuals.window_stroke = Stroke::new(1.0_f32, border_stroke_color);

    visuals.hyperlink_color = accent_sec;
    visuals.warn_fg_color = stat_warn;
    visuals.error_fg_color = stat_err;

    visuals.selection = Selection {
        bg_fill: accent_pri,
        stroke: Stroke::new(1.0_f32, BG_DEEP_GRAPHITE),
    };

    visuals.widgets = Widgets {
        noninteractive: WidgetVisuals {
            weak_bg_fill: bg_card,
            bg_fill: bg_card,
            bg_stroke: Stroke::new(1.0_f32, border_stroke_color),
            fg_stroke: Stroke::new(1.0_f32, text_pri),
            corner_radius: corner_card,
            expansion: 0.0,
        },
        inactive: WidgetVisuals {
            weak_bg_fill: bg_card,
            bg_fill: bg_card,
            bg_stroke: Stroke::new(1.0_f32, border_stroke_color),
            fg_stroke: Stroke::new(1.0_f32, text_pri),
            corner_radius: corner_btn,
            expansion: 0.0,
        },
        hovered: WidgetVisuals {
            weak_bg_fill: CARD_SURFACE_HOVER,
            bg_fill: CARD_SURFACE_HOVER,
            bg_stroke: Stroke::new(1.0_f32, BORDER_HOVER),
            fg_stroke: Stroke::new(1.5_f32, Color32::WHITE),
            corner_radius: corner_btn,
            expansion: 0.0,
        },
        active: WidgetVisuals {
            weak_bg_fill: CARD_SURFACE_HOVER,
            bg_fill: CARD_SURFACE_HOVER,
            bg_stroke: Stroke::new(1.0_f32, accent_pri),
            fg_stroke: Stroke::new(1.5_f32, Color32::WHITE),
            corner_radius: corner_btn,
            expansion: 0.0,
        },
        open: WidgetVisuals {
            weak_bg_fill: CARD_SURFACE_HOVER,
            bg_fill: CARD_SURFACE_HOVER,
            bg_stroke: Stroke::new(1.0_f32, accent_sec),
            fg_stroke: Stroke::new(1.0_f32, text_mut),
            corner_radius: corner_btn,
            expansion: 0.0,
        },
    };

    visuals
}

/// Builds dynamic [`Style`] from the active [`ThemeConfig`].
pub fn build_style_from_config(cfg: &ThemeConfig) -> Style {
    let mut style = Style {
        visuals: build_visuals_from_config(cfg),
        ..Style::default()
    };

    let (item_sp, btn_pad) = match cfg.density {
        UiDensity::Compact => (Vec2::new(6.0, 6.0), Vec2::new(10.0, 5.0)),
        UiDensity::Comfortable => (Vec2::new(10.0, 10.0), Vec2::new(14.0, 8.0)),
        UiDensity::Spacious => (Vec2::new(14.0, 14.0), Vec2::new(18.0, 11.0)),
    };

    style.spacing.item_spacing = item_sp;
    style.spacing.window_margin = WINDOW_MARGIN;
    style.spacing.button_padding = btn_pad;
    style.spacing.interact_size = egui::vec2(40.0, 32.0);
    style.spacing.indent = 16.0;

    style
}

/// Applies dynamic theme configuration to [`egui::Context`].
pub fn apply_theme(ctx: &Context, cfg: &ThemeConfig) {
    let style = build_style_from_config(cfg);
    ctx.set_visuals(style.visuals.clone());
    ctx.set_style(style);
    ctx.set_zoom_factor(cfg.ui_scale.clamp(0.8, 1.5));
}

/// Dynamic Liquid Glass card frame based on active theme configuration.
pub fn custom_glass_card_frame(cfg: &ThemeConfig) -> Frame {
    let fill = rgb_to_glass_c32(cfg.bg_card, cfg.card_opacity);
    let glow = (25.0 * cfg.border_glow.clamp(0.0, 1.0)).round() as u8;
    let stroke_col = if glow > 0 {
        Color32::from_rgba_unmultiplied(255, 255, 255, glow.max(20))
    } else {
        BORDER_SUBTLE
    };

    Frame::new()
        .fill(fill)
        .stroke(Stroke::new(1.0_f32, stroke_col))
        .corner_radius(CornerRadius::same(cfg.corner_radius.round() as u8))
        .inner_margin(Margin::same(14))
}

/// Left navigation sidebar frame based on active theme configuration.
pub fn custom_sidebar_frame(cfg: &ThemeConfig) -> Frame {
    let fill = rgb_to_glass_c32(cfg.bg_card, (cfg.card_opacity * 0.95).clamp(0.3, 1.0));
    Frame::new()
        .fill(fill)
        .stroke(Stroke::new(1.0_f32, BORDER_SUBTLE))
        .inner_margin(Margin::symmetric(12, 14))
}

/// Bottom status HUD frame based on active theme configuration.
pub fn custom_hud_frame(cfg: &ThemeConfig) -> Frame {
    let fill = rgb_to_glass_c32(cfg.bg_card, (cfg.card_opacity * 0.95).clamp(0.3, 1.0));
    Frame::new()
        .fill(fill)
        .stroke(Stroke::new(1.0_f32, BORDER_SUBTLE))
        .inner_margin(Margin::symmetric(14, 6))
}

/// Standard Dark Glass visuals.
pub fn build_dark_glass_visuals() -> Visuals {
    build_visuals_from_config(&ThemeConfig::preset_liquid_glass())
}

/// Standard Dark Glass style.
pub fn build_dark_glass_style() -> Style {
    build_style_from_config(&ThemeConfig::preset_liquid_glass())
}

/// Legacy helper for default dark glass theme.
pub fn apply_dark_glass_theme(ctx: &Context) {
    apply_theme(ctx, &ThemeConfig::preset_liquid_glass());
}

pub fn glass_card_frame() -> Frame {
    custom_glass_card_frame(&ThemeConfig::preset_liquid_glass())
}

pub fn glass_card_hover_frame() -> Frame {
    Frame::new()
        .fill(CARD_SURFACE_HOVER)
        .stroke(Stroke::new(1.0_f32, BORDER_HOVER))
        .corner_radius(RADIUS_CARD)
        .inner_margin(Margin::same(14))
}

pub fn sidebar_frame() -> Frame {
    custom_sidebar_frame(&ThemeConfig::preset_liquid_glass())
}

pub fn hud_frame() -> Frame {
    custom_hud_frame(&ThemeConfig::preset_liquid_glass())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dark_glass_style_matches_exact_color_rounding_and_spacing_spec() {
        let style = build_dark_glass_style();

        assert_eq!(BG_DEEP_GRAPHITE, Color32::from_rgb(0x0D, 0x0F, 0x14));
        assert_eq!(CARD_SURFACE, Color32::from_rgb(0x15, 0x18, 0x21));
        assert_eq!(BORDER_SUBTLE, Color32::from_rgb(0x25, 0x2A, 0x36));
        assert_eq!(ACCENT_EMERALD, Color32::from_rgb(0x10, 0xB9, 0x81));
        assert_eq!(ACCENT_COBALT, Color32::from_rgb(0x3B, 0x82, 0xF6));

        assert_eq!(RADIUS_CARD, CornerRadius::same(8));
        assert_eq!(RADIUS_BUTTON, CornerRadius::same(6));
        assert_eq!(RADIUS_TAG, CornerRadius::same(4));
        assert_eq!(style.spacing.item_spacing, egui::vec2(10.0, 10.0));
    }
}
