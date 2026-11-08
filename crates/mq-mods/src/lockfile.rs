//! `.mq-lock.json` lockfile implementation for tracking installed mods, resourcepacks,
//! shaderpacks, dependencies, origins (`modrinth` | `curseforge` | `manual`), and updates.

use std::collections::HashMap;
use std::fs;
use std::path::Path;

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::models::{LockSource, ProjectKind};

pub const LOCKFILE_NAME: &str = ".mq-lock.json";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModLockFile {
    pub version: u32,
    pub updated_at: DateTime<Utc>,
    pub entries: HashMap<String, ModLockEntry>,
}

impl Default for ModLockFile {
    fn default() -> Self {
        Self {
            version: 1,
            updated_at: Utc::now(),
            entries: HashMap::new(),
        }
    }
}

#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModLockEntry {
    #[serde(default)]
    pub source: LockSource,
    #[serde(default)]
    pub project_id: String,
    #[serde(default)]
    pub version_id: String,
    #[serde(default)]
    pub file_id: Option<String>,
    pub filename: String,
    #[serde(default)]
    pub sha1: Option<String>,
    #[serde(default)]
    pub sha512: String,
    #[serde(default)]
    pub installed_as_dependency: bool,
    #[serde(default)]
    pub required_by: Vec<String>,
    #[serde(default)]
    pub kind: ProjectKind,
}

impl ModLockEntry {
    /// Creates a new [`ModLockEntry`] with the given source, project ID, version ID, and filename.
    pub fn new(
        source: LockSource,
        project_id: impl Into<String>,
        version_id: impl Into<String>,
        filename: impl Into<String>,
    ) -> Self {
        let vid = version_id.into();
        let file_id = (!vid.is_empty()).then(|| vid.clone());
        Self {
            source,
            project_id: project_id.into(),
            version_id: vid,
            file_id,
            filename: filename.into(),
            sha1: None,
            sha512: String::new(),
            installed_as_dependency: false,
            required_by: Vec::new(),
            kind: ProjectKind::Mod,
        }
    }

    /// Sets the content kind (`Mod`, `ResourcePack`, `Shader`, `Modpack`).
    pub fn with_kind(mut self, kind: ProjectKind) -> Self {
        self.kind = kind;
        self
    }

    /// Sets the SHA-1 digest hex string.
    pub fn with_sha1(mut self, sha1: Option<String>) -> Self {
        self.sha1 = sha1;
        self
    }

    /// Sets the SHA-512 digest hex string.
    pub fn with_sha512(mut self, sha512: impl Into<String>) -> Self {
        self.sha512 = sha512.into();
        self
    }

    /// Sets the provider file ID.
    pub fn with_file_id(mut self, file_id: Option<String>) -> Self {
        self.file_id = file_id;
        self
    }

    /// Normalizes `version_id` and `file_id` when deserialized from legacy or partial JSON.
    pub fn normalize(&mut self) {
        if self.version_id.is_empty() {
            if let Some(ref fid) = self.file_id {
                self.version_id = fid.clone();
            }
        } else if self.file_id.is_none() {
            self.file_id = Some(self.version_id.clone());
        }
    }
}

impl ModLockFile {
    pub fn load_or_default(mods_dir: &Path) -> Self {
        let direct_path = mods_dir.join(LOCKFILE_NAME);
        if let Ok(content) = fs::read_to_string(&direct_path) {
            if let Ok(mut lock) = serde_json::from_str::<Self>(&content) {
                for entry in lock.entries.values_mut() {
                    entry.normalize();
                }
                return lock;
            }
        }

        // Also check `<instance_dir>/mods/.mq-lock.json` when `mods_dir` is an instance root.
        let nested_path = mods_dir.join("mods").join(LOCKFILE_NAME);
        if let Ok(content) = fs::read_to_string(&nested_path) {
            if let Ok(mut lock) = serde_json::from_str::<Self>(&content) {
                for entry in lock.entries.values_mut() {
                    entry.normalize();
                }
                return lock;
            }
        }

        Self::default()
    }

    /// Loads the unified lockfile for an instance, merging `<instance_dir>/.mq-lock.json`
    /// and `<instance_dir>/mods/.mq-lock.json` for backwards compatibility.
    pub fn load_for_instance(instance_dir: &Path) -> Self {
        let mut lock = Self::load_or_default(instance_dir);
        let mods_lock_path = instance_dir.join("mods").join(LOCKFILE_NAME);
        if let Ok(content) = fs::read_to_string(&mods_lock_path) {
            if let Ok(mut mods_lock) = serde_json::from_str::<Self>(&content) {
                for (k, mut v) in mods_lock.entries.drain() {
                    v.normalize();
                    lock.entries.entry(k).or_insert(v);
                }
            }
        }
        lock
    }

    pub fn save(&self, mods_dir: &Path) -> Result<()> {
        fs::create_dir_all(mods_dir).context("Failed to create lockfile directory")?;
        let path = mods_dir.join(LOCKFILE_NAME);
        let content =
            serde_json::to_string_pretty(self).context("Failed to serialize mod lockfile")?;
        fs::write(&path, content).context("Failed to write mod lockfile")?;
        Ok(())
    }

    /// Saves the lockfile to `<instance_dir>/.mq-lock.json` and mirrors it to `<instance_dir>/mods/.mq-lock.json`
    /// if the `mods/` directory exists.
    pub fn save_for_instance(&self, instance_dir: &Path) -> Result<()> {
        self.save(instance_dir)?;
        let mods_dir = instance_dir.join("mods");
        if mods_dir.is_dir() {
            self.save(&mods_dir)?;
        }
        Ok(())
    }

