//! Matte Dark solid design system tokens and theme engine.
//! Zero transparency, pure solid surfaces, 4/8px spatial rhythm, 8px card radius, 6px button radius.
//! Strictly enforces Section 2 rules: no blurs, no glassy sheens, no glow borders, contrast >= 4.5:1.

use egui::style::{Selection, WidgetVisuals, Widgets};
use egui::{Color32, Context, CornerRadius, Frame, Margin, Stroke, Style, Vec2, Visuals};
use mq_core::{ThemeConfig, ThemePreset, UiDensity};

/// Comprehensive opaque design tokens for MQ v2.1.0.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Tokens {
    /// Window background (#0F1115)
    pub bg_app: Color32,
    /// Sidebar & top navigation surface (#14171D)
    pub bg_sidebar: Color32,
    /// Cards, panels, elevated surfaces (#1A1E26)
    pub bg_surface: Color32,
    /// Surface hover state (#212631)
    pub bg_surface_hover: Color32,
    /// Text input and log viewer backgrounds (#12151B)
    pub bg_input: Color32,
    /// Structural 1px border (#272C37)
    pub border: Color32,
    /// Hover / focus 1px border (#363D4C)
    pub border_strong: Color32,
    /// Primary high-contrast text (#E6E8EE)
    pub text: Color32,
    /// Secondary muted text (#9AA1B2)
    pub text_muted: Color32,
    /// Disabled control text (#5B6274)
    pub text_disabled: Color32,
    /// Primary singular accent (#3DDC97)
    pub accent: Color32,
    /// Accent hover state (#34C386)
    pub accent_hover: Color32,
    /// Text on accent backgrounds (#06110B)
    pub accent_text: Color32,
    /// Danger / error status (#E5484D)
    pub danger: Color32,
    /// Warning status (#F5A524)
    pub warning: Color32,
    /// Success status (#3DDC97)
    pub success: Color32,
}

impl Default for Tokens {
    fn default() -> Self {
        Self::matte_dark()
    }
}

impl Tokens {
    /// Matte Dark (Default) — deep obsidian graphite with mint emerald accent.
    pub const fn matte_dark() -> Self {
        Self {
            bg_app: Color32::from_rgb(0x0F, 0x11, 0x15),
            bg_sidebar: Color32::from_rgb(0x14, 0x17, 0x1D),
            bg_surface: Color32::from_rgb(0x1A, 0x1E, 0x26),
            bg_surface_hover: Color32::from_rgb(0x21, 0x26, 0x31),
            bg_input: Color32::from_rgb(0x12, 0x15, 0x1B),
            border: Color32::from_rgb(0x27, 0x2C, 0x37),
            border_strong: Color32::from_rgb(0x36, 0x3D, 0x4C),
            text: Color32::from_rgb(0xE6, 0xE8, 0xEE),
            text_muted: Color32::from_rgb(0x9A, 0xA1, 0xB2),
            text_disabled: Color32::from_rgb(0x5B, 0x62, 0x74),
            accent: Color32::from_rgb(0x3D, 0xDC, 0x97),
            accent_hover: Color32::from_rgb(0x34, 0xC3, 0x86),
            accent_text: Color32::from_rgb(0x06, 0x11, 0x0B),
            danger: Color32::from_rgb(0xE5, 0x48, 0x4D),
            warning: Color32::from_rgb(0xF5, 0xA5, 0x24),
            success: Color32::from_rgb(0x3D, 0xDC, 0x97),
        }
    }

