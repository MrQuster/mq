#![deny(clippy::await_holding_lock)]
//! Safe native `.jar` library extraction with strict Zip-Slip, symlink, and path traversal defense.
//! Supports both legacy (flat/classifier extraction) and modern (nested/LWJGL 3) native archives.

use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use mq_core::{resolve_safe_path, MqError, MqResult};
use mq_sched::{ComputePool, EpochGuard};

/// Chunk size for streaming decompression (64 KiB).
pub const NATIVES_CHUNK_SIZE: usize = 64 * 1024;

/// Safely extracts Linux native library `.jar` files into `target_natives_dir`.
///
/// Defenses:
/// - Rejects path traversal (`..`, leading `/` or `\`, drive prefixes).
/// - Rejects Unix symlinks (`0o120000`).
/// - Skips entries matching `META-INF/` or `exclude_prefixes`.
/// - Streams decompression in bounded 64 KiB chunks without reading whole `.jar` into memory.
/// - For modern nested archives (e.g. `linux/x64/*.so`), creates top-level links/copies in
///   `target_natives_dir` so `-Djava.library.path` resolves shared libraries without recursive search.
/// - Guarantees that `target_natives_dir` actually exists on disk even if `native_jars` is empty.
pub async fn extract_natives_archives(
    native_jars: &[PathBuf],
    exclude_prefixes: &[Vec<String>],
    target_natives_dir: &Path,
    compute_pool: &ComputePool,
    epoch_guard: &EpochGuard,
) -> MqResult<usize> {
    // Guarantee target directory exists immediately, even if no archives are provided.
    fs::create_dir_all(target_natives_dir)?;

    if native_jars.is_empty() {
        return Ok(0);
    }

    let native_jars = native_jars.to_vec();
    let exclude_prefixes = exclude_prefixes.to_vec();
    let target_dir = target_natives_dir.to_path_buf();
    let epoch = epoch_guard.epoch();

    compute_pool
        .spawn_cpu_cancellable(epoch_guard, move || {
            fs::create_dir_all(&target_dir)?;
            let mut extracted_count = 0usize;

            for (idx, jar_path) in native_jars.iter().enumerate() {
                if !jar_path.exists() {
                    continue;
                }

                let file = File::open(jar_path)?;
                let mut archive = zip::ZipArchive::new(file).map_err(|e| {
                    MqError::Archive(format!(
                        "failed to open native JAR {}: {e}",
                        jar_path.display()
                    ))
                })?;

                let excludes = exclude_prefixes.get(idx).cloned().unwrap_or_default();

                for i in 0..archive.len() {
                    let mut entry = archive.by_index(i).map_err(|e| {
                        MqError::Archive(format!("failed to read ZIP entry {i}: {e}"))
                    })?;

                    let raw_name = entry.name().to_string();

                    // Directory entries don't need extraction
                    if entry.is_dir() || raw_name.ends_with('/') {
                        continue;
                    }

                    // Exclude META-INF and user/profile-specified prefixes
                    if raw_name.starts_with("META-INF/")
                        || raw_name.starts_with("META-INF\\")
                        || excludes.iter().any(|ex| raw_name.starts_with(ex))
                    {
                        continue;
                    }

                    // Security check: reject symlinks
                    if let Some(mode) = entry.unix_mode() {
                        if (mode & 0o170000) == 0o120000 {
                            return Err(MqError::ZipSlip {
                                entry: format!("symlink entry prohibited: {raw_name}"),
                            });
                        }
                    }

                    // Security check: zip-slip and path traversal
                    if raw_name.contains("..")
                        || raw_name.starts_with('/')
                        || raw_name.starts_with('\\')
                    {
                        return Err(MqError::ZipSlip { entry: raw_name });
                    }

                    let dest_path = resolve_safe_path(&target_dir, &raw_name)?;

                    if let Some(parent) = dest_path.parent() {
                        fs::create_dir_all(parent)?;
                    }

                    // Stream to a temp file then rename
                    let temp_path =
                        target_dir.join(format!(".extract_tmp.{}.{}.part", epoch, extracted_count));
                    {
                        let mut out_file = File::create(&temp_path)?;
                        let mut buffer = [0u8; NATIVES_CHUNK_SIZE];
                        loop {
                            let n = entry.read(&mut buffer)?;
                            if n == 0 {
                                break;
                            }
                            out_file.write_all(&buffer[..n])?;
                        }
                        out_file.flush()?;
                    }

                    fs::rename(&temp_path, &dest_path)?;
                    extracted_count += 1;

                    // Modern nested format support (e.g. linux/x64/liblwjgl.so):
                    // In Java, -Djava.library.path is non-recursive. If a native .so file is nested
                    // in subdirectories, also ensure it is available at the root of `target_dir`.
                    if is_native_shared_object(&dest_path) {
                        if let Some(file_name) = dest_path.file_name() {
                            let root_target = target_dir.join(file_name);
                            if root_target != dest_path
                                && !root_target.exists()
                                && fs::hard_link(&dest_path, &root_target).is_err()
                            {
                                let _ = fs::copy(&dest_path, &root_target);
                            }
                        }
                    }
                }
            }

            Ok(extracted_count)
        })
        .await
        .map_err(|e| MqError::Internal(format!("natives extraction task failed: {e}")))?
}

/// Guarantees that the natives directory exists on disk for `-Djava.library.path`.
pub fn ensure_natives_dir_exists(target_natives_dir: &Path) -> MqResult<()> {
    fs::create_dir_all(target_natives_dir)?;
    Ok(())
}

