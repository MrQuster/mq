#![deny(clippy::await_holding_lock)]
//! Parallel `rayon` `.jar` mod scanner (`mq mods scan`).
//!
//! Inspects installed `.jar` and `.jar.disabled` mod archives across all CPU cores,
//! extracting metadata from `fabric.mod.json`, `quilt.mod.json`, `META-INF/mods.toml`,
//! and `META-INF/neoforge.mods.toml`, computing streaming dual SHA-1/SHA-512 digests,
//! and detecting duplicate mod IDs and missing dependencies.

use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::Instant;

use mq_core::{ModLoaderKind, MqError, MqResult, Sha1Digest, Sha512Digest};
use mq_sched::{ComputePool, EpochGuard};
use rayon::prelude::*;
use sha1::{Digest as _, Sha1};
use sha2::Sha512;
use zip::ZipArchive;

use crate::curseforge::{compute_curseforge_fingerprint_file, CurseForgeClient};
use crate::lockfile::{ModLockEntry, ModLockFile};
use crate::models::{
    Hashes, InstalledModEntry, LockSource, ModConflict, ModScanReport, ProjectKind,
};
use crate::modrinth::ModrinthClient;
use crate::resolver::ContentProvider;
use crate::zip_safe::stream_hash_file_sha1_sha512;

const SCAN_STREAM_CHUNK_SIZE: usize = 64 * 1024;
const MAX_DESCRIPTOR_SIZE: usize = 2 * 1024 * 1024; // 2 MiB descriptor cap

/// Scans a directory of mods in parallel on the CPU compute pool.
pub async fn scan_mods_directory(
    mods_dir: &Path,
    compute_pool: &ComputePool,
    epoch_guard: &EpochGuard,
) -> MqResult<ModScanReport> {
    if !mods_dir.exists() {
        return Ok(ModScanReport {
            mods: Vec::new(),
            conflicts: Vec::new(),
            scan_duration_ms: 0,
        });
    }

    let mut candidate_paths = Vec::new();
    let entries = fs::read_dir(mods_dir)?;
    for entry in entries {
        let entry = entry?;
        let p = entry.path();
        if !p.is_file() {
            continue;
        }
        let fname = entry.file_name().to_string_lossy().to_string();
        if fname.ends_with(".jar") || fname.ends_with(".jar.disabled") {
            candidate_paths.push(p);
        }
    }

    candidate_paths.sort();

    let start = Instant::now();

    compute_pool
        .spawn_cpu_cancellable(epoch_guard, move || {
            let mods: Vec<InstalledModEntry> = candidate_paths
                .par_iter()
                .map(|p| scan_single_jar(p))
                .filter_map(Result::ok)
                .collect();

            // Analyze conflicts among enabled mods
            let mut enabled_ids: BTreeMap<String, Vec<String>> = BTreeMap::new();
            let mut required_deps: Vec<(String, String, String)> = Vec::new(); // (mod_id, file_name, dep)

            for m in &mods {
                if m.enabled {
                    enabled_ids
                        .entry(m.mod_id.clone())
                        .or_default()
                        .push(m.file_name.clone());

                    for dep in &m.dependencies {
                        required_deps.push((m.mod_id.clone(), m.file_name.clone(), dep.clone()));
                    }
                }
            }

            let mut conflicts = Vec::new();

            // 1. Duplicate Mod IDs
            for (mod_id, files) in enabled_ids.iter() {
                if files.len() > 1 {
                    conflicts.push(ModConflict::DuplicateModId {
                        mod_id: mod_id.clone(),
                        files: files.clone(),
                    });
                }
            }

            // 2. Missing Dependencies
            let standard_builtins: BTreeSet<&str> = [
                "minecraft",
                "java",
                "fabricloader",
                "fabric-loader",
                "quilt_loader",
                "forge",
                "neoforge",
            ]
            .into_iter()
            .collect();

            for (mod_id, file_name, dep) in required_deps {
                if standard_builtins.contains(dep.as_str()) {
                    continue;
                }
                if !enabled_ids.contains_key(&dep) {
                    conflicts.push(ModConflict::MissingDependency {
                        mod_id,
                        file_name,
                        required_dep: dep,
                    });
                }
            }

            let elapsed_ms = start.elapsed().as_millis() as u64;

            Ok(ModScanReport {
                mods,
                conflicts,
                scan_duration_ms: elapsed_ms,
            })
        })
        .await
        .map_err(|e| MqError::Internal(format!("mod scan failed: {e}")))?
}

