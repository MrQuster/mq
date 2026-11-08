//! Modrinth `.mrpack` (`modrinth.index.json`) parser and multi-threaded installer.
//!
//! Enforces strict Zip-Slip pre-validation on all archive entries and `files[].path`,
//! filters client files (`env.client != "unsupported"`), downloads artifacts in parallel
//! into the SHA-1 CAS via [`NetEngine`], verifies both SHA-1 and SHA-512 digests in
//! parallel across all CPU cores on [`ComputePool`] using `rayon` (`64 KiB` streaming chunks),
//! hardlinks verified blobs into the instance directory, and safely extracts `overrides/`
//! followed by `client-overrides/`.

use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File};
use std::path::{Path, PathBuf};
use std::time::Instant;

use mq_core::{
    resolve_safe_path, Event, EventBus, ModLoaderKind, MqError, MqPaths, MqResult, Sha1Digest,
    Sha512Digest,
};
use mq_net::{DownloadSpec, NetEngine};
use mq_sched::{ComputeError, ComputePool, EpochGuard};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use zip::ZipArchive;

use crate::lockfile::{ModLockEntry, ModLockFile};
use crate::models::{LockSource, ProjectKind};
use crate::zip_safe::{
    extract_zip_prefix_filtered, link_or_copy_cas_blob, read_zip_entry_bounded,
    stream_hash_file_sha1_sha512, validate_archive_entry_path, validate_download_url,
    validate_zip_archive, verify_file_sha1_and_sha512, ZipBombLimits,
};

/// Maximum size in bytes of `modrinth.index.json` inside a `.mrpack` archive (`16 MiB`).
const MAX_MRPACK_INDEX_BYTES: usize = 16 * 1024 * 1024;

/// State file stored in `<instance_dir>/.mq-mrpack-state.json` to track files managed by a `.mrpack`
/// so modpack updates only remove/overwrite unmodified files from the previous modpack version.
pub const MRPACK_STATE_FILE: &str = ".mq-mrpack-state.json";

const fn default_mrpack_format_version() -> u32 {
    1
}

fn default_mrpack_game() -> String {
    "minecraft".to_owned()
}

/// Cryptographic digests required for every file entry in `modrinth.index.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MrpackHashes {
    pub sha1: Sha1Digest,
    pub sha512: Sha512Digest,
}

/// Optional client/server environment support matrix for a `.mrpack` file entry.
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MrpackEnv {
    #[serde(default)]
    pub client: Option<String>,
    #[serde(default)]
    pub server: Option<String>,
}

/// Downloadable file entry declared in `modrinth.index.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MrpackFileEntry {
    pub path: String,
    pub hashes: MrpackHashes,
    #[serde(default)]
    pub env: Option<MrpackEnv>,
    #[serde(default)]
    pub downloads: Vec<String>,
    #[serde(rename = "fileSize", alias = "file_size", default)]
    pub file_size: u64,
}

impl MrpackFileEntry {
    /// Returns `true` if this entry should be installed on a client instance:
    /// keeps `"required"`, `"optional"`, or absent `env.client`, and skips `"unsupported"`.
    #[inline]
    pub fn should_install_on_client(&self) -> bool {
        match self
            .env
            .as_ref()
            .and_then(|e| e.client.as_deref())
            .map(str::trim)
        {
            Some(client_req) => !client_req.eq_ignore_ascii_case("unsupported"),
            None => true,
        }
    }
}

/// Parsed `modrinth.index.json` manifest from a Modrinth `.mrpack` archive.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MrpackIndex {
    #[serde(
        rename = "formatVersion",
        alias = "format_version",
        default = "default_mrpack_format_version"
    )]
    pub format_version: u32,
    #[serde(default = "default_mrpack_game")]
    pub game: String,
    #[serde(rename = "versionId", alias = "version_id", default)]
    pub version_id: String,
    pub name: String,
    #[serde(default)]
    pub summary: Option<String>,
    #[serde(default)]
    pub dependencies: BTreeMap<String, String>,
    #[serde(default)]
    pub files: Vec<MrpackFileEntry>,
}

impl MrpackIndex {
    /// Returns the target Minecraft version (`dependencies.get("minecraft")`).
    #[inline]
    pub fn minecraft_version(&self) -> Option<&str> {
        self.dependencies.get("minecraft").map(String::as_str)
    }

    /// Detects the mod loader kind and version from `dependencies`
    /// (`fabric-loader`, `quilt-loader`, `forge`, `neoforge`).
    pub fn loader_and_version(&self) -> (ModLoaderKind, Option<String>) {
        if let Some(ver) = self
            .dependencies
            .get("fabric-loader")
            .or_else(|| self.dependencies.get("fabric"))
        {
            return (ModLoaderKind::Fabric, Some(ver.clone()));
        }
        if let Some(ver) = self
            .dependencies
            .get("quilt-loader")
            .or_else(|| self.dependencies.get("quilt"))
        {
            return (ModLoaderKind::Quilt, Some(ver.clone()));
        }
        if let Some(ver) = self
            .dependencies
            .get("neoforge")
            .or_else(|| self.dependencies.get("neo-forge"))
        {
            return (ModLoaderKind::NeoForge, Some(ver.clone()));
        }
        if let Some(ver) = self.dependencies.get("forge") {
            return (ModLoaderKind::Forge, Some(ver.clone()));
        }
        (ModLoaderKind::Vanilla, None)
    }

    /// Filters `self.files` for client installation (`env.client != "unsupported"`).
    pub fn client_files(&self) -> Vec<&MrpackFileEntry> {
        self.files
            .iter()
            .filter(|f| f.should_install_on_client())
            .collect()
    }
}

/// Summary report returned after installing a Modrinth `.mrpack` modpack into an instance.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MrpackInstallOutcome {
    pub index: MrpackIndex,
    pub minecraft_version: String,
    pub loader: ModLoaderKind,
    pub loader_version: Option<String>,
    pub installed_files: Vec<PathBuf>,
    pub skipped_files: usize,
    pub overrides_extracted: usize,
    pub client_overrides_extracted: usize,
}

