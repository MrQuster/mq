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
    MatteDark,
    PrismClassicDark,
    ModrinthEmerald,
    CatppuccinMocha,
    Nord,
    LightClean,
    #[serde(other)]
    Custom,
}

impl ThemePreset {
    pub const ALL: [Self; 6] = [
        Self::MatteDark,
        Self::PrismClassicDark,
        Self::ModrinthEmerald,
        Self::CatppuccinMocha,
        Self::Nord,
        Self::LightClean,
    ];

    pub const fn label(&self) -> &'static str {
        match self {
            Self::MatteDark => "Matte Dark (По умолчанию)",
            Self::PrismClassicDark => "Prism Classic Dark",
            Self::ModrinthEmerald => "Modrinth Emerald",
            Self::CatppuccinMocha => "Catppuccin Mocha",
            Self::Nord => "Nord",
            Self::LightClean => "Light Clean",
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
        Self::preset_matte_dark()
    }
}

impl ThemeConfig {
    pub fn preset_matte_dark() -> Self {
        Self {
            preset: ThemePreset::MatteDark,
            window_opacity: 1.0,
            card_opacity: 1.0,
            border_glow: 0.0,
            bg_window: [0x0F, 0x11, 0x15],
            bg_card: [0x1A, 0x1E, 0x26],
            accent_primary: [0x3D, 0xDC, 0x97],
            accent_secondary: [0x34, 0xC3, 0x86],
            text_primary: [0xE6, 0xE8, 0xEE],
            text_muted: [0x9A, 0xA1, 0xB2],
            status_success: [0x3D, 0xDC, 0x97],
            status_error: [0xE5, 0x48, 0x4D],
            corner_radius: 8.0,
            density: UiDensity::Comfortable,
            ui_scale: 1.0,
            font_size: 14.0,
            backdrop_path: None,
            backdrop_dim: 0.0,
            launch_action: LaunchAction::KeepOpen,
        }
    }

    pub fn preset_prism_classic_dark() -> Self {
        Self {
            preset: ThemePreset::PrismClassicDark,
            window_opacity: 1.0,
            card_opacity: 1.0,
            border_glow: 0.0,
            bg_window: [0x19, 0x19, 0x19],
            bg_card: [0x24, 0x24, 0x24],
            accent_primary: [0x5B, 0x8D, 0xEF],
            accent_secondary: [0x4A, 0x7C, 0xD8],
            text_primary: [0xEA, 0xEA, 0xEA],
            text_muted: [0xA0, 0xA0, 0xA0],
            status_success: [0x3D, 0xDC, 0x97],
            status_error: [0xE5, 0x48, 0x4D],
            corner_radius: 8.0,
            density: UiDensity::Comfortable,
            ui_scale: 1.0,
            font_size: 14.0,
            backdrop_path: None,
            backdrop_dim: 0.0,
            launch_action: LaunchAction::KeepOpen,
        }
    }

    pub fn preset_modrinth_emerald() -> Self {
        Self {
            preset: ThemePreset::ModrinthEmerald,
            window_opacity: 1.0,
            card_opacity: 1.0,
            border_glow: 0.0,
            bg_window: [0x11, 0x12, 0x15],
            bg_card: [0x1C, 0x1F, 0x26],
            accent_primary: [0x1B, 0xD9, 0x6A],
            accent_secondary: [0x17, 0xBF, 0x5D],
            text_primary: [0xED, 0xED, 0xED],
            text_muted: [0x9D, 0xA3, 0xAF],
            status_success: [0x1B, 0xD9, 0x6A],
            status_error: [0xE5, 0x48, 0x4D],
            corner_radius: 8.0,
            density: UiDensity::Comfortable,
            ui_scale: 1.0,
            font_size: 14.0,
            backdrop_path: None,
            backdrop_dim: 0.0,
            launch_action: LaunchAction::KeepOpen,
        }
    }

    pub fn preset_catppuccin_mocha() -> Self {
        Self {
            preset: ThemePreset::CatppuccinMocha,
            window_opacity: 1.0,
            card_opacity: 1.0,
            border_glow: 0.0,
            bg_window: [0x1E, 0x1E, 0x2E],
            bg_card: [0x31, 0x32, 0x44],
            accent_primary: [0x89, 0xB4, 0xFA],
            accent_secondary: [0x74, 0xC7, 0xEC],
            text_primary: [0xCD, 0xD6, 0xF4],
            text_muted: [0xA6, 0xAD, 0xC8],
            status_success: [0xA6, 0xE3, 0xA1],
            status_error: [0xF3, 0x8B, 0xA8],
            corner_radius: 8.0,
            density: UiDensity::Comfortable,
            ui_scale: 1.0,
            font_size: 14.0,
            backdrop_path: None,
            backdrop_dim: 0.0,
            launch_action: LaunchAction::KeepOpen,
        }
    }

