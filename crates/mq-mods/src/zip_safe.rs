//! Strict Zip-Slip, Unix symlink, and path traversal protection for ZIP/JAR/modpack archives,
//! streaming prefix extraction (`64 KiB` chunks), CAS hardlink materialization, and
//! streaming dual SHA-1 + SHA-512 verification.

use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};

use mq_core::{resolve_safe_path, MqError, MqResult, Sha1Digest, Sha512Digest};
use sha1::{Digest as _, Sha1};
use sha2::Sha512;
use zip::ZipArchive;

/// Streaming I/O chunk size (`64 KiB`) used for archive extraction, CAS copy fallback,
/// and dual SHA-1/SHA-512 hashing so whole archives or `.jar` files are never buffered in RAM.
pub const STREAM_CHUNK_SIZE: usize = 64 * 1024;

/// Unix `S_IFMT` file type bitmask.
const S_IFMT: u32 = 0o170000;
/// Unix `S_IFLNK` symbolic link file type bits.
const S_IFLNK: u32 = 0o120000;

/// Returns `true` if the given Unix mode bits encode a symbolic link (`(mode & 0o170000) == 0o120000`).
#[inline]
pub const fn is_unix_symlink_mode(mode: u32) -> bool {
    (mode & S_IFMT) == S_IFLNK
}

/// Validates a raw archive entry path and optional Unix mode bits against Zip-Slip,
/// symlink escapes, absolute paths, Windows drive letters, NUL bytes, and `..` segments.
///
/// Rejects:
/// - Unix symlinks (`(mode & 0o170000) == 0o120000`) with [`MqError::ZipSlip`].
/// - Paths containing `..`, leading `/` or `\`, Windows drive prefixes (`C:`), or `\0` with [`MqError::ZipSlip`].
/// - Existing symlink escapes beneath `base_dir` via [`mq_core::resolve_safe_path`].
pub fn validate_archive_entry_path(
    base_dir: &Path,
    raw_entry_name: &str,
    unix_mode: Option<u32>,
) -> MqResult<PathBuf> {
    if let Some(mode) = unix_mode {
        if is_unix_symlink_mode(mode) {
            return Err(MqError::ZipSlip {
                entry: raw_entry_name.to_owned(),
            });
        }
    }

    if raw_entry_name.is_empty()
        || raw_entry_name.contains('\0')
        || raw_entry_name.starts_with('/')
        || raw_entry_name.starts_with('\\')
    {
        return Err(MqError::ZipSlip {
            entry: raw_entry_name.to_owned(),
        });
    }

    let bytes = raw_entry_name.as_bytes();
    if bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' {
        return Err(MqError::ZipSlip {
            entry: raw_entry_name.to_owned(),
        });
    }

    // Explicitly inspect raw path segments before normalization so any `..` or `.`
    // traversal component is caught immediately even if collapsed by Path components.
    for seg in raw_entry_name.split(['/', '\\']) {
        if seg == ".." || seg == "." {
            return Err(MqError::ZipSlip {
                entry: raw_entry_name.to_owned(),
            });
        }
    }

    let normalized = raw_entry_name.replace('\\', "/");
    let trimmed = normalized.trim_end_matches('/');
    if trimmed.is_empty() {
        return Err(MqError::ZipSlip {
            entry: raw_entry_name.to_owned(),
        });
    }

    for comp in Path::new(trimmed).components() {
        match comp {
            Component::Normal(part) if !part.is_empty() && part != "." && part != ".." => {}
            _ => {
                return Err(MqError::ZipSlip {
                    entry: raw_entry_name.to_owned(),
                });
            }
        }
    }

    resolve_safe_path(base_dir, trimmed).map_err(|err| match err {
        MqError::ZipSlip { .. } => MqError::ZipSlip {
            entry: raw_entry_name.to_owned(),
        },
        other => other,
    })
}

/// Pre-scans every entry in a [`ZipArchive`] to ensure no entry contains Zip-Slip path
/// traversal or Unix symbolic links before any file is read or extracted.
pub fn validate_zip_archive<R: Read + std::io::Seek>(
    archive: &mut ZipArchive<R>,
    base_dir: &Path,
) -> MqResult<()> {
    for idx in 0..archive.len() {
        let entry = archive.by_index_raw(idx).map_err(|err| {
            MqError::Archive(format!(
                "failed to read ZIP central directory entry {idx}: {err}"
            ))
        })?;
        let raw_name = entry.name().to_owned();
        let unix_mode = entry.unix_mode();
        if entry.is_symlink() {
            return Err(MqError::ZipSlip { entry: raw_name });
        }
        validate_archive_entry_path(base_dir, &raw_name, unix_mode)?;
    }
    Ok(())
}

