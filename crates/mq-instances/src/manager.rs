//! Lifecycle manager (`InstanceManager`) for isolated Minecraft instances: creation,
//! atomic TOML persistence, slug/display-name lookup, deterministic listing, hardlink cloning,
//! and deletion.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use chrono::Utc;
use mq_core::{AppConfig, InstanceId, ModLoaderKind, MqError, MqPaths, MqResult};
use serde::de::DeserializeOwned;
use serde::Serialize;

use crate::manifest::{
    InstanceManifest, INSTANCE_MANIFEST_FILE, INSTANCE_SUBDIRS, RESOLVED_VERSION_FILE,
};

static MANIFEST_TMP_SEQ: AtomicU64 = AtomicU64::new(0);

/// High-level manager for isolated Minecraft instances under `<data_dir>/instances/<id>`.
#[derive(Debug, Clone)]
pub struct InstanceManager {
    paths: MqPaths,
}

impl InstanceManager {
    /// Creates an [`InstanceManager`] bound to `paths`.
    #[inline]
    pub fn new(paths: MqPaths) -> Self {
        Self { paths }
    }

    /// Borrows the underlying [`MqPaths`].
    #[inline]
    pub fn paths(&self) -> &MqPaths {
        &self.paths
    }

    /// Returns `<data_dir>/instances/<slug>` for an instance slug or ID.
    #[inline]
    pub fn instance_dir(&self, slug: &str) -> PathBuf {
        self.paths.instance_dir(slug)
    }

    /// Returns the isolated Minecraft `--gameDir` (`<data_dir>/instances/<slug>`).
    #[inline]
    pub fn game_dir(&self, slug: &str) -> PathBuf {
        self.paths.instance_dir(slug)
    }

    /// Returns `<data_dir>/instances/<slug>/mods`.
    #[inline]
    pub fn mods_dir(&self, slug: &str) -> PathBuf {
        self.paths.instance_dir(slug).join("mods")
    }

    /// Returns `<data_dir>/instances/<slug>/config`.
    #[inline]
    pub fn config_dir(&self, slug: &str) -> PathBuf {
        self.paths.instance_dir(slug).join("config")
    }

    /// Returns `<data_dir>/instances/<slug>/resourcepacks`.
    #[inline]
    pub fn resourcepacks_dir(&self, slug: &str) -> PathBuf {
        self.paths.instance_dir(slug).join("resourcepacks")
    }

    /// Returns `<data_dir>/instances/<slug>/shaderpacks`.
    #[inline]
    pub fn shaderpacks_dir(&self, slug: &str) -> PathBuf {
        self.paths.instance_dir(slug).join("shaderpacks")
    }

    /// Returns `<data_dir>/instances/<slug>/natives`.
    #[inline]
    pub fn natives_dir(&self, slug: &str) -> PathBuf {
        self.paths.instance_dir(slug).join("natives")
    }

    /// Returns `<data_dir>/instances/<slug>/resolved_version.json`.
    #[inline]
    pub fn resolved_version_json_path(&self, slug: &str) -> PathBuf {
        self.paths.instance_dir(slug).join(RESOLVED_VERSION_FILE)
    }

    /// Creates a new isolated instance with standard subdirectories (`mods/`, `config/`,
    /// `resourcepacks/`, `shaderpacks/`, `natives/`) and atomically writes `instance.toml`.
    ///
    /// Inherits default memory, JVM arguments, and wrapper command from `config.toml` when present.
    pub fn create_instance(
        &self,
        name: &str,
        mc_version: &str,
        loader: ModLoaderKind,
        loader_version: Option<String>,
    ) -> MqResult<InstanceManifest> {
        let trimmed_name = name.trim();
        if trimmed_name.is_empty() {
            return Err(MqError::Metadata(
                "instance name must not be empty".to_owned(),
            ));
        }

        let trimmed_mc = mc_version.trim();
        if trimmed_mc.is_empty() {
            return Err(MqError::Metadata(
                "Minecraft version must not be empty".to_owned(),
            ));
        }

        let cfg = AppConfig::load_or_default(&self.paths).unwrap_or_default();
        let mut manifest = InstanceManifest::new(trimmed_name, trimmed_mc, loader, loader_version);
        manifest.memory_min_mb = cfg.default_memory_min_mb;
        manifest.memory_max_mb = cfg.default_memory_max_mb;
        manifest.jvm_args = cfg.default_jvm_args;
        manifest.wrapper_cmd = cfg.default_wrapper;

        self.create_from_manifest(manifest)
    }

