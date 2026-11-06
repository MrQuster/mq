//! Global launcher configuration (`config.toml`), theme configuration (`theme.toml`),
//! and environment variable overrides.

use std::env;
use std::fmt;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::Path;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::error::{MqError, MqResult};
use crate::paths::MqPaths;
use crate::secret::SecretString;

/// Environment variable overriding `curseforge_api_key` in `config.toml`.
pub const ENV_CF_API_KEY: &str = "MQ_CF_API_KEY";

/// Environment variable overriding `ms_client_id` in `config.toml`.
pub const ENV_MS_CLIENT_ID: &str = "MQ_MS_CLIENT_ID";

/// Graphical rendering backend selection for the `eframe`/`egui` launcher UI.
#[derive(Debug, Default, Copy, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum GuiBackend {
    #[default]
    #[serde(alias = "Auto", alias = "AUTO")]
    Auto,
    #[serde(alias = "Vulkan", alias = "VULKAN", alias = "wgpu")]
    Vulkan,
    #[serde(alias = "Gl", alias = "GL", alias = "opengl", alias = "glow")]
    Gl,
}

impl GuiBackend {
    /// Returns the canonical lowercase identifier for this backend.
    #[inline]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Vulkan => "vulkan",
            Self::Gl => "gl",
        }
    }
}

impl fmt::Display for GuiBackend {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for GuiBackend {
    type Err = MqError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_ascii_lowercase().as_str() {
            "auto" => Ok(Self::Auto),
            "vulkan" | "wgpu" => Ok(Self::Vulkan),
            "gl" | "opengl" | "glow" => Ok(Self::Gl),
            other => Err(MqError::Metadata(format!(
                "unknown GUI backend '{other}' (expected auto, vulkan, or gl)"
            ))),
        }
    }
}

/// Ready-made visual theme presets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ThemePreset {
    #[default]
    LiquidGlass,
    ObsidianOled,
    CatppuccinMocha,
    NordicFrost,
    GruvboxMaterial,
    Custom,
}

impl ThemePreset {
    pub const ALL: [Self; 6] = [
        Self::LiquidGlass,
        Self::ObsidianOled,
        Self::CatppuccinMocha,
        Self::NordicFrost,
        Self::GruvboxMaterial,
        Self::Custom,
    ];

    pub const fn label(&self) -> &'static str {
        match self {
            Self::LiquidGlass => "Liquid Glass (По умолчанию)",
            Self::ObsidianOled => "Obsidian OLED",
            Self::CatppuccinMocha => "Catppuccin Mocha",
            Self::NordicFrost => "Nordic Frost",
            Self::GruvboxMaterial => "Gruvbox Material",
            Self::Custom => "Custom (Пользовательская)",
        }
    }
}

/// Interface layout density.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum UiDensity {
    Compact,
    #[default]
    Comfortable,
    Spacious,
}

/// Window behavior upon launching Minecraft.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum LaunchAction {
    #[default]
    KeepOpen,
    Minimize,
    HideUntilExit,
}

/// Complete theme and UI styling configuration.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ThemeConfig {
    pub preset: ThemePreset,
    pub window_opacity: f32,
    pub card_opacity: f32,
    pub border_glow: f32,
    pub bg_window: [u8; 3],
    pub bg_card: [u8; 3],
    pub accent_primary: [u8; 3],
    pub accent_secondary: [u8; 3],
    pub text_primary: [u8; 3],
    pub text_muted: [u8; 3],
    pub status_success: [u8; 3],
    pub status_error: [u8; 3],
    pub corner_radius: f32,
    pub density: UiDensity,
    pub ui_scale: f32,
    pub font_size: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub backdrop_path: Option<String>,
    pub backdrop_dim: f32,
    pub launch_action: LaunchAction,
}

impl Default for ThemeConfig {
    fn default() -> Self {
        Self::preset_liquid_glass()
    }
}

