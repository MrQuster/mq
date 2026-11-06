#![deny(clippy::await_holding_lock)]
//! Safe native `.jar` library extraction with strict Zip-Slip, symlink, and path traversal defense.

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
pub async fn extract_natives_archives(
    native_jars: &[PathBuf],
    exclude_prefixes: &[Vec<String>],
    target_natives_dir: &Path,
    compute_pool: &ComputePool,
    epoch_guard: &EpochGuard,
) -> MqResult<usize> {
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
                }
            }

            Ok(extracted_count)
        })
        .await
        .map_err(|e| MqError::Internal(format!("natives extraction task failed: {e}")))?
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
    }
}
