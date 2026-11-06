#![deny(clippy::await_holding_lock)]
//! Deep multi-threaded instance file verification (`mq verify`).
//!
//! Uses `rayon` parallel iteration over all targets in 64 KiB streaming chunks,
//! saturating all available CPU cores without whole-file memory buffering.

use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Instant;

use mq_core::{Event, EventBus, MqPaths, MqResult, PhaseProgress, Sha1Digest, Sha512Digest};
use mq_meta::{AssetIndex, ResolvedVersion};
use mq_sched::{ComputePool, EpochGuard};
use rayon::prelude::*;
use sha1::{Digest as _, Sha1};
use sha2::Sha512;

/// A single target file to be deeply verified.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifyFileTarget {
    pub path: PathBuf,
    pub expected_sha1: Option<Sha1Digest>,
    pub expected_sha512: Option<Sha512Digest>,
    pub expected_size: Option<u64>,
}

/// Verification report detailing results of the deep verify phase.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifyReport {
    pub total_files: usize,
    pub verified_ok: usize,
    pub corrupted_files: Vec<PathBuf>,
    pub missing_files: Vec<PathBuf>,
    pub total_bytes_hashed: u64,
    pub elapsed_ms: u64,
}

impl VerifyReport {
    pub fn is_clean(&self) -> bool {
        self.corrupted_files.is_empty() && self.missing_files.is_empty()
    }
}

/// Collects all instance verification targets in deterministic order from `ResolvedVersion` and `AssetIndex`.
pub fn collect_instance_verify_targets(
    paths: &MqPaths,
    instance_dir: &Path,
    version: &ResolvedVersion,
    asset_index: &AssetIndex,
) -> Vec<VerifyFileTarget> {
    let mut targets = Vec::with_capacity(version.libraries.len() + asset_index.objects.len() + 2);

    // Client JAR
    let client_jar_path = instance_dir.join("client.jar");
    targets.push(VerifyFileTarget {
        path: client_jar_path,
        expected_sha1: version.client_jar.sha1,
        expected_sha512: None,
        expected_size: if version.client_jar.size > 0 {
            Some(version.client_jar.size)
        } else {
            None
        },
    });

    // Libraries
    let lib_base = paths.libraries_dir();
    for lib in &version.libraries {
        if let Some(artifact) = &lib.artifact {
            let p = lib_base.join(&artifact.path);
            targets.push(VerifyFileTarget {
                path: p,
                expected_sha1: artifact.sha1,
                expected_sha512: None,
                expected_size: if artifact.size > 0 {
                    Some(artifact.size)
                } else {
                    None
                },
            });
        }
        if let Some(native) = &lib.native_linux {
            let p = lib_base.join(&native.path);
            targets.push(VerifyFileTarget {
                path: p,
                expected_sha1: native.sha1,
                expected_sha512: None,
                expected_size: if native.size > 0 {
                    Some(native.size)
                } else {
                    None
                },
            });
        }
    }

    // Assets
    let assets_base = paths.assets_dir().join("objects");
    for obj in &asset_index.objects {
        let p = assets_base.join(obj.relative_object_path());
        targets.push(VerifyFileTarget {
            path: p,
            expected_sha1: Some(obj.hash),
            expected_sha512: None,
            expected_size: if obj.size > 0 { Some(obj.size) } else { None },
        });
    }

    targets
}