    /// Prism Classic Dark — dark neutral slate with classic blue accent.
    pub const fn prism_classic_dark() -> Self {
        Self {
            bg_app: Color32::from_rgb(0x19, 0x19, 0x19),
            bg_sidebar: Color32::from_rgb(0x1F, 0x1F, 0x1F),
            bg_surface: Color32::from_rgb(0x24, 0x24, 0x24),
            bg_surface_hover: Color32::from_rgb(0x2E, 0x2E, 0x2E),
            bg_input: Color32::from_rgb(0x14, 0x14, 0x14),
            border: Color32::from_rgb(0x33, 0x33, 0x33),
            border_strong: Color32::from_rgb(0x44, 0x44, 0x44),
            text: Color32::from_rgb(0xEA, 0xEA, 0xEA),
            text_muted: Color32::from_rgb(0xA0, 0xA0, 0xA0),
            text_disabled: Color32::from_rgb(0x60, 0x60, 0x60),
            accent: Color32::from_rgb(0x5B, 0x8D, 0xEF),
            accent_hover: Color32::from_rgb(0x4A, 0x7C, 0xD8),
            accent_text: Color32::from_rgb(0xFF, 0xFF, 0xFF),
            danger: Color32::from_rgb(0xE5, 0x48, 0x4D),
            warning: Color32::from_rgb(0xF5, 0xA5, 0x24),
            success: Color32::from_rgb(0x3D, 0xDC, 0x97),
        }
    }

    /// Modrinth Emerald — dark carbon with vivid emerald accent.
    pub const fn modrinth_emerald() -> Self {
        Self {
            bg_app: Color32::from_rgb(0x11, 0x12, 0x15),
            bg_sidebar: Color32::from_rgb(0x16, 0x18, 0x1D),
            bg_surface: Color32::from_rgb(0x1C, 0x1F, 0x26),
            bg_surface_hover: Color32::from_rgb(0x23, 0x27, 0x31),
            bg_input: Color32::from_rgb(0x0D, 0x0E, 0x11),
            border: Color32::from_rgb(0x2A, 0x2E, 0x39),
            border_strong: Color32::from_rgb(0x39, 0x3F, 0x4E),
            text: Color32::from_rgb(0xED, 0xED, 0xED),
            text_muted: Color32::from_rgb(0x9D, 0xA3, 0xAF),
            text_disabled: Color32::from_rgb(0x5E, 0x64, 0x72),
            accent: Color32::from_rgb(0x1B, 0xD9, 0x6A),
            accent_hover: Color32::from_rgb(0x17, 0xBF, 0x5D),
            accent_text: Color32::from_rgb(0x08, 0x1C, 0x0F),
            danger: Color32::from_rgb(0xE5, 0x48, 0x4D),
            warning: Color32::from_rgb(0xF5, 0xA5, 0x24),
            success: Color32::from_rgb(0x1B, 0xD9, 0x6A),
        }
    }

    /// Catppuccin Mocha — cozy pastel dark palette.
    pub const fn catppuccin_mocha() -> Self {
        Self {
            bg_app: Color32::from_rgb(0x1E, 0x1E, 0x2E),
            bg_sidebar: Color32::from_rgb(0x18, 0x18, 0x25),
            bg_surface: Color32::from_rgb(0x31, 0x32, 0x44),
            bg_surface_hover: Color32::from_rgb(0x45, 0x47, 0x5A),
            bg_input: Color32::from_rgb(0x11, 0x11, 0x1B),
            border: Color32::from_rgb(0x45, 0x47, 0x5A),
            border_strong: Color32::from_rgb(0x58, 0x5B, 0x70),
            text: Color32::from_rgb(0xCD, 0xD6, 0xF4),
            text_muted: Color32::from_rgb(0xA6, 0xAD, 0xC8),
            text_disabled: Color32::from_rgb(0x6C, 0x70, 0x86),
            accent: Color32::from_rgb(0x89, 0xB4, 0xFA),
            accent_hover: Color32::from_rgb(0x74, 0xC7, 0xEC),
            accent_text: Color32::from_rgb(0x11, 0x11, 0x1B),
            danger: Color32::from_rgb(0xF3, 0x8B, 0xA8),
            warning: Color32::from_rgb(0xFA, 0xB3, 0x87),
            success: Color32::from_rgb(0xA6, 0xE3, 0xA1),
        }
    }

