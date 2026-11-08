#![deny(clippy::await_holding_lock)]
//! CurseForge `.zip` modpack parser, multi-threaded installer, and blocked-file manual download watcher.
//!
//! - Strictly complies with CurseForge third-party distribution rules (never synthesizes fallback CDN URLs).
//! - Queries project `classId` (`6` -> `mods/`, `12` -> `resourcepacks/`, `6552` -> `shaderpacks/`) and
//!   file metadata (`fileName`, `downloadUrl`, `hashes` SHA-1, `fileLength`).
//! - Verifies allowed HTTPS download hosts and SHA-1 digests, hardlinks from CAS, extracts `overrides/`,
//!   and records `.mq-lock.json` with `LockSource::CurseForge`.
//! - When `downloadUrl` is `null` (blocked), returns [`BlockedDownload`] items (`{websiteUrl}/download/{fileId}`)
//!   and provides [`BlockedDownloadsWatcher`] to watch the user's Downloads folder (polling + file events +
//!   drag-and-drop), verify SHA-1 / MurmurHash2 fingerprint, atomically move matching files into the instance,
//!   and decrement the remaining count.

use std::collections::{BTreeMap, HashMap};
use std::fs::{self, File};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::Duration;

use mq_core::{ModLoaderKind, MqError, MqPaths, MqResult, Sha1Digest};
use mq_net::{DownloadSpec, NetEngine};
use mq_sched::{ComputePool, EpochGuard};
use serde::{Deserialize, Serialize};
use zip::ZipArchive;

use crate::curseforge::{
    build_curseforge_manual_download_url, compute_curseforge_fingerprint_file, CurseForgeClient,
    CurseForgeModDetails,
};
use crate::lockfile::{ModLockEntry, ModLockFile};
use crate::models::{BlockedDownload, LockSource, ProjectKind, ProjectVersion};
use crate::zip_safe::{
    extract_zip_prefix_safely, link_or_copy_cas_blob, stream_hash_file_sha1_sha512,
    validate_archive_entry_path, validate_download_url, validate_zip_archive,
    verify_file_sha1_and_sha512,
};

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
    #[serde(rename = "projectID", alias = "project_id")]
    pub project_id: u64,
    #[serde(rename = "fileID", alias = "file_id")]
    pub file_id: u64,
    #[serde(default = "default_true")]
    pub required: bool,
    #[serde(rename = "downloadUrl", alias = "download_url", default)]
    pub download_url: Option<String>,
    #[serde(rename = "fileName", alias = "file_name", default)]
    pub file_name: Option<String>,
    #[serde(rename = "classId", alias = "class_id", default)]
    pub class_id: Option<u32>,
    #[serde(default)]
    pub sha1: Option<Sha1Digest>,
    #[serde(rename = "fileFingerprint", alias = "fingerprint", default)]
    pub fingerprint: Option<u32>,
    #[serde(rename = "websiteUrl", alias = "website_url", default)]
    pub website_url: Option<String>,
    #[serde(rename = "projectTitle", alias = "project_title", default)]
    pub project_title: Option<String>,
    #[serde(rename = "fileLength", alias = "file_size", default)]
    pub file_size: Option<u64>,
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

/// Detailed outcome returned after installing a CurseForge `.zip` modpack.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CurseForgeInstallOutcome {
    pub manifest: CurseForgeManifest,
    pub minecraft_version: String,
    pub loader: ModLoaderKind,
    pub loader_version: Option<String>,
    pub installed_files: Vec<PathBuf>,
    pub blocked_downloads: Vec<BlockedDownload>,
    pub overrides_extracted: usize,
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

/// Maps a CurseForge `classId` to the target instance subdirectory (`6 -> mods`, `12 -> resourcepacks`, `6552 -> shaderpacks`).
#[inline]
pub const fn class_id_to_project_kind(class_id: Option<u32>) -> ProjectKind {
    match class_id {
        Some(12) => ProjectKind::ResourcePack,
        Some(6552) => ProjectKind::Shader,
        Some(4471) => ProjectKind::Modpack,
        _ => ProjectKind::Mod,
    }
}