    pub fn record_install(&mut self, slug_or_id: impl Into<String>, mut entry: ModLockEntry) {
        entry.normalize();
        self.entries.insert(slug_or_id.into(), entry);
        self.updated_at = Utc::now();
    }

    pub fn remove_entry(&mut self, slug_or_id: &str) -> Option<ModLockEntry> {
        let removed = self.entries.remove(slug_or_id);
        if removed.is_some() {
            // Remove from required_by lists of other dependencies
            for entry in self.entries.values_mut() {
                entry.required_by.retain(|r| r != slug_or_id);
            }
            self.updated_at = Utc::now();
        }
        removed
    }

    /// Finds an entry by its installed filename (or `.disabled` variant).
    pub fn find_by_filename(&self, filename: &str) -> Option<(&String, &ModLockEntry)> {
        let clean = filename.trim_end_matches(".disabled");
        self.entries.iter().find(|(_, entry)| {
            let entry_clean = entry.filename.trim_end_matches(".disabled");
            entry.filename == filename || entry_clean == clean
        })
    }

    /// Finds a mutable entry by its installed filename (or `.disabled` variant).
    pub fn find_by_filename_mut(&mut self, filename: &str) -> Option<&mut ModLockEntry> {
        let clean = filename.trim_end_matches(".disabled");
        self.entries.values_mut().find(|entry| {
            let entry_clean = entry.filename.trim_end_matches(".disabled");
            entry.filename == filename || entry_clean == clean
        })
    }

    /// Finds an entry matching either SHA-1 or SHA-512 hex string.
    pub fn find_by_hash(
        &self,
        sha1: Option<&str>,
        sha512: Option<&str>,
    ) -> Option<(&String, &ModLockEntry)> {
        self.entries.iter().find(|(_, entry)| {
            let sha1_match = match (sha1, entry.sha1.as_deref()) {
                (Some(a), Some(b)) if !a.is_empty() && !b.is_empty() => a.eq_ignore_ascii_case(b),
                _ => false,
            };
            let sha512_match = match sha512 {
                Some(a) if !a.is_empty() && !entry.sha512.is_empty() => {
                    a.eq_ignore_ascii_case(&entry.sha512)
                }
                _ => false,
            };
            sha1_match || sha512_match
        })
    }

    /// Returns all entries matching the given [`ProjectKind`].
    pub fn entries_by_kind(&self, kind: ProjectKind) -> Vec<(&String, &ModLockEntry)> {
        self.entries
            .iter()
            .filter(|(_, e)| e.kind == kind)
            .collect()
    }

    /// Identify dependencies that are not required by any explicitly installed mods.
    pub fn find_unused_dependencies(&self) -> Vec<String> {
        let mut unused = Vec::new();
        for (key, entry) in &self.entries {
            if entry.installed_as_dependency && entry.required_by.is_empty() {
                unused.push(key.clone());
            }
        }
        unused.sort();
        unused
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lockfile_roundtrip_and_unused() {
        let tmp = tempfile::tempdir().unwrap();
        let mut lock = ModLockFile::default();
        lock.record_install(
            "sodium",
            ModLockEntry {
                source: LockSource::Modrinth,
                project_id: "sodium".into(),
                version_id: "v1".into(),
                file_id: Some("v1".into()),
                filename: "sodium.jar".into(),
                sha1: Some("111".into()),
                sha512: "abc".into(),
                installed_as_dependency: false,
                required_by: vec![],
                kind: ProjectKind::Mod,
            },
        );
        lock.record_install(
            "fabric-api",
            ModLockEntry {
                source: LockSource::Modrinth,
                project_id: "fapi".into(),
                version_id: "v2".into(),
                file_id: Some("v2".into()),
                filename: "fapi.jar".into(),
                sha1: Some("222".into()),
                sha512: "def".into(),
                installed_as_dependency: true,
                required_by: vec!["sodium".into()],
                kind: ProjectKind::Mod,
            },
        );
        lock.save(tmp.path()).unwrap();

        let loaded = ModLockFile::load_or_default(tmp.path());
        assert_eq!(loaded.entries.len(), 2);
        assert!(loaded.find_unused_dependencies().is_empty());

        let mut lock_mut = loaded;
        lock_mut.remove_entry("sodium");
        assert_eq!(lock_mut.find_unused_dependencies(), vec!["fabric-api"]);
    }

    #[test]
    fn test_legacy_lockfile_backwards_compatibility() {
        let tmp = tempfile::tempdir().unwrap();
        let legacy_json = r#"{
            "version": 1,
            "updated_at": "2026-01-01T00:00:00Z",
            "entries": {
                "sodium": {
                    "project_id": "AANobbMI",
                    "version_id": "ver-1",
                    "filename": "sodium.jar",
                    "sha512": "deadbeef",
                    "installed_as_dependency": false,
                    "required_by": []
                }
            }
        }"#;
        fs::write(tmp.path().join(LOCKFILE_NAME), legacy_json).unwrap();

        let loaded = ModLockFile::load_or_default(tmp.path());
        let entry = loaded.entries.get("sodium").expect("sodium entry");
        assert_eq!(entry.source, LockSource::Modrinth);
        assert_eq!(entry.kind, ProjectKind::Mod);
        assert_eq!(entry.file_id.as_deref(), Some("ver-1"));
        assert_eq!(entry.sha1, None);
    }
}
