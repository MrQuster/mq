//! Custom Dark Glass visual theme (`#0D0F14` deep graphite background, `#151821` elevated
//! cards, `#10B981` emerald primary accent, `#3B82F6` cobalt secondary accent) and `egui::Style`
//! configuration.

use egui::style::{Selection, WidgetVisuals, Widgets};
use egui::{Color32, Context, CornerRadius, Frame, Margin, Stroke, Style, Vec2, Visuals};

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

/// Builds the custom Dark Glass [`Visuals`] palette.
pub fn build_dark_glass_visuals() -> Visuals {
    let mut visuals = Visuals::dark();

    visuals.dark_mode = true;
    visuals.override_text_color = Some(TEXT_PRIMARY);
    visuals.panel_fill = BG_DEEP_GRAPHITE;
    visuals.window_fill = CARD_SURFACE;
    visuals.faint_bg_color = CARD_SURFACE;
    visuals.extreme_bg_color = BG_RECESSED;
    visuals.code_bg_color = BG_RECESSED;

    visuals.window_corner_radius = RADIUS_CARD;
    visuals.menu_corner_radius = RADIUS_CARD;
    visuals.window_stroke = Stroke::new(1.0_f32, BORDER_SUBTLE);

    visuals.hyperlink_color = ACCENT_COBALT;
    visuals.warn_fg_color = STATUS_WARN;
    visuals.error_fg_color = STATUS_DANGER;

    visuals.selection = Selection {
        bg_fill: ACCENT_EMERALD,
        stroke: Stroke::new(1.0_f32, BG_DEEP_GRAPHITE),
    };

    visuals.widgets = Widgets {
        noninteractive: WidgetVisuals {
            weak_bg_fill: CARD_SURFACE,
            bg_fill: CARD_SURFACE,
            bg_stroke: Stroke::new(1.0_f32, BORDER_SUBTLE),
            fg_stroke: Stroke::new(1.0_f32, TEXT_PRIMARY),
            corner_radius: RADIUS_CARD,
            expansion: 0.0,
        },
        inactive: WidgetVisuals {
            weak_bg_fill: CARD_SURFACE,
            bg_fill: CARD_SURFACE,
            bg_stroke: Stroke::new(1.0_f32, BORDER_SUBTLE),
            fg_stroke: Stroke::new(1.0_f32, TEXT_PRIMARY),
            corner_radius: RADIUS_BUTTON,
            expansion: 0.0,
        },
        hovered: WidgetVisuals {
            weak_bg_fill: CARD_SURFACE_HOVER,
            bg_fill: CARD_SURFACE_HOVER,
            bg_stroke: Stroke::new(1.0_f32, BORDER_HOVER),
            fg_stroke: Stroke::new(1.5_f32, Color32::WHITE),
            corner_radius: RADIUS_BUTTON,
            expansion: 0.0,
        },
        active: WidgetVisuals {
            weak_bg_fill: CARD_SURFACE_HOVER,
            bg_fill: CARD_SURFACE_HOVER,
            bg_stroke: Stroke::new(1.0_f32, ACCENT_EMERALD),
            fg_stroke: Stroke::new(1.5_f32, Color32::WHITE),
            corner_radius: RADIUS_BUTTON,
            expansion: 0.0,
        },
        open: WidgetVisuals {
            weak_bg_fill: CARD_SURFACE_HOVER,
            bg_fill: CARD_SURFACE_HOVER,
            bg_stroke: Stroke::new(1.0_f32, ACCENT_COBALT),
            fg_stroke: Stroke::new(1.0_f32, TEXT_PRIMARY),
            corner_radius: RADIUS_BUTTON,
            expansion: 0.0,
        },
    };

    visuals
}

/// Builds the complete Dark Glass [`Style`] including spacing, margins, and visuals.
pub fn build_dark_glass_style() -> Style {
    let mut style = Style {
        visuals: build_dark_glass_visuals(),
        ..Style::default()
    };

    style.spacing.item_spacing = ITEM_SPACING;
    style.spacing.window_margin = WINDOW_MARGIN;
    style.spacing.button_padding = BUTTON_PADDING;
    style.spacing.interact_size = egui::vec2(40.0, 32.0);
    style.spacing.indent = 16.0;

    style
}