impl ThemeConfig {
    pub fn preset_liquid_glass() -> Self {
        Self {
            preset: ThemePreset::LiquidGlass,
            window_opacity: 0.82,
            card_opacity: 0.75,
            border_glow: 0.45,
            bg_window: [0x0D, 0x0F, 0x14],
            bg_card: [0x15, 0x18, 0x21],
            accent_primary: [0x10, 0xB9, 0x81],   // Emerald
            accent_secondary: [0x3B, 0x82, 0xF6], // Cobalt
            text_primary: [0xF3, 0xF4, 0xF6],
            text_muted: [0x9C, 0xA3, 0xAF],
            status_success: [0x10, 0xB9, 0x81],
            status_error: [0xEF, 0x44, 0x44],
            corner_radius: 8.0,
            density: UiDensity::Comfortable,
            ui_scale: 1.0,
            font_size: 14.0,
            backdrop_path: None,
            backdrop_dim: 0.70,
            launch_action: LaunchAction::KeepOpen,
        }
    }

    pub fn preset_obsidian_oled() -> Self {
        Self {
            preset: ThemePreset::ObsidianOled,
            window_opacity: 1.0,
            card_opacity: 0.95,
            border_glow: 0.20,
            bg_window: [0x05, 0x05, 0x07],
            bg_card: [0x0E, 0x10, 0x15],
            accent_primary: [0x3B, 0x82, 0xF6], // Cobalt
            accent_secondary: [0x60, 0xA5, 0xFA],
            text_primary: [0xF9, 0xFA, 0xFB],
            text_muted: [0x6B, 0x72, 0x80],
            status_success: [0x10, 0xB9, 0x81],
            status_error: [0xEF, 0x44, 0x44],
            corner_radius: 6.0,
            density: UiDensity::Comfortable,
            ui_scale: 1.0,
            font_size: 14.0,
            backdrop_path: None,
            backdrop_dim: 0.85,
            launch_action: LaunchAction::KeepOpen,
        }
    }

    pub fn preset_catppuccin_mocha() -> Self {
        Self {
            preset: ThemePreset::CatppuccinMocha,
            window_opacity: 0.88,
            card_opacity: 0.82,
            border_glow: 0.40,
            bg_window: [0x1E, 0x1E, 0x2E],
            bg_card: [0x18, 0x18, 0x25],
            accent_primary: [0xCB, 0xA6, 0xF7], // Lavender
            accent_secondary: [0x89, 0xB4, 0xFA],
            text_primary: [0xCD, 0xD6, 0xF4],
            text_muted: [0xA6, 0xAD, 0xC8],
            status_success: [0xA6, 0xE3, 0xA1],
            status_error: [0xF3, 0x8B, 0xA8],
            corner_radius: 10.0,
            density: UiDensity::Comfortable,
            ui_scale: 1.0,
            font_size: 14.0,
            backdrop_path: None,
            backdrop_dim: 0.70,
            launch_action: LaunchAction::KeepOpen,
        }
    }

    pub fn preset_nordic_frost() -> Self {
        Self {
            preset: ThemePreset::NordicFrost,
            window_opacity: 0.85,
            card_opacity: 0.80,
            border_glow: 0.50,
            bg_window: [0x2E, 0x34, 0x40],
            bg_card: [0x3B, 0x42, 0x52],
            accent_primary: [0x88, 0xC0, 0xD0], // Ice
            accent_secondary: [0x81, 0xA1, 0xC1],
            text_primary: [0xEC, 0xEF, 0xF4],
            text_muted: [0xD8, 0xDE, 0xE9],
            status_success: [0xA3, 0xBE, 0x8C],
            status_error: [0xBF, 0x61, 0x6A],
            corner_radius: 8.0,
            density: UiDensity::Comfortable,
            ui_scale: 1.0,
            font_size: 14.0,
            backdrop_path: None,
            backdrop_dim: 0.70,
            launch_action: LaunchAction::KeepOpen,
        }
    }