    /// Nord — arctic cool north palette.
    pub const fn nord() -> Self {
        Self {
            bg_app: Color32::from_rgb(0x24, 0x29, 0x33),
            bg_sidebar: Color32::from_rgb(0x2E, 0x34, 0x40),
            bg_surface: Color32::from_rgb(0x3B, 0x42, 0x52),
            bg_surface_hover: Color32::from_rgb(0x43, 0x4C, 0x5E),
            bg_input: Color32::from_rgb(0x1D, 0x21, 0x2A),
            border: Color32::from_rgb(0x4C, 0x56, 0x6A),
            border_strong: Color32::from_rgb(0x5E, 0x81, 0xAC),
            text: Color32::from_rgb(0xEC, 0xEF, 0xF4),
            text_muted: Color32::from_rgb(0xD8, 0xDE, 0xE9),
            text_disabled: Color32::from_rgb(0x7B, 0x88, 0xA1),
            accent: Color32::from_rgb(0x88, 0xC0, 0xD0),
            accent_hover: Color32::from_rgb(0x81, 0xA1, 0xC1),
            accent_text: Color32::from_rgb(0x2E, 0x34, 0x40),
            danger: Color32::from_rgb(0xBF, 0x61, 0x6A),
            warning: Color32::from_rgb(0xEB, 0xCB, 0x8B),
            success: Color32::from_rgb(0xA3, 0xBE, 0x8C),
        }
    }

    /// Light Clean — high-contrast clean daylight theme.
    pub const fn light_clean() -> Self {
        Self {
            bg_app: Color32::from_rgb(0xF4, 0xF5, 0xF7),
            bg_sidebar: Color32::from_rgb(0xEB, 0xED, 0xF0),
            bg_surface: Color32::from_rgb(0xFF, 0xFF, 0xFF),
            bg_surface_hover: Color32::from_rgb(0xF0, 0xF2, 0xF5),
            bg_input: Color32::from_rgb(0xFF, 0xFF, 0xFF),
            border: Color32::from_rgb(0xD1, 0xD5, 0xDB),
            border_strong: Color32::from_rgb(0x9C, 0xA3, 0xAF),
            text: Color32::from_rgb(0x11, 0x18, 0x27),
            text_muted: Color32::from_rgb(0x4B, 0x55, 0x63),
            text_disabled: Color32::from_rgb(0x9C, 0xA3, 0xAF),
            accent: Color32::from_rgb(0x05, 0x96, 0x69),
            accent_hover: Color32::from_rgb(0x04, 0x78, 0x57),
            accent_text: Color32::from_rgb(0xFF, 0xFF, 0xFF),
            danger: Color32::from_rgb(0xDC, 0x26, 0x26),
            warning: Color32::from_rgb(0xD9, 0x77, 0x06),
            success: Color32::from_rgb(0x05, 0x96, 0x69),
        }
    }

    /// Resolves `Tokens` for a given preset.
    pub fn from_preset(preset: ThemePreset) -> Self {
        match preset {
            ThemePreset::MatteDark => Self::matte_dark(),
            ThemePreset::PrismClassicDark => Self::prism_classic_dark(),
            ThemePreset::ModrinthEmerald => Self::modrinth_emerald(),
            ThemePreset::CatppuccinMocha => Self::catppuccin_mocha(),
            ThemePreset::Nord => Self::nord(),
            ThemePreset::LightClean => Self::light_clean(),
            ThemePreset::Custom => Self::matte_dark(),
        }
    }

    /// Resolves `Tokens` from an active [`ThemeConfig`].
    pub fn from_config(cfg: &ThemeConfig) -> Self {
        Self::from_preset(cfg.preset)
    }
}