/// Streams all files under `prefix` (e.g. `"overrides/"` or `"client-overrides/"`) from
/// `zip_path` into `target_dir` in `64 KiB` chunks after validating every entry in the
/// archive via [`validate_archive_entry_path`].
///
/// Never buffers whole archive entries in memory. Returns the number of extracted files.
pub fn extract_zip_prefix_safely(
    zip_path: &Path,
    prefix: &str,
    target_dir: &Path,
) -> MqResult<usize> {
    let file = File::open(zip_path)?;
    let mut archive = ZipArchive::new(file).map_err(|err| {
        MqError::Archive(format!(
            "failed to open ZIP archive '{}': {err}",
            zip_path.display()
        ))
    })?;

    // Validate the entire archive upfront so malicious entries anywhere in the ZIP fail fast.
    validate_zip_archive(&mut archive, target_dir)?;

    let clean_prefix = prefix.replace('\\', "/");
    let clean_prefix = clean_prefix.trim_matches('/');
    let prefix_with_slash = if clean_prefix.is_empty() {
        String::new()
    } else {
        format!("{clean_prefix}/")
    };

    fs::create_dir_all(target_dir)?;

    let mut buf = vec![0u8; STREAM_CHUNK_SIZE];
    let mut extracted_count = 0usize;

    for idx in 0..archive.len() {
        let mut entry = archive.by_index(idx).map_err(|err| {
            MqError::Archive(format!(
                "failed to access entry {idx} in '{}': {err}",
                zip_path.display()
            ))
        })?;

        let raw_name = entry.name().to_owned();
        let unix_mode = entry.unix_mode();
        if entry.is_symlink() {
            return Err(MqError::ZipSlip { entry: raw_name });
        }

        // Validate full entry path first.
        validate_archive_entry_path(target_dir, &raw_name, unix_mode)?;

        let normalized_name = raw_name.replace('\\', "/");
        if normalized_name == clean_prefix {
            continue;
        }

        let rel_inside_prefix = if prefix_with_slash.is_empty() {
            normalized_name.as_str()
        } else {
            let Some(stripped) = normalized_name.strip_prefix(&prefix_with_slash) else {
                continue;
            };
            stripped
        };

        let rel_trimmed = rel_inside_prefix.trim_end_matches('/');
        if rel_trimmed.is_empty() {
            continue;
        }

        // Validate the stripped relative target path inside `target_dir`.
        let dest_path = validate_archive_entry_path(target_dir, rel_trimmed, unix_mode)?;

        if entry.is_dir() || normalized_name.ends_with('/') {
            fs::create_dir_all(&dest_path)?;
            continue;
        }

        if let Some(parent) = dest_path.parent() {
            fs::create_dir_all(parent)?;
        }

        let mut out_file = File::create(&dest_path)?;
        loop {
            let n = entry.read(&mut buf)?;
            if n == 0 {
                break;
            }
            out_file.write_all(&buf[..n])?;
        }
        out_file.flush()?;
        extracted_count += 1;
    }

    Ok(extracted_count)
}

/// Hardlinks `cas_blob` to `target_path` (`std::fs::hard_link`), falling back to a `64 KiB`
/// streaming copy if `hard_link` fails (e.g. cross-device mount `EXDEV`).
///
/// Returns `Ok(true)` if a hardlink was created, or `Ok(false)` if the streaming copy fallback was used.
pub fn link_or_copy_cas_blob(cas_blob: &Path, target_path: &Path) -> MqResult<bool> {
    if !cas_blob.exists() {
        return Err(MqError::Io(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("CAS blob does not exist: '{}'", cas_blob.display()),
        )));
    }

    if let Some(parent) = target_path.parent() {
        fs::create_dir_all(parent)?;
    }

    if target_path.symlink_metadata().is_ok() {
        fs::remove_file(target_path)?;
    }

    if fs::hard_link(cas_blob, target_path).is_ok() {
        return Ok(true);
    }

    let mut src = File::open(cas_blob)?;
    let mut dst = File::create(target_path)?;
    let mut buf = vec![0u8; STREAM_CHUNK_SIZE];
    loop {
        let n = src.read(&mut buf)?;
        if n == 0 {
            break;
        }
        dst.write_all(&buf[..n])?;
    }
    dst.flush()?;
    Ok(false)
}