    pub fn preset_nord() -> Self {
        Self {
            preset: ThemePreset::Nord,
            window_opacity: 1.0,
            card_opacity: 1.0,
            border_glow: 0.0,
            bg_window: [0x24, 0x29, 0x33],
            bg_card: [0x3B, 0x42, 0x52],
            accent_primary: [0x88, 0xC0, 0xD0],
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
            backdrop_dim: 0.0,
            launch_action: LaunchAction::KeepOpen,
        }
    }

    pub fn preset_light_clean() -> Self {
        Self {
            preset: ThemePreset::LightClean,
            window_opacity: 1.0,
            card_opacity: 1.0,
            border_glow: 0.0,
            bg_window: [0xF4, 0xF5, 0xF7],
            bg_card: [0xFF, 0xFF, 0xFF],
            accent_primary: [0x05, 0x96, 0x69],
            accent_secondary: [0x04, 0x78, 0x57],
            text_primary: [0x11, 0x18, 0x27],
            text_muted: [0x4B, 0x55, 0x63],
            status_success: [0x05, 0x96, 0x69],
            status_error: [0xDC, 0x26, 0x26],
            corner_radius: 8.0,
            density: UiDensity::Comfortable,
            ui_scale: 1.0,
            font_size: 14.0,
            backdrop_path: None,
            backdrop_dim: 0.0,
            launch_action: LaunchAction::KeepOpen,
        }
    }

    pub fn apply_preset(&mut self, preset: ThemePreset) {
        let mut new_cfg = match preset {
            ThemePreset::MatteDark => Self::preset_matte_dark(),
            ThemePreset::PrismClassicDark => Self::preset_prism_classic_dark(),
            ThemePreset::ModrinthEmerald => Self::preset_modrinth_emerald(),
            ThemePreset::CatppuccinMocha => Self::preset_catppuccin_mocha(),
            ThemePreset::Nord => Self::preset_nord(),
            ThemePreset::LightClean => Self::preset_light_clean(),
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
    "ru_RU".to_string()
}

fn default_mc_locale() -> String {
    "ru_ru".to_string()
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
            match toml::from_str(&raw) {
                Ok(parsed) => parsed,
                Err(e) => {
                    tracing::warn!("Failed to parse config.toml ({e}), falling back to default");
                    Self::default()
                }
            }
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
        assert_eq!(initial.theme.preset, ThemePreset::MatteDark);

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
            launcher_locale: "ru_RU".into(),
            default_mc_locale: "ru_ru".into(),
            theme: ThemeConfig::preset_prism_classic_dark(),
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
        let md = ThemeConfig::preset_matte_dark();
        assert_eq!(md.preset, ThemePreset::MatteDark);
        assert_eq!(md.window_opacity, 1.0);
        assert_eq!(md.card_opacity, 1.0);
        assert_eq!(md.bg_window, [0x0F, 0x11, 0x15]);
        assert_eq!(md.accent_primary, [0x3D, 0xDC, 0x97]);

        let prism = ThemeConfig::preset_prism_classic_dark();
        assert_eq!(prism.preset, ThemePreset::PrismClassicDark);
        assert_eq!(prism.bg_window, [0x19, 0x19, 0x19]);
        assert_eq!(prism.accent_primary, [0x5B, 0x8D, 0xEF]);

        let modrinth = ThemeConfig::preset_modrinth_emerald();
        assert_eq!(modrinth.preset, ThemePreset::ModrinthEmerald);
        assert_eq!(modrinth.accent_primary, [0x1B, 0xD9, 0x6A]);

        let cat = ThemeConfig::preset_catppuccin_mocha();
        assert_eq!(cat.preset, ThemePreset::CatppuccinMocha);
        assert_eq!(cat.accent_primary, [0x89, 0xB4, 0xFA]);

        let nord = ThemeConfig::preset_nord();
        assert_eq!(nord.preset, ThemePreset::Nord);
        assert_eq!(nord.accent_primary, [0x88, 0xC0, 0xD0]);

        let light = ThemeConfig::preset_light_clean();
        assert_eq!(light.preset, ThemePreset::LightClean);
        assert_eq!(light.accent_primary, [0x05, 0x96, 0x69]);
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