    /// Creates the on-disk directory hierarchy and persists a pre-built [`InstanceManifest`],
    /// rejecting duplicate instance slugs or case-insensitive display names.
    pub fn create_from_manifest(&self, manifest: InstanceManifest) -> MqResult<InstanceManifest> {
        if manifest.name.trim().is_empty() {
            return Err(MqError::Metadata(
                "instance name must not be empty".to_owned(),
            ));
        }
        if manifest.mc_version.trim().is_empty() {
            return Err(MqError::Metadata(
                "Minecraft version must not be empty".to_owned(),
            ));
        }

        let inst_dir = manifest.instance_dir(&self.paths);
        if inst_dir.exists() {
            return Err(MqError::InstanceAlreadyExists(manifest.id.to_string()));
        }

        for existing in self.list_instances()? {
            if existing.id == manifest.id
                || existing.name.eq_ignore_ascii_case(manifest.name.trim())
            {
                return Err(MqError::InstanceAlreadyExists(manifest.name.clone()));
            }
        }

        Self::ensure_instance_subdirs(&inst_dir)?;
        self.save_instance(&manifest)?;
        Ok(manifest)
    }

    /// Ensures `<inst_dir>/`, `mods/`, `config/`, `resourcepacks/`, `shaderpacks/`, and `natives/`
    /// exist on disk.
    pub fn ensure_instance_subdirs(inst_dir: &Path) -> MqResult<()> {
        fs::create_dir_all(inst_dir)?;
        for sub in INSTANCE_SUBDIRS {
            fs::create_dir_all(inst_dir.join(sub))?;
        }
        Ok(())
    }

    /// Loads an [`InstanceManifest`] by exact `InstanceId` slug OR case-insensitive display `name`.
    pub fn load_instance(&self, id_or_name: &str) -> MqResult<InstanceManifest> {
        let query = id_or_name.trim();
        if query.is_empty() {
            return Err(MqError::InstanceNotFound(id_or_name.to_owned()));
        }

        // 1. Fast path: exact valid slug directory on disk.
        if InstanceId::is_valid_slug(query) {
            let direct_toml = self.paths.instance_dir(query).join(INSTANCE_MANIFEST_FILE);
            if direct_toml.is_file() {
                return Self::read_manifest_file(&direct_toml, Some(query));
            }
        }

        // 2. Scan all instances to match case-insensitive slug or case-insensitive display name.
        let all = self.list_instances()?;
        if let Some(found) = all.iter().find(|m| {
            m.id.as_str().eq_ignore_ascii_case(query) || m.name.eq_ignore_ascii_case(query)
        }) {
            return Ok(found.clone());
        }

        // 3. Also check if sanitized slug matches an existing instance ID.
        let sanitized = InstanceId::sanitize_slug(query);
        if let Some(found) = all.into_iter().find(|m| m.id == sanitized) {
            return Ok(found);
        }

        Err(MqError::InstanceNotFound(id_or_name.to_owned()))
    }

    /// Atomically serializes and writes `manifest` to `<data_dir>/instances/<id>/instance.toml`
    /// (`write .tmp -> fsync -> rename`) and ensures standard subdirectories exist.
    pub fn save_instance(&self, manifest: &InstanceManifest) -> MqResult<()> {
        let inst_dir = manifest.instance_dir(&self.paths);
        Self::ensure_instance_subdirs(&inst_dir)?;

        let manifest_path = manifest.manifest_path(&self.paths);
        let serialized = toml::to_string_pretty(manifest)?;

        let seq = MANIFEST_TMP_SEQ.fetch_add(1, Ordering::Relaxed);
        let tmp_path = inst_dir.join(format!(
            ".{INSTANCE_MANIFEST_FILE}.{}.{seq}.tmp",
            std::process::id()
        ));

        let write_res = (|| -> std::io::Result<()> {
            let mut file = fs::File::create(&tmp_path)?;
            file.write_all(serialized.as_bytes())?;
            file.sync_all()?;
            drop(file);
            fs::rename(&tmp_path, &manifest_path)?;
            Ok(())
        })();

        if let Err(err) = write_res {
            let _ = fs::remove_file(&tmp_path);
            return Err(MqError::Io(err));
        }

        Ok(())
    }

    /// Lists all valid instances under `<data_dir>/instances`, sorted deterministically
    /// by `(name, id)`.
    pub fn list_instances(&self) -> MqResult<Vec<InstanceManifest>> {
        let instances_root = self.paths.instances_dir();
        if !instances_root.exists() {
            return Ok(Vec::new());
        }

        let mut instances = Vec::new();
        for entry_res in fs::read_dir(&instances_root)? {
            let entry = entry_res?;
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            let Some(dir_name) = path.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            if !InstanceId::is_valid_slug(dir_name) {
                continue;
            }
            let manifest_file = path.join(INSTANCE_MANIFEST_FILE);
            if !manifest_file.is_file() {
                continue;
            }

            let manifest = Self::read_manifest_file(&manifest_file, Some(dir_name))?;
            instances.push(manifest);
        }

        instances.sort_by(|a, b| a.name.cmp(&b.name).then_with(|| a.id.cmp(&b.id)));
        Ok(instances)
    }