/// Constructs a [`BlockedDownload`] descriptor when a CurseForge mod or modpack file has `downloadUrl == null`.
pub fn build_blocked_download_for_mod(
    details: &CurseForgeModDetails,
    version: &ProjectVersion,
    expected_fingerprint: Option<u32>,
) -> Option<BlockedDownload> {
    let file = version.primary_file()?;
    if details.allow_mod_distribution && !file.is_download_blocked() {
        return None;
    }
    let file_id_num = version.id.parse::<u64>().unwrap_or(0);
    let webpage_url = details.file_webpage_url(file_id_num);
    let subfolder = details.hit.project_kind.target_subfolder();
    let target_rel_path = if subfolder.is_empty() {
        file.filename.clone()
    } else {
        format!("{subfolder}/{}", file.filename)
    };

    Some(BlockedDownload {
        project_id: details.hit.project_id.clone(),
        file_id: version.id.clone(),
        project_title: details.hit.title.clone(),
        filename: file.filename.clone(),
        webpage_url,
        expected_size: file.size,
        expected_sha1: file.sha1,
        expected_fingerprint,
        target_rel_path,
    })
}

struct PlannedCfDownload {
    project_id: u64,
    file_id: u64,
    kind: ProjectKind,
    filename: String,
    url: String,
    expected_sha1: Option<Sha1Digest>,
    expected_size: Option<u64>,
    cas_path: PathBuf,
    dest_path: PathBuf,
}

