//! Instance manifest (`instance.toml`) domain model and directory layout helpers.

use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use chrono::{DateTime, Utc};
use mq_core::{InstanceId, ModLoaderKind, MqError, MqPaths, MqResult};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

static RESOLVED_TMP_SEQ: AtomicU64 = AtomicU64::new(0);

/// Default minimum JVM heap allocation in MiB (`-Xms1024M`).
pub const DEFAULT_MEMORY_MIN_MB: u32 = 1024;

/// Default maximum JVM heap allocation in MiB (`-Xmx4096M`).
pub const DEFAULT_MEMORY_MAX_MB: u32 = 4096;

/// Canonical filename for the per-instance TOML manifest (`instance.toml`).
pub const INSTANCE_MANIFEST_FILE: &str = "instance.toml";

/// Canonical filename for the cached resolved version metadata (`resolved_version.json`).
pub const RESOLVED_VERSION_FILE: &str = "resolved_version.json";

/// Subdirectories created automatically inside every isolated instance directory.
pub const INSTANCE_SUBDIRS: [&str; 5] =
    ["mods", "config", "resourcepacks", "shaderpacks", "natives"];

const fn default_memory_min_mb() -> u32 {
    DEFAULT_MEMORY_MIN_MB
}

const fn default_memory_max_mb() -> u32 {
    DEFAULT_MEMORY_MAX_MB
}

/// Persistent metadata describing an isolated Minecraft game instance, serialized
/// at `<data_dir>/instances/<id>/instance.toml`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstanceManifest {
    /// Validated filesystem-safe directory slug (`[a-z0-9._-]+`).
    pub id: InstanceId,

    /// Human-readable display name.
    pub name: String,

    /// Target Minecraft version identifier (e.g. `"1.21.1"`).
    pub mc_version: String,

    /// Mod loader family (defaults to [`ModLoaderKind::Vanilla`]).
    #[serde(default)]
    pub loader: ModLoaderKind,

    /// Specific mod loader version (e.g. `Some("0.16.9")` for Fabric).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub loader_version: Option<String>,

    /// Minimum JVM heap size in MiB (`-Xms`, defaults to `1024`).
    #[serde(default = "default_memory_min_mb")]
    pub memory_min_mb: u32,

    /// Maximum JVM heap size in MiB (`-Xmx`, defaults to `4096`).
    #[serde(default = "default_memory_max_mb")]
    pub memory_max_mb: u32,

    /// Optional custom path to a `java` binary overriding automatic JVM selection.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub java_path: Option<PathBuf>,

    /// Additional user-configured JVM arguments appended after `-Xms`/`-Xmx`.
    #[serde(default)]
    pub jvm_args: Vec<String>,

    /// Optional process wrapper command prefix (e.g. `"gamemoderun"`, `"mangohud"`, `"prime-run"`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wrapper_cmd: Option<String>,

    /// UTC timestamp when the instance was created.
    pub created_at: DateTime<Utc>,

    /// UTC timestamp when the instance was last launched.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_played: Option<DateTime<Utc>>,

    /// Cumulative recorded playtime in seconds.
    #[serde(default)]
    pub playtime_secs: u64,

    /// Optional origin identifier if imported from a `.mrpack` or CurseForge `.zip` modpack.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub modpack_source: Option<String>,
}

impl InstanceManifest {
    /// Constructs a new [`InstanceManifest`] with sanitized slug ID and standard defaults.
    pub fn new(
        name: impl Into<String>,
        mc_version: impl Into<String>,
        loader: ModLoaderKind,
        loader_version: Option<String>,
    ) -> Self {
        let name_str = name.into().trim().to_owned();
        let id = InstanceId::sanitize_slug(&name_str);
        Self {
            id,
            name: name_str,
            mc_version: mc_version.into().trim().to_owned(),
            loader,
            loader_version: loader_version
                .map(|v| v.trim().to_owned())
                .filter(|v| !v.is_empty()),
            memory_min_mb: DEFAULT_MEMORY_MIN_MB,
            memory_max_mb: DEFAULT_MEMORY_MAX_MB,
            java_path: None,
            jvm_args: Vec::new(),
            wrapper_cmd: None,
            created_at: Utc::now(),
            last_played: None,
            playtime_secs: 0,
            modpack_source: None,
        }
    }