// Global tokens constants for Matte Dark (backward-compatible and direct token access)
pub const BG_APP: Color32 = Color32::from_rgb(0x0F, 0x11, 0x15);
pub const BG_SIDEBAR: Color32 = Color32::from_rgb(0x14, 0x17, 0x1D);
pub const BG_SURFACE: Color32 = Color32::from_rgb(0x1A, 0x1E, 0x26);
pub const BG_SURFACE_HOVER: Color32 = Color32::from_rgb(0x21, 0x26, 0x31);
pub const BG_INPUT: Color32 = Color32::from_rgb(0x12, 0x15, 0x1B);
pub const BORDER: Color32 = Color32::from_rgb(0x27, 0x2C, 0x37);
pub const BORDER_STRONG: Color32 = Color32::from_rgb(0x36, 0x3D, 0x4C);
pub const TEXT_PRIMARY: Color32 = Color32::from_rgb(0xE6, 0xE8, 0xEE);
pub const TEXT_MUTED: Color32 = Color32::from_rgb(0x9A, 0xA1, 0xB2);
pub const TEXT_DISABLED: Color32 = Color32::from_rgb(0x5B, 0x62, 0x74);
pub const ACCENT: Color32 = Color32::from_rgb(0x3D, 0xDC, 0x97);
pub const ACCENT_HOVER: Color32 = Color32::from_rgb(0x34, 0xC3, 0x86);
pub const ACCENT_TEXT: Color32 = Color32::from_rgb(0x06, 0x11, 0x0B);
pub const STATUS_DANGER: Color32 = Color32::from_rgb(0xE5, 0x48, 0x4D);
pub const STATUS_WARN: Color32 = Color32::from_rgb(0xF5, 0xA5, 0x24);
pub const STATUS_SUCCESS: Color32 = Color32::from_rgb(0x3D, 0xDC, 0x97);

// Legacy aliases mapped to solid tokens
pub const BG_DEEP_GRAPHITE: Color32 = BG_APP;
pub const PANEL_DARK_GLASS: Color32 = BG_SIDEBAR;
pub const CARD_SURFACE: Color32 = BG_SURFACE;
pub const CARD_SURFACE_HOVER: Color32 = BG_SURFACE_HOVER;
pub const BG_RECESSED: Color32 = BG_INPUT;
pub const BORDER_SUBTLE: Color32 = BORDER;
pub const BORDER_HOVER: Color32 = BORDER_STRONG;
pub const ACCENT_EMERALD: Color32 = ACCENT;
pub const ACCENT_EMERALD_HOVER: Color32 = ACCENT_HOVER;
pub const ACCENT_COBALT: Color32 = Color32::from_rgb(0x5B, 0x8D, 0xEF);

// 4/8 px Spacing and radius rules (Section 2.2)
pub const RADIUS_CARD: CornerRadius = CornerRadius::same(8);
pub const RADIUS_BUTTON: CornerRadius = CornerRadius::same(6);
pub const RADIUS_TAG: CornerRadius = CornerRadius::same(4);

pub const ITEM_SPACING: Vec2 = egui::vec2(8.0, 8.0);
pub const WINDOW_MARGIN: Margin = Margin::same(16);
pub const BUTTON_PADDING: Vec2 = egui::vec2(12.0, 6.0);