struct ParsedJarMeta {
    mod_id: String,
    name: String,
    version: String,
    authors: Vec<String>,
    description: Option<String>,
    loader: Option<ModLoaderKind>,
    dependencies: Vec<String>,
}

/// Scans a single `.jar` or `.jar.disabled` file.
fn scan_single_jar(path: &Path) -> MqResult<InstalledModEntry> {
    let file_name = path
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();
    let enabled = file_name.ends_with(".jar");

    let file = File::open(path)?;
    let metadata = file.metadata()?;
    let size = metadata.len();

    // Stream SHA-1 and SHA-512
    let mut reader = std::io::BufReader::with_capacity(SCAN_STREAM_CHUNK_SIZE, file);
    let mut sha1_hasher = Sha1::new();
    let mut sha512_hasher = Sha512::new();
    let mut buf = [0u8; SCAN_STREAM_CHUNK_SIZE];

    loop {
        let n = reader.read(&mut buf)?;
        if n == 0 {
            break;
        }
        sha1_hasher.update(&buf[..n]);
        sha512_hasher.update(&buf[..n]);
    }

    let sha1_bytes: [u8; 20] = sha1_hasher.finalize().into();
    let sha512_bytes: [u8; 64] = sha512_hasher.finalize().into();
    let sha1 = Sha1Digest::from_bytes(sha1_bytes);
    let sha512 = Sha512Digest::from_bytes(sha512_bytes);

    // Reopen for ZIP reading
    let file2 = File::open(path)?;
    let mut archive = ZipArchive::new(file2)
        .map_err(|e| MqError::Archive(format!("failed to open ZIP {}: {e}", path.display())))?;

    let mut parsed_info: Option<ParsedJarMeta> = None;

    // Check descriptors in priority order
    // 1. fabric.mod.json
    if let Ok(entry) = archive.by_name("fabric.mod.json") {
        let mut content = Vec::with_capacity(entry.size().min(MAX_DESCRIPTOR_SIZE as u64) as usize);
        let _ = entry
            .take(MAX_DESCRIPTOR_SIZE as u64)
            .read_to_end(&mut content);
        if let Ok(val) = serde_json::from_slice::<serde_json::Value>(&content) {
            let mod_id = val
                .get("id")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let name = val
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or(&mod_id)
                .to_string();
            let version = val
                .get("version")
                .and_then(|v| v.as_str())
                .unwrap_or("1.0.0")
                .to_string();
            let desc = val
                .get("description")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());

            let mut authors = Vec::new();
            if let Some(arr) = val.get("authors").and_then(|v| v.as_array()) {
                for a in arr {
                    if let Some(s) = a.as_str() {
                        authors.push(s.to_string());
                    } else if let Some(n) = a.get("name").and_then(|v| v.as_str()) {
                        authors.push(n.to_string());
                    }
                }
            }

            let mut deps = Vec::new();
            if let Some(dep_map) = val.get("depends").and_then(|v| v.as_object()) {
                for k in dep_map.keys() {
                    deps.push(k.clone());
                }
            }

            if !mod_id.is_empty() {
                parsed_info = Some(ParsedJarMeta {
                    mod_id,
                    name,
                    version,
                    authors,
                    description: desc,
                    loader: Some(ModLoaderKind::Fabric),
                    dependencies: deps,
                });
            }
        }
    }

    // 2. quilt.mod.json
    if parsed_info.is_none() {
        if let Ok(entry) = archive.by_name("quilt.mod.json") {
            let mut content =
                Vec::with_capacity(entry.size().min(MAX_DESCRIPTOR_SIZE as u64) as usize);
            let _ = entry
                .take(MAX_DESCRIPTOR_SIZE as u64)
                .read_to_end(&mut content);
            if let Ok(val) = serde_json::from_slice::<serde_json::Value>(&content) {
                if let Some(ql) = val.get("quilt_loader") {
                    let mod_id = ql
                        .get("id")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    let version = ql
                        .get("version")
                        .and_then(|v| v.as_str())
                        .unwrap_or("1.0.0")
                        .to_string();
                    let meta = ql.get("metadata");
                    let name = meta
                        .and_then(|m| m.get("name"))
                        .and_then(|v| v.as_str())
                        .unwrap_or(&mod_id)
                        .to_string();
                    let desc = meta
                        .and_then(|m| m.get("description"))
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string());

                    let mut deps = Vec::new();
                    if let Some(arr) = ql.get("depends").and_then(|v| v.as_array()) {
                        for item in arr {
                            if let Some(s) = item.as_str() {
                                deps.push(s.to_string());
                            } else if let Some(s) = item.get("id").and_then(|v| v.as_str()) {
                                deps.push(s.to_string());
                            }
                        }
                    }

                    if !mod_id.is_empty() {
                        parsed_info = Some(ParsedJarMeta {
                            mod_id,
                            name,
                            version,
                            authors: Vec::new(),
                            description: desc,
                            loader: Some(ModLoaderKind::Quilt),
                            dependencies: deps,
                        });
                    }
                }
            }
        }
    }

    // 3. META-INF/neoforge.mods.toml or META-INF/mods.toml
    if parsed_info.is_none() {
        for (toml_name, loader) in [
            ("META-INF/neoforge.mods.toml", ModLoaderKind::NeoForge),
            ("META-INF/mods.toml", ModLoaderKind::Forge),
        ] {
            if let Ok(entry) = archive.by_name(toml_name) {
                let mut content = String::new();
                let _ = entry
                    .take(MAX_DESCRIPTOR_SIZE as u64)
                    .read_to_string(&mut content);
                if let Ok(val) = content.parse::<toml::Value>() {
                    if let Some(mods_arr) = val.get("mods").and_then(|v| v.as_array()) {
                        if let Some(first_mod) = mods_arr.first() {
                            let mod_id = first_mod
                                .get("modId")
                                .and_then(|v| v.as_str())
                                .unwrap_or("")
                                .to_string();
                            let name = first_mod
                                .get("displayName")
                                .and_then(|v| v.as_str())
                                .unwrap_or(&mod_id)
                                .to_string();
                            let version = first_mod
                                .get("version")
                                .and_then(|v| v.as_str())
                                .unwrap_or("1.0.0")
                                .to_string();
                            let desc = first_mod
                                .get("description")
                                .and_then(|v| v.as_str())
                                .map(|s| s.to_string());
                            let authors = first_mod
                                .get("authors")
                                .and_then(|v| v.as_str())
                                .map(|s| vec![s.to_string()])
                                .unwrap_or_default();

                            let mut deps = Vec::new();
                            if let Some(dep_table) =
                                val.get("dependencies").and_then(|v| v.as_table())
                            {
                                for (_target_mod, dep_list) in dep_table {
                                    if let Some(arr) = dep_list.as_array() {
                                        for d in arr {
                                            if let Some(dep_id) =
                                                d.get("modId").and_then(|v| v.as_str())
                                            {
                                                deps.push(dep_id.to_string());
                                            }
                                        }
                                    }
                                }
                            }

                            if !mod_id.is_empty() {
                                parsed_info = Some(ParsedJarMeta {
                                    mod_id,
                                    name,
                                    version,
                                    authors,
                                    description: desc,
                                    loader: Some(loader),
                                    dependencies: deps,
                                });
                                break;
                            }
                        }
                    }
                }
            }
        }
    }

    // Fallback: filename stem
    let (mod_id, name, version, authors, description, loader, dependencies) = match parsed_info {
        Some(m) => (
            m.mod_id,
            m.name,
            m.version,
            m.authors,
            m.description,
            m.loader,
            m.dependencies,
        ),
        None => {
            let stem = file_name
                .trim_end_matches(".disabled")
                .trim_end_matches(".jar");
            (
                stem.to_string(),
                stem.to_string(),
                "1.0.0".to_string(),
                Vec::new(),
                None,
                None,
                Vec::new(),
            )
        }
    };

    Ok(InstalledModEntry {
        file_name,
        path: path.to_path_buf(),
        enabled,
        mod_id,
        name,
        version,
        authors,
        description,
        loader,
        dependencies,
        sha1,
        sha512,
        size,
    })
}

