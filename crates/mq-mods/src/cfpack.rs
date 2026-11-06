#![deny(clippy::await_holding_lock)]
//! CurseForge `.zip` modpack parser and multi-threaded installer.
//!
//! Validates `manifest.json`, resolves project/file download URLs, downloads mod files
//! in parallel into the CAS via [`NetEngine`], and safely extracts `overrides/` into the instance.

use std::fs::{self, File};
use std::io::Read;
use std::path::Path;

use mq_core::{ModLoaderKind, MqError, MqPaths, MqResult};
use mq_net::{DownloadSpec, NetEngine};
use mq_sched::{ComputePool, EpochGuard};
use serde::{Deserialize, Serialize};
use zip::ZipArchive;

use crate::curseforge::CurseForgeClient;
use crate::zip_safe::{extract_zip_prefix_safely, validate_zip_archive};

/// Parsed representation of CurseForge `manifest.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CurseForgeManifest {
    pub minecraft: CfMinecraftInfo,
    #[serde(rename = "manifestType")]
    pub manifest_type: Option<String>,
    #[serde(rename = "manifestVersion")]
    pub manifest_version: Option<u32>,
    pub name: String,
    pub version: String,
    pub author: Option<String>,
    pub files: Vec<CfManifestFile>,
    pub overrides: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CfMinecraftInfo {
    pub version: String,
    #[serde(rename = "modLoaders")]
    pub mod_loaders: Vec<CfModLoaderEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CfModLoaderEntry {
    pub id: String,
    pub primary: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CfManifestFile {
    #[serde(rename = "projectID")]
    pub project_id: u64,
    #[serde(rename = "fileID")]
    pub file_id: u64,
    #[serde(default = "default_true")]
    pub required: bool,
    #[serde(rename = "downloadUrl")]
    pub download_url: Option<String>,
}

fn default_true() -> bool {
    true
}

impl CurseForgeManifest {
    pub fn loader_and_version(&self) -> (ModLoaderKind, Option<String>) {
        for l in &self.minecraft.mod_loaders {
            let lower = l.id.to_ascii_lowercase();
            if lower.starts_with("fabric-") {
                return (
                    ModLoaderKind::Fabric,
                    Some(lower.trim_start_matches("fabric-").to_string()),
                );
            } else if lower.starts_with("quilt-") {
                return (
                    ModLoaderKind::Quilt,
                    Some(lower.trim_start_matches("quilt-").to_string()),
                );
            } else if lower.starts_with("forge-") {
                return (
                    ModLoaderKind::Forge,
                    Some(lower.trim_start_matches("forge-").to_string()),
                );
            } else if lower.starts_with("neoforge-") {
                return (
                    ModLoaderKind::NeoForge,
                    Some(lower.trim_start_matches("neoforge-").to_string()),
                );
            }
        }
        (ModLoaderKind::Vanilla, None)
    }
}

/// Parses and pre-validates `manifest.json` from a CurseForge `.zip` archive.
pub fn parse_curseforge_manifest_from_zip(zip_path: &Path) -> MqResult<CurseForgeManifest> {
    let file = File::open(zip_path)?;
    let mut archive = ZipArchive::new(file).map_err(|e| {
        MqError::Archive(format!(
            "failed to open CurseForge zip {}: {e}",
            zip_path.display()
        ))
    })?;

    validate_zip_archive(&mut archive, Path::new("."))?;

    let entry = archive
        .by_name("manifest.json")
        .map_err(|e| MqError::Archive(format!("manifest.json not found in CurseForge zip: {e}")))?;

    let mut content = Vec::with_capacity(entry.size().min(8 * 1024 * 1024) as usize);
    entry.take(8 * 1024 * 1024).read_to_end(&mut content)?;

    let manifest: CurseForgeManifest = serde_json::from_slice(&content)
        .map_err(|e| MqError::Metadata(format!("failed to parse CurseForge manifest.json: {e}")))?;

    Ok(manifest)
}

/// Installs a CurseForge modpack `.zip` archive into the target instance directory.
pub async fn install_curseforge_zip(
    zip_path: &Path,
    instance_dir: &Path,
    _paths: &MqPaths,
    net: &NetEngine,
    cf_client: Option<&CurseForgeClient>,
    _compute_pool: &ComputePool,
    epoch_guard: &EpochGuard,
) -> MqResult<usize> {
    let manifest = parse_curseforge_manifest_from_zip(zip_path)?;
    let mods_dir = instance_dir.join("mods");
    fs::create_dir_all(&mods_dir)?;

    let mut download_specs = Vec::new();
    let mut link_pairs = Vec::new();

    for f in &manifest.files {
        if !f.required {
            continue;
        }

        // Determine URL
        let url = if let Some(u) = &f.download_url {
            u.clone()
        } else if let Some(client) = cf_client {
            // Query files from CurseForge API
            let files = client.get_mod_files(f.project_id, None, None).await?;
            let matched = files
                .iter()
                .find(|ver| ver.id == f.file_id.to_string())
                .and_then(|ver| ver.files.first())
                .map(|file| file.url.clone());

            matched.unwrap_or_else(|| {
                crate::curseforge::compute_curseforge_cdn_fallback_url(
                    f.file_id,
                    &format!("{}.jar", f.file_id),
                )
            })
        } else {
            crate::curseforge::compute_curseforge_cdn_fallback_url(
                f.file_id,
                &format!("{}.jar", f.file_id),
            )
        };

        let file_name = format!("{}.jar", f.file_id);
        let dest_jar = mods_dir.join(&file_name);

        download_specs.push(DownloadSpec {
            id: format!("cf_{}_{}", f.project_id, f.file_id),
            urls: vec![url],
            dest_path: dest_jar.clone(),
            expected_sha1: None,
            expected_sha512: None,
            expected_size: None,
        });

        link_pairs.push(dest_jar);
    }

    if !download_specs.is_empty() {
        net.download_batch(download_specs, "install_cfpack", epoch_guard)
            .await?;
    }

    // Safely extract overrides
    let overrides_prefix = manifest.overrides.as_deref().unwrap_or("overrides");
    let extracted_overrides = extract_zip_prefix_safely(zip_path, overrides_prefix, instance_dir)?;

    Ok(link_pairs.len() + extracted_overrides)
}
