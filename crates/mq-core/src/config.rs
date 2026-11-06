//! Global launcher configuration (`config.toml`) and environment variable overrides.

use std::env;
use std::fmt;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::str::FromStr;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

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

/// Global application configuration persisted in `~/.config/mq/config.toml`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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
        }
    }
}

impl AppConfig {
    /// Loads `config.toml` from `paths.config_file()`, or returns [`AppConfig::default()`]
    /// if the file does not exist yet.
    pub fn load_or_default(paths: &MqPaths) -> MqResult<Self> {
        let config_path = paths.config_file();
        if !config_path.exists() {
            return Ok(Self::default());
        }
        let raw = fs::read_to_string(&config_path)?;
        let cfg: Self = toml::from_str(&raw)?;
        Ok(cfg)
    }

    /// Atomically persists this configuration to `paths.config_file()` (`write temp -> fsync -> rename`)
    /// with `0600` permissions on Unix systems to protect embedded API keys.
    pub fn save(&self, paths: &MqPaths) -> MqResult<()> {
        let config_path = paths.config_file();
        if let Some(parent) = config_path.parent() {
            fs::create_dir_all(parent)?;
        }

        let serialized = toml::to_string_pretty(self)?;
        let tmp_name = format!(".config.toml.{}.tmp", Uuid::new_v4());
        let tmp_path = paths.config_dir.join(tmp_name);

        let write_res = (|| -> std::io::Result<()> {
            let mut opts = OpenOptions::new();
            opts.write(true).create_new(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                opts.mode(0o600);
            }
            let mut file = opts.open(&tmp_path)?;
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
        };

        // Ensure Debug never leaks the CurseForge API key.
        let dbg = format!("{populated:?}");
        assert!(!dbg.contains("secret_cf_key_value"));
        assert!(dbg.contains("[REDACTED]"));

        populated.save(&paths).expect("save config.toml");
        assert!(paths.config_file().is_file());

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
