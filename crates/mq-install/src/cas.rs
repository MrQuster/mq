#![deny(clippy::await_holding_lock)]
//! Content-Addressable Storage (`<data_dir>/cas/sha1/<2-hex>/<40-hex>`) with O(1) `DashMap`
//! resident blob lookup, per-hash lock coordination, hardlink materialization, and streaming
//! chunked copy fallback across filesystem boundaries (`EXDEV`).

use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use dashmap::DashMap;
use mq_core::{MqError, MqPaths, MqResult, Sha1Digest};
use sha1::{Digest as _, Sha1};

/// Size of the streaming I/O buffer (64 KiB) used for CAS verification and cross-device copy fallback.
pub const CAS_STREAM_CHUNK_SIZE: usize = 64 * 1024;

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(1);

/// Materialization method used when linking a verified CAS blob into an instance or shared target path.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LinkMethod {
    /// Target file was linked via `std::fs::hard_link` (zero additional disk blocks).
    HardLink,
    /// `hard_link` failed (e.g. `EXDEV` cross-filesystem mount), so the blob was streamed via
    /// a temporary file and atomically renamed into place.
    CopiedFallback,
}

/// Content-Addressable Storage manager rooted at `<data_dir>/cas/sha1/<2-hex>/<40-hex>`.
///
/// Cloning [`CasStore`] is O(1) and shares the underlying `DashMap` of verified resident blobs
/// and per-digest coordination locks across all async and CPU worker tasks (Lesson E6).
#[derive(Debug, Clone)]
pub struct CasStore {
    paths: MqPaths,
    verified_blobs: Arc<DashMap<Sha1Digest, ()>>,
    hash_locks: Arc<DashMap<Sha1Digest, Arc<Mutex<()>>>>,
}

impl CasStore {
    /// Creates a new [`CasStore`] backed by the given [`MqPaths`].
    #[must_use]
    pub fn new(paths: MqPaths) -> Self {
        Self {
            paths,
            verified_blobs: Arc::new(DashMap::new()),
            hash_locks: Arc::new(DashMap::new()),
        }
    }

    /// Borrows the underlying [`MqPaths`].
    #[must_use]
    pub fn paths(&self) -> &MqPaths {
        &self.paths
    }

    /// Returns the number of verified SHA-1 blobs currently indexed in the O(1) resident map.
    #[must_use]
    pub fn verified_count(&self) -> usize {
        self.verified_blobs.len()
    }

    /// Explicitly marks `sha1` as verified and resident in CAS.
    pub fn mark_verified(&self, sha1: Sha1Digest) {
        self.verified_blobs.insert(sha1, ());
    }

    /// Returns the canonical sharded filesystem path for `sha1`
    /// (`<data_dir>/cas/sha1/<2-hex>/<40-hex>`).
    #[must_use]
    pub fn blob_path(&self, sha1: &Sha1Digest) -> PathBuf {
        self.paths.cas_sha1_path(&sha1.to_hex())
    }

    /// Checks whether a verified blob for `sha1` exists in CAS.
    ///
    /// Fast path is an O(1) `DashMap` lookup (Lesson E6). If the digest is not yet in the
    /// in-memory map (e.g. cold process start) but the blob file already exists on disk,
    /// its SHA-1 is verified via 64 KiB streaming reads and cached in `verified_blobs`.
    #[must_use]
    pub fn has_blob(&self, sha1: &Sha1Digest) -> bool {
        let path = self.blob_path(sha1);
        if self.verified_blobs.contains_key(sha1) {
            if path.is_file() {
                return true;
            }
            self.verified_blobs.remove(sha1);
            return false;
        }

        if !path.is_file() {
            return false;
        }

        match compute_file_sha1_streaming(&path) {
            Ok(actual) if actual == *sha1 => {
                self.verified_blobs.insert(*sha1, ());
                true
            }
            _ => {
                let _ = fs::remove_file(&path);
                false
            }
        }
    }

    /// Acquires the per-digest coordination mutex for `sha1` so concurrent tasks never race
    /// when populating the same CAS blob.
    fn acquire_hash_lock(&self, sha1: Sha1Digest) -> Arc<Mutex<()>> {
        self.hash_locks
            .entry(sha1)
            .or_insert_with(|| Arc::new(Mutex::new(())))
            .value()
            .clone()
    }