/// Summary report returned after updating an existing instance to a new `.mrpack` version.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MrpackUpdateOutcome {
    pub install_outcome: MrpackInstallOutcome,
    /// Unmodified files from the previous modpack version that are no longer in the new version and were deleted.
    pub removed_old_files: Vec<PathBuf>,
    /// User-added or user-modified files (`saves/`, `options.txt`, custom mods, edited configs) that were preserved.
    pub preserved_user_files: Vec<PathBuf>,
}

/// Persistent snapshot of files installed by a `.mrpack` version so updates can distinguish
/// untouched modpack files from user-added or user-modified files.
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MrpackInstalledState {
    pub name: String,
    pub version_id: String,
    pub tracked_files: BTreeMap<String, MrpackHashes>,
}

impl MrpackInstalledState {
    pub fn load(instance_dir: &Path) -> Option<Self> {
        let path = instance_dir.join(MRPACK_STATE_FILE);
        let raw = fs::read_to_string(path).ok()?;
        serde_json::from_str(&raw).ok()
    }

    pub fn save(&self, instance_dir: &Path) -> MqResult<()> {
        fs::create_dir_all(instance_dir)?;
        let path = instance_dir.join(MRPACK_STATE_FILE);
        let json = serde_json::to_string_pretty(self)?;
        fs::write(path, json)?;
        Ok(())
    }
}

/// Returns `true` if `rel_path` (normalized with `/`) refers to user state that must never be
/// overwritten or deleted during a modpack update (`saves/`, `options.txt`, `servers.dat`, `screenshots/`).
pub fn is_protected_user_rel_path(rel_path: &str) -> bool {
    let norm = rel_path.replace('\\', "/");
    let clean = norm.trim_matches('/');
    clean.eq_ignore_ascii_case("options.txt")
        || clean.eq_ignore_ascii_case("optionsof.txt")
        || clean.eq_ignore_ascii_case("optionsshaders.txt")
        || clean.eq_ignore_ascii_case("servers.dat")
        || clean.eq_ignore_ascii_case("saves")
        || clean.starts_with("saves/")
        || clean.eq_ignore_ascii_case("screenshots")
        || clean.starts_with("screenshots/")
}

fn infer_project_kind_from_rel_path(rel_path: &str) -> ProjectKind {
    let norm = rel_path.replace('\\', "/");
    let clean = norm.trim_start_matches('/');
    if clean.starts_with("resourcepacks/") {
        ProjectKind::ResourcePack
    } else if clean.starts_with("shaderpacks/") {
        ProjectKind::Shader
    } else {
        ProjectKind::Mod
    }
}

fn extract_modrinth_ids_from_urls(urls: &[String]) -> Option<(String, String)> {
    for raw in urls {
        if let Ok(parsed) = url::Url::parse(raw.trim()) {
            if let Some(segs) = parsed.path_segments() {
                let parts: Vec<&str> = segs.collect();
                // Standard Modrinth CDN layout: /data/<project_id>/versions/<version_id>/<filename>
                for window in parts.windows(4) {
                    if window[0] == "data" && window[2] == "versions" {
                        let pid = window[1].trim();
                        let vid = window[3].trim();
                        if !pid.is_empty() && !vid.is_empty() {
                            return Some((pid.to_owned(), vid.to_owned()));
                        }
                    }
                }
            }
        }
    }
    None
}

fn collect_zip_prefix_rel_paths(mrpack_path: &Path, prefixes: &[&str]) -> MqResult<BTreeSet<String>> {
    let file = File::open(mrpack_path)?;
    let mut archive = ZipArchive::new(file).map_err(|err| {
        MqError::Archive(format!(
            "failed to open .mrpack archive '{}': {err}",
            mrpack_path.display()
        ))
    })?;
    let mut out = BTreeSet::new();
    for idx in 0..archive.len() {
        let entry = archive.by_index_raw(idx).map_err(|err| {
            MqError::Archive(format!(
                "failed to inspect ZIP entry {idx} in '{}': {err}",
                mrpack_path.display()
            ))
        })?;
        if entry.is_dir() || entry.name().ends_with('/') {
            continue;
        }
        let norm = entry.name().replace('\\', "/");
        for prefix in prefixes {
            let p_clean = prefix.trim_matches('/');
            let p_slash = format!("{p_clean}/");
            if let Some(stripped) = norm.strip_prefix(&p_slash) {
                let rel = stripped.trim_matches('/');
                if !rel.is_empty() {
                    out.insert(rel.to_owned());
                }
            }
        }
    }
    Ok(out)
}

/// Validates every entry in `mrpack_path` against Zip-Slip, Unix symlinks, and Zip-Bombs, then reads
/// and parses `modrinth.index.json` while validating every `file.path` via [`resolve_safe_path`].
pub fn parse_mrpack_index_from_archive(mrpack_path: &Path) -> MqResult<MrpackIndex> {
    let file = File::open(mrpack_path)?;
    let mut archive = ZipArchive::new(file).map_err(|err| {
        MqError::Archive(format!(
            "failed to open .mrpack archive '{}': {err}",
            mrpack_path.display()
        ))
    })?;

    // 1. Pre-validate every entry in the .mrpack ZIP for zip-slip, symlink escapes, and zip-bombs.
    validate_zip_archive(&mut archive, Path::new("."))?;

    // 2. Locate and stream-read `modrinth.index.json`.
    let raw_bytes = {
        let index_entry = archive.by_name("modrinth.index.json").map_err(|err| {
            MqError::Metadata(format!(
                "missing 'modrinth.index.json' in '{}': {err}",
                mrpack_path.display()
            ))
        })?;
        read_zip_entry_bounded(index_entry, "modrinth.index.json", MAX_MRPACK_INDEX_BYTES)?
    };

    let index: MrpackIndex = serde_json::from_slice(&raw_bytes)?;

    if !index.game.trim().is_empty() && !index.game.trim().eq_ignore_ascii_case("minecraft") {
        return Err(MqError::Metadata(format!(
            "unsupported .mrpack game '{}' (expected 'minecraft')",
            index.game
        )));
    }

    // 3. Validate every file target path in `files[]` against Zip-Slip.
    let dummy_base = Path::new(".");
    for file_entry in &index.files {
        validate_archive_entry_path(dummy_base, &file_entry.path, None)?;
        resolve_safe_path(dummy_base, &file_entry.path)?;
    }

    Ok(index)
}