    pub fn preset_gruvbox_material() -> Self {
        Self {
            preset: ThemePreset::GruvboxMaterial,
            window_opacity: 0.88,
            card_opacity: 0.82,
            border_glow: 0.35,
            bg_window: [0x28, 0x28, 0x28],
            bg_card: [0x32, 0x30, 0x2F],
            accent_primary: [0xFA, 0xBD, 0x2F], // Amber
            accent_secondary: [0x83, 0xA5, 0x98],
            text_primary: [0xEB, 0xDB, 0xB2],
            text_muted: [0xA8, 0x99, 0x84],
            status_success: [0xB8, 0xBB, 0x26],
            status_error: [0xFB, 0x49, 0x34],
            corner_radius: 6.0,
            density: UiDensity::Comfortable,
            ui_scale: 1.0,
            font_size: 14.0,
            backdrop_path: None,
            backdrop_dim: 0.70,
            launch_action: LaunchAction::KeepOpen,
        }
    }

    pub fn apply_preset(&mut self, preset: ThemePreset) {
        let mut new_cfg = match preset {
            ThemePreset::LiquidGlass => Self::preset_liquid_glass(),
            ThemePreset::ObsidianOled => Self::preset_obsidian_oled(),
            ThemePreset::CatppuccinMocha => Self::preset_catppuccin_mocha(),
            ThemePreset::NordicFrost => Self::preset_nordic_frost(),
            ThemePreset::GruvboxMaterial => Self::preset_gruvbox_material(),
            ThemePreset::Custom => {
                self.preset = ThemePreset::Custom;
                return;
            }
        };
        new_cfg.backdrop_path = self.backdrop_path.clone();
        new_cfg.launch_action = self.launch_action;
        *self = new_cfg;
    }

    /// Loads theme from a given TOML file path.
    pub fn load_from_file(path: &Path) -> MqResult<Self> {
        let raw = fs::read_to_string(path)?;
        let cfg: Self = toml::from_str(&raw)?;
        Ok(cfg)
    }

    /// Saves theme atomically to a given TOML file path.
    pub fn save_to_file(&self, path: &Path) -> MqResult<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let serialized = toml::to_string_pretty(self)?;
        let tmp_path = path.with_extension(format!("tmp.{}", std::process::id()));
        let mut file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(&tmp_path)?;
        file.write_all(serialized.as_bytes())?;
        file.sync_all()?;
        drop(file);
        fs::rename(&tmp_path, path)?;
        Ok(())
    }
}

fn default_launcher_locale() -> String {
    "uk_UA".to_string()
}

fn default_mc_locale() -> String {
    "uk_ua".to_string()
}

/// Global application configuration persisted in `~/.config/mq/config.toml`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AppConfig {
    /// Optional worker thread count override (`None` = use logical CPU cores).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub threads: Option<usize>,

    /// Maximum concurrent HTTP download streams (default: `32`).
    pub max_connections: usize,

    /// Optional CurseForge Core API key (redacted in `Debug` output).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub curseforge_api_key: Option<SecretString>,

    /// Optional Microsoft / Azure AD OAuth2 client ID.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ms_client_id: Option<String>,

    /// Default minimum JVM heap size (`-Xms`) in MiB (default: `1024`).
    pub default_memory_min_mb: u32,

    /// Default maximum JVM heap size (`-Xmx`) in MiB (default: `4096`).
    pub default_memory_max_mb: u32,

    /// Default additional JVM flags applied to newly created instances.
    pub default_jvm_args: Vec<String>,

    /// Optional wrapper command prefix (e.g., `"gamemoderun"` or `"mangohud"`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_wrapper: Option<String>,

    /// Preferred graphics backend for the launcher window.
    pub ui_backend: GuiBackend,

    /// Optional base URL override for hermetic mock-server testing.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mock_url: Option<String>,

    /// Launcher interface language code (e.g. "uk_UA", "ru_RU", "en_US").
    pub launcher_locale: String,

    /// Default Minecraft game language code (e.g. "uk_ua", "ru_ru", "en_us").
    pub default_mc_locale: String,

    /// Active theme and visual customization settings.
    pub theme: ThemeConfig,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            threads: None,
            max_connections: 32,
            curseforge_api_key: None,
            ms_client_id: None,
            default_memory_min_mb: 1024,
            default_memory_max_mb: 4096,
            default_jvm_args: Vec::new(),
            default_wrapper: None,
            ui_backend: GuiBackend::Auto,
            mock_url: None,
            launcher_locale: default_launcher_locale(),
            default_mc_locale: default_mc_locale(),
            theme: ThemeConfig::default(),
        }
    }
}