    /// Stores an already-verified file `src_path` into CAS under `sha1` and records it in the
    /// O(1) `DashMap` index, returning the canonical CAS blob path.
    ///
    /// Coordinates concurrent writers per `sha1` digest. Attempts `std::fs::hard_link` first
    /// to avoid data copying, falling back to an atomic 64 KiB chunked copy + rename if
    /// `src_path` is on a different filesystem.
    pub fn store_verified_file(&self, src_path: &Path, sha1: Sha1Digest) -> MqResult<PathBuf> {
        let cas_path = self.blob_path(&sha1);

        // Fast path: already verified and resident on disk.
        if self.verified_blobs.contains_key(&sha1) && cas_path.is_file() {
            return Ok(cas_path);
        }

        let lock = self.acquire_hash_lock(sha1);
        let _guard = lock
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);

        // Re-check under per-hash lock.
        if self.verified_blobs.contains_key(&sha1) && cas_path.is_file() {
            return Ok(cas_path);
        }

        if !src_path.is_file() {
            return Err(MqError::Io(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!(
                    "cannot store missing source file '{}' in CAS",
                    src_path.display()
                ),
            )));
        }

        if src_path == cas_path {
            self.verified_blobs.insert(sha1, ());
            return Ok(cas_path);
        }

        if let Some(parent) = cas_path.parent() {
            fs::create_dir_all(parent)?;
        }

        if !cas_path.exists() {
            if fs::hard_link(src_path, &cas_path).is_ok() {
                self.verified_blobs.insert(sha1, ());
                return Ok(cas_path);
            }
        } else if cas_path.is_file() {
            if let Ok(existing_sha1) = compute_file_sha1_streaming(&cas_path) {
                if existing_sha1 == sha1 {
                    self.verified_blobs.insert(sha1, ());
                    return Ok(cas_path);
                }
            }
            let _ = fs::remove_file(&cas_path);
        }

        copy_chunked_atomic(src_path, &cas_path)?;
        self.verified_blobs.insert(sha1, ());
        Ok(cas_path)
    }

    /// Materializes the CAS blob for `sha1` at `target_path`.
    ///
    /// - Ensures the parent directory of `target_path` exists.
    /// - Removes any existing stale file at `target_path`.
    /// - Attempts `std::fs::hard_link(&cas_blob, target_path)` first (`LinkMethod::HardLink`).
    /// - If `hard_link` fails (e.g. `EXDEV` cross-device mount), automatically falls back to
    ///   streaming 64 KiB chunked copy via a temporary file + atomic rename (`LinkMethod::CopiedFallback`).
    pub fn link_to_target(&self, sha1: &Sha1Digest, target_path: &Path) -> MqResult<LinkMethod> {
        let cas_blob = self.blob_path(sha1);
        if !cas_blob.is_file() {
            self.verified_blobs.remove(sha1);
            return Err(MqError::Io(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!(
                    "CAS blob '{}' for digest {sha1} does not exist",
                    cas_blob.display()
                ),
            )));
        }

        self.verified_blobs.insert(*sha1, ());

        if target_path == cas_blob {
            return Ok(LinkMethod::HardLink);
        }

        if let Some(parent) = target_path.parent() {
            if !parent.as_os_str().is_empty() {
                fs::create_dir_all(parent)?;
            }
        }

        if target_path.exists() || target_path.symlink_metadata().is_ok() {
            fs::remove_file(target_path)?;
        }

        match fs::hard_link(&cas_blob, target_path) {
            Ok(()) => Ok(LinkMethod::HardLink),
            Err(err) => {
                tracing::debug!(
                    cas_blob = %cas_blob.display(),
                    target = %target_path.display(),
                    error = %err,
                    "hard_link failed; falling back to streaming chunked copy"
                );
                copy_chunked_atomic(&cas_blob, target_path)?;
                Ok(LinkMethod::CopiedFallback)
            }
        }
    }
}

/// Streams `src` to a temporary sibling of `target_path` in 64 KiB chunks and atomically
/// renames it into `target_path`.
pub fn copy_chunked_atomic(src: &Path, target_path: &Path) -> MqResult<u64> {
    let parent = target_path.parent().unwrap_or_else(|| Path::new("."));
    if !parent.as_os_str().is_empty() {
        fs::create_dir_all(parent)?;
    }

    let file_name = target_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("blob");
    let seq = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    let pid = std::process::id();
    let tmp_path = parent.join(format!(".{file_name}.{pid}.{seq}.cas.tmp"));

    let mut reader = File::open(src)?;
    let write_res = (|| -> std::io::Result<u64> {
        let mut writer = OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .open(&tmp_path)?;

        let mut buf = [0u8; CAS_STREAM_CHUNK_SIZE];
        let mut total_written = 0u64;
        loop {
            let n = reader.read(&mut buf)?;
            if n == 0 {
                break;
            }
            writer.write_all(&buf[..n])?;
            total_written += n as u64;
        }
        writer.flush()?;
        drop(writer);

        if target_path.exists() || target_path.symlink_metadata().is_ok() {
            let _ = fs::remove_file(target_path);
        }
        fs::rename(&tmp_path, target_path)?;
        Ok(total_written)
    })();

    if let Err(err) = write_res {
        let _ = fs::remove_file(&tmp_path);
        return Err(MqError::Io(err));
    }

    write_res.map_err(MqError::Io)
}