/// Installs a CurseForge modpack `.zip` archive into `instance_dir`, returning full details
/// including installed file paths, extracted overrides count, and any [`BlockedDownload`] entries
/// whose `downloadUrl` is `null`.
pub async fn install_curseforge_modpack(
    zip_path: &Path,
    instance_dir: &Path,
    paths: &MqPaths,
    net: &NetEngine,
    cf_client: Option<&CurseForgeClient>,
    _compute_pool: &ComputePool,
    epoch_guard: &EpochGuard,
) -> MqResult<CurseForgeInstallOutcome> {
    epoch_guard
        .check()
        .map_err(|e| MqError::Cancelled { epoch: e.epoch })?;

    let manifest = parse_curseforge_manifest_from_zip(zip_path)?;
    fs::create_dir_all(instance_dir)?;

    let (loader, loader_version) = manifest.loader_and_version();
    let required_files: Vec<&CfManifestFile> =
        manifest.files.iter().filter(|f| f.required).collect();

    // 1. Batch-fetch project details (`classId`, `websiteUrl`, `allowModDistribution`, `title`, `slug`)
    // and file metadata (`fileName`, `downloadUrl`, `hashes` SHA-1, `fileLength`) when `cf_client` is provided.
    let mut details_map: HashMap<u64, CurseForgeModDetails> = HashMap::new();
    let mut versions_map: HashMap<(u64, u64), ProjectVersion> = HashMap::new();

    if let Some(client) = cf_client {
        let project_ids: Vec<u64> = required_files.iter().map(|f| f.project_id).collect();
        if !project_ids.is_empty() {
            if let Ok(batch_details) = client.get_mod_details_batch(&project_ids).await {
                for d in batch_details {
                    if let Ok(pid) = d.hit.project_id.parse::<u64>() {
                        details_map.insert(pid, d);
                    }
                }
            }
            for &pid in &project_ids {
                if let std::collections::hash_map::Entry::Vacant(e) = details_map.entry(pid) {
                    if let Ok(d) = client.get_mod_details(pid).await {
                        e.insert(d);
                    }
                }
            }
        }

        let file_ids: Vec<u64> = required_files.iter().map(|f| f.file_id).collect();
        if !file_ids.is_empty() {
            if let Ok(batch_files) = client.get_files_batch(&file_ids).await {
                for ver in batch_files {
                    if let (Ok(pid), Ok(fid)) =
                        (ver.project_id.parse::<u64>(), ver.id.parse::<u64>())
                    {
                        versions_map.insert((pid, fid), ver);
                    }
                }
            }
            for f in &required_files {
                let key = (f.project_id, f.file_id);
                if let std::collections::hash_map::Entry::Vacant(e) = versions_map.entry(key) {
                    if let Ok(ver) = client.get_mod_file(f.project_id, f.file_id).await {
                        e.insert(ver);
                    } else if let Ok(all_files) = client.get_mod_files(f.project_id, None, None).await
                    {
                        if let Some(matched) = all_files
                            .into_iter()
                            .find(|v| v.id == f.file_id.to_string())
                        {
                            e.insert(matched);
                        }
                    }
                }
            }
        }
    }

    let mut planned = Vec::with_capacity(required_files.len());
    let mut download_specs = Vec::new();
    let mut blocked_downloads = Vec::new();

    for f in required_files {
        let details = details_map.get(&f.project_id);
        let ver = versions_map.get(&(f.project_id, f.file_id));
        let primary_file = ver.and_then(|v| v.primary_file());

        let kind = f
            .class_id
            .and_then(ProjectKind::from_curseforge_class_id)
            .or_else(|| details.map(|d| d.hit.project_kind))
            .unwrap_or(ProjectKind::Mod);

        let subfolder = kind.target_subfolder();
        let subfolder = if subfolder.is_empty() {
            "mods"
        } else {
            subfolder
        };

        let filename = f
            .file_name
            .clone()
            .or_else(|| primary_file.map(|pf| pf.filename.clone()))
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| {
                let ext = match kind {
                    ProjectKind::ResourcePack | ProjectKind::Shader => "zip",
                    _ => "jar",
                };
                format!("{}.{ext}", f.file_id)
            });

        let target_rel_path = format!("{subfolder}/{filename}");
        let dest_path = validate_archive_entry_path(instance_dir, &target_rel_path, None)?;

        let expected_sha1 = f.sha1.or_else(|| primary_file.and_then(|pf| pf.sha1));
        let expected_size = f
            .file_size
            .or_else(|| primary_file.map(|pf| pf.size))
            .filter(|&sz| sz > 0);
        let allow_dist = details.map(|d| d.allow_mod_distribution).unwrap_or(true);

        let resolved_url = if !allow_dist {
            None
        } else {
            f.download_url
                .as_deref()
                .map(str::trim)
                .filter(|u| !u.is_empty())
                .map(str::to_owned)
                .or_else(|| {
                    primary_file
                        .and_then(|pf| pf.download_url())
                        .map(str::to_owned)
                })
        };

        // Strict compliance: NEVER synthesize edge.forgecdn.net fallback URLs when downloadUrl is null!
        let Some(url) = resolved_url else {
            let project_title = f
                .project_title
                .clone()
                .or_else(|| details.map(|d| d.hit.title.clone()))
                .unwrap_or_else(|| format!("CurseForge Project {}", f.project_id));
            let website_url = f
                .website_url
                .as_deref()
                .or_else(|| details.and_then(|d| d.website_url.as_deref()));
            let slug = details.map(|d| d.hit.slug.as_str());
            let webpage_url =
                build_curseforge_manual_download_url(website_url, slug, kind, f.file_id);

            blocked_downloads.push(BlockedDownload {
                project_id: f.project_id.to_string(),
                file_id: f.file_id.to_string(),
                project_title,
                filename,
                webpage_url,
                expected_size: expected_size.unwrap_or(0),
                expected_sha1,
                expected_fingerprint: f.fingerprint,
                target_rel_path,
            });
            continue;
        };

        // Enforce HTTPS download host allowlist.
        validate_download_url(&url)?;

        let cas_path = match expected_sha1 {
            Some(sha1) => paths.cas_sha1_path(&sha1.to_hex()),
            None => dest_path.clone(),
        };

        // Resume / retry support: skip downloading if already present and verified on disk.
        let mut already_verified = false;
        if dest_path.is_file() {
            if let Some(exp_sha1) = expected_sha1 {
                if verify_file_sha1_and_sha512(&dest_path, &filename, Some(exp_sha1), None).is_ok()
                {
                    if cas_path != dest_path && !cas_path.is_file() {
                        let _ = link_or_copy_cas_blob(&dest_path, &cas_path);
                    }
                    already_verified = true;
                }
            }
        }
        if !already_verified && cas_path != dest_path && cas_path.is_file() {
            if let Some(exp_sha1) = expected_sha1 {
                if verify_file_sha1_and_sha512(&cas_path, &filename, Some(exp_sha1), None).is_ok() {
                    already_verified = true;
                } else {
                    let _ = fs::remove_file(&cas_path);
                }
            }
        }

        if !already_verified {
            if let Some(parent) = cas_path.parent() {
                fs::create_dir_all(parent)?;
            }
            download_specs.push(DownloadSpec {
                id: format!("cf_{}_{}", f.project_id, f.file_id),
                urls: vec![url.clone()],
                dest_path: cas_path.clone(),
                expected_sha1,
                expected_sha512: None,
                expected_size,
            });
        }

        planned.push(PlannedCfDownload {
            project_id: f.project_id,
            file_id: f.file_id,
            kind,
            filename,
            url,
            expected_sha1,
            expected_size,
            cas_path,
            dest_path,
        });
    }

    if !download_specs.is_empty() {
        net.download_batch(download_specs, "install_cfpack", epoch_guard)
            .await?;
    }

    let mut lockfile = ModLockFile::load_for_instance(instance_dir);
    let mut installed_files = Vec::with_capacity(planned.len());

    for item in planned {
        let _ = (&item.url, item.expected_size);
        if item.cas_path != item.dest_path {
            link_or_copy_cas_blob(&item.cas_path, &item.dest_path)?;
        }

        let (actual_sha1, actual_sha512, _) = verify_file_sha1_and_sha512(
            &item.dest_path,
            &item.filename,
            item.expected_sha1,
            None,
        )?;

        lockfile.record_install(
            item.project_id.to_string(),
            ModLockEntry {
                source: LockSource::CurseForge,
                project_id: item.project_id.to_string(),
                version_id: item.file_id.to_string(),
                file_id: Some(item.file_id.to_string()),
                filename: item.filename,
                sha1: Some(actual_sha1.to_hex()),
                sha512: actual_sha512.to_hex(),
                installed_as_dependency: false,
                required_by: Vec::new(),
                kind: item.kind,
            },
        );

        installed_files.push(item.dest_path);
    }

    let _ = lockfile.save_for_instance(instance_dir);

    // Safely extract overrides
    let overrides_prefix = manifest.overrides.as_deref().unwrap_or("overrides");
    let overrides_extracted = extract_zip_prefix_safely(zip_path, overrides_prefix, instance_dir)?;

    let mc_version = manifest.minecraft.version.clone();
    Ok(CurseForgeInstallOutcome {
        manifest,
        minecraft_version: mc_version,
        loader,
        loader_version,
        installed_files,
        blocked_downloads,
        overrides_extracted,
    })
}