/// Computes SHA-1, SHA-512, and CurseForge MurmurHash2 (seed 1, whitespace-stripped) fingerprint for a file.
pub fn hash_and_fingerprint_file(path: &Path) -> MqResult<Hashes> {
    let (sha1, sha512, _) = stream_hash_file_sha1_sha512(path)?;
    let fp = compute_curseforge_fingerprint_file(path)?;
    Ok(Hashes {
        sha1: Some(sha1),
        sha512: Some(sha512),
        md5: None,
        murmur2_fingerprint: Some(fp),
    })
}

/// Scans `dir` for `.jar`, `.jar.disabled`, `.zip`, `.zip.disabled` files, registering any
/// untracked files in `lockfile` with `source = LockSource::Manual` and returning all
/// `LockSource::Manual` entries with their computed [`Hashes`].
pub fn sync_directory_manual_entries(
    dir: &Path,
    kind: ProjectKind,
    lockfile: &mut ModLockFile,
) -> MqResult<Vec<(String, PathBuf, Hashes)>> {
    if !dir.exists() {
        return Ok(Vec::new());
    }

    let mut manual_candidates = Vec::new();
    let mut paths = Vec::new();
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let p = entry.path();
        if !p.is_file() {
            continue;
        }
        let fname = entry.file_name().to_string_lossy().to_string();
        if fname.ends_with(".jar")
            || fname.ends_with(".jar.disabled")
            || fname.ends_with(".zip")
            || fname.ends_with(".zip.disabled")
        {
            paths.push((fname, p));
        }
    }
    paths.sort_by(|a, b| a.0.cmp(&b.0));

    for (fname, path) in paths {
        let hashes = hash_and_fingerprint_file(&path)?;
        let sha1_hex = hashes.sha1.map(|h| h.to_hex());
        let sha512_hex = hashes.sha512.map(|h| h.to_hex()).unwrap_or_default();

        let existing_key = lockfile
            .find_by_filename(&fname)
            .map(|(k, _)| k.clone())
            .or_else(|| {
                lockfile
                    .find_by_hash(sha1_hex.as_deref(), Some(&sha512_hex))
                    .map(|(k, _)| k.clone())
            });

        let key = if let Some(k) = existing_key {
            if let Some(entry) = lockfile.entries.get_mut(&k) {
                if entry.sha1.is_none() {
                    entry.sha1 = sha1_hex.clone();
                }
                if entry.sha512.is_empty() {
                    entry.sha512.clone_from(&sha512_hex);
                }
            }
            k
        } else {
            let stem = fname
                .trim_end_matches(".disabled")
                .trim_end_matches(".jar")
                .trim_end_matches(".zip")
                .to_owned();
            let key = if lockfile.entries.contains_key(&stem) {
                fname.clone()
            } else {
                stem.clone()
            };
            lockfile.record_install(
                key.clone(),
                ModLockEntry {
                    source: LockSource::Manual,
                    project_id: stem,
                    version_id: String::new(),
                    file_id: None,
                    filename: fname,
                    sha1: sha1_hex,
                    sha512: sha512_hex,
                    installed_as_dependency: false,
                    required_by: Vec::new(),
                    kind,
                },
            );
            key
        };

        if lockfile
            .entries
            .get(&key)
            .is_some_and(|e| e.source == LockSource::Manual)
        {
            manual_candidates.push((key, path, hashes));
        }
    }

    Ok(manual_candidates)
}