impl AppConfig {
    /// Loads `config.toml` from `paths.config_file()`, or returns [`AppConfig::default()`]
    /// if the file does not exist yet. Also checks `paths.theme_file()` if present.
    pub fn load_or_default(paths: &MqPaths) -> MqResult<Self> {
        let config_path = paths.config_file();
        let mut cfg = if config_path.exists() {
            let raw = fs::read_to_string(&config_path)?;
            let parsed: Self = toml::from_str(&raw)?;
            parsed
        } else {
            Self::default()
        };

        let theme_path = paths.theme_file();
        if theme_path.exists() {
            if let Ok(theme) = ThemeConfig::load_from_file(&theme_path) {
                cfg.theme = theme;
            }
        }

        Ok(cfg)
    }

    /// Atomically persists this configuration to `paths.config_file()` (`write temp -> fsync -> rename`)
    /// with `0600` permissions on Unix systems to protect embedded API keys.
    /// Also saves `self.theme` to `paths.theme_file()`.
    pub fn save(&self, paths: &MqPaths) -> MqResult<()> {
        let config_path = paths.config_file();
        if let Some(parent) = config_path.parent() {
            fs::create_dir_all(parent)?;
        }

        let serialized = toml::to_string_pretty(self)?;
        let tmp_path = config_path.with_extension(format!("tmp.{}", std::process::id()));

        let write_res = (|| -> std::io::Result<()> {
            let mut file = OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .open(&tmp_path)?;

            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let perms = fs::Permissions::from_mode(0o600);
                file.set_permissions(perms)?;
            }

            file.write_all(serialized.as_bytes())?;
            file.sync_all()?;
            drop(file);

            fs::rename(&tmp_path, &config_path)?;
            Ok(())
        })();

        if let Err(err) = write_res {
            let _ = fs::remove_file(&tmp_path);
            return Err(MqError::Io(err));
        }

        // Save theme.toml in parallel
        let _ = self.theme.save_to_file(&paths.theme_file());