/// Streams `path` in `64 KiB` chunks and computes both [`Sha1Digest`] and [`Sha512Digest`]
/// simultaneously without reading the whole file into memory.
///
/// Returns `(sha1, sha512, file_size_bytes)`.
pub fn stream_hash_file_sha1_sha512(path: &Path) -> MqResult<(Sha1Digest, Sha512Digest, u64)> {
    let mut file = File::open(path)?;
    let mut sha1_hasher = Sha1::new();
    let mut sha512_hasher = Sha512::new();
    let mut buf = vec![0u8; STREAM_CHUNK_SIZE];
    let mut total_bytes = 0u64;

    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        sha1_hasher.update(&buf[..n]);
        sha512_hasher.update(&buf[..n]);
        total_bytes = total_bytes.saturating_add(n as u64);
    }

    let sha1 = Sha1Digest::from_bytes(sha1_hasher.finalize().into());
    let sha512 = Sha512Digest::from_bytes(sha512_hasher.finalize().into());
    Ok((sha1, sha512, total_bytes))
}

/// Streams `path` in `64 KiB` chunks and verifies its SHA-1 and SHA-512 digests against
/// `expected_sha1` and `expected_sha512`, returning [`MqError::HashMismatch`] on mismatch.
pub fn verify_file_sha1_and_sha512(
    path: &Path,
    target_label: &str,
    expected_sha1: Option<Sha1Digest>,
    expected_sha512: Option<Sha512Digest>,
) -> MqResult<(Sha1Digest, Sha512Digest, u64)> {
    let (actual_sha1, actual_sha512, size) = stream_hash_file_sha1_sha512(path)?;

    if let Some(exp_sha1) = expected_sha1 {
        if actual_sha1 != exp_sha1 {
            return Err(MqError::HashMismatch {
                target: target_label.to_owned(),
                algo: "SHA-1",
                expected: exp_sha1.to_hex(),
                actual: actual_sha1.to_hex(),
            });
        }
    }

    if let Some(exp_sha512) = expected_sha512 {
        if actual_sha512 != exp_sha512 {
            return Err(MqError::HashMismatch {
                target: target_label.to_owned(),
                algo: "SHA-512",
                expected: exp_sha512.to_hex(),
                actual: actual_sha512.to_hex(),
            });
        }
    }

    Ok((actual_sha1, actual_sha512, size))
}