/// Verifies targets in parallel on the CPU compute pool using `rayon`.
pub async fn verify_files_parallel(
    targets: &[VerifyFileTarget],
    compute_pool: &ComputePool,
    epoch_guard: &EpochGuard,
    event_bus: Option<&EventBus>,
) -> MqResult<VerifyReport> {
    let total_files = targets.len();
    if total_files == 0 {
        return Ok(VerifyReport {
            total_files: 0,
            verified_ok: 0,
            corrupted_files: Vec::new(),
            missing_files: Vec::new(),
            total_bytes_hashed: 0,
            elapsed_ms: 0,
        });
    }

    let start = Instant::now();
    let targets = targets.to_vec();
    let verified_counter = Arc::new(AtomicUsize::new(0));
    let bytes_counter = Arc::new(AtomicU64::new(0));

    let vc = verified_counter.clone();
    let bc = bytes_counter.clone();
    let bus_opt = event_bus.cloned();

    compute_pool
        .spawn_cpu_cancellable(epoch_guard, move || {
            let results: Vec<(PathBuf, Result<(), bool>)> = targets
                .par_iter()
                .map(|target| {
                    if !target.path.exists() {
                        return (target.path.clone(), Err(false)); // false = missing
                    }

                    let file = match File::open(&target.path) {
                        Ok(f) => f,
                        Err(_) => return (target.path.clone(), Err(false)),
                    };

                    let mut reader = std::io::BufReader::with_capacity(64 * 1024, file);
                    let mut sha1_hasher = Sha1::new();
                    let mut sha512_hasher = if target.expected_sha512.is_some() {
                        Some(Sha512::new())
                    } else {
                        None
                    };

                    let mut buf = [0u8; 64 * 1024];
                    let mut bytes_read = 0u64;

                    loop {
                        let n = match reader.read(&mut buf) {
                            Ok(0) => break,
                            Ok(n) => n,
                            Err(_) => return (target.path.clone(), Err(true)), // true = corrupted
                        };
                        bytes_read += n as u64;
                        sha1_hasher.update(&buf[..n]);
                        if let Some(h) = &mut sha512_hasher {
                            h.update(&buf[..n]);
                        }
                    }

                    if let Some(exp_size) = target.expected_size {
                        if bytes_read != exp_size {
                            return (target.path.clone(), Err(true));
                        }
                    }

                    if let Some(exp_sha1) = target.expected_sha1 {
                        let actual: [u8; 20] = sha1_hasher.finalize().into();
                        if actual != *exp_sha1.as_bytes() {
                            return (target.path.clone(), Err(true));
                        }
                    }

                    if let Some(exp_sha512) = target.expected_sha512 {
                        if let Some(h) = sha512_hasher {
                            let actual: [u8; 64] = h.finalize().into();
                            if actual != *exp_sha512.as_bytes() {
                                return (target.path.clone(), Err(true));
                            }
                        }
                    }

                    vc.fetch_add(1, Ordering::Relaxed);
                    bc.fetch_add(bytes_read, Ordering::Relaxed);

                    (target.path.clone(), Ok(()))
                })
                .collect();

            let mut missing = Vec::new();
            let mut corrupted = Vec::new();
            let mut ok_count = 0usize;

            for (path, res) in results {
                match res {
                    Ok(()) => ok_count += 1,
                    Err(false) => missing.push(path),
                    Err(true) => corrupted.push(path),
                }
            }

            let elapsed_ms = start.elapsed().as_millis() as u64;
            let total_bytes = bc.load(Ordering::Relaxed);

            if let Some(bus) = bus_opt {
                bus.publish(Event::Progress(PhaseProgress {
                    phase: "verify".to_string(),
                    completed_items: ok_count as u64,
                    total_items: total_files as u64,
                    bytes_done: total_bytes,
                    bytes_total: total_bytes,
                    files_per_sec: if elapsed_ms > 0 {
                        (ok_count as f64) / (elapsed_ms as f64 / 1000.0)
                    } else {
                        0.0
                    },
                    mib_per_sec: if elapsed_ms > 0 {
                        (total_bytes as f64 / 1024.0 / 1024.0) / (elapsed_ms as f64 / 1000.0)
                    } else {
                        0.0
                    },
                    active_tasks: 0,
                }));
            }

            Ok(VerifyReport {
                total_files,
                verified_ok: ok_count,
                corrupted_files: corrupted,
                missing_files: missing,
                total_bytes_hashed: total_bytes,
                elapsed_ms,
            })
        })
        .await
        .map_err(|e| mq_core::MqError::Internal(format!("verify task error: {e}")))?
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[tokio::test]
    async fn verifies_files_detecting_ok_corrupted_and_missing() {
        let dir = tempdir().unwrap();
        let f_ok = dir.path().join("ok.bin");
        let f_corrupt = dir.path().join("corrupt.bin");
        let f_missing = dir.path().join("missing.bin");

        fs::write(&f_ok, b"hello world").unwrap();
        fs::write(&f_corrupt, b"tampered").unwrap();

        let sha1_ok = Sha1Digest::from_hex("2aae6c35c94fcfb415dbe95f408b9ce91ee846ed").unwrap();
        let sha1_expected =
            Sha1Digest::from_hex("0000000000000000000000000000000000000000").unwrap();

        let targets = vec![
            VerifyFileTarget {
                path: f_ok,
                expected_sha1: Some(sha1_ok),
                expected_sha512: None,
                expected_size: Some(11),
            },
            VerifyFileTarget {
                path: f_corrupt,
                expected_sha1: Some(sha1_expected),
                expected_sha512: None,
                expected_size: Some(8),
            },
            VerifyFileTarget {
                path: f_missing,
                expected_sha1: Some(sha1_ok),
                expected_sha512: None,
                expected_size: None,
            },
        ];

        let pool = ComputePool::new(2);
        let cancel_epoch = mq_sched::CancelEpoch::new();
        let guard = cancel_epoch.snapshot();

        let rep = verify_files_parallel(&targets, &pool, &guard, None)
            .await
            .unwrap();
        assert_eq!(rep.total_files, 3);
        assert_eq!(rep.verified_ok, 1);
        assert_eq!(rep.corrupted_files.len(), 1);
        assert_eq!(rep.missing_files.len(), 1);
        assert!(!rep.is_clean());
    }
}
