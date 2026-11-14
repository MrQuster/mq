//! Matte Dark solid design system tokens and theme engine.
//! Zero transparency, pure solid surfaces, 4/8px spatial rhythm, 8px card radius, 6px button radius.
//! Strictly enforces Section 2 rules: no blurs, no glassy sheens, no glow borders, contrast >= 4.5:1.

use std::fs;
use std::path::Path;

use egui::epaint::{Mesh, Vertex};
use egui::style::{Selection, WidgetVisuals, Widgets};
use egui::{
    Color32, ColorImage, Context, CornerRadius, FontId, Frame, Id, Margin, Painter, Pos2, Rect,
    Stroke, Style, TextStyle, TextureHandle, TextureOptions, Vec2, Visuals,
};
use mq_core::{BackgroundStyle, ThemeConfig, ThemePreset, UiDensity};

/// Curated calm accent swatch preset for customization controls.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AccentPreset {
    pub name: &'static str,
    pub primary: [u8; 3],
    pub secondary: [u8; 3],
}

/// Calm, balanced accent presets that avoid harsh neon saturation.
pub const CALM_ACCENT_PRESETS: [AccentPreset; 6] = [
    AccentPreset {
        name: "Шалфей",
        primary: [0x4C, 0x9A, 0x82],
        secondary: [0x3D, 0x83, 0x6D],
    },
    AccentPreset {
        name: "Грифельно-синий",
        primary: [0x5B, 0x8D, 0xEF],
        secondary: [0x46, 0x74, 0xD0],
    },
    AccentPreset {
        name: "Нордический льдистый",
        primary: [0x6E, 0xA8, 0xB8],
        secondary: [0x56, 0x8E, 0x9E],
    },
    AccentPreset {
        name: "Янтарь",
        primary: [0xC9, 0x8A, 0x4B],
        secondary: [0xAA, 0x71, 0x3A],
    },
    AccentPreset {
        name: "Лавандовый графит",
        primary: [0x8B, 0x86, 0xC6],
        secondary: [0x71, 0x6C, 0xAA],
    },
    AccentPreset {
        name: "Терракота",
        primary: [0xC2, 0x6D, 0x68],
        secondary: [0xA4, 0x56, 0x52],
    },
];

/// Linearly blends two opaque RGB colors (`t = 0.0` returns `a`, `t = 1.0` returns `b`).
#[inline]
pub fn blend_color(a: Color32, b: Color32, t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    let inv = 1.0 - t;
    Color32::from_rgb(
        ((a.r() as f32) * inv + (b.r() as f32) * t).round() as u8,
        ((a.g() as f32) * inv + (b.g() as f32) * t).round() as u8,
        ((a.b() as f32) * inv + (b.b() as f32) * t).round() as u8,
    )
}

/// Computes W3C relative luminance (`0.0..=1.0`) for an sRGB color.
#[inline]
pub fn relative_luminance(c: Color32) -> f32 {
    fn channel(v: u8) -> f32 {
        let s = (v as f32) / 255.0;
        if s <= 0.04045 {
            s / 12.92
        } else {
            ((s + 0.055) / 1.055).powf(2.4)
        }
    }
    0.2126 * channel(c.r()) + 0.7152 * channel(c.g()) + 0.0722 * channel(c.b())
}

/// Picks a high-contrast foreground color (dark charcoal or crisp off-white) on `bg`.
#[inline]
pub fn contrast_text_for(bg: Color32) -> Color32 {
    if relative_luminance(bg) > 0.24 {
        Color32::from_rgb(0x08, 0x13, 0x10)
    } else {
        Color32::from_rgb(0xF6, 0xF8, 0xFA)
    }
}

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
    /// Primary singular accent (#4C9A82)
    pub accent: Color32,
    /// Accent hover state (#3D836D)
    pub accent_hover: Color32,
    /// Text on accent backgrounds (#081310)
    pub accent_text: Color32,
    /// Danger / error status (#E5484D)
    pub danger: Color32,
    /// Warning status (#F5A524)
    pub warning: Color32,
    /// Success status (#4C9A82)
    pub success: Color32,
}

impl Default for Tokens {
    fn default() -> Self {
        Self::matte_dark()
    }
}

