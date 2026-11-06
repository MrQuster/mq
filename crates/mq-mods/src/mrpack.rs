//! Modrinth `.mrpack` (`modrinth.index.json`) parser and multi-threaded installer.
//!
//! Enforces strict Zip-Slip pre-validation on all archive entries and `files[].path`,
//! filters client files (`env.client != "unsupported"`), downloads artifacts in parallel
//! into the SHA-1 CAS via [`NetEngine`], verifies both SHA-1 and SHA-512 digests in
//! parallel across all CPU cores on [`ComputePool`] using `rayon` (`64 KiB` streaming chunks),
//! hardlinks verified blobs into the instance directory, and safely extracts `overrides/`
//! followed by `client-overrides/`.

use std::collections::BTreeMap;
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

use crate::zip_safe::{
    extract_zip_prefix_safely, link_or_copy_cas_blob, read_zip_entry_bounded,
    validate_archive_entry_path, validate_zip_archive, verify_file_sha1_and_sha512,
};

/// Maximum size in bytes of `modrinth.index.json` inside a `.mrpack` archive (`16 MiB`).
const MAX_MRPACK_INDEX_BYTES: usize = 16 * 1024 * 1024;

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

/// Validates every entry in `mrpack_path` against Zip-Slip and Unix symlinks, then reads
/// and parses `modrinth.index.json` while validating every `file.path` via [`resolve_safe_path`].
pub fn parse_mrpack_index_from_archive(mrpack_path: &Path) -> MqResult<MrpackIndex> {
    let file = File::open(mrpack_path)?;
    let mut archive = ZipArchive::new(file).map_err(|err| {
        MqError::Archive(format!(
            "failed to open .mrpack archive '{}': {err}",
            mrpack_path.display()
        ))
    })?;

    // 1. Pre-validate every entry in the .mrpack ZIP for zip-slip and symlink escapes.
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
/// 1. Validates the archive and `modrinth.index.json` against Zip-Slip and symlinks.
/// 2. Filters `files[]` by `env.client` (skipping `"unsupported"`).
/// 3. Downloads all required/optional files in parallel into the SHA-1 CAS via [`NetEngine::download_batch`].
/// 4. Verifies both SHA-1 and SHA-512 in parallel across all CPU cores on [`ComputePool`] using `rayon` (`par_iter`) in `64 KiB` streaming chunks.
/// 5. Hardlinks verified CAS blobs into `<instance_dir>/<file.path>`.
/// 6. Safely extracts `"overrides/"` and then `"client-overrides/"` onto `instance_dir`.
pub async fn install_mrpack(
    mrpack_path: &Path,
    instance_dir: &Path,
    paths: &MqPaths,
    net: &NetEngine,
    compute_pool: &ComputePool,
    epoch_guard: &EpochGuard,
    event_bus: Option<&EventBus>,
) -> MqResult<MrpackInstallOutcome> {
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
        let target_path = validate_archive_entry_path(instance_dir, &file_entry.path, None)?;
        let cas_path = paths.cas_sha1_path(&file_entry.hashes.sha1.to_hex());
        kept_entries.push((file_entry.clone(), cas_path, target_path));
    }

    let specs: Vec<DownloadSpec> = kept_entries
        .iter()
        .map(|(entry, cas_path, _target_path)| {
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
            spec
        })
        .collect();

    if let Some(bus) = event_bus {
        bus.publish(Event::PhaseStarted {
            phase: "mrpack_install".to_owned(),
            total_items: specs.len() as u64,
        });
    }

    net.download_batch(specs, "mrpack_download", epoch_guard)
        .await?;

    // Verify BOTH SHA-1 and SHA-512 in parallel via rayon on ComputePool, hardlink into instance_dir,
    // and extract overrides/ + client-overrides/ deterministically.
    let mrpack_path_owned = mrpack_path.to_path_buf();
    let instance_dir_owned = instance_dir.to_path_buf();
    let pool_clone = compute_pool.clone();
    let worker_guard = epoch_guard.clone();

    let (installed_files, overrides_extracted, client_overrides_extracted) = compute_pool
        .spawn_cpu_cancellable(
            epoch_guard,
            move || -> MqResult<(Vec<PathBuf>, usize, usize)> {
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
                            Ok(target_path.clone())
                        })
                        .collect()
                });

                let installed_files = installed?;
                worker_guard
                    .check()
                    .map_err(|e| MqError::Cancelled { epoch: e.epoch })?;

                let overrides_extracted = extract_zip_prefix_safely(
                    &mrpack_path_owned,
                    "overrides",
                    &instance_dir_owned,
                )?;
                let client_overrides_extracted = extract_zip_prefix_safely(
                    &mrpack_path_owned,
                    "client-overrides",
                    &instance_dir_owned,
                )?;

                Ok((
                    installed_files,
                    overrides_extracted,
                    client_overrides_extracted,
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

    if let Some(bus) = event_bus {
        bus.publish(Event::PhaseCompleted {
            phase: "mrpack_install".to_owned(),
            elapsed_ms: started_at.elapsed().as_millis() as u64,
        });
    }

    Ok(MrpackInstallOutcome {
        index,
        minecraft_version: mc_version,
        loader,
        loader_version,
        installed_files,
        skipped_files,
        overrides_extracted,
        client_overrides_extracted,
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
    }
}
