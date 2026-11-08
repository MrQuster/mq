//! Official Mojang Java Runtime downloader and package extractor.
//!
//! Downloads official JRE packages maintained by Mojang from:
//! `https://launchermeta.mojang.com/v1/products/java-runtime/2ec0cc96c44e5a76b9c8b7c39df7210883d12871/all.json`
//!
//! Supports components:
//! - `java-runtime-epsilon` (Java 25 for Minecraft 26.x)
//! - `java-runtime-delta`   (Java 21 for Minecraft 1.20.5+)
//! - `java-runtime-gamma`   (Java 17 for Minecraft 1.18 - 1.20.4)
//! - `jre-legacy`           (Java 8 for Minecraft <= 1.17.x)
//!
//! Validates SHA-1 digests for every package file and sets `0o755` permissions on executables.

use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::{Cursor, Write};
use std::path::Path;
use std::sync::Arc;

use mq_core::{MqError, MqResult};
use serde::{Deserialize, Serialize};
use sha1::{Digest, Sha1};

use crate::java::{probe_java_binary, verify_java_executable, JavaRuntime};

/// Mojang Java Runtime product catalog URL.
pub const MOJANG_JAVA_RUNTIME_ALL_URL: &str =
    "https://launchermeta.mojang.com/v1/products/java-runtime/2ec0cc96c44e5a76b9c8b7c39df7210883d12871/all.json";

/// Returns the Mojang platform identifier corresponding to the target OS and CPU architecture.
pub fn current_mojang_platform() -> &'static str {
    #[cfg(target_os = "windows")]
    {
        #[cfg(target_arch = "x86_64")]
        return "windows-x64";
        #[cfg(target_arch = "x86")]
        return "windows-x86";
        #[cfg(target_arch = "aarch64")]
        return "windows-arm64";
        #[cfg(not(any(target_arch = "x86_64", target_arch = "x86", target_arch = "aarch64")))]
        return "windows-x64";
    }
    #[cfg(target_os = "macos")]
    {
        #[cfg(target_arch = "aarch64")]
        return "mac-os-arm64";
        #[cfg(not(target_arch = "aarch64"))]
        return "mac-os";
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        #[cfg(target_arch = "x86")]
        return "linux-i386";
        #[cfg(not(target_arch = "x86"))]
        return "linux";
    }
}

/// Returns the default Mojang component name for a given Java major version.
pub fn component_for_major_version(major_version: u32) -> &'static str {
    match major_version {
        25.. => "java-runtime-epsilon",
        21..=24 => "java-runtime-delta",
        17..=20 => "java-runtime-gamma",
        _ => "jre-legacy",
    }
}

/// Infers the Java major feature version from a Mojang component name.
pub fn major_version_for_component(component: &str) -> Option<u32> {
    match component.trim() {
        "java-runtime-epsilon" => Some(25),
        "java-runtime-delta" => Some(21),
        "java-runtime-gamma" | "java-runtime-gamma-snapshot" | "java-runtime-beta" => Some(17),
        "java-runtime-alpha" => Some(16),
        "jre-legacy" => Some(8),
        _ => None,
    }
}

/// Top-level Mojang `all.json` catalog structure.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MojangProductCatalog {
    #[serde(flatten)]
    pub platforms: BTreeMap<String, BTreeMap<String, Vec<MojangCatalogEntry>>>,
}

/// Single component entry inside `all.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MojangCatalogEntry {
    #[serde(default)]
    pub availability: Option<MojangAvailability>,
    pub manifest: MojangManifestPointer,
    pub version: MojangRuntimeVersion,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MojangAvailability {
    pub group: Option<u32>,
    pub progress: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MojangManifestPointer {
    pub sha1: String,
    pub size: u64,
    pub url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MojangRuntimeVersion {
    pub name: Option<String>,
    pub released: Option<String>,
}

/// Detailed component package manifest describing all files, directories, and symlinks.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MojangPackageManifest {
    pub files: BTreeMap<String, MojangFileNode>,
}

/// A node within the component package file tree.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum MojangFileNode {
    Directory,
    File {
        #[serde(default)]
        executable: bool,
        downloads: MojangDownloadVariants,
    },
    Link {
        target: String,
    },
}