    /// Root directory for this instance (`<data_dir>/instances/<id>`).
    #[inline]
    pub fn instance_dir(&self, paths: &MqPaths) -> PathBuf {
        paths.instance_dir(self.id.as_str())
    }

    /// Isolated Minecraft `--gameDir` for this instance (`<data_dir>/instances/<id>`).
    #[inline]
    pub fn game_dir(&self, paths: &MqPaths) -> PathBuf {
        self.instance_dir(paths)
    }

    /// Path to `<data_dir>/instances/<id>/instance.toml`.
    #[inline]
    pub fn manifest_path(&self, paths: &MqPaths) -> PathBuf {
        self.instance_dir(paths).join(INSTANCE_MANIFEST_FILE)
    }

    /// Path to `<data_dir>/instances/<id>/mods`.
    #[inline]
    pub fn mods_dir(&self, paths: &MqPaths) -> PathBuf {
        self.instance_dir(paths).join("mods")
    }

    /// Path to `<data_dir>/instances/<id>/config`.
    #[inline]
    pub fn config_dir(&self, paths: &MqPaths) -> PathBuf {
        self.instance_dir(paths).join("config")
    }

    /// Path to `<data_dir>/instances/<id>/resourcepacks`.
    #[inline]
    pub fn resourcepacks_dir(&self, paths: &MqPaths) -> PathBuf {
        self.instance_dir(paths).join("resourcepacks")
    }

    /// Path to `<data_dir>/instances/<id>/shaderpacks`.
    #[inline]
    pub fn shaderpacks_dir(&self, paths: &MqPaths) -> PathBuf {
        self.instance_dir(paths).join("shaderpacks")
    }

    /// Path to `<data_dir>/instances/<id>/natives`.
    #[inline]
    pub fn natives_dir(&self, paths: &MqPaths) -> PathBuf {
        self.instance_dir(paths).join("natives")
    }

    /// Path to `<data_dir>/instances/<id>/resolved_version.json` for offline launch resolution.
    #[inline]
    pub fn resolved_version_json_path(&self, paths: &MqPaths) -> PathBuf {
        self.instance_dir(paths).join(RESOLVED_VERSION_FILE)
    }

    /// Atomically persists the cached resolved version metadata (`resolved_version.json`)
    /// inside this instance's directory so `mq launch` works completely offline.
    pub fn save_resolved_version<T: Serialize>(
        &self,
        paths: &MqPaths,
        version: &T,
    ) -> MqResult<PathBuf> {
        let inst_dir = self.instance_dir(paths);
        fs::create_dir_all(&inst_dir)?;
        let target_path = self.resolved_version_json_path(paths);
        let bytes = serde_json::to_vec_pretty(version)?;

        let seq = RESOLVED_TMP_SEQ.fetch_add(1, Ordering::Relaxed);
        let tmp_path = inst_dir.join(format!(
            ".{RESOLVED_VERSION_FILE}.{}.{seq}.tmp",
            std::process::id()
        ));

        let write_res = (|| -> std::io::Result<()> {
            let mut file = fs::File::create(&tmp_path)?;
            file.write_all(&bytes)?;
            file.sync_all()?;
            drop(file);
            fs::rename(&tmp_path, &target_path)?;
            Ok(())
        })();

        if let Err(err) = write_res {
            let _ = fs::remove_file(&tmp_path);
            return Err(MqError::Io(err));
        }

        Ok(target_path)
    }

    /// Loads the cached resolved version metadata (`resolved_version.json`) from this
    /// instance's directory.
    pub fn load_resolved_version<T: DeserializeOwned>(&self, paths: &MqPaths) -> MqResult<T> {
        let path = self.resolved_version_json_path(paths);
        if !path.is_file() {
            return Err(MqError::Metadata(format!(
                "cached resolved_version.json not found for instance '{}' at {}",
                self.id,
                path.display()
            )));
        }
        let raw = fs::read_to_string(&path)?;
        let parsed = serde_json::from_str(&raw)?;
        Ok(parsed)
    }
}