/// Installs a CurseForge modpack `.zip` archive into the target instance directory,
/// returning the total count of installed files + extracted overrides.
pub async fn install_curseforge_zip(
    zip_path: &Path,
    instance_dir: &Path,
    paths: &MqPaths,
    net: &NetEngine,
    cf_client: Option<&CurseForgeClient>,
    compute_pool: &ComputePool,
    epoch_guard: &EpochGuard,
) -> MqResult<usize> {
    let outcome = install_curseforge_modpack(
        zip_path,
        instance_dir,
        paths,
        net,
        cf_client,
        compute_pool,
        epoch_guard,
    )
    .await?;
    Ok(outcome.installed_files.len() + outcome.overrides_extracted)
}

/// Service that watches the user's Downloads folder (polling + filesystem event notifications +
/// drag-and-drop ingestion) for manually downloaded CurseForge files whose direct `downloadUrl` was `null`.
///
/// When a matching file (by SHA-1 digest and/or CurseForge MurmurHash2 fingerprint) appears,
/// it is atomically moved into `<instance_dir>/<target_rel_path>`, recorded in `.mq-lock.json`,
/// and the remaining counter (`remaining_count()`) is decremented.
#[derive(Debug, Clone)]
pub struct BlockedDownloadsWatcher {
    instance_dir: PathBuf,
    watch_dir: PathBuf,
    pending: Vec<BlockedDownload>,
    completed: BTreeMap<String, PathBuf>,
}

impl BlockedDownloadsWatcher {
    /// Creates a new [`BlockedDownloadsWatcher`] for `instance_dir` watching `watch_dir`.
    pub fn new(
        instance_dir: impl Into<PathBuf>,
        watch_dir: impl Into<PathBuf>,
        blocked: Vec<BlockedDownload>,
    ) -> Self {
        Self {
            instance_dir: instance_dir.into(),
            watch_dir: watch_dir.into(),
            pending: blocked,
            completed: BTreeMap::new(),
        }
    }

    /// Returns the number of blocked files still waiting to be downloaded ("осталось N").
    #[inline]
    pub fn remaining_count(&self) -> usize {
        self.pending.len()
    }

    /// Returns the number of blocked files that have been matched and moved into the instance.
    #[inline]
    pub fn completed_count(&self) -> usize {
        self.completed.len()
    }

    /// Returns `true` when all blocked files have been resolved (`remaining_count() == 0`).
    #[inline]
    pub fn is_complete(&self) -> bool {
        self.pending.is_empty()
    }

    /// Returns the slice of remaining [`BlockedDownload`] items.
    #[inline]
    pub fn pending_downloads(&self) -> &[BlockedDownload] {
        &self.pending
    }

    /// Returns a map from `file_id` to installed [`PathBuf`] for all resolved downloads.
    #[inline]
    pub fn completed_downloads(&self) -> &BTreeMap<String, PathBuf> {
        &self.completed
    }

    /// Returns `true` if `path` looks like an in-progress browser download (`.part`, `.crdownload`, `.tmp`, `.download`).
    pub fn is_partial_download_path(path: &Path) -> bool {
        let Some(name) = path.file_name().and_then(|s| s.to_str()) else {
            return true;
        };
        let lower = name.to_ascii_lowercase();
        lower.starts_with('.')
            || lower.ends_with(".part")
            || lower.ends_with(".crdownload")
            || lower.ends_with(".download")
            || lower.ends_with(".tmp")
            || lower.ends_with(".opdownload")
    }