/// Builds fully opaque [`Visuals`] for the active theme configuration.
pub fn build_visuals_from_config(cfg: &ThemeConfig) -> Visuals {
    let tokens = Tokens::from_config(cfg);
    let mut visuals = if cfg.preset == ThemePreset::LightClean {
        Visuals::light()
    } else {
        Visuals::dark()
    };

    visuals.dark_mode = cfg.preset != ThemePreset::LightClean;
    visuals.override_text_color = Some(tokens.text);
    visuals.panel_fill = tokens.bg_app;
    visuals.window_fill = tokens.bg_surface;
    visuals.faint_bg_color = tokens.bg_surface;
    visuals.extreme_bg_color = tokens.bg_input;
    visuals.code_bg_color = tokens.bg_input;

    visuals.window_corner_radius = RADIUS_CARD;
    visuals.menu_corner_radius = RADIUS_CARD;
    visuals.window_stroke = Stroke::new(1.0_f32, tokens.border);

    visuals.hyperlink_color = tokens.accent;
    visuals.warn_fg_color = tokens.warning;
    visuals.error_fg_color = tokens.danger;

    visuals.selection = Selection {
        bg_fill: tokens.accent,
        stroke: Stroke::new(1.0_f32, tokens.accent_hover),
    };

    visuals.widgets = Widgets {
        noninteractive: WidgetVisuals {
            weak_bg_fill: tokens.bg_surface,
            bg_fill: tokens.bg_surface,
            bg_stroke: Stroke::new(1.0_f32, tokens.border),
            fg_stroke: Stroke::new(1.0_f32, tokens.text),
            corner_radius: RADIUS_CARD,
            expansion: 0.0,
        },
        inactive: WidgetVisuals {
            weak_bg_fill: tokens.bg_surface,
            bg_fill: tokens.bg_surface,
            bg_stroke: Stroke::new(1.0_f32, tokens.border),
            fg_stroke: Stroke::new(1.0_f32, tokens.text),
            corner_radius: RADIUS_BUTTON,
            expansion: 0.0,
        },
        hovered: WidgetVisuals {
            weak_bg_fill: tokens.bg_surface_hover,
            bg_fill: tokens.bg_surface_hover,
            bg_stroke: Stroke::new(1.0_f32, tokens.border_strong),
            fg_stroke: Stroke::new(1.5_f32, tokens.text),
            corner_radius: RADIUS_BUTTON,
            expansion: 0.0,
        },
        active: WidgetVisuals {
            weak_bg_fill: tokens.bg_surface_hover,
            bg_fill: tokens.bg_surface_hover,
            bg_stroke: Stroke::new(1.5_f32, tokens.accent),
            fg_stroke: Stroke::new(1.5_f32, tokens.accent),
            corner_radius: RADIUS_BUTTON,
            expansion: 0.0,
        },
        open: WidgetVisuals {
            weak_bg_fill: tokens.bg_surface_hover,
            bg_fill: tokens.bg_surface_hover,
            bg_stroke: Stroke::new(1.0_f32, tokens.border_strong),
            fg_stroke: Stroke::new(1.0_f32, tokens.text_muted),
            corner_radius: RADIUS_BUTTON,
            expansion: 0.0,
        },
    };

    visuals
}