impl Tokens {
    /// Matte Dark (Default) — deep obsidian graphite with calm sage teal accent.
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
            accent: Color32::from_rgb(0x4C, 0x9A, 0x82),
            accent_hover: Color32::from_rgb(0x3D, 0x83, 0x6D),
            accent_text: Color32::from_rgb(0x08, 0x13, 0x10),
            danger: Color32::from_rgb(0xE5, 0x48, 0x4D),
            warning: Color32::from_rgb(0xF5, 0xA5, 0x24),
            success: Color32::from_rgb(0x4C, 0x9A, 0x82),
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
            success: Color32::from_rgb(0x4C, 0x9A, 0x82),
        }
    }

    /// Modrinth Emerald — dark carbon with calm botanical emerald accent.
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
            accent: Color32::from_rgb(0x2E, 0x9E, 0x6B),
            accent_hover: Color32::from_rgb(0x24, 0x83, 0x57),
            accent_text: Color32::from_rgb(0x07, 0x16, 0x0F),
            danger: Color32::from_rgb(0xE5, 0x48, 0x4D),
            warning: Color32::from_rgb(0xF5, 0xA5, 0x24),
            success: Color32::from_rgb(0x2E, 0x9E, 0x6B),
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

    /// Resolves `Tokens` from an active [`ThemeConfig`], honoring custom accent and surface colors.
    pub fn from_config(cfg: &ThemeConfig) -> Self {
        let bg_app = rgb_to_c32(cfg.bg_window);
        let is_light_bg = relative_luminance(bg_app) > 0.5;
        let mut tokens = match cfg.preset {
            ThemePreset::Custom => {
                if is_light_bg {
                    Self::light_clean()
                } else {
                    Self::matte_dark()
                }
            }
            other => Self::from_preset(other),
        };

        let accent = rgb_to_c32(cfg.accent_primary);
        let accent_hover = if cfg.accent_secondary == cfg.accent_primary {
            let mix_target = if is_light_bg {
                Color32::BLACK
            } else {
                Color32::from_rgb(0x0A, 0x10, 0x14)
            };
            blend_color(accent, mix_target, 0.16)
        } else {
            rgb_to_c32(cfg.accent_secondary)
        };
        tokens.accent = accent;
        tokens.accent_hover = accent_hover;
        tokens.accent_text = contrast_text_for(accent);
        tokens.success = rgb_to_c32(cfg.status_success);

        if cfg.preset == ThemePreset::Custom {
            let bg_surface = rgb_to_c32(cfg.bg_card);
            let text = rgb_to_c32(cfg.text_primary);
            let text_muted = rgb_to_c32(cfg.text_muted);
            tokens.bg_app = bg_app;
            tokens.bg_surface = bg_surface;
            tokens.bg_sidebar = blend_color(bg_app, bg_surface, 0.45);
            tokens.bg_surface_hover = blend_color(
                bg_surface,
                if is_light_bg {
                    Color32::BLACK
                } else {
                    Color32::WHITE
                },
                0.06,
            );
            tokens.bg_input = blend_color(
                bg_app,
                Color32::BLACK,
                if is_light_bg { 0.02 } else { 0.12 },
            );
            tokens.border = blend_color(bg_surface, text_muted, 0.22);
            tokens.border_strong = blend_color(bg_surface, text_muted, 0.42);
            tokens.text = text;
            tokens.text_muted = text_muted;
            tokens.text_disabled = blend_color(bg_surface, text_muted, 0.55);
            tokens.danger = rgb_to_c32(cfg.status_error);
        }

        tokens
    }

    /// Reads the active [`Tokens`] stored by [`apply_theme`], falling back to dark/light defaults.
    pub fn active(ctx: &Context) -> Self {
        ctx.data(|d| d.get_temp::<Self>(Id::new("mq_active_tokens")))
            .unwrap_or_else(|| {
                if ctx.style().visuals.dark_mode {
                    Self::matte_dark()
                } else {
                    Self::light_clean()
                }
            })
    }

    /// Returns `true` when the active background is light.
    #[inline]
    pub fn is_light(&self) -> bool {
        relative_luminance(self.bg_app) > 0.5
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
pub const ACCENT: Color32 = Color32::from_rgb(0x4C, 0x9A, 0x82);
pub const ACCENT_HOVER: Color32 = Color32::from_rgb(0x3D, 0x83, 0x6D);
pub const ACCENT_TEXT: Color32 = Color32::from_rgb(0x08, 0x13, 0x10);
pub const STATUS_DANGER: Color32 = Color32::from_rgb(0xE5, 0x48, 0x4D);
pub const STATUS_WARN: Color32 = Color32::from_rgb(0xF5, 0xA5, 0x24);
pub const STATUS_SUCCESS: Color32 = Color32::from_rgb(0x4C, 0x9A, 0x82);

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

/// Builds dynamic [`Visuals`] for the active theme configuration.
pub fn build_visuals_from_config(cfg: &ThemeConfig) -> Visuals {
    let tokens = Tokens::from_config(cfg);
    let is_light = tokens.is_light();
    let mut visuals = if is_light {
        Visuals::light()
    } else {
        Visuals::dark()
    };

    let card_r_u8 = crate::state::safe_clamp_f32(cfg.corner_radius, 0.0, 16.0).round() as u8;
    let btn_r_u8 =
        (crate::state::safe_clamp_f32(cfg.corner_radius, 0.0, 16.0) * 0.75).round() as u8;
    let card_radius = CornerRadius::same(card_r_u8);
    let btn_radius = CornerRadius::same(btn_r_u8.min(12));

    visuals.dark_mode = !is_light;
    visuals.override_text_color = Some(tokens.text);
    visuals.panel_fill = tokens.bg_app;
    visuals.window_fill = tokens.bg_surface;
    visuals.faint_bg_color = tokens.bg_surface;
    visuals.extreme_bg_color = tokens.bg_input;
    visuals.code_bg_color = tokens.bg_input;

    visuals.window_corner_radius = card_radius;
    visuals.menu_corner_radius = card_radius;
    visuals.window_stroke = Stroke::new(1.0_f32, tokens.border);

    visuals.hyperlink_color = tokens.accent;
    visuals.warn_fg_color = tokens.warning;
    visuals.error_fg_color = tokens.danger;

    let sel_bg = blend_color(tokens.bg_surface_hover, tokens.accent, 0.25);
    visuals.selection = Selection {
        bg_fill: sel_bg,
        stroke: Stroke::new(1.2_f32, tokens.text),
    };

    visuals.widgets = Widgets {
        noninteractive: WidgetVisuals {
            weak_bg_fill: tokens.bg_surface,
            bg_fill: tokens.bg_surface,
            bg_stroke: Stroke::new(1.0_f32, tokens.border),
            fg_stroke: Stroke::new(1.0_f32, tokens.text),
            corner_radius: card_radius,
            expansion: 0.0,
        },
        inactive: WidgetVisuals {
            weak_bg_fill: tokens.bg_surface,
            bg_fill: blend_color(
                tokens.bg_input,
                tokens.border_strong,
                if is_light { 0.50 } else { 0.40 },
            ),
            bg_stroke: Stroke::new(1.0_f32, tokens.border),
            fg_stroke: Stroke::new(1.0_f32, tokens.text),
            corner_radius: btn_radius,
            expansion: 0.0,
        },
        hovered: WidgetVisuals {
            weak_bg_fill: tokens.bg_surface_hover,
            bg_fill: tokens.bg_surface_hover,
            bg_stroke: Stroke::new(1.0_f32, tokens.border_strong),
            fg_stroke: Stroke::new(1.5_f32, tokens.text),
            corner_radius: btn_radius,
            expansion: 0.0,
        },
        active: WidgetVisuals {
            weak_bg_fill: tokens.bg_surface_hover,
            bg_fill: tokens.bg_surface_hover,
            bg_stroke: Stroke::new(1.5_f32, tokens.accent),
            fg_stroke: Stroke::new(1.5_f32, tokens.text),
            corner_radius: btn_radius,
            expansion: 0.0,
        },
        open: WidgetVisuals {
            weak_bg_fill: tokens.bg_surface_hover,
            bg_fill: tokens.bg_surface_hover,
            bg_stroke: Stroke::new(1.0_f32, tokens.border_strong),
            fg_stroke: Stroke::new(1.0_f32, tokens.text_muted),
            corner_radius: btn_radius,
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

    let base_font = crate::state::safe_clamp_f32(cfg.font_size, 11.0, 20.0);
    style
        .text_styles
        .insert(TextStyle::Body, FontId::proportional(base_font));
    style
        .text_styles
        .insert(TextStyle::Button, FontId::proportional(base_font));
    style.text_styles.insert(
        TextStyle::Small,
        FontId::proportional((base_font - 2.5).max(10.0)),
    );
    style
        .text_styles
        .insert(TextStyle::Heading, FontId::proportional(base_font + 5.0));
    style.text_styles.insert(
        TextStyle::Monospace,
        FontId::monospace((base_font - 1.0).max(10.0)),
    );

    style
}

/// Applies dynamic theme configuration to [`egui::Context`].
pub fn apply_theme(ctx: &Context, cfg: &ThemeConfig) {
    let tokens = Tokens::from_config(cfg);
    let style = build_style_from_config(cfg);
    ctx.set_visuals(style.visuals.clone());
    ctx.set_style(style);
    ctx.set_zoom_factor(crate::state::safe_clamp_f32(cfg.ui_scale, 0.8, 1.5));
    ctx.data_mut(|d| d.insert_temp(Id::new("mq_active_tokens"), tokens));
}

/// Solid opaque card frame (8px radius, 1px border, surface background).
pub fn solid_card_frame(tokens: &Tokens) -> Frame {
    Frame::new()
        .fill(tokens.bg_surface)
        .stroke(Stroke::new(1.0_f32, tokens.border))
        .corner_radius(RADIUS_CARD)
        .inner_margin(Margin::same(12))
}

/// Configured card frame honoring [`ThemeConfig::card_opacity`], [`ThemeConfig::corner_radius`],
/// and [`ThemeConfig::border_glow`].
pub fn solid_card_frame_with_config(tokens: &Tokens, cfg: &ThemeConfig) -> Frame {
    let alpha = (crate::state::safe_clamp_f32(cfg.card_opacity, 0.45, 1.0) * 255.0).round() as u8;
    let fill = Color32::from_rgba_unmultiplied(
        tokens.bg_surface.r(),
        tokens.bg_surface.g(),
        tokens.bg_surface.b(),
        alpha,
    );
    let glow = crate::state::safe_clamp_f32(cfg.border_glow, 0.0, 1.0);
    let border_col = if glow > 0.01 {
        blend_color(tokens.border, tokens.accent, glow * 0.45)
    } else {
        tokens.border
    };
    let r = crate::state::safe_clamp_f32(cfg.corner_radius, 0.0, 16.0).round() as u8;
    Frame::new()
        .fill(fill)
        .stroke(Stroke::new(1.0_f32, border_col))
        .corner_radius(CornerRadius::same(r))
        .inner_margin(Margin::same(12))
}

#[inline]
pub fn rgb_to_c32(rgb: [u8; 3]) -> Color32 {
    Color32::from_rgb(rgb[0], rgb[1], rgb[2])
}

/// Solid opaque sidebar frame (surface background, 1px border).
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

/// HUD bottom bar frame matching the active theme tokens.
pub fn solid_hud_frame(tokens: &Tokens) -> Frame {
    Frame::new()
        .fill(tokens.bg_sidebar)
        .stroke(Stroke::new(1.0_f32, tokens.border))
        .inner_margin(Margin::symmetric(12, 4))
}

#[derive(Clone)]
struct CachedBackdrop {
    path: String,
    texture: Option<TextureHandle>,
    size: [usize; 2],
}

fn resolve_backdrop_texture(ctx: &Context, path: &str) -> Option<(TextureHandle, [usize; 2])> {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return None;
    }

    let cache_id = Id::new("mq_backdrop_tex_cache");
    if let Some(cached) = ctx.data(|d| d.get_temp::<CachedBackdrop>(cache_id)) {
        if cached.path == trimmed {
            return cached.texture.map(|tex| (tex, cached.size));
        }
    }

    let loaded = (|| {
        let bytes = fs::read(Path::new(trimmed)).ok()?;
        let img = image::load_from_memory(&bytes).ok()?;
        let rgba = img.to_rgba8();
        let size = [rgba.width() as usize, rgba.height() as usize];
        if size[0] == 0 || size[1] == 0 {
            return None;
        }
        let color_image = ColorImage::from_rgba_unmultiplied(size, rgba.as_raw());
        let tex = ctx.load_texture("mq_custom_backdrop", color_image, TextureOptions::LINEAR);
        Some((tex, size))
    })();

    ctx.data_mut(|d| {
        d.insert_temp(
            cache_id,
            CachedBackdrop {
                path: trimmed.to_owned(),
                texture: loaded.as_ref().map(|(t, _)| t.clone()),
                size: loaded.as_ref().map(|(_, s)| *s).unwrap_or([1, 1]),
            },
        );
    });

    loaded
}

fn paint_procedural_backdrop(painter: &Painter, rect: Rect, tokens: &Tokens) {
    let is_light = tokens.is_light();
    let cool_secondary = if is_light {
        Color32::from_rgb(0x4B, 0x6B, 0x99)
    } else {
        Color32::from_rgb(0x4A, 0x6B, 0x9C)
    };

    // 4x3 layered gradient mesh: subtle ambient corner glow over flat base.
    let cols = 4usize;
    let rows = 3usize;
    let mut mesh = Mesh::default();
    for r in 0..=rows {
        let fy = (r as f32) / (rows as f32);
        let y = rect.top() + rect.height() * fy;
        for c in 0..=cols {
            let fx = (c as f32) / (cols as f32);
            let x = rect.left() + rect.width() * fx;

            let tl_dist = ((fx * fx) + (fy * fy)).sqrt().clamp(0.0, 1.42);
            let br_dist = (((1.0 - fx).powi(2)) + ((1.0 - fy).powi(2)))
                .sqrt()
                .clamp(0.0, 1.42);

            let tl_weight =
                ((1.0 - tl_dist / 1.1).max(0.0)).powf(1.8) * if is_light { 0.08 } else { 0.11 };
            let br_weight =
                ((1.0 - br_dist / 1.1).max(0.0)).powf(1.8) * if is_light { 0.06 } else { 0.09 };

            let mid = blend_color(tokens.bg_app, tokens.accent, tl_weight);
            let color = blend_color(mid, cool_secondary, br_weight);
            mesh.vertices.push(Vertex {
                pos: Pos2::new(x, y),
                uv: Pos2::ZERO,
                color,
            });
        }
    }

    let stride = (cols + 1) as u32;
    for r in 0..(rows as u32) {
        for c in 0..(cols as u32) {
            let i0 = r * stride + c;
            let i1 = i0 + 1;
            let i2 = i0 + stride;
            let i3 = i2 + 1;
            mesh.add_triangle(i0, i1, i2);
            mesh.add_triangle(i1, i3, i2);
        }
    }
    painter.add(mesh);

    // Architectural 32px micro-grid & 128px registration crosshairs.
    let grid_step = 32.0_f32;
    let grid_alpha = if is_light { 12 } else { 14 };
    let grid_color = Color32::from_rgba_unmultiplied(
        tokens.border_strong.r(),
        tokens.border_strong.g(),
        tokens.border_strong.b(),
        grid_alpha,
    );
    let cross_color = Color32::from_rgba_unmultiplied(
        tokens.accent.r(),
        tokens.accent.g(),
        tokens.accent.b(),
        if is_light { 34 } else { 38 },
    );

    let max_span = 4096.0_f32;
    let right_limit = rect.left() + rect.width().min(max_span);
    let bottom_limit = rect.top() + rect.height().min(max_span);

    let mut x = (rect.left() / grid_step).ceil() * grid_step;
    while x < right_limit {
        painter.line_segment(
            [Pos2::new(x, rect.top()), Pos2::new(x, bottom_limit)],
            Stroke::new(1.0_f32, grid_color),
        );
        x += grid_step;
    }

    let mut y = (rect.top() / grid_step).ceil() * grid_step;
    while y < bottom_limit {
        painter.line_segment(
            [Pos2::new(rect.left(), y), Pos2::new(right_limit, y)],
            Stroke::new(1.0_f32, grid_color),
        );
        y += grid_step;
    }

    let major_step = 128.0_f32;
    let mut cx = (rect.left() / major_step).ceil() * major_step;
    while cx < right_limit {
        let mut cy = (rect.top() / major_step).ceil() * major_step;
        while cy < bottom_limit {
            painter.line_segment(
                [Pos2::new(cx - 3.5, cy), Pos2::new(cx + 3.5, cy)],
                Stroke::new(1.0_f32, cross_color),
            );
            painter.line_segment(
                [Pos2::new(cx, cy - 3.5), Pos2::new(cx, cy + 3.5)],
                Stroke::new(1.0_f32, cross_color),
            );
            cy += major_step;
        }
        cx += major_step;
    }
}

/// Paints the configured launcher backdrop (`Procedural`, `Solid`, or `CustomImage`) inside `rect`.
pub fn paint_launcher_background(
    ctx: &Context,
    painter: &Painter,
    rect: Rect,
    cfg: &ThemeConfig,
    tokens: &Tokens,
) {
    if !rect.is_positive() {
        return;
    }
    painter.rect_filled(rect, CornerRadius::ZERO, tokens.bg_app);

    match cfg.background_style {
        BackgroundStyle::Solid => {}
        BackgroundStyle::Procedural => paint_procedural_backdrop(painter, rect, tokens),
        BackgroundStyle::CustomImage => {
            if let Some((tex, size)) = cfg
                .backdrop_path
                .as_deref()
                .and_then(|p| resolve_backdrop_texture(ctx, p))
            {
                let tex_aspect = (size[0] as f32) / (size[1].max(1) as f32);
                let rect_aspect = rect.width() / rect.height().max(1.0);
                let uv = if rect_aspect > tex_aspect {
                    let h = tex_aspect / rect_aspect;
                    let pad = (1.0 - h) * 0.5;
                    Rect::from_min_max(Pos2::new(0.0, pad), Pos2::new(1.0, 1.0 - pad))
                } else {
                    let w = rect_aspect / tex_aspect;
                    let pad = (1.0 - w) * 0.5;
                    Rect::from_min_max(Pos2::new(pad, 0.0), Pos2::new(1.0 - pad, 1.0))
                };
                painter.image(tex.id(), rect, uv, Color32::WHITE);

                let dim = crate::state::safe_clamp_f32(cfg.backdrop_dim, 0.0, 0.92);
                if dim > 0.01 {
                    let scrim = Color32::from_rgba_unmultiplied(
                        tokens.bg_app.r(),
                        tokens.bg_app.g(),
                        tokens.bg_app.b(),
                        (dim * 255.0).round() as u8,
                    );
                    painter.rect_filled(rect, CornerRadius::ZERO, scrim);
                }
            } else {
                paint_procedural_backdrop(painter, rect, tokens);
            }
        }
    }
}

// Aliases for legacy call sites
#[inline]
pub fn custom_glass_card_frame(cfg: &ThemeConfig) -> Frame {
    solid_card_frame_with_config(&Tokens::from_config(cfg), cfg)
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
        assert_eq!(t.accent, Color32::from_rgb(0x4C, 0x9A, 0x82));
        assert_eq!(t.accent_hover, Color32::from_rgb(0x3D, 0x83, 0x6D));
        assert_eq!(t.accent_text, Color32::from_rgb(0x08, 0x13, 0x10));
        assert_eq!(t.danger, Color32::from_rgb(0xE5, 0x48, 0x4D));
        assert_eq!(t.warning, Color32::from_rgb(0xF5, 0xA5, 0x24));
        assert_eq!(t.success, Color32::from_rgb(0x4C, 0x9A, 0x82));

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

    #[test]
    fn from_config_honors_custom_accent_and_selection_contrast() {
        let mut cfg = ThemeConfig::preset_matte_dark();
        cfg.accent_primary = [0x5B, 0x8D, 0xEF];
        cfg.accent_secondary = [0x46, 0x74, 0xD0];
        let tokens = Tokens::from_config(&cfg);
        assert_eq!(tokens.accent, Color32::from_rgb(0x5B, 0x8D, 0xEF));
        assert_eq!(tokens.accent_hover, Color32::from_rgb(0x46, 0x74, 0xD0));

        let visuals = build_visuals_from_config(&cfg);
        assert_ne!(visuals.selection.bg_fill, visuals.selection.stroke.color);
        assert_eq!(visuals.selection.stroke.color, tokens.text);
    }
}