/// Checks whether a given path corresponds to a dynamic shared object library (`.so` or `.so.*`).
pub fn is_native_shared_object(path: &Path) -> bool {
    let name = match path.file_name().and_then(|n| n.to_str()) {
        Some(n) => n,
        None => return false,
    };
    name.ends_with(".so") || name.contains(".so.")
}

/// Recursively lists all native shared object (.so) files present in `target_natives_dir`.
pub fn list_native_shared_objects(target_natives_dir: &Path) -> MqResult<Vec<PathBuf>> {
    let mut so_files = Vec::new();
    if !target_natives_dir.is_dir() {
        return Ok(so_files);
    }

    collect_so_files_recursive(target_natives_dir, &mut so_files)?;
    so_files.sort();
    Ok(so_files)
}

fn collect_so_files_recursive(dir: &Path, acc: &mut Vec<PathBuf>) -> MqResult<()> {
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                let _ = collect_so_files_recursive(&path, acc);
            } else if is_native_shared_object(&path) {
                acc.push(path);
            }
        }
    }
    Ok(())
}

/// Verifies that `target_natives_dir` exists and contains at least one `.so` native library.
pub fn verify_natives_dir(target_natives_dir: &Path) -> MqResult<bool> {
    if !target_natives_dir.is_dir() {
        return Ok(false);
    }
    let objects = list_native_shared_objects(target_natives_dir)?;
    Ok(!objects.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;
    use zip::write::SimpleFileOptions;

    #[tokio::test]
    async fn rejects_zip_slip_in_native_jar() {
        let dir = tempdir().unwrap();
        let jar_path = dir.path().join("malicious.jar");
        let target_dir = dir.path().join("natives");

        {
            let file = File::create(&jar_path).unwrap();
            let mut zip = zip::ZipWriter::new(file);
            let options =
                SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
            zip.start_file("../../etc/passwd", options).unwrap();
            zip.write_all(b"root:x:0:0:root:/root:/bin/bash").unwrap();
            zip.finish().unwrap();
        }

        let pool = ComputePool::new(2);
        let cancel_epoch = mq_sched::CancelEpoch::new();
        let epoch_guard = cancel_epoch.snapshot();

        let res =
            extract_natives_archives(&[jar_path], &[vec![]], &target_dir, &pool, &epoch_guard)
                .await;
        assert!(matches!(res, Err(MqError::ZipSlip { .. })));
    }

    #[tokio::test]
    async fn extracts_valid_natives_and_skips_meta_inf() {
        let dir = tempdir().unwrap();
        let jar_path = dir.path().join("good.jar");
        let target_dir = dir.path().join("natives");

        {
            let file = File::create(&jar_path).unwrap();
            let mut zip = zip::ZipWriter::new(file);
            let options =
                SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
            zip.start_file("libsample.so", options).unwrap();
            zip.write_all(b"elf binary content").unwrap();
            zip.start_file("META-INF/MANIFEST.MF", options).unwrap();
            zip.write_all(b"Manifest-Version: 1.0").unwrap();
            zip.finish().unwrap();
        }

        let pool = ComputePool::new(2);
        let cancel_epoch = mq_sched::CancelEpoch::new();
        let epoch_guard = cancel_epoch.snapshot();

        let count =
            extract_natives_archives(&[jar_path], &[vec![]], &target_dir, &pool, &epoch_guard)
                .await
                .unwrap();
        assert_eq!(count, 1);
        assert!(target_dir.join("libsample.so").exists());
        assert!(!target_dir.join("META-INF/MANIFEST.MF").exists());
        assert!(verify_natives_dir(&target_dir).unwrap());
    }

    #[tokio::test]
    async fn creates_natives_dir_even_when_jar_list_empty() {
        let dir = tempdir().unwrap();
        let target_dir = dir.path().join("natives_empty");

        let pool = ComputePool::new(1);
        let cancel_epoch = mq_sched::CancelEpoch::new();
        let epoch_guard = cancel_epoch.snapshot();

        let count = extract_natives_archives(&[], &[], &target_dir, &pool, &epoch_guard)
            .await
            .unwrap();
        assert_eq!(count, 0);
        assert!(target_dir.is_dir());
    }

    #[tokio::test]
    async fn extracts_nested_modern_natives_and_links_to_root() {
        let dir = tempdir().unwrap();
        let jar_path = dir.path().join("modern-natives.jar");
        let target_dir = dir.path().join("natives_modern");

        {
            let file = File::create(&jar_path).unwrap();
            let mut zip = zip::ZipWriter::new(file);
            let options =
                SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
            zip.start_file("linux/x64/liblwjgl.so", options).unwrap();
            zip.write_all(b"lwjgl binary bytes").unwrap();
            zip.finish().unwrap();
        }

        let pool = ComputePool::new(2);
        let cancel_epoch = mq_sched::CancelEpoch::new();
        let epoch_guard = cancel_epoch.snapshot();

        let count =
            extract_natives_archives(&[jar_path], &[vec![]], &target_dir, &pool, &epoch_guard)
                .await
                .unwrap();
        assert_eq!(count, 1);
        assert!(target_dir.join("linux/x64/liblwjgl.so").exists());
        // Root link exists for non-recursive -Djava.library.path lookup
        assert!(target_dir.join("liblwjgl.so").exists());
        let so_list = list_native_shared_objects(&target_dir).unwrap();
        assert!(so_list.iter().any(|p| p.ends_with("liblwjgl.so")));
    }
}