/// Raw and LZMA compressed download options for a file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MojangDownloadVariants {
    pub raw: Option<MojangDownloadFile>,
    pub lzma: Option<MojangDownloadFile>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MojangDownloadFile {
    pub sha1: String,
    pub size: u64,
    pub url: String,
}

/// Parses the official `all.json` catalog string.
pub fn parse_mojang_catalog(json: &str) -> MqResult<MojangProductCatalog> {
    serde_json::from_str(json).map_err(|e| {
        MqError::Metadata(format!("failed to parse Mojang java-runtime all.json: {e}"))
    })
}

/// Parses a component package manifest JSON string.
pub fn parse_mojang_package_manifest(json: &str) -> MqResult<MojangPackageManifest> {
    serde_json::from_str(json)
        .map_err(|e| MqError::Metadata(format!("failed to parse Mojang component manifest: {e}")))
}

/// Finds the catalog entry for the specified platform and component.
pub fn find_mojang_runtime_entry<'a>(
    catalog: &'a MojangProductCatalog,
    platform: &str,
    component: &str,
) -> Option<&'a MojangCatalogEntry> {
    catalog
        .platforms
        .get(platform)
        .and_then(|comps| comps.get(component))
        .and_then(|entries| entries.first())
}

/// Resolves or downloads the official Mojang Java runtime for the given component or major version.
///
/// Packages are installed into `<data_dir>/java/mojang-<component>/`.
pub async fn ensure_mojang_java_runtime(
    client: &reqwest::Client,
    java_parent_dir: &Path,
    component: &str,
    platform: Option<&str>,
    custom_catalog_url: Option<&str>,
) -> MqResult<JavaRuntime> {
    let plat = platform.unwrap_or(current_mojang_platform());
    let target_dir_name = format!("mojang-{component}");
    let dest_dir = java_parent_dir.join(&target_dir_name);
    let bin_name = if cfg!(target_os = "windows") {
        "java.exe"
    } else {
        "java"
    };
    let java_bin = dest_dir.join("bin").join(bin_name);

    // If already installed and functional, return it immediately.
    if java_bin.is_file() {
        if let Ok(rt) = verify_java_executable(&java_bin) {
            return Ok(rt);
        }
        if let Some(rt) = probe_java_binary(&java_bin) {
            return Ok(rt);
        }
    }

    // 1. Fetch all.json catalog
    let catalog_url = custom_catalog_url.unwrap_or(MOJANG_JAVA_RUNTIME_ALL_URL);
    let catalog_resp = client.get(catalog_url).send().await.map_err(|e| {
        MqError::Network(format!("failed to fetch Mojang java-runtime catalog: {e}"))
    })?;

    if !catalog_resp.status().is_success() {
        return Err(MqError::Network(format!(
            "Mojang java-runtime catalog returned HTTP status {}",
            catalog_resp.status()
        )));
    }

    let catalog_text = catalog_resp
        .text()
        .await
        .map_err(|e| MqError::Network(format!("failed to read catalog response: {e}")))?;
    let catalog = parse_mojang_catalog(&catalog_text)?;

    // 2. Find component entry
    let entry = find_mojang_runtime_entry(&catalog, plat, component).ok_or_else(|| {
        MqError::JavaNotFound {
            required_major: major_version_for_component(component).unwrap_or(21),
            hint: format!(
                "Mojang java-runtime catalog has no component '{component}' for platform '{plat}'"
            ),
        }
    })?;

    // 3. Fetch component package manifest
    let manifest_resp = client
        .get(&entry.manifest.url)
        .send()
        .await
        .map_err(|e| MqError::Network(format!("failed to fetch component manifest: {e}")))?;

    if !manifest_resp.status().is_success() {
        return Err(MqError::Network(format!(
            "component manifest returned HTTP status {}",
            manifest_resp.status()
        )));
    }

    let manifest_bytes = manifest_resp.bytes().await.map_err(|e| {
        MqError::Network(format!("failed to download component manifest bytes: {e}"))
    })?;

    // Verify manifest SHA-1
    let mut manifest_hasher = Sha1::new();
    manifest_hasher.update(&manifest_bytes);
    let actual_sha1 = hex::encode(manifest_hasher.finalize()).to_lowercase();
    if actual_sha1 != entry.manifest.sha1.to_lowercase() {
        return Err(MqError::Internal(format!(
            "Mojang component manifest SHA-1 mismatch: expected {}, got {}",
            entry.manifest.sha1, actual_sha1
        )));
    }

    let manifest_str = String::from_utf8_lossy(&manifest_bytes);
    let package_manifest = parse_mojang_package_manifest(&manifest_str)?;

    // 4. Download and unpack package files atomically into temporary directory
    fs::create_dir_all(java_parent_dir)?;
    let temp_extract = java_parent_dir.join(format!(
        ".tmp_mojang_{component}_{}_{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    if temp_extract.exists() {
        let _ = fs::remove_dir_all(&temp_extract);
    }
    fs::create_dir_all(&temp_extract)?;

    let result = unpack_mojang_package(client, &package_manifest, &temp_extract).await;
    if let Err(e) = result {
        let _ = fs::remove_dir_all(&temp_extract);
        return Err(e);
    }

    // 5. Atomic swap into destination directory
    if dest_dir.exists() {
        let _ = fs::remove_dir_all(&dest_dir);
    }
    fs::rename(&temp_extract, &dest_dir)?;

    // 6. Ensure executable permissions on bin/java
    let installed_bin = dest_dir.join("bin").join(bin_name);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if installed_bin.exists() {
            let _ = fs::set_permissions(&installed_bin, fs::Permissions::from_mode(0o755));
        }
    }

    // 7. Verify installed JVM with `<java> -version`
    verify_java_executable(&installed_bin).map_err(|e| {
        MqError::Internal(format!(
            "installed Mojang Java runtime at {} failed verification: {e}",
            installed_bin.display()
        ))
    })
}