        Ok(())
    }

    /// Resolves the effective CurseForge API key, checking `$MQ_CF_API_KEY` first
    /// and falling back to `self.curseforge_api_key`.
    pub fn effective_cf_api_key(&self) -> Option<String> {
        let env_val = env::var(ENV_CF_API_KEY).ok();
        self.effective_cf_api_key_with_env(env_val.as_deref())
    }

    /// Deterministic helper for testing `$MQ_CF_API_KEY` precedence without mutating process env.
    pub fn effective_cf_api_key_with_env(&self, env_val: Option<&str>) -> Option<String> {
        if let Some(val) = env_val.map(str::trim).filter(|s| !s.is_empty()) {
            return Some(val.to_owned());
        }
        self.curseforge_api_key
            .as_ref()
            .map(|s| s.expose_secret().trim())
            .filter(|s| !s.is_empty())
            .map(ToOwned::to_owned)
    }

    /// Resolves the effective Microsoft OAuth2 client ID, checking `$MQ_MS_CLIENT_ID` first
    /// and falling back to `self.ms_client_id`.
    pub fn effective_ms_client_id(&self) -> Option<String> {
        let env_val = env::var(ENV_MS_CLIENT_ID).ok();
        self.effective_ms_client_id_with_env(env_val.as_deref())
    }

    /// Deterministic helper for testing `$MQ_MS_CLIENT_ID` precedence without mutating process env.
    pub fn effective_ms_client_id_with_env(&self, env_val: Option<&str>) -> Option<String> {
        if let Some(val) = env_val.map(str::trim).filter(|s| !s.is_empty()) {
            return Some(val.to_owned());
        }
        self.ms_client_id
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(ToOwned::to_owned)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_values_and_toml_roundtrip() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let paths = MqPaths::with_root(tmp.path());

        let initial = AppConfig::load_or_default(&paths).expect("load default when missing");
        assert_eq!(initial.threads, None);
        assert_eq!(initial.max_connections, 32);
        assert_eq!(initial.default_memory_min_mb, 1024);
        assert_eq!(initial.default_memory_max_mb, 4096);
        assert!(initial.default_jvm_args.is_empty());
        assert_eq!(initial.ui_backend, GuiBackend::Auto);
        assert_eq!(initial.theme.preset, ThemePreset::LiquidGlass);

        let populated = AppConfig {
            threads: Some(12),
            max_connections: 48,
            curseforge_api_key: Some(SecretString::new("$2a$10$secret_cf_key_value")),
            ms_client_id: Some("00000000-0000-0000-0000-000000000000".into()),
            default_memory_min_mb: 2048,
            default_memory_max_mb: 8192,
            default_jvm_args: vec!["-XX:+UseG1GC".into(), "-XX:+ParallelRefProcEnabled".into()],
            default_wrapper: Some("gamemoderun".into()),
            ui_backend: GuiBackend::Vulkan,
            mock_url: Some("http://127.0.0.1:8080".into()),
            launcher_locale: "uk_UA".into(),
            default_mc_locale: "uk_ua".into(),
            theme: ThemeConfig::preset_obsidian_oled(),
        };

        // Ensure Debug never leaks the CurseForge API key.
        let dbg = format!("{populated:?}");
        assert!(!dbg.contains("secret_cf_key_value"));
        assert!(dbg.contains("[REDACTED]"));

        populated.save(&paths).expect("save config.toml");
        assert!(paths.config_file().is_file());
        assert!(paths.theme_file().is_file());

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = fs::metadata(paths.config_file())
                .expect("stat config")
                .permissions()
                .mode()
                & 0o777;
            assert_eq!(mode, 0o600);
        }

        let reloaded = AppConfig::load_or_default(&paths).expect("reload saved config");
        assert_eq!(reloaded, populated);
    }

    #[test]
    fn theme_presets_match_specification() {
        let lg = ThemeConfig::preset_liquid_glass();
        assert_eq!(lg.preset, ThemePreset::LiquidGlass);
        assert!((lg.window_opacity - 0.82).abs() < 0.001);
        assert!((lg.card_opacity - 0.75).abs() < 0.001);
        assert_eq!(lg.accent_primary, [0x10, 0xB9, 0x81]);

        let oled = ThemeConfig::preset_obsidian_oled();
        assert_eq!(oled.preset, ThemePreset::ObsidianOled);
        assert_eq!(oled.bg_window, [0x05, 0x05, 0x07]);
        assert_eq!(oled.accent_primary, [0x3B, 0x82, 0xF6]);

        let cat = ThemeConfig::preset_catppuccin_mocha();
        assert_eq!(cat.preset, ThemePreset::CatppuccinMocha);
        assert_eq!(cat.accent_primary, [0xCB, 0xA6, 0xF7]);

        let nord = ThemeConfig::preset_nordic_frost();
        assert_eq!(nord.preset, ThemePreset::NordicFrost);
        assert_eq!(nord.accent_primary, [0x88, 0xC0, 0xD0]);

        let gruv = ThemeConfig::preset_gruvbox_material();
        assert_eq!(gruv.preset, ThemePreset::GruvboxMaterial);
        assert_eq!(gruv.accent_primary, [0xFA, 0xBD, 0x2F]);
    }

    #[test]
    fn env_overrides_take_precedence_over_file_config() {
        let cfg = AppConfig {
            curseforge_api_key: Some(SecretString::new("config_cf_key")),
            ms_client_id: Some("config_ms_id".into()),
            ..AppConfig::default()
        };

        assert_eq!(
            cfg.effective_cf_api_key_with_env(None).as_deref(),
            Some("config_cf_key")
        );
        assert_eq!(
            cfg.effective_cf_api_key_with_env(Some("  env_cf_key  "))
                .as_deref(),
            Some("env_cf_key")
        );
        assert_eq!(
            cfg.effective_cf_api_key_with_env(Some("   ")).as_deref(),
            Some("config_cf_key")
        );

        assert_eq!(
            cfg.effective_ms_client_id_with_env(None).as_deref(),
            Some("config_ms_id")
        );
        assert_eq!(
            cfg.effective_ms_client_id_with_env(Some("env_ms_id"))
                .as_deref(),
            Some("env_ms_id")
        );
    }
}