    /// Checks a single file (e.g. from drag-and-drop or a filesystem event) against pending [`BlockedDownload`]
    /// entries. If its SHA-1 and/or CurseForge MurmurHash2 fingerprint matches, atomically moves it into
    /// the target subfolder of `instance_dir`, updates `.mq-lock.json`, and decrements `remaining_count()`.
    pub fn ingest_dropped_file(&mut self, candidate_path: &Path) -> MqResult<Option<PathBuf>> {
        if self.pending.is_empty()
            || !candidate_path.is_file()
            || Self::is_partial_download_path(candidate_path)
        {
            return Ok(None);
        }

        let (actual_sha1, actual_sha512, actual_size) = stream_hash_file_sha1_sha512(candidate_path)?;
        let need_fp = self
            .pending
            .iter()
            .any(|b| b.expected_fingerprint.is_some());
        let actual_fp = if need_fp {
            Some(compute_curseforge_fingerprint_file(candidate_path)?)
        } else {
            None
        };
        let candidate_name = candidate_path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("");

        let matched_idx = self.pending.iter().position(|item| {
            let has_cryptographic_expectation =
                item.expected_sha1.is_some() || item.expected_fingerprint.is_some();

            if has_cryptographic_expectation {
                if let Some(exp_sha1) = item.expected_sha1 {
                    if actual_sha1 != exp_sha1 {
                        return false;
                    }
                }
                if let Some(exp_fp) = item.expected_fingerprint {
                    if actual_fp != Some(exp_fp) {
                        return false;
                    }
                }
                true
            } else {
                candidate_name == item.filename
                    && (item.expected_size == 0 || actual_size == item.expected_size)
            }
        });

        let Some(idx) = matched_idx else {
            return Ok(None);
        };

        let item = self.pending.remove(idx);
        let dest_path =
            validate_archive_entry_path(&self.instance_dir, &item.target_rel_path, None)?;

        atomic_move_file(candidate_path, &dest_path)?;

        // Update `.mq-lock.json`
        let kind = if item.target_rel_path.starts_with("resourcepacks/") {
            ProjectKind::ResourcePack
        } else if item.target_rel_path.starts_with("shaderpacks/") {
            ProjectKind::Shader
        } else {
            ProjectKind::Mod
        };

        let mut lock = ModLockFile::load_for_instance(&self.instance_dir);
        lock.record_install(
            item.project_id.clone(),
            ModLockEntry {
                source: LockSource::CurseForge,
                project_id: item.project_id.clone(),
                version_id: item.file_id.clone(),
                file_id: Some(item.file_id.clone()),
                filename: item.filename.clone(),
                sha1: Some(actual_sha1.to_hex()),
                sha512: actual_sha512.to_hex(),
                installed_as_dependency: false,
                required_by: Vec::new(),
                kind,
            },
        );
        let _ = lock.save_for_instance(&self.instance_dir);

        self.completed.insert(item.file_id, dest_path.clone());
        Ok(Some(dest_path))
    }

    /// Handles a filesystem notification event for `event_path`.
    #[inline]
    pub fn notify_file_event(&mut self, event_path: &Path) -> MqResult<Option<PathBuf>> {
        self.ingest_dropped_file(event_path)
    }

    /// Scans `watch_dir` once, ingesting any newly completed files that match pending [`BlockedDownload`] items.
    pub fn poll_once(&mut self) -> MqResult<Vec<PathBuf>> {
        if self.pending.is_empty() || !self.watch_dir.is_dir() {
            return Ok(Vec::new());
        }

        let mut moved = Vec::new();
        for entry in fs::read_dir(&self.watch_dir)?.flatten() {
            if self.pending.is_empty() {
                break;
            }
            let path = entry.path();
            if !path.is_file() || Self::is_partial_download_path(&path) {
                continue;
            }
            if let Some(dest) = self.ingest_dropped_file(&path)? {
                moved.push(dest);
            }
        }
        Ok(moved)
    }

    /// Polls `watch_dir` at `poll_interval` until all pending [`BlockedDownload`] items are resolved
    /// or `epoch_guard` is cancelled.
    pub async fn wait_until_all_resolved(
        &mut self,
        poll_interval: Duration,
        epoch_guard: &EpochGuard,
    ) -> MqResult<Vec<PathBuf>> {
        let mut all_moved = Vec::new();
        loop {
            epoch_guard
                .check()
                .map_err(|e| MqError::Cancelled { epoch: e.epoch })?;

            let mut batch = self.poll_once()?;
            all_moved.append(&mut batch);

            if self.is_complete() {
                return Ok(all_moved);
            }

            tokio::time::sleep(poll_interval).await;
        }
    }
}