/// Applies the custom Dark Glass theme to an active [`egui::Context`].
pub fn apply_dark_glass_theme(ctx: &Context) {
    let style = build_dark_glass_style();
    ctx.set_visuals(style.visuals.clone());
    ctx.set_style(style);
}

/// Standard Dark Glass card frame (`#151821` translucent fill, 1px `#252A36` border, 8px rounding).
pub fn glass_card_frame() -> Frame {
    Frame::new()
        .fill(PANEL_DARK_GLASS)
        .stroke(Stroke::new(1.0_f32, BORDER_SUBTLE))
        .corner_radius(RADIUS_CARD)
        .inner_margin(Margin::same(14))
}

/// Hovered or focused Dark Glass card frame (`#1C202C` fill, 1px `#384152` border, 8px rounding).
pub fn glass_card_hover_frame() -> Frame {
    Frame::new()
        .fill(CARD_SURFACE_HOVER)
        .stroke(Stroke::new(1.0_f32, BORDER_HOVER))
        .corner_radius(RADIUS_CARD)
        .inner_margin(Margin::same(14))
}

/// Left navigation sidebar container frame.
pub fn sidebar_frame() -> Frame {
    Frame::new()
        .fill(PANEL_DARK_GLASS)
        .stroke(Stroke::new(1.0_f32, BORDER_SUBTLE))
        .inner_margin(Margin::symmetric(12, 14))
}

/// Bottom status & CPU telemetry HUD bar frame.
pub fn hud_frame() -> Frame {
    Frame::new()
        .fill(PANEL_DARK_GLASS)
        .stroke(Stroke::new(1.0_f32, BORDER_SUBTLE))
        .inner_margin(Margin::symmetric(14, 6))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dark_glass_style_matches_exact_color_rounding_and_spacing_spec() {
        let style = build_dark_glass_style();

        assert_eq!(
            BG_DEEP_GRAPHITE,
            Color32::from_rgb(0x0D, 0x0F, 0x14),
            "background must be #0D0F14"
        );
        assert_eq!(
            CARD_SURFACE,
            Color32::from_rgb(0x15, 0x18, 0x21),
            "card surface must be #151821"
        );
        assert_eq!(
            BORDER_SUBTLE,
            Color32::from_rgb(0x25, 0x2A, 0x36),
            "subtle border must be #252A36"
        );
        assert_eq!(
            BORDER_HOVER,
            Color32::from_rgb(0x38, 0x41, 0x52),
            "hover border must be #384152"
        );
        assert_eq!(
            ACCENT_EMERALD,
            Color32::from_rgb(0x10, 0xB9, 0x81),
            "primary emerald must be #10B981"
        );
        assert_eq!(
            ACCENT_COBALT,
            Color32::from_rgb(0x3B, 0x82, 0xF6),
            "secondary cobalt must be #3B82F6"
        );

        assert_eq!(style.visuals.panel_fill, BG_DEEP_GRAPHITE);
        assert_eq!(style.visuals.window_fill, CARD_SURFACE);
        assert_eq!(style.visuals.faint_bg_color, CARD_SURFACE);
        assert_eq!(style.visuals.selection.bg_fill, ACCENT_EMERALD);

        assert_eq!(RADIUS_CARD, CornerRadius::same(8));
        assert_eq!(RADIUS_BUTTON, CornerRadius::same(6));
        assert_eq!(RADIUS_TAG, CornerRadius::same(4));
        assert_eq!(
            style.visuals.widgets.noninteractive.corner_radius,
            RADIUS_CARD
        );
        assert_eq!(style.visuals.widgets.inactive.corner_radius, RADIUS_BUTTON);

        assert_eq!(style.spacing.item_spacing, egui::vec2(10.0, 10.0));
        assert_eq!(style.spacing.window_margin, Margin::same(16));
        assert_eq!(style.spacing.button_padding, egui::vec2(14.0, 8.0));
    }
}
