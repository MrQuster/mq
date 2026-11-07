//! `mods/.mq-lock.json` lockfile implementation for tracking dependencies and updates.

use std::collections::HashMap;
use std::fs;
use std::path::Path;

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModLockEntry {
    pub project_id: String,
    pub version_id: String,
    pub filename: String,
    pub sha512: String,
    pub installed_as_dependency: bool,
    pub required_by: Vec<String>,
}

impl ModLockFile {
    pub fn load_or_default(mods_dir: &Path) -> Self {
        let path = mods_dir.join(LOCKFILE_NAME);
        if let Ok(content) = fs::read_to_string(&path) {
            if let Ok(lock) = serde_json::from_str(&content) {
                return lock;
            }
        }
        Self::default()
    }

    pub fn save(&self, mods_dir: &Path) -> Result<()> {
        let path = mods_dir.join(LOCKFILE_NAME);
        let content =
            serde_json::to_string_pretty(self).context("Failed to serialize mod lockfile")?;
        fs::write(&path, content).context("Failed to write mod lockfile")?;
        Ok(())
    }

    pub fn record_install(&mut self, slug_or_id: impl Into<String>, entry: ModLockEntry) {
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

    /// Identify dependencies that are not required by any explicitly installed mods.
    pub fn find_unused_dependencies(&self) -> Vec<String> {
        let mut unused = Vec::new();
        for (key, entry) in &self.entries {
            if entry.installed_as_dependency && entry.required_by.is_empty() {
                unused.push(key.clone());
            }
        }
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
                project_id: "sodium".into(),
                version_id: "v1".into(),
                filename: "sodium.jar".into(),
                sha512: "abc".into(),
                installed_as_dependency: false,
                required_by: vec![],
            },
        );
        lock.record_install(
            "fabric-api",
            ModLockEntry {
                project_id: "fapi".into(),
                version_id: "v2".into(),
                filename: "fapi.jar".into(),
                sha512: "def".into(),
                installed_as_dependency: true,
                required_by: vec!["sodium".into()],
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
}