/// Computes the SHA-1 digest of `path` using a 64 KiB stack buffer without allocating
/// the full file in memory.
pub fn compute_file_sha1_streaming(path: &Path) -> MqResult<Sha1Digest> {
    let mut file = File::open(path)?;
    let mut hasher = Sha1::new();
    let mut buf = [0u8; CAS_STREAM_CHUNK_SIZE];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(Sha1Digest::from_bytes(hasher.finalize().into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sha1_of(data: &[u8]) -> Sha1Digest {
        let mut h = Sha1::new();
        h.update(data);
        Sha1Digest::from_bytes(h.finalize().into())
    }

    #[test]
    fn cas_store_deduplicates_o1_and_creates_hardlinks_sharing_inode() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let paths = MqPaths::with_root(tmp.path());
        paths.ensure_dirs().expect("ensure dirs");

        let cas = CasStore::new(paths.clone());
        let payload = b"lwjgl-native-and-shared-library-payload-bytes".repeat(512);
        let digest = sha1_of(&payload);

        assert!(!cas.has_blob(&digest));
        assert_eq!(cas.verified_count(), 0);

        let staging = paths.temp_dir().join("staged_artifact.jar");
        fs::write(&staging, &payload).expect("write staging file");

        let cas_blob_path = cas
            .store_verified_file(&staging, digest)
            .expect("store verified file into CAS");
        assert_eq!(cas_blob_path, paths.cas_sha1_path(&digest.to_hex()));
        assert!(cas_blob_path.is_file());
        assert!(cas.has_blob(&digest));
        assert_eq!(cas.verified_count(), 1);

        // Link the same CAS blob into two distinct instance targets and verify identical inodes.
        let target_a = paths
            .instance_dir("inst-a")
            .join("mods")
            .join("shared-mod.jar");
        let target_b = paths
            .instance_dir("inst-b")
            .join("mods")
            .join("shared-mod.jar");

        // Pre-populate stale content at target_a to verify stale file replacement.
        fs::create_dir_all(target_a.parent().unwrap()).expect("create parent");
        fs::write(&target_a, b"stale-corrupted-bytes").expect("write stale file");

        let method_a = cas
            .link_to_target(&digest, &target_a)
            .expect("link target_a");
        let method_b = cas
            .link_to_target(&digest, &target_b)
            .expect("link target_b");

        assert_eq!(method_a, LinkMethod::HardLink);
        assert_eq!(method_b, LinkMethod::HardLink);
        assert_eq!(fs::read(&target_a).expect("read target_a"), payload);
        assert_eq!(fs::read(&target_b).expect("read target_b"), payload);

        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            let cas_ino = fs::metadata(&cas_blob_path).expect("stat cas blob").ino();
            let ino_a = fs::metadata(&target_a).expect("stat target_a").ino();
            let ino_b = fs::metadata(&target_b).expect("stat target_b").ino();
            assert_eq!(cas_ino, ino_a, "target_a must share inode with CAS blob");
            assert_eq!(cas_ino, ino_b, "target_b must share inode with CAS blob");
        }

        // Cold-start CasStore discovers and verifies the existing disk blob on demand.
        let cold_cas = CasStore::new(paths);
        assert_eq!(cold_cas.verified_count(), 0);
        assert!(cold_cas.has_blob(&digest));
        assert_eq!(cold_cas.verified_count(), 1);
    }

    #[test]
    fn copy_chunked_atomic_streams_multi_chunk_files_cleanly() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let src = tmp.path().join("source.bin");
        let dst = tmp.path().join("nested/dir/dest.bin");

        let data: Vec<u8> = (0..(200 * 1024))
            .map(|i| ((i * 17 + 5) & 0xff) as u8)
            .collect();
        fs::write(&src, &data).expect("write src");

        let written = copy_chunked_atomic(&src, &dst).expect("copy_chunked_atomic");
        assert_eq!(written, data.len() as u64);
        assert_eq!(fs::read(&dst).expect("read dst"), data);
    }
}