/// Identifies manually added files in `dir` against a single [`ContentProvider`]
/// (by SHA-512/SHA-1 on Modrinth or Murmur2 fingerprint on CurseForge) and upgrades
/// matching `.mq-lock.json` entries from `LockSource::Manual` to `LockSource::Modrinth` / `LockSource::CurseForge`.
pub async fn identify_manual_files_with_provider<P>(
    dir: &Path,
    kind: ProjectKind,
    lockfile: &mut ModLockFile,
    provider: &P,
) -> MqResult<usize>
where
    P: ContentProvider + ?Sized,
{
    let candidates = sync_directory_manual_entries(dir, kind, lockfile)?;
    if candidates.is_empty() {
        return Ok(0);
    }

    let mut hash_queries = Vec::new();
    let mut fp_queries = Vec::new();

    for (_, _, hashes) in &candidates {
        if let Some(s512) = hashes.sha512 {
            hash_queries.push(s512.to_hex());
        } else if let Some(s1) = hashes.sha1 {
            hash_queries.push(s1.to_hex());
        }
        if let Some(fp) = hashes.murmur2_fingerprint {
            fp_queries.push(fp);
        }
    }

    let resolved_map = provider
        .identify_by_hashes(&hash_queries, &fp_queries)
        .await?;
    let target_source = LockSource::from(provider.source());
    let mut upgraded = 0usize;

    for (key, _, hashes) in candidates {
        let sha512_hex = hashes.sha512.map(|h| h.to_hex());
        let sha1_hex = hashes.sha1.map(|h| h.to_hex());
        let fp_str = hashes.murmur2_fingerprint.map(|fp| fp.to_string());

        let matched_ver = sha512_hex
            .as_deref()
            .and_then(|h| resolved_map.get(h))
            .or_else(|| sha1_hex.as_deref().and_then(|h| resolved_map.get(h)))
            .or_else(|| fp_str.as_deref().and_then(|f| resolved_map.get(f)));

        if let Some(ver) = matched_ver {
            if let Some(entry) = lockfile.entries.get_mut(&key) {
                entry.source = target_source;
                entry.project_id.clone_from(&ver.project_id);
                entry.version_id.clone_from(&ver.id);
                entry.file_id = Some(ver.id.clone());
                entry.kind = kind;
                upgraded += 1;
            }
        }
    }

    if dir.exists() {
        let _ = lockfile.save(dir);
    }

    Ok(upgraded)
}