/// Installs a Modrinth `.mrpack` archive into `instance_dir`:
/// 1. Validates the archive and `modrinth.index.json` against Zip-Slip, symlinks, and Zip-Bombs.
/// 2. Filters `files[]` by `env.client` (skipping `"unsupported"`).
/// 3. Enforces the HTTPS download host allowlist on all `downloads[]` URLs.
/// 4. Skips network downloads for files already present on disk or in CAS with matching SHA-1 and SHA-512 digests (resume/retry support).
/// 5. Downloads remaining files in parallel into the SHA-1 CAS via [`NetEngine::download_batch`].
/// 6. Verifies both SHA-1 and SHA-512 in parallel across all CPU cores on [`ComputePool`] using `rayon` (`par_iter`) in `64 KiB` streaming chunks.
/// 7. Hardlinks verified CAS blobs into `<instance_dir>/<file.path>`.
/// 8. Safely extracts `"overrides/"` and then `"client-overrides/"` onto `instance_dir`.
/// 9. Records installed entries in `.mq-lock.json` (`source = "modrinth"`) and `.mq-mrpack-state.json`.
pub async fn install_mrpack(
    mrpack_path: &Path,
    instance_dir: &Path,
    paths: &MqPaths,
    net: &NetEngine,
    compute_pool: &ComputePool,
    epoch_guard: &EpochGuard,
    event_bus: Option<&EventBus>,
) -> MqResult<MrpackInstallOutcome> {
    install_or_update_mrpack_inner(
        mrpack_path,
        instance_dir,
        paths,
        net,
        compute_pool,
        epoch_guard,
        event_bus,
        false,
    )
    .await
    .map(|u| u.install_outcome)
}