/// Atomically moves `src` to `dst`, falling back to a temporary file + `rename` + `remove_file`
/// if `src` and `dst` reside on different filesystems (`EXDEV`).
fn atomic_move_file(src: &Path, dst: &Path) -> MqResult<()> {
    if let Some(parent) = dst.parent() {
        fs::create_dir_all(parent)?;
    }

    if fs::rename(src, dst).is_ok() {
        return Ok(());
    }

    let tmp_dst = dst.with_extension("mq-atomic.tmp");
    fs::copy(src, &tmp_dst)?;
    fs::rename(&tmp_dst, dst)?;
    let _ = fs::remove_file(src);
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::io::Write;
    use std::sync::Arc;

    use mq_net::NetConfig;
    use mq_sched::{BufferPool, CancelEpoch, MIN_CHUNK_SIZE};
    use sha1::{Digest as _, Sha1};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;
    use zip::write::SimpleFileOptions;
    use zip::ZipWriter;

    use super::*;
    use crate::curseforge::compute_curseforge_fingerprint;

    fn sha1_of(data: &[u8]) -> Sha1Digest {
        let mut h = Sha1::new();
        h.update(data);
        Sha1Digest::from_bytes(h.finalize().into())
    }

    #[tokio::test]
    async fn cfpack_routes_by_class_id_verifies_sha1_writes_lockfile_and_watches_blocked_downloads()
    {
        let mod_bytes = b"cf-jei-mod-class-6-payload".to_vec();
        let rp_bytes = b"cf-faithful-resourcepack-class-12-payload".to_vec();
        let shader_bytes = b"cf-complementary-shader-class-6552-payload".to_vec();
        let blocked_mod_bytes = b"cf-optifine-blocked-null-download-url-payload".to_vec();

        let mod_sha1 = sha1_of(&mod_bytes);
        let rp_sha1 = sha1_of(&rp_bytes);
        let shader_sha1 = sha1_of(&shader_bytes);
        let blocked_sha1 = sha1_of(&blocked_mod_bytes);
        let blocked_fp = compute_curseforge_fingerprint(&blocked_mod_bytes);

        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let addr = listener.local_addr().unwrap();

        let mod_srv = mod_bytes.clone();
        let rp_srv = rp_bytes.clone();
        let shader_srv = shader_bytes.clone();
        let mod_sha1_hex = mod_sha1.to_hex();
        let rp_sha1_hex = rp_sha1.to_hex();
        let shader_sha1_hex = shader_sha1.to_hex();
        let blocked_sha1_hex = blocked_sha1.to_hex();

        tokio::spawn(async move {
            loop {
                let Ok((mut socket, _)) = listener.accept().await else {
                    break;
                };
                let m_b = mod_srv.clone();
                let r_b = rp_srv.clone();
                let s_b = shader_srv.clone();
                let m_h = mod_sha1_hex.clone();
                let r_h = rp_sha1_hex.clone();
                let s_h = shader_sha1_hex.clone();
                let b_h = blocked_sha1_hex.clone();

                tokio::spawn(async move {
                    let mut req_buf = [0u8; 4096];
                    let n = socket.read(&mut req_buf).await.unwrap_or(0);
                    let req = String::from_utf8_lossy(&req_buf[..n]);
                    let first_line = req.lines().next().unwrap_or("");

                    let (ctype, body): (&str, Vec<u8>) = if first_line.starts_with("POST /v1/mods/files")
                    {
                        let json = serde_json::json!({
                            "data": [
                                {
                                    "id": 1001,
                                    "modId": 10,
                                    "fileName": "jei-1.21.1.jar",
                                    "fileLength": m_b.len(),
                                    "downloadUrl": format!("http://{addr}/files/jei-1.21.1.jar"),
                                    "hashes": [{ "value": m_h, "algo": 1 }],
                                    "gameVersions": ["1.21.1", "Fabric"]
                                },
                                {
                                    "id": 2002,
                                    "modId": 20,
                                    "fileName": "faithful-32x.zip",
                                    "fileLength": r_b.len(),
                                    "downloadUrl": format!("http://{addr}/files/faithful-32x.zip"),
                                    "hashes": [{ "value": r_h, "algo": 1 }],
                                    "gameVersions": ["1.21.1"]
                                },
                                {
                                    "id": 3003,
                                    "modId": 30,
                                    "fileName": "complementary-reimagined.zip",
                                    "fileLength": s_b.len(),
                                    "downloadUrl": format!("http://{addr}/files/complementary-reimagined.zip"),
                                    "hashes": [{ "value": s_h, "algo": 1 }],
                                    "gameVersions": ["1.21.1"]
                                },
                                {
                                    "id": 4004,
                                    "modId": 40,
                                    "fileName": " restricted-mod-1.21.1.jar".trim(),
                                    "fileLength": 45,
                                    "downloadUrl": null,
                                    "fileFingerprint": blocked_fp,
                                    "hashes": [{ "value": b_h, "algo": 1 }],
                                    "gameVersions": ["1.21.1", "Fabric"]
                                }
                            ]
                        });
                        ("application/json", json.to_string().into_bytes())
                    } else if first_line.starts_with("POST /v1/mods") {
                        let json = serde_json::json!({
                            "data": [
                                {
                                    "id": 10,
                                    "name": "Just Enough Items",
                                    "slug": "jei",
                                    "classId": 6,
                                    "allowModDistribution": true,
                                    "links": { "websiteUrl": "https://www.curseforge.com/minecraft/mc-mods/jei" }
                                },
                                {
                                    "id": 20,
                                    "name": "Faithful 32x",
                                    "slug": "faithful-32x",
                                    "classId": 12,
                                    "allowModDistribution": true,
                                    "links": { "websiteUrl": "https://www.curseforge.com/minecraft/texture-packs/faithful-32x" }
                                },
                                {
                                    "id": 30,
                                    "name": "Complementary Shaders",
                                    "slug": "complementary-reimagined",
                                    "classId": 6552,
                                    "allowModDistribution": true,
                                    "links": { "websiteUrl": "https://www.curseforge.com/minecraft/shaders/complementary-reimagined" }
                                },
                                {
                                    "id": 40,
                                    "name": "Restricted Mod",
                                    "slug": "restricted-mod",
                                    "classId": 6,
                                    "allowModDistribution": false,
                                    "links": { "websiteUrl": "https://www.curseforge.com/minecraft/mc-mods/restricted-mod" }
                                }
                            ]
                        });
                        ("application/json", json.to_string().into_bytes())
                    } else if first_line.contains("/files/jei-1.21.1.jar") {
                        ("application/java-archive", m_b)
                    } else if first_line.contains("/files/faithful-32x.zip") {
                        ("application/zip", r_b)
                    } else {
                        ("application/zip", s_b)
                    };

                    let header = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: {ctype}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                        body.len()
                    );
                    let _ = socket.write_all(header.as_bytes()).await;
                    let _ = socket.write_all(&body).await;
                    let _ = socket.shutdown().await;
                });
            }
        });

        let tmp = tempfile::tempdir().expect("tempdir");
        let paths = MqPaths::with_root(tmp.path().join("mq_root"));
        paths.ensure_dirs().unwrap();

        let cf_zip = tmp.path().join("cf-pack.zip");
        {
            let manifest_json = serde_json::json!({
                "minecraft": {
                    "version": "1.21.1",
                    "modLoaders": [{ "id": "fabric-0.16.9", "primary": true }]
                },
                "manifestType": "minecraftModpack",
                "manifestVersion": 1,
                "name": "All Classes CF Pack",
                "version": "1.0.0",
                "author": "MQ",
                "files": [
                    { "projectID": 10, "fileID": 1001, "required": true },
                    { "projectID": 20, "fileID": 2002, "required": true },
                    { "projectID": 30, "fileID": 3003, "required": true },
                    { "projectID": 40, "fileID": 4004, "required": true, "fileFingerprint": blocked_fp }
                ],
                "overrides": "overrides"
            });
            let file = File::create(&cf_zip).unwrap();
            let mut zw = ZipWriter::new(file);
            let opts = SimpleFileOptions::default();
            zw.start_file("manifest.json", opts).unwrap();
            zw.write_all(manifest_json.to_string().as_bytes()).unwrap();
            zw.start_file("overrides/config/cf_pack.toml", opts)
                .unwrap();
            zw.write_all(b"pack = true\n").unwrap();
            zw.finish().unwrap();
        }

        let cf_client =
            CurseForgeClient::with_base_url(format!("http://{addr}/v1"), Some("test-key".into()))
                .unwrap();
        let epoch = Arc::new(CancelEpoch::new());
        let guard = epoch.snapshot();
        let buffer_pool = Arc::new(BufferPool::with_limits(MIN_CHUNK_SIZE, 4, 4));
        let net = NetEngine::with_components(
            NetConfig::default().with_max_retries(1),
            buffer_pool,
            epoch,
            None,
        )
        .unwrap();
        let compute_pool = ComputePool::new(2);

        let inst_dir = paths.instance_dir("cf-all-classes");
        let outcome = install_curseforge_modpack(
            &cf_zip,
            &inst_dir,
            &paths,
            &net,
            Some(&cf_client),
            &compute_pool,
            &guard,
        )
        .await
        .expect("install_curseforge_modpack");

        // 1. Verify placement by classId (6 -> mods/, 12 -> resourcepacks/, 6552 -> shaderpacks/).
        assert_eq!(outcome.installed_files.len(), 3);
        assert_eq!(outcome.overrides_extracted, 1);
        assert_eq!(
            fs::read(inst_dir.join("mods/jei-1.21.1.jar")).unwrap(),
            mod_bytes
        );
        assert_eq!(
            fs::read(inst_dir.join("resourcepacks/faithful-32x.zip")).unwrap(),
            rp_bytes
        );
        assert_eq!(
            fs::read(inst_dir.join("shaderpacks/complementary-reimagined.zip")).unwrap(),
            shader_bytes
        );
        assert_eq!(
            fs::read_to_string(inst_dir.join("config/cf_pack.toml")).unwrap(),
            "pack = true\n"
        );

        // 2. Verify .mq-lock.json entries for all 3 installed items.
        let lock = ModLockFile::load_for_instance(&inst_dir);
        assert_eq!(lock.entries.get("10").unwrap().kind, ProjectKind::Mod);
        assert_eq!(
            lock.entries.get("10").unwrap().source,
            LockSource::CurseForge
        );
        assert_eq!(
            lock.entries.get("20").unwrap().kind,
            ProjectKind::ResourcePack
        );
        assert_eq!(lock.entries.get("30").unwrap().kind, ProjectKind::Shader);

        // 3. Verify BlockedDownload was returned (not synthesized via edge.forgecdn.net!).
        assert_eq!(outcome.blocked_downloads.len(), 1);
        let blocked = &outcome.blocked_downloads[0];
        assert_eq!(blocked.project_id, "40");
        assert_eq!(blocked.file_id, "4004");
        assert_eq!(blocked.filename, "restricted-mod-1.21.1.jar");
        assert_eq!(
            blocked.webpage_url,
            "https://www.curseforge.com/minecraft/mc-mods/restricted-mod/download/4004"
        );
        assert_eq!(blocked.expected_sha1, Some(blocked_sha1));
        assert_eq!(blocked.expected_fingerprint, Some(blocked_fp));
        assert_eq!(blocked.target_rel_path, "mods/restricted-mod-1.21.1.jar");

        // 4. Test BlockedDownloadsWatcher watching the user's Downloads directory.
        let downloads_dir = tmp.path().join("Downloads");
        fs::create_dir_all(&downloads_dir).unwrap();

        let mut watcher =
            BlockedDownloadsWatcher::new(&inst_dir, &downloads_dir, outcome.blocked_downloads);
        assert_eq!(watcher.remaining_count(), 1);
        assert!(!watcher.is_complete());

        // Partial `.crdownload` file must be ignored while browser is still downloading.
        let part_file = downloads_dir.join("restricted-mod-1.21.1.jar.crdownload");
        fs::write(&part_file, &blocked_mod_bytes).unwrap();
        assert!(watcher.poll_once().unwrap().is_empty());
        assert_eq!(watcher.remaining_count(), 1);

        // Wrong-hash file must not be picked up.
        let wrong_file = downloads_dir.join("wrong.jar");
        fs::write(&wrong_file, b"wrong-bytes").unwrap();
        assert!(watcher.poll_once().unwrap().is_empty());
        assert_eq!(watcher.remaining_count(), 1);

        // Browser finishes download -> renames `.crdownload` to final `.jar`.
        let final_download = downloads_dir.join("restricted-mod-1.21.1.jar");
        fs::rename(&part_file, &final_download).unwrap();

        let resolved = watcher
            .wait_until_all_resolved(Duration::from_millis(10), &guard)
            .await
            .expect("watcher should resolve file");
        assert_eq!(resolved.len(), 1);
        assert_eq!(watcher.remaining_count(), 0);
        assert_eq!(watcher.completed_count(), 1);
        assert!(watcher.is_complete());

        // File was atomically moved from Downloads into `<inst_dir>/mods/restricted-mod-1.21.1.jar`.
        assert!(!final_download.exists());
        assert_eq!(
            fs::read(inst_dir.join("mods/restricted-mod-1.21.1.jar")).unwrap(),
            blocked_mod_bytes
        );

        // And recorded in `.mq-lock.json`!
        let updated_lock = ModLockFile::load_for_instance(&inst_dir);
        let entry_40 = updated_lock.entries.get("40").expect("lock entry for 40");
        assert_eq!(entry_40.source, LockSource::CurseForge);
        assert_eq!(entry_40.filename, "restricted-mod-1.21.1.jar");
        assert_eq!(entry_40.sha1.as_deref(), Some(blocked_sha1.to_hex().as_str()));
    }
}