/// Unpacks all files and symlinks from a [`MojangPackageManifest`] into `dest_root`.
async fn unpack_mojang_package(
    client: &reqwest::Client,
    manifest: &MojangPackageManifest,
    dest_root: &Path,
) -> MqResult<()> {
    let mut files_to_download: Vec<(&String, bool, &MojangDownloadVariants)> = Vec::new();
    let mut symlinks: Vec<(&String, &String)> = Vec::new();

    for (rel_path, node) in &manifest.files {
        match node {
            MojangFileNode::Directory => {
                let dir_path = dest_root.join(rel_path);
                fs::create_dir_all(&dir_path)?;
            }
            MojangFileNode::File {
                executable,
                downloads,
            } => {
                files_to_download.push((rel_path, *executable, downloads));
            }
            MojangFileNode::Link { target } => {
                symlinks.push((rel_path, target));
            }
        }
    }

    // Concurrently download and extract files with concurrency ceiling
    let semaphore = Arc::new(tokio::sync::Semaphore::new(16));
    let mut tasks = Vec::with_capacity(files_to_download.len());

    for (rel_path, executable, downloads) in files_to_download {
        let rel_path_owned = rel_path.clone();
        let target_path = dest_root.join(&rel_path_owned);
        let downloads_owned = downloads.clone();
        let client_clone = client.clone();
        let sem_clone = Arc::clone(&semaphore);

        tasks.push(tokio::spawn(async move {
            let _permit = sem_clone
                .acquire()
                .await
                .map_err(|e| MqError::Internal(e.to_string()))?;

            if let Some(parent) = target_path.parent() {
                fs::create_dir_all(parent)?;
            }

            download_and_verify_single_file(
                &client_clone,
                &downloads_owned,
                &target_path,
                &rel_path_owned,
                executable,
            )
            .await
        }));
    }

    for task in tasks {
        match task.await {
            Ok(Ok(())) => {}
            Ok(Err(e)) => return Err(e),
            Err(join_err) => {
                return Err(MqError::Internal(format!(
                    "download task panic: {join_err}"
                )))
            }
        }
    }

    // Second pass: create symlinks after all target files have been extracted
    for (rel_path, target) in symlinks {
        let link_path = dest_root.join(rel_path);
        if let Some(parent) = link_path.parent() {
            fs::create_dir_all(parent)?;
        }

        #[cfg(unix)]
        {
            // Remove existing if any
            let _ = fs::remove_file(&link_path);
            let _ = std::os::unix::fs::symlink(target, &link_path);
        }
        #[cfg(not(unix))]
        {
            let resolved_target = link_path.parent().unwrap_or(dest_root).join(target);
            if resolved_target.exists() {
                let _ = fs::copy(&resolved_target, &link_path);
            }
        }
    }

    Ok(())
}