/// Identifies manually added files in `dir` across Modrinth (SHA-512/SHA-1) and CurseForge (Murmur2 fingerprint),
/// upgrading `LockSource::Manual` entries in `lockfile` to `LockSource::Modrinth` or `LockSource::CurseForge`.
pub async fn identify_manual_files(
    dir: &Path,
    kind: ProjectKind,
    lockfile: &mut ModLockFile,
    modrinth: Option<&ModrinthClient>,
    curseforge: Option<&CurseForgeClient>,
) -> MqResult<usize> {
    let _ = sync_directory_manual_entries(dir, kind, lockfile)?;
    let mut total = 0usize;

    if let Some(mr) = modrinth {
        total += identify_manual_files_with_provider(dir, kind, lockfile, mr).await?;
    }
    if let Some(cf) = curseforge {
        total += identify_manual_files_with_provider(dir, kind, lockfile, cf).await?;
    }

    if dir.exists() {
        let _ = lockfile.save(dir);
    }

    Ok(total)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::io::Write;
    use tempfile::tempdir;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;
    use zip::write::SimpleFileOptions;

    #[tokio::test]
    async fn scans_mods_detects_metadata_and_missing_dependencies() {
        let dir = tempdir().unwrap();
        let sodium_jar = dir.path().join("sodium-0.5.8.jar");
        let extra_jar = dir.path().join("sodium-duplicate.jar");

        // Create sodium jar with fabric.mod.json that depends on fabric-api
        {
            let file = File::create(&sodium_jar).unwrap();
            let mut zip = zip::ZipWriter::new(file);
            let opt = SimpleFileOptions::default();
            zip.start_file("fabric.mod.json", opt).unwrap();
            zip.write_all(
                br#"{
                "id": "sodium",
                "name": "Sodium",
                "version": "0.5.8",
                "depends": {
                    "minecraft": ">=1.20.1",
                    "fabric-api": "*"
                }
            }"#,
            )
            .unwrap();
            zip.finish().unwrap();
        }

        // Create duplicate sodium jar
        {
            let file = File::create(&extra_jar).unwrap();
            let mut zip = zip::ZipWriter::new(file);
            let opt = SimpleFileOptions::default();
            zip.start_file("fabric.mod.json", opt).unwrap();
            zip.write_all(
                br#"{
                "id": "sodium",
                "name": "Sodium Duplicate",
                "version": "0.5.9"
            }"#,
            )
            .unwrap();
            zip.finish().unwrap();
        }

        let pool = ComputePool::new(2);
        let epoch = mq_sched::CancelEpoch::new();
        let guard = epoch.snapshot();

        let report = scan_mods_directory(dir.path(), &pool, &guard)
            .await
            .unwrap();
        assert_eq!(report.mods.len(), 2);
        assert!(!report.conflicts.is_empty());

        let has_dup = report
            .conflicts
            .iter()
            .any(|c| matches!(c, ModConflict::DuplicateModId { mod_id, .. } if mod_id == "sodium"));
        let has_missing_dep = report.conflicts.iter().any(|c| matches!(c, ModConflict::MissingDependency { required_dep, .. } if required_dep == "fabric-api"));

        assert!(has_dup, "Should detect duplicate mod ID");
        assert!(
            has_missing_dep,
            "Should detect missing fabric-api dependency"
        );
    }

    #[tokio::test]
    async fn manual_files_identified_by_sha512_or_murmur2_and_bound_to_lockfile() {
        let dir = tempdir().unwrap();
        let mr_jar = dir.path().join("manual-sodium.jar");
        let cf_jar = dir.path().join("manual-jei.jar");
        fs::write(&mr_jar, b"modrinth-sodium-payload").unwrap();
        fs::write(&cf_jar, b"curseforge-jei-payload").unwrap();

        let mr_hashes = hash_and_fingerprint_file(&mr_jar).unwrap();
        let cf_hashes = hash_and_fingerprint_file(&cf_jar).unwrap();
        let mr_sha512 = mr_hashes.sha512.unwrap().to_hex();
        let cf_fp = cf_hashes.murmur2_fingerprint.unwrap();

        let mut lock = ModLockFile::default();
        let candidates = sync_directory_manual_entries(dir.path(), ProjectKind::Mod, &mut lock)
            .expect("sync manual entries");
        assert_eq!(candidates.len(), 2);
        assert_eq!(
            lock.entries.get("manual-sodium").unwrap().source,
            LockSource::Manual
        );
        assert_eq!(
            lock.entries.get("manual-jei").unwrap().source,
            LockSource::Manual
        );

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let mr_sha512_clone = mr_sha512.clone();
        tokio::spawn(async move {
            while let Ok((mut stream, _)) = listener.accept().await {
                let sha512_key = mr_sha512_clone.clone();
                tokio::spawn(async move {
                    let mut buf = vec![0u8; 8192];
                    let n = stream.read(&mut buf).await.unwrap_or(0);
                    if n == 0 {
                        return;
                    }
                    let req = String::from_utf8_lossy(&buf[..n]);
                    let first_line = req.lines().next().unwrap_or("");
                    let body = if first_line.contains("/modrinth/v2/version_files") {
                        json!({
                            sha512_key.clone(): {
                                "id": "ver-mr-sodium",
                                "project_id": "AANobbMI",
                                "name": "Sodium 0.6.0",
                                "version_number": "0.6.0",
                                "game_versions": ["26.1"],
                                "loaders": ["fabric"],
                                "files": [{
                                    "hashes": { "sha512": sha512_key },
                                    "url": "https://cdn.modrinth.com/sodium.jar",
                                    "filename": "manual-sodium.jar",
                                    "primary": true,
                                    "size": 23
                                }],
                                "dependencies": []
                            }
                        })
                        .to_string()
                    } else if first_line.contains("/curseforge/v1/fingerprints/432") {
                        json!({
                            "data": {
                                "exactMatches": [{
                                    "id": 238222,
                                    "file": {
                                        "id": 9001,
                                        "modId": 238222,
                                        "fileName": "manual-jei.jar",
                                        "fileLength": 22,
                                        "downloadUrl": "https://edge.forgecdn.net/files/9/1/manual-jei.jar",
                                        "fileFingerprint": cf_fp,
                                        "hashes": [],
                                        "gameVersions": ["26.1", "Fabric"],
                                        "dependencies": []
                                    },
                                    "latestFiles": []
                                }],
                                "exactFingerprints": [cf_fp]
                            }
                        })
                        .to_string()
                    } else {
                        "{}".to_owned()
                    };
                    let resp = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    );
                    let _ = stream.write_all(resp.as_bytes()).await;
                });
            }
        });

        let mr_client =
            ModrinthClient::with_base_url(format!("http://{addr}/modrinth/v2")).unwrap();
        let cf_client = CurseForgeClient::with_base_url(
            format!("http://{addr}/curseforge/v1"),
            Some("test-key".into()),
        )
        .unwrap();

        let upgraded = identify_manual_files(
            dir.path(),
            ProjectKind::Mod,
            &mut lock,
            Some(&mr_client),
            Some(&cf_client),
        )
        .await
        .expect("identify manual files");
        assert_eq!(upgraded, 2);

        let sodium_entry = lock.entries.get("manual-sodium").unwrap();
        assert_eq!(sodium_entry.source, LockSource::Modrinth);
        assert_eq!(sodium_entry.project_id, "AANobbMI");
        assert_eq!(sodium_entry.version_id, "ver-mr-sodium");

        let jei_entry = lock.entries.get("manual-jei").unwrap();
        assert_eq!(jei_entry.source, LockSource::CurseForge);
        assert_eq!(jei_entry.project_id, "238222");
        assert_eq!(jei_entry.version_id, "9001");
    }
}