/// Updates an existing instance to a new `.mrpack` version while:
/// - Preserving `saves/`, `options.txt`, `servers.dat`, `screenshots/`, and user-added files;
/// - Preserving files from the previous modpack version that were modified by the user;
/// - Removing only unmodified files from the previous modpack version that are no longer in the new version;
/// - Reusing already-downloaded files whose SHA-1 and SHA-512 digests match.
pub async fn update_mrpack(
    mrpack_path: &Path,
    instance_dir: &Path,
    paths: &MqPaths,
    net: &NetEngine,
    compute_pool: &ComputePool,
    epoch_guard: &EpochGuard,
    event_bus: Option<&EventBus>,
) -> MqResult<MrpackUpdateOutcome> {
    install_or_update_mrpack_inner(
        mrpack_path,
        instance_dir,
        paths,
        net,
        compute_pool,
        epoch_guard,
        event_bus,
        true,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
async fn install_or_update_mrpack_inner(
    mrpack_path: &Path,
    instance_dir: &Path,
    paths: &MqPaths,
    net: &NetEngine,
    compute_pool: &ComputePool,
    epoch_guard: &EpochGuard,
    event_bus: Option<&EventBus>,
    is_update: bool,
) -> MqResult<MrpackUpdateOutcome> {
    epoch_guard
        .check()
        .map_err(|e| MqError::Cancelled { epoch: e.epoch })?;

    let started_at = Instant::now();
    let index = parse_mrpack_index_from_archive(mrpack_path)?;
    fs::create_dir_all(instance_dir)?;

    let mc_version = index
        .minecraft_version()
        .ok_or_else(|| {
            MqError::Metadata(format!(
                "mrpack '{}' is missing 'minecraft' in dependencies",
                index.name
            ))
        })?
        .to_owned();
    let (loader, loader_version) = index.loader_and_version();

    let mut kept_entries: Vec<(MrpackFileEntry, PathBuf, PathBuf)> =
        Vec::with_capacity(index.files.len());
    let mut skipped_files = 0usize;

    for file_entry in &index.files {
        if !file_entry.should_install_on_client() {
            skipped_files += 1;
            continue;
        }
        if file_entry.downloads.is_empty() {
            return Err(MqError::Network(format!(
                "mrpack file '{}' has no download URLs",
                file_entry.path
            )));
        }
        for url in &file_entry.downloads {
            validate_download_url(url)?;
        }

        let target_path = validate_archive_entry_path(instance_dir, &file_entry.path, None)?;
        let cas_path = paths.cas_sha1_path(&file_entry.hashes.sha1.to_hex());
        kept_entries.push((file_entry.clone(), cas_path, target_path));
    }

    // If updating, load previous modpack state (falling back to ModLockFile entries if state file is absent).
    let mut old_state = if is_update {
        MrpackInstalledState::load(instance_dir).unwrap_or_default()
    } else {
        MrpackInstalledState::default()
    };

    let mut lockfile = ModLockFile::load_for_instance(instance_dir);
    if is_update && old_state.tracked_files.is_empty() {
        for entry in lockfile.entries.values() {
            if entry.source == LockSource::Modrinth {
                if let (Some(s1_hex), false) = (entry.sha1.as_deref(), entry.sha512.is_empty()) {
                    if let (Ok(sha1), Ok(sha512)) = (
                        Sha1Digest::from_hex(s1_hex),
                        Sha512Digest::from_hex(&entry.sha512),
                    ) {
                        let sub = entry.kind.target_subfolder();
                        let rel = if sub.is_empty() || entry.filename.contains('/') {
                            entry.filename.replace('\\', "/")
                        } else {
                            format!("{sub}/{}", entry.filename)
                        };
                        old_state
                            .tracked_files
                            .insert(rel, MrpackHashes { sha1, sha512 });
                    }
                }
            }
        }
    }

    let mut removed_old_files = Vec::new();
    let mut preserved_user_files = Vec::new();
    let mut protected_override_rels = BTreeSet::new();

    if is_update {
        let new_mod_rels: BTreeSet<String> = kept_entries
            .iter()
            .map(|(e, _, _)| e.path.replace('\\', "/").trim_matches('/').to_owned())
            .collect();
        let new_override_rels =
            collect_zip_prefix_rel_paths(mrpack_path, &["overrides", "client-overrides"])?;

        // 1. Inspect files tracked by the previous modpack version.
        for (old_rel, old_hashes) in &old_state.tracked_files {
            let norm_rel = old_rel.replace('\\', "/");
            let clean_rel = norm_rel.trim_matches('/');
            let Ok(disk_path) = validate_archive_entry_path(instance_dir, clean_rel, None) else {
                continue;
            };
            if !disk_path.is_file() {
                continue;
            }

            if is_protected_user_rel_path(clean_rel) {
                protected_override_rels.insert(clean_rel.to_owned());
                if !preserved_user_files.contains(&disk_path) {
                    preserved_user_files.push(disk_path);
                }
                continue;
            }

            let is_unchanged = verify_file_sha1_and_sha512(
                &disk_path,
                clean_rel,
                Some(old_hashes.sha1),
                Some(old_hashes.sha512),
            )
            .is_ok();

            let in_new_pack =
                new_mod_rels.contains(clean_rel) || new_override_rels.contains(clean_rel);

            if !in_new_pack {
                if is_unchanged {
                    // Unmodified file from old modpack version: delete it and remove from lockfile.
                    let _ = fs::remove_file(&disk_path);
                    if let Some(fname) = disk_path.file_name().and_then(|s| s.to_str()) {
                        if let Some((key, _)) = lockfile
                            .find_by_filename(fname)
                            .map(|(k, v)| (k.clone(), v.clone()))
                        {
                            lockfile.remove_entry(&key);
                        }
                    }
                    removed_old_files.push(disk_path);
                } else {
                    // User modified this file: preserve it!
                    if !preserved_user_files.contains(&disk_path) {
                        preserved_user_files.push(disk_path);
                    }
                }
            } else if !is_unchanged && new_override_rels.contains(clean_rel) {
                // File is in new overrides, but user modified the old version: do not overwrite!
                protected_override_rels.insert(clean_rel.to_owned());
                if !preserved_user_files.contains(&disk_path) {
                    preserved_user_files.push(disk_path);
                }
            }
        }

        // 2. Also protect any existing `saves/`, `options.txt`, `servers.dat`, and user-added files on disk.
        for candidate_rel in &new_override_rels {
            let Ok(disk_path) = validate_archive_entry_path(instance_dir, candidate_rel, None)
            else {
                continue;
            };
            if disk_path.is_file()
                && (is_protected_user_rel_path(candidate_rel)
                    || (!old_state.tracked_files.is_empty()
                        && !old_state.tracked_files.contains_key(candidate_rel)))
            {
                protected_override_rels.insert(candidate_rel.clone());
                if !preserved_user_files.contains(&disk_path) {
                    preserved_user_files.push(disk_path);
                }
            }
        }

        for fixed_rel in ["options.txt", "servers.dat"] {
            let p = instance_dir.join(fixed_rel);
            if p.is_file() && !preserved_user_files.contains(&p) {
                preserved_user_files.push(p);
            }
        }
        let saves_dir = instance_dir.join("saves");
        if saves_dir.is_dir() {
            if let Ok(rd) = fs::read_dir(&saves_dir) {
                for entry in rd.flatten() {
                    let p = entry.path();
                    if !preserved_user_files.contains(&p) {
                        preserved_user_files.push(p);
                    }
                }
            }
        }
        let mods_dir = instance_dir.join("mods");
        if mods_dir.is_dir() && !old_state.tracked_files.is_empty() {
            if let Ok(rd) = fs::read_dir(&mods_dir) {
                for entry in rd.flatten() {
                    let p = entry.path();
                    if p.is_file() {
                        if let Some(fname) = p.file_name().and_then(|s| s.to_str()) {
                            if fname == crate::lockfile::LOCKFILE_NAME {
                                continue;
                            }
                            let rel = format!("mods/{fname}");
                            if !old_state.tracked_files.contains_key(&rel)
                                && !new_mod_rels.contains(&rel)
                                && !preserved_user_files.contains(&p)
                            {
                                preserved_user_files.push(p);
                            }
                        }
                    }
                }
            }
        }
    }

    // Resume / Retry support: check which files are already verified in `target_path` or `cas_path`.
    let mut specs: Vec<DownloadSpec> = Vec::with_capacity(kept_entries.len());
    for (entry, cas_path, target_path) in &kept_entries {
        if target_path.is_file()
            && verify_file_sha1_and_sha512(
                target_path,
                &entry.path,
                Some(entry.hashes.sha1),
                Some(entry.hashes.sha512),
            )
            .is_ok()
        {
            if !cas_path.is_file() {
                let _ = link_or_copy_cas_blob(target_path, cas_path);
            }
            continue;
        }

        if cas_path.is_file() {
            if verify_file_sha1_and_sha512(
                cas_path,
                &entry.path,
                Some(entry.hashes.sha1),
                Some(entry.hashes.sha512),
            )
            .is_ok()
            {
                continue;
            }
            let _ = fs::remove_file(cas_path);
        }

        let mut spec = DownloadSpec::with_urls(
            entry.path.clone(),
            entry.downloads.clone(),
            cas_path.clone(),
        )
        .with_sha1(entry.hashes.sha1)
        .with_sha512(entry.hashes.sha512);
        if entry.file_size > 0 {
            spec = spec.with_expected_size(entry.file_size);
        }
        specs.push(spec);
    }

    if let Some(bus) = event_bus {
        bus.publish(Event::PhaseStarted {
            phase: "mrpack_install".to_owned(),
            total_items: specs.len() as u64,
        });
    }

    if !specs.is_empty() {
        net.download_batch(specs, "mrpack_download", epoch_guard)
            .await?;
    }

    // Verify BOTH SHA-1 and SHA-512 in parallel via rayon on ComputePool, hardlink into instance_dir,
    // and extract overrides/ + client-overrides/ deterministically.
    let mrpack_path_owned = mrpack_path.to_path_buf();
    let instance_dir_owned = instance_dir.to_path_buf();
    let pool_clone = compute_pool.clone();
    let worker_guard = epoch_guard.clone();
    let kept_entries_for_lock = kept_entries.clone();
    let protected_for_extract = protected_override_rels.clone();

    let (
        installed_files,
        overrides_outcome,
        client_overrides_outcome,
    ) = compute_pool
        .spawn_cpu_cancellable(
            epoch_guard,
            move || -> MqResult<(
                Vec<PathBuf>,
                crate::zip_safe::PrefixExtractOutcome,
                crate::zip_safe::PrefixExtractOutcome,
            )> {
                let installed: MqResult<Vec<PathBuf>> = pool_clone.install_sync(|| {
                    kept_entries
                        .par_iter()
                        .map(|(entry, cas_path, target_path)| {
                            worker_guard
                                .check()
                                .map_err(|e| MqError::Cancelled { epoch: e.epoch })?;

                            verify_file_sha1_and_sha512(
                                cas_path,
                                &entry.path,
                                Some(entry.hashes.sha1),
                                Some(entry.hashes.sha512),
                            )?;

                            link_or_copy_cas_blob(cas_path, target_path)?;
                            verify_file_sha1_and_sha512(
                                target_path,
                                &entry.path,
                                Some(entry.hashes.sha1),
                                Some(entry.hashes.sha512),
                            )?;
                            Ok(target_path.clone())
                        })
                        .collect()
                });

                let installed_files = installed?;
                worker_guard
                    .check()
                    .map_err(|e| MqError::Cancelled { epoch: e.epoch })?;

                let limits = ZipBombLimits::default();
                let overrides_outcome = extract_zip_prefix_filtered(
                    &mrpack_path_owned,
                    "overrides",
                    &instance_dir_owned,
                    &limits,
                    |rel_path| !protected_for_extract.contains(rel_path),
                )?;
                let client_overrides_outcome = extract_zip_prefix_filtered(
                    &mrpack_path_owned,
                    "client-overrides",
                    &instance_dir_owned,
                    &limits,
                    |rel_path| !protected_for_extract.contains(rel_path),
                )?;

                Ok((
                    installed_files,
                    overrides_outcome,
                    client_overrides_outcome,
                ))
            },
        )
        .await
        .map_err(|err| match err {
            ComputeError::Cancelled => MqError::Cancelled {
                epoch: epoch_guard.epoch(),
            },
            other => MqError::Internal(other.to_string()),
        })??;

    // Record installed entries in `.mq-lock.json` (`source = "modrinth"`) and `.mq-mrpack-state.json`.
    let mut new_state = MrpackInstalledState {
        name: index.name.clone(),
        version_id: index.version_id.clone(),
        tracked_files: BTreeMap::new(),
    };

    for (entry, _cas_path, _target_path) in &kept_entries_for_lock {
        let rel_norm = entry.path.replace('\\', "/").trim_matches('/').to_owned();
        new_state
            .tracked_files
            .insert(rel_norm.clone(), entry.hashes.clone());

        let filename = Path::new(&rel_norm)
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or(&rel_norm)
            .to_owned();
        let (project_id, version_id) = extract_modrinth_ids_from_urls(&entry.downloads)
            .unwrap_or_else(|| {
                let fallback_vid = if index.version_id.is_empty() {
                    entry.hashes.sha1.to_hex()
                } else {
                    index.version_id.clone()
                };
                (filename.clone(), fallback_vid)
            });
        let kind = infer_project_kind_from_rel_path(&rel_norm);

        lockfile.record_install(
            project_id.clone(),
            ModLockEntry {
                source: LockSource::Modrinth,
                project_id,
                version_id: version_id.clone(),
                file_id: Some(version_id),
                filename,
                sha1: Some(entry.hashes.sha1.to_hex()),
                sha512: entry.hashes.sha512.to_hex(),
                installed_as_dependency: false,
                required_by: Vec::new(),
                kind,
            },
        );
    }

    for rel_ov in overrides_outcome
        .extracted_files
        .iter()
        .chain(client_overrides_outcome.extracted_files.iter())
    {
        if let Ok(ov_path) = validate_archive_entry_path(instance_dir, rel_ov, None) {
            if let Ok((sha1, sha512, _)) = stream_hash_file_sha1_sha512(&ov_path) {
                new_state
                    .tracked_files
                    .insert(rel_ov.clone(), MrpackHashes { sha1, sha512 });
            }
        }
    }

    let _ = lockfile.save_for_instance(instance_dir);
    let _ = new_state.save(instance_dir);

    if let Some(bus) = event_bus {
        bus.publish(Event::PhaseCompleted {
            phase: "mrpack_install".to_owned(),
            elapsed_ms: started_at.elapsed().as_millis() as u64,
        });
    }

    let install_outcome = MrpackInstallOutcome {
        index,
        minecraft_version: mc_version,
        loader,
        loader_version,
        installed_files,
        skipped_files,
        overrides_extracted: overrides_outcome.extracted_files.len(),
        client_overrides_extracted: client_overrides_outcome.extracted_files.len(),
    };

    Ok(MrpackUpdateOutcome {
        install_outcome,
        removed_old_files,
        preserved_user_files,
    })
}

#[cfg(test)]
mod tests {
    use std::io::Write;
    use std::sync::Arc;

    use mq_net::NetConfig;
    use mq_sched::{BufferPool, CancelEpoch, MIN_CHUNK_SIZE};
    use sha1::{Digest as _, Sha1};
    use sha2::Sha512;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;
    use zip::write::SimpleFileOptions;
    use zip::ZipWriter;

    use super::*;

    fn hash_bytes(data: &[u8]) -> (Sha1Digest, Sha512Digest) {
        let mut h1 = Sha1::new();
        h1.update(data);
        let mut h512 = Sha512::new();
        h512.update(data);
        (
            Sha1Digest::from_bytes(h1.finalize().into()),
            Sha512Digest::from_bytes(h512.finalize().into()),
        )
    }

    #[test]
    fn mrpack_rejects_zip_slip_archive_and_unsafe_index_paths() {
        let tmp = tempfile::tempdir().expect("tempdir");

        // 1. Malicious archive entry `overrides/../../etc/passwd` rejected before reading index.
        let bad_archive = tmp.path().join("evil_entry.mrpack");
        {
            let file = File::create(&bad_archive).unwrap();
            let mut zw = ZipWriter::new(file);
            let opts = SimpleFileOptions::default();
            zw.start_file("overrides/../../etc/passwd", opts).unwrap();
            zw.write_all(b"evil").unwrap();
            zw.finish().unwrap();
        }
        let err = parse_mrpack_index_from_archive(&bad_archive).unwrap_err();
        assert!(matches!(err, MqError::ZipSlip { .. }));

        // 2. Safe archive entries, but `modrinth.index.json` has `files[].path = "../../outside.jar"`.
        let bad_index_archive = tmp.path().join("evil_index.mrpack");
        {
            let (sha1, sha512) = hash_bytes(b"jar-bytes");
            let index_json = serde_json::json!({
                "formatVersion": 1,
                "game": "minecraft",
                "versionId": "1.0.0",
                "name": "Evil Pack",
                "dependencies": {
                    "minecraft": "1.21.1",
                    "fabric-loader": "0.16.9"
                },
                "files": [
                    {
                        "path": "mods/../../outside.jar",
                        "hashes": {
                            "sha1": sha1.to_hex(),
                            "sha512": sha512.to_hex()
                        },
                        "downloads": ["https://example.invalid/outside.jar"],
                        "fileSize": 9
                    }
                ]
            });
            let file = File::create(&bad_index_archive).unwrap();
            let mut zw = ZipWriter::new(file);
            let opts = SimpleFileOptions::default();
            zw.start_file("modrinth.index.json", opts).unwrap();
            zw.write_all(index_json.to_string().as_bytes()).unwrap();
            zw.finish().unwrap();
        }
        let err2 = parse_mrpack_index_from_archive(&bad_index_archive).unwrap_err();
        assert!(matches!(err2, MqError::ZipSlip { .. }));
    }

    #[tokio::test]
    async fn install_mrpack_parses_1_21_1_fabric_filters_env_verifies_hashes_and_extracts_overrides(
    ) {
        let mod_a_bytes = b"fake-sodium-1.21.1-fabric-jar-payload".to_vec();
        let mod_b_bytes = b"fake-lithium-1.21.1-fabric-jar-payload".to_vec();
        let server_only_bytes = b"server-only-dedicated-mod-payload".to_vec();

        let (sha1_a, sha512_a) = hash_bytes(&mod_a_bytes);
        let (sha1_b, sha512_b) = hash_bytes(&mod_b_bytes);
        let (sha1_srv, sha512_srv) = hash_bytes(&server_only_bytes);

        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let addr = listener.local_addr().unwrap();

        let mod_a_srv = mod_a_bytes.clone();
        let mod_b_srv = mod_b_bytes.clone();
        tokio::spawn(async move {
            loop {
                let Ok((mut socket, _)) = listener.accept().await else {
                    break;
                };
                let a = mod_a_srv.clone();
                let b = mod_b_srv.clone();
                tokio::spawn(async move {
                    let mut req_buf = [0u8; 1024];
                    let n = socket.read(&mut req_buf).await.unwrap_or(0);
                    let req = String::from_utf8_lossy(&req_buf[..n]);
                    let body = if req.contains("/sodium.jar") { a } else { b };
                    let header = format!(
                        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
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

        let mrpack_path = tmp.path().join("fabulously-1.21.1.mrpack");
        {
            let index_json = serde_json::json!({
                "formatVersion": 1,
                "game": "minecraft",
                "versionId": "6.1.0",
                "name": "Fabulously Optimized",
                "summary": "Performance modpack for 1.21.1",
                "dependencies": {
                    "minecraft": "1.21.1",
                    "fabric-loader": "0.16.9"
                },
                "files": [
                    {
                        "path": "mods/sodium.jar",
                        "hashes": {
                            "sha1": sha1_a.to_hex(),
                            "sha512": sha512_a.to_hex()
                        },
                        "env": {
                            "client": "required",
                            "server": "unsupported"
                        },
                        "downloads": [format!("http://{addr}/sodium.jar")],
                        "fileSize": mod_a_bytes.len()
                    },
                    {
                        "path": "mods/lithium.jar",
                        "hashes": {
                            "sha1": sha1_b.to_hex(),
                            "sha512": sha512_b.to_hex()
                        },
                        "env": {
                            "client": "optional",
                            "server": "required"
                        },
                        "downloads": [format!("http://{addr}/lithium.jar")],
                        "fileSize": mod_b_bytes.len()
                    },
                    {
                        "path": "mods/server_only.jar",
                        "hashes": {
                            "sha1": sha1_srv.to_hex(),
                            "sha512": sha512_srv.to_hex()
                        },
                        "env": {
                            "client": "unsupported",
                            "server": "required"
                        },
                        "downloads": [format!("http://{addr}/server_only.jar")],
                        "fileSize": server_only_bytes.len()
                    }
                ]
            });

            let file = File::create(&mrpack_path).unwrap();
            let mut zw = ZipWriter::new(file);
            let opts = SimpleFileOptions::default().unix_permissions(0o100644);
            zw.start_file("modrinth.index.json", opts).unwrap();
            zw.write_all(index_json.to_string().as_bytes()).unwrap();

            // Both overrides/ and client-overrides/ with overlapping config file to verify priority order.
            zw.start_file("overrides/config/common.toml", opts).unwrap();
            zw.write_all(b"common = true\n").unwrap();
            zw.start_file("overrides/config/client.toml", opts).unwrap();
            zw.write_all(b"from_overrides = true\n").unwrap();
            zw.start_file("client-overrides/config/client.toml", opts)
                .unwrap();
            zw.write_all(b"from_client_overrides = true\n").unwrap();
            zw.finish().unwrap();
        }

        let parsed = parse_mrpack_index_from_archive(&mrpack_path).expect("parse index");
        assert_eq!(parsed.minecraft_version(), Some("1.21.1"));
        assert_eq!(
            parsed.loader_and_version(),
            (ModLoaderKind::Fabric, Some("0.16.9".to_owned()))
        );
        assert_eq!(parsed.client_files().len(), 2);

        let epoch = Arc::new(CancelEpoch::new());
        let guard = epoch.snapshot();
        let buffer_pool = Arc::new(BufferPool::with_limits(MIN_CHUNK_SIZE, 4, 4));
        let event_bus = EventBus::new();
        let net = NetEngine::with_components(
            NetConfig::default().with_max_retries(1),
            buffer_pool,
            epoch,
            Some(event_bus.clone()),
        )
        .unwrap();
        let compute_pool = ComputePool::new(2);

        let instance_dir = paths.instance_dir("fabulously-1.21.1");
        let outcome = install_mrpack(
            &mrpack_path,
            &instance_dir,
            &paths,
            &net,
            &compute_pool,
            &guard,
            Some(&event_bus),
        )
        .await
        .expect("install_mrpack should succeed");

        assert_eq!(outcome.minecraft_version, "1.21.1");
        assert_eq!(outcome.loader, ModLoaderKind::Fabric);
        assert_eq!(outcome.loader_version.as_deref(), Some("0.16.9"));
        assert_eq!(outcome.installed_files.len(), 2);
        assert_eq!(outcome.skipped_files, 1);
        assert_eq!(outcome.overrides_extracted, 2);
        assert_eq!(outcome.client_overrides_extracted, 1);

        // Verify CAS storage and hardlinked instance files.
        assert!(paths.cas_sha1_path(&sha1_a.to_hex()).exists());
        assert!(paths.cas_sha1_path(&sha1_b.to_hex()).exists());
        assert_eq!(
            fs::read(instance_dir.join("mods/sodium.jar")).unwrap(),
            mod_a_bytes
        );
        assert_eq!(
            fs::read(instance_dir.join("mods/lithium.jar")).unwrap(),
            mod_b_bytes
        );
        assert!(!instance_dir.join("mods/server_only.jar").exists());

        // Verify overrides and client-overrides order.
        assert_eq!(
            fs::read_to_string(instance_dir.join("config/common.toml")).unwrap(),
            "common = true\n"
        );
        assert_eq!(
            fs::read_to_string(instance_dir.join("config/client.toml")).unwrap(),
            "from_client_overrides = true\n"
        );

        // Verify .mq-lock.json was written with LockSource::Modrinth.
        let lock = ModLockFile::load_for_instance(&instance_dir);
        assert_eq!(lock.entries.len(), 2);
        let sodium_lock = lock
            .find_by_filename("sodium.jar")
            .expect("sodium lock entry")
            .1;
        assert_eq!(sodium_lock.source, LockSource::Modrinth);
        assert_eq!(
            sodium_lock.sha1.as_deref(),
            Some(sha1_a.to_hex().as_str())
        );
        assert_eq!(sodium_lock.sha512, sha512_a.to_hex());
    }

    #[tokio::test]
    async fn mrpack_rejects_forbidden_download_host_and_supports_resume_and_update() {
        use std::sync::atomic::{AtomicUsize, Ordering};

        let mod_v1_bytes = b"mod-v1-old-version-payload".to_vec();
        let mod_kept_bytes = b"mod-kept-across-versions-payload".to_vec();
        let mod_v2_bytes = b"mod-v2-new-version-payload".to_vec();

        let (sha1_v1, sha512_v1) = hash_bytes(&mod_v1_bytes);
        let (sha1_kept, sha512_kept) = hash_bytes(&mod_kept_bytes);
        let (sha1_v2, sha512_v2) = hash_bytes(&mod_v2_bytes);

        let tmp = tempfile::tempdir().expect("tempdir");
        let paths = MqPaths::with_root(tmp.path().join("mq_root"));
        paths.ensure_dirs().unwrap();

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

        // 1. Forbidden download host must be rejected before any network call.
        let forbidden_mrpack = tmp.path().join("forbidden.mrpack");
        {
            let index_json = serde_json::json!({
                "formatVersion": 1,
                "game": "minecraft",
                "versionId": "1.0.0",
                "name": "Forbidden Host Pack",
                "dependencies": { "minecraft": "1.21.1" },
                "files": [
                    {
                        "path": "mods/evil.jar",
                        "hashes": { "sha1": sha1_v1.to_hex(), "sha512": sha512_v1.to_hex() },
                        "downloads": ["https://evil.example.com/evil.jar"],
                        "fileSize": mod_v1_bytes.len()
                    }
                ]
            });
            let file = File::create(&forbidden_mrpack).unwrap();
            let mut zw = ZipWriter::new(file);
            let opts = SimpleFileOptions::default();
            zw.start_file("modrinth.index.json", opts).unwrap();
            zw.write_all(index_json.to_string().as_bytes()).unwrap();
            zw.finish().unwrap();
        }

        let bad_inst = paths.instance_dir("forbidden-inst");
        let host_err = install_mrpack(
            &forbidden_mrpack,
            &bad_inst,
            &paths,
            &net,
            &compute_pool,
            &guard,
            None,
        )
        .await
        .expect_err("forbidden download host must fail");
        assert!(matches!(host_err, MqError::Network(_)));

        // 2. Set up local mock server counting how many times `kept.jar` is requested.
        let kept_hits = Arc::new(AtomicUsize::new(0));
        let kept_hits_srv = kept_hits.clone();
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let addr = listener.local_addr().unwrap();

        let v1_srv = mod_v1_bytes.clone();
        let kept_srv = mod_kept_bytes.clone();
        let v2_srv = mod_v2_bytes.clone();
        tokio::spawn(async move {
            loop {
                let Ok((mut socket, _)) = listener.accept().await else {
                    break;
                };
                let v1 = v1_srv.clone();
                let kept = kept_srv.clone();
                let v2 = v2_srv.clone();
                let counter = kept_hits_srv.clone();
                tokio::spawn(async move {
                    let mut req_buf = [0u8; 1024];
                    let n = socket.read(&mut req_buf).await.unwrap_or(0);
                    let req = String::from_utf8_lossy(&req_buf[..n]);
                    let body = if req.contains("/kept.jar") {
                        counter.fetch_add(1, Ordering::SeqCst);
                        kept
                    } else if req.contains("/v1.jar") {
                        v1
                    } else {
                        v2
                    };
                    let header = format!(
                        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                        body.len()
                    );
                    let _ = socket.write_all(header.as_bytes()).await;
                    let _ = socket.write_all(&body).await;
                    let _ = socket.shutdown().await;
                });
            }
        });

        let pack_v1 = tmp.path().join("pack-v1.mrpack");
        {
            let index_json = serde_json::json!({
                "formatVersion": 1,
                "game": "minecraft",
                "versionId": "1.0.0",
                "name": "Updatable Pack",
                "dependencies": { "minecraft": "1.21.1", "fabric-loader": "0.16.9" },
                "files": [
                    {
                        "path": "mods/v1.jar",
                        "hashes": { "sha1": sha1_v1.to_hex(), "sha512": sha512_v1.to_hex() },
                        "downloads": [format!("http://{addr}/data/P1/versions/V1/v1.jar")],
                        "fileSize": mod_v1_bytes.len()
                    },
                    {
                        "path": "mods/kept.jar",
                        "hashes": { "sha1": sha1_kept.to_hex(), "sha512": sha512_kept.to_hex() },
                        "downloads": [format!("http://{addr}/data/P2/versions/VK/kept.jar")],
                        "fileSize": mod_kept_bytes.len()
                    }
                ]
            });
            let file = File::create(&pack_v1).unwrap();
            let mut zw = ZipWriter::new(file);
            let opts = SimpleFileOptions::default();
            zw.start_file("modrinth.index.json", opts).unwrap();
            zw.write_all(index_json.to_string().as_bytes()).unwrap();
            zw.start_file("overrides/config/untouched.toml", opts).unwrap();
            zw.write_all(b"version = 1\n").unwrap();
            zw.start_file("overrides/config/user_edited.toml", opts)
                .unwrap();
            zw.write_all(b"custom = false\n").unwrap();
            zw.finish().unwrap();
        }

        let inst_dir = paths.instance_dir("updatable-pack");
        install_mrpack(
            &pack_v1,
            &inst_dir,
            &paths,
            &net,
            &compute_pool,
            &guard,
            None,
        )
        .await
        .expect("install v1");
        assert_eq!(kept_hits.load(Ordering::SeqCst), 1);

        // Retry / resume on v1: `kept.jar` is already verified on disk, so it must not be re-downloaded.
        install_mrpack(
            &pack_v1,
            &inst_dir,
            &paths,
            &net,
            &compute_pool,
            &guard,
            None,
        )
        .await
        .expect("retry install v1");
        assert_eq!(
            kept_hits.load(Ordering::SeqCst),
            1,
            "retry should reuse verified file without re-downloading"
        );

        // Simulate user activity before updating to v2:
        // - Create `saves/MyWorld/level.dat`
        // - Create `options.txt`
        // - Add custom mod `mods/user_added.jar`
        // - Modify `config/user_edited.toml`
        fs::create_dir_all(inst_dir.join("saves/MyWorld")).unwrap();
        fs::write(inst_dir.join("saves/MyWorld/level.dat"), b"world-data").unwrap();
        fs::write(inst_dir.join("options.txt"), b"fov:110\n").unwrap();
        fs::write(inst_dir.join("mods/user_added.jar"), b"user-custom-mod").unwrap();
        fs::write(inst_dir.join("config/user_edited.toml"), b"custom = true\n").unwrap();

        let pack_v2 = tmp.path().join("pack-v2.mrpack");
        {
            let index_json = serde_json::json!({
                "formatVersion": 1,
                "game": "minecraft",
                "versionId": "2.0.0",
                "name": "Updatable Pack",
                "dependencies": { "minecraft": "1.21.1", "fabric-loader": "0.16.9" },
                "files": [
                    {
                        "path": "mods/v2.jar",
                        "hashes": { "sha1": sha1_v2.to_hex(), "sha512": sha512_v2.to_hex() },
                        "downloads": [format!("http://{addr}/data/P1/versions/V2/v2.jar")],
                        "fileSize": mod_v2_bytes.len()
                    },
                    {
                        "path": "mods/kept.jar",
                        "hashes": { "sha1": sha1_kept.to_hex(), "sha512": sha512_kept.to_hex() },
                        "downloads": [format!("http://{addr}/data/P2/versions/VK/kept.jar")],
                        "fileSize": mod_kept_bytes.len()
                    }
                ]
            });
            let file = File::create(&pack_v2).unwrap();
            let mut zw = ZipWriter::new(file);
            let opts = SimpleFileOptions::default();
            zw.start_file("modrinth.index.json", opts).unwrap();
            zw.write_all(index_json.to_string().as_bytes()).unwrap();
            zw.start_file("overrides/config/untouched.toml", opts).unwrap();
            zw.write_all(b"version = 2\n").unwrap();
            zw.start_file("overrides/config/user_edited.toml", opts)
                .unwrap();
            zw.write_all(b"custom = false\n").unwrap();
            zw.start_file("overrides/options.txt", opts).unwrap();
            zw.write_all(b"fov:70\n").unwrap();
            zw.finish().unwrap();
        }

        let update_res = update_mrpack(
            &pack_v2,
            &inst_dir,
            &paths,
            &net,
            &compute_pool,
            &guard,
            None,
        )
        .await
        .expect("update to v2");

        // `v1.jar` was unmodified from v1 and removed in v2 -> must be deleted!
        assert!(!inst_dir.join("mods/v1.jar").exists());
        assert!(update_res
            .removed_old_files
            .contains(&inst_dir.join("mods/v1.jar")));

        // `v2.jar` and `kept.jar` must exist (`kept.jar` still only downloaded once!).
        assert!(inst_dir.join("mods/v2.jar").is_file());
        assert!(inst_dir.join("mods/kept.jar").is_file());
        assert_eq!(kept_hits.load(Ordering::SeqCst), 1);

        // User files (`saves/`, `options.txt`, `mods/user_added.jar`, `config/user_edited.toml`) must be preserved!
        assert_eq!(
            fs::read(inst_dir.join("saves/MyWorld/level.dat")).unwrap(),
            b"world-data"
        );
        assert_eq!(
            fs::read_to_string(inst_dir.join("options.txt")).unwrap(),
            "fov:110\n"
        );
        assert_eq!(
            fs::read(inst_dir.join("mods/user_added.jar")).unwrap(),
            b"user-custom-mod"
        );
        assert_eq!(
            fs::read_to_string(inst_dir.join("config/user_edited.toml")).unwrap(),
            "custom = true\n"
        );

        // Unmodified config from v1 (`config/untouched.toml`) was updated to v2!
        assert_eq!(
            fs::read_to_string(inst_dir.join("config/untouched.toml")).unwrap(),
            "version = 2\n"
        );
    }
}