/// Downloads and verifies a single file using `lzma` (if present and valid) or `raw` fallback.
async fn download_and_verify_single_file(
    client: &reqwest::Client,
    downloads: &MojangDownloadVariants,
    target_path: &Path,
    rel_path: &str,
    executable: bool,
) -> MqResult<()> {
    // 1. Try LZMA download if available
    let mut decompressed_bytes: Option<Vec<u8>> = None;

    if let Some(lzma_spec) = &downloads.lzma {
        if let Ok(resp) = client.get(&lzma_spec.url).send().await {
            if resp.status().is_success() {
                if let Ok(bytes) = resp.bytes().await {
                    let mut hasher = Sha1::new();
                    hasher.update(&bytes);
                    let actual_sha1 = hex::encode(hasher.finalize()).to_lowercase();
                    if actual_sha1 == lzma_spec.sha1.to_lowercase() {
                        let mut decompressed = Vec::new();
                        let mut cursor = Cursor::new(&bytes[..]);
                        if lzma_rs::lzma_decompress(&mut cursor, &mut decompressed).is_ok() {
                            // If raw checksum is specified, verify it on decompressed payload
                            if let Some(raw_spec) = &downloads.raw {
                                let mut raw_hasher = Sha1::new();
                                raw_hasher.update(&decompressed);
                                let dec_sha1 = hex::encode(raw_hasher.finalize()).to_lowercase();
                                if dec_sha1 == raw_spec.sha1.to_lowercase() {
                                    decompressed_bytes = Some(decompressed);
                                }
                            } else {
                                decompressed_bytes = Some(decompressed);
                            }
                        }
                    }
                }
            }
        }
    }

    // 2. Fall back to raw download
    let final_bytes = match decompressed_bytes {
        Some(b) => b,
        None => {
            let raw_spec = downloads.raw.as_ref().ok_or_else(|| {
                MqError::Internal(format!(
                    "file '{rel_path}' has neither valid lzma nor raw download"
                ))
            })?;

            let resp =
                client.get(&raw_spec.url).send().await.map_err(|e| {
                    MqError::Network(format!("failed to download '{rel_path}': {e}"))
                })?;

            if !resp.status().is_success() {
                return Err(MqError::Network(format!(
                    "downloading '{rel_path}' returned HTTP status {}",
                    resp.status()
                )));
            }

            let bytes = resp.bytes().await.map_err(|e| {
                MqError::Network(format!("failed to read body for '{rel_path}': {e}"))
            })?;

            // Verify raw SHA-1
            let mut hasher = Sha1::new();
            hasher.update(&bytes);
            let actual_sha1 = hex::encode(hasher.finalize()).to_lowercase();
            if actual_sha1 != raw_spec.sha1.to_lowercase() {
                return Err(MqError::Internal(format!(
                    "SHA-1 mismatch for '{rel_path}': expected {}, got {}",
                    raw_spec.sha1, actual_sha1
                )));
            }

            bytes.to_vec()
        }
    };

    // 3. Write file
    let mut out_file = File::create(target_path)?;
    out_file.write_all(&final_bytes)?;
    out_file.flush()?;

    // 4. Set executable permissions if needed (0o755)
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let is_bin = executable || rel_path.starts_with("bin/");
        if is_bin {
            let mut perms = out_file.metadata()?.permissions();
            perms.set_mode(0o755);
            let _ = fs::set_permissions(target_path, perms);
        }
    }

    Ok(())
}