/// Builds dynamic [`Style`] for the active theme configuration.
pub fn build_style_from_config(cfg: &ThemeConfig) -> Style {
    let mut style = Style {
        visuals: build_visuals_from_config(cfg),
        ..Style::default()
    };

    let (item_sp, btn_pad) = match cfg.density {
        UiDensity::Compact => (Vec2::new(6.0, 6.0), Vec2::new(10.0, 4.0)),
        UiDensity::Comfortable => (Vec2::new(8.0, 8.0), Vec2::new(12.0, 6.0)),
        UiDensity::Spacious => (Vec2::new(12.0, 12.0), Vec2::new(16.0, 8.0)),
    };

    style.spacing.item_spacing = item_sp;
    style.spacing.window_margin = WINDOW_MARGIN;
    style.spacing.button_padding = btn_pad;
    style.spacing.interact_size = egui::vec2(36.0, 32.0);
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

/// Solid opaque card frame (8px radius, 1px border, surface background).
pub fn solid_card_frame(tokens: &Tokens) -> Frame {
    Frame::new()
        .fill(tokens.bg_surface)
        .stroke(Stroke::new(1.0_f32, tokens.border))
        .corner_radius(RADIUS_CARD)
        .inner_margin(Margin::same(12))
}

#[inline]
pub fn rgb_to_c32(rgb: [u8; 3]) -> Color32 {
    Color32::from_rgb(rgb[0], rgb[1], rgb[2])
}

/// Solid opaque sidebar frame (surface background, 1px right border).
pub fn solid_sidebar_frame(tokens: &Tokens) -> Frame {
    Frame::new()
        .fill(tokens.bg_sidebar)
        .stroke(Stroke::new(1.0_f32, tokens.border))
        .inner_margin(Margin::same(12))
}

/// Modal dialog panel frame (8px radius, border_strong).
pub fn solid_modal_frame(tokens: &Tokens) -> Frame {
    Frame::new()
        .fill(tokens.bg_surface)
        .stroke(Stroke::new(1.0_f32, tokens.border_strong))
        .corner_radius(RADIUS_CARD)
        .inner_margin(Margin::same(20))
}

// Aliases for legacy call sites
#[inline]
pub fn custom_glass_card_frame(cfg: &ThemeConfig) -> Frame {
    solid_card_frame(&Tokens::from_config(cfg))
}

#[inline]
pub fn custom_sidebar_frame(cfg: &ThemeConfig) -> Frame {
    solid_sidebar_frame(&Tokens::from_config(cfg))
}

#[inline]
pub fn sidebar_frame() -> Frame {
    Frame::new()
        .fill(BG_SIDEBAR)
        .stroke(Stroke::new(1.0_f32, BORDER))
        .inner_margin(Margin::same(12))
}

#[inline]
pub fn hud_frame() -> Frame {
    Frame::new()
        .fill(BG_SIDEBAR)
        .stroke(Stroke::new(1.0_f32, BORDER))
        .inner_margin(Margin::symmetric(12, 4))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn theme_tokens_match_matte_dark_specification() {
        let t = Tokens::matte_dark();
        assert_eq!(t.bg_app, Color32::from_rgb(0x0F, 0x11, 0x15));
        assert_eq!(t.bg_sidebar, Color32::from_rgb(0x14, 0x17, 0x1D));
        assert_eq!(t.bg_surface, Color32::from_rgb(0x1A, 0x1E, 0x26));
        assert_eq!(t.bg_surface_hover, Color32::from_rgb(0x21, 0x26, 0x31));
        assert_eq!(t.bg_input, Color32::from_rgb(0x12, 0x15, 0x1B));
        assert_eq!(t.border, Color32::from_rgb(0x27, 0x2C, 0x37));
        assert_eq!(t.border_strong, Color32::from_rgb(0x36, 0x3D, 0x4C));
        assert_eq!(t.text, Color32::from_rgb(0xE6, 0xE8, 0xEE));
        assert_eq!(t.text_muted, Color32::from_rgb(0x9A, 0xA1, 0xB2));
        assert_eq!(t.text_disabled, Color32::from_rgb(0x5B, 0x62, 0x74));
        assert_eq!(t.accent, Color32::from_rgb(0x3D, 0xDC, 0x97));
        assert_eq!(t.accent_hover, Color32::from_rgb(0x34, 0xC3, 0x86));
        assert_eq!(t.accent_text, Color32::from_rgb(0x06, 0x11, 0x0B));
        assert_eq!(t.danger, Color32::from_rgb(0xE5, 0x48, 0x4D));
        assert_eq!(t.warning, Color32::from_rgb(0xF5, 0xA5, 0x24));
        assert_eq!(t.success, Color32::from_rgb(0x3D, 0xDC, 0x97));

        // All colors strictly opaque (alpha == 255)
        assert_eq!(t.bg_app.a(), 255);
        assert_eq!(t.bg_sidebar.a(), 255);
        assert_eq!(t.bg_surface.a(), 255);
        assert_eq!(t.border.a(), 255);
        assert_eq!(t.accent.a(), 255);
    }

    #[test]
    fn six_opaque_presets_exist_and_resolve() {
        for preset in ThemePreset::ALL {
            let tokens = Tokens::from_preset(preset);
            assert_eq!(tokens.bg_app.a(), 255);
            assert_eq!(tokens.bg_surface.a(), 255);
            assert_eq!(tokens.border.a(), 255);
            assert_eq!(tokens.accent.a(), 255);
        }
    }
}