    /// Clones an existing instance (resolved by slug or display name) into a new instance
    /// named `new_name`, linking `mods/`, `config/`, `resourcepacks/`, `shaderpacks/`, `natives/`,
    /// and `resolved_version.json` via hardlinks with automatic file-copy fallback.
    pub fn clone_instance(
        &self,
        src_id_or_name: &str,
        new_name: &str,
    ) -> MqResult<InstanceManifest> {
        let trimmed_new_name = new_name.trim();
        if trimmed_new_name.is_empty() {
            return Err(MqError::Metadata(
                "cloned instance name must not be empty".to_owned(),
            ));
        }

        let src_manifest = self.load_instance(src_id_or_name)?;
        let new_id = InstanceId::sanitize_slug(trimmed_new_name);
        if new_id == src_manifest.id {
            return Err(MqError::InstanceAlreadyExists(new_id.to_string()));
        }

        let dst_dir = self.paths.instance_dir(new_id.as_str());
        if dst_dir.exists() {
            return Err(MqError::InstanceAlreadyExists(new_id.to_string()));
        }

        for existing in self.list_instances()? {
            if existing.id == new_id || existing.name.eq_ignore_ascii_case(trimmed_new_name) {
                return Err(MqError::InstanceAlreadyExists(trimmed_new_name.to_owned()));
            }
        }

        let src_dir = src_manifest.instance_dir(&self.paths);
        Self::ensure_instance_subdirs(&dst_dir)?;
        clone_tree_hardlink_or_copy(&src_dir, &dst_dir)?;

        let cloned_manifest = InstanceManifest {
            id: new_id,
            name: trimmed_new_name.to_owned(),
            mc_version: src_manifest.mc_version,
            loader: src_manifest.loader,
            loader_version: src_manifest.loader_version,
            memory_min_mb: src_manifest.memory_min_mb,
            memory_max_mb: src_manifest.memory_max_mb,
            java_path: src_manifest.java_path,
            jvm_args: src_manifest.jvm_args,
            wrapper_cmd: src_manifest.wrapper_cmd,
            created_at: Utc::now(),
            last_played: None,
            playtime_secs: 0,
            modpack_source: src_manifest.modpack_source,
        };

        self.save_instance(&cloned_manifest)?;
        Ok(cloned_manifest)
    }

    /// Deletes an instance directory (`<data_dir>/instances/<id>`) by slug or display name.
    pub fn delete_instance(&self, id_or_name: &str) -> MqResult<()> {
        let manifest = self.load_instance(id_or_name)?;
        let inst_dir = manifest.instance_dir(&self.paths);
        if inst_dir.exists() {
            fs::remove_dir_all(&inst_dir)?;
        }
        Ok(())
    }

    /// Updates `last_played` to the current UTC timestamp and adds `added_playtime_secs`
    /// to `playtime_secs`, persisting the updated manifest atomically.
    pub fn touch_last_played(
        &self,
        id_or_name: &str,
        added_playtime_secs: u64,
    ) -> MqResult<InstanceManifest> {
        let mut manifest = self.load_instance(id_or_name)?;
        manifest.last_played = Some(Utc::now());
        manifest.playtime_secs = manifest.playtime_secs.saturating_add(added_playtime_secs);
        self.save_instance(&manifest)?;
        Ok(manifest)
    }

    /// Saves `resolved_version.json` inside the specified instance directory.
    pub fn save_resolved_version<T: Serialize>(
        &self,
        id_or_name: &str,
        version: &T,
    ) -> MqResult<PathBuf> {
        let manifest = self.load_instance(id_or_name)?;
        manifest.save_resolved_version(&self.paths, version)
    }

    /// Loads `resolved_version.json` from the specified instance directory.
    pub fn load_resolved_version<T: DeserializeOwned>(&self, id_or_name: &str) -> MqResult<T> {
        let manifest = self.load_instance(id_or_name)?;
        manifest.load_resolved_version(&self.paths)
    }

    fn read_manifest_file(path: &Path, expected_slug: Option<&str>) -> MqResult<InstanceManifest> {
        let raw = fs::read_to_string(path)?;
        let mut manifest: InstanceManifest = toml::from_str(&raw)?;
        if let Some(slug) = expected_slug {
            if manifest.id.as_str() != slug {
                manifest.id = InstanceId::new(slug)?;
            }
        }
        Ok(manifest)
    }
}

fn clone_tree_hardlink_or_copy(src_dir: &Path, dst_dir: &Path) -> MqResult<()> {
    if !src_dir.is_dir() {
        return Ok(());
    }

    fs::create_dir_all(dst_dir)?;

    for entry_res in fs::read_dir(src_dir)? {
        let entry = entry_res?;
        let file_name = entry.file_name();
        let name_str = file_name.to_string_lossy();

        // Skip the source instance.toml and any transient atomic .tmp files.
        if name_str == INSTANCE_MANIFEST_FILE
            || (name_str.starts_with('.') && name_str.ends_with(".tmp"))
        {
            continue;
        }

        let src_path = entry.path();
        let dst_path = dst_dir.join(&file_name);
        let file_type = entry.file_type()?;

        if file_type.is_dir() {
            clone_tree_hardlink_or_copy(&src_path, &dst_path)?;
        } else if file_type.is_file() {
            if dst_path.exists() {
                let _ = fs::remove_file(&dst_path);
            }
            if fs::hard_link(&src_path, &dst_path).is_err() {
                fs::copy(&src_path, &dst_path)?;
            }
        }
    }

    Ok(())
}