/// Reads an entry from a [`ZipArchive`] in `64 KiB` chunks up to `max_bytes`.
pub(crate) fn read_zip_entry_bounded<R: Read>(
    mut entry: R,
    entry_name: &str,
    max_bytes: usize,
) -> MqResult<Vec<u8>> {
    let mut out = Vec::new();
    let mut chunk = [0u8; STREAM_CHUNK_SIZE];
    loop {
        let n = entry.read(&mut chunk)?;
        if n == 0 {
            break;
        }
        if out.len().saturating_add(n) > max_bytes {
            return Err(MqError::Archive(format!(
                "archive descriptor '{entry_name}' exceeds maximum size of {max_bytes} bytes"
            )));
        }
        out.extend_from_slice(&chunk[..n]);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use std::io::Write;
    use zip::write::SimpleFileOptions;
    use zip::ZipWriter;

    use super::*;

    #[test]
    fn validate_archive_entry_path_rejects_zip_slip_and_symlinks() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let base = tmp.path();

        // Normal safe paths succeed.
        let safe = validate_archive_entry_path(
            base,
            "overrides/config/sodium-options.json",
            Some(0o100644),
        )
        .expect("safe entry must pass");
        assert_eq!(safe, base.join("overrides/config/sodium-options.json"));

        // Traversal, absolute paths, drive letters, and NUL bytes must fail with ZipSlip.
        let bad_paths = [
            "",
            ".",
            "..",
            "../evil.jar",
            "overrides/../../etc/passwd",
            "overrides\\..\\..\\Windows\\win.ini",
            "/etc/passwd",
            "\\etc\\passwd",
            "C:\\Users\\admin\\evil.jar",
            "mods/evil\0.jar",
        ];

        for bad in bad_paths {
            let err = validate_archive_entry_path(base, bad, Some(0o100644))
                .expect_err("traversal path must be rejected");
            assert!(
                matches!(err, MqError::ZipSlip { .. }),
                "expected ZipSlip for {bad:?}, got {err:?}"
            );
        }

        // Unix symlink mode (0o120777) must fail with ZipSlip even for an innocent filename.
        let symlink_err =
            validate_archive_entry_path(base, "overrides/mods/link.jar", Some(0o120777))
                .expect_err("symlink mode must be rejected");
        assert!(matches!(symlink_err, MqError::ZipSlip { .. }));
    }

    #[test]
    fn extract_zip_prefix_safely_rejects_traversal_and_symlink_entries() {
        let tmp = tempfile::tempdir().expect("tempdir");

        // 1. Archive with `overrides/../../etc/passwd` must return Err(MqError::ZipSlip).
        let traversal_zip = tmp.path().join("traversal.zip");
        {
            let file = File::create(&traversal_zip).expect("create zip");
            let mut zw = ZipWriter::new(file);
            let opts = SimpleFileOptions::default();
            zw.start_file("overrides/../../etc/passwd", opts).unwrap();
            zw.write_all(b"root:x:0:0:root:/root:/bin/bash\n").unwrap();
            zw.finish().unwrap();
        }

        let target_dir = tmp.path().join("instance");
        let err = extract_zip_prefix_safely(&traversal_zip, "overrides", &target_dir)
            .expect_err("zip-slip archive must be rejected");
        assert!(matches!(err, MqError::ZipSlip { .. }));
        assert!(!tmp.path().join("etc/passwd").exists());

        // 2. Archive with a Unix symlink entry must return Err(MqError::ZipSlip).
        let symlink_zip = tmp.path().join("symlink.zip");
        {
            let file = File::create(&symlink_zip).expect("create symlink zip");
            let mut zw = ZipWriter::new(file);
            let symlink_opts = SimpleFileOptions::default();
            zw.add_symlink("overrides/config/escape_link", "/etc/shadow", symlink_opts)
                .unwrap();
            zw.finish().unwrap();
        }

        let sym_err = extract_zip_prefix_safely(&symlink_zip, "overrides", &target_dir)
            .expect_err("symlink archive must be rejected");
        assert!(matches!(sym_err, MqError::ZipSlip { .. }));
    }

    #[test]
    fn extract_zip_prefix_and_cas_hardlink_roundtrip() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let zip_path = tmp.path().join("valid.zip");
        let large_payload: Vec<u8> = (0..(150 * 1024)).map(|i| (i % 251) as u8).collect();

        {
            let file = File::create(&zip_path).expect("create valid zip");
            let mut zw = ZipWriter::new(file);
            let opts = SimpleFileOptions::default().unix_permissions(0o100644);
            zw.start_file("overrides/config/sodium.json", opts).unwrap();
            zw.write_all(b"{\"quality\":\"high\"}").unwrap();
            zw.start_file("overrides/resourcepacks/pack.bin", opts)
                .unwrap();
            zw.write_all(&large_payload).unwrap();
            zw.start_file("client-overrides/options.txt", opts).unwrap();
            zw.write_all(b"fov:1.0\n").unwrap();
            zw.finish().unwrap();
        }

        let instance_dir = tmp.path().join("instance");
        let n_ov = extract_zip_prefix_safely(&zip_path, "overrides/", &instance_dir)
            .expect("extract overrides");
        assert_eq!(n_ov, 2);
        assert_eq!(
            fs::read_to_string(instance_dir.join("config/sodium.json")).unwrap(),
            "{\"quality\":\"high\"}"
        );
        assert_eq!(
            fs::read(instance_dir.join("resourcepacks/pack.bin")).unwrap(),
            large_payload
        );

        let n_client = extract_zip_prefix_safely(&zip_path, "client-overrides", &instance_dir)
            .expect("extract client-overrides");
        assert_eq!(n_client, 1);
        assert_eq!(
            fs::read_to_string(instance_dir.join("options.txt")).unwrap(),
            "fov:1.0\n"
        );

        // Verify CAS hardlink helper and streaming dual digest verification.
        let cas_blob = tmp.path().join("cas_blob.bin");
        fs::write(&cas_blob, &large_payload).unwrap();
        let linked_target = instance_dir.join("mods/linked.jar");
        let hardlinked = link_or_copy_cas_blob(&cas_blob, &linked_target).expect("link cas blob");
        assert!(hardlinked);
        assert_eq!(fs::read(&linked_target).unwrap(), large_payload);

        let (sha1, sha512, size) =
            stream_hash_file_sha1_sha512(&cas_blob).expect("stream hash cas blob");
        assert_eq!(size, large_payload.len() as u64);
        verify_file_sha1_and_sha512(&linked_target, "linked.jar", Some(sha1), Some(sha512))
            .expect("verify linked target");
    }
}
