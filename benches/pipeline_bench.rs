//! Criterion microbenchmarks for multithreaded hash verification and mod scanning.

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use mq_core::{Sha1Digest, Sha512Digest};
use mq_install::{verify_files_parallel, VerifyFileTarget};
use mq_mods::scan_mods_directory;
use mq_sched::{CancelEpoch, ComputePool};
use std::sync::Arc;
use tempfile::tempdir;
use testsupport::SyntheticModSpec;

fn bench_parallel_hash_verification(c: &mut Criterion) {
    let mut group = c.benchmark_group("parallel_hash_verification");
    let rt = tokio::runtime::Runtime::new().unwrap();

    let temp = tempdir().unwrap();
    let num_files = 200;
    let mut targets = Vec::new();

    for i in 0..num_files {
        let path = temp.path().join(format!("file_{i}.bin"));
        let data = format!("synthetic-content-for-benchmark-{i}-{}", "A".repeat(1024)).into_bytes();
        std::fs::write(&path, &data).unwrap();

        let sha1 = Sha1Digest::from_bytes(testsupport::sha1_bytes(&data));
        let sha512 = Sha512Digest::from_bytes(testsupport::sha512_bytes(&data));

        targets.push(VerifyFileTarget {
            path,
            expected_sha1: Some(sha1),
            expected_sha512: Some(sha512),
            expected_size: Some(data.len() as u64),
        });
    }

    for threads in [1, 2, 4] {
        group.bench_with_input(BenchmarkId::new("threads", threads), &threads, |b, &t| {
            let pool = Arc::new(ComputePool::new(t));
            let cancel = Arc::new(CancelEpoch::new());

            b.iter(|| {
                rt.block_on(async {
                    let report =
                        verify_files_parallel(black_box(&targets), &pool, &cancel.snapshot(), None)
                            .await
                            .unwrap();
                    assert_eq!(report.verified_ok, num_files);
                });
            });
        });
    }
    group.finish();
}

fn bench_mod_scanner(c: &mut Criterion) {
    let mut group = c.benchmark_group("mod_jar_scanner");
    let rt = tokio::runtime::Runtime::new().unwrap();

    let temp = tempdir().unwrap();
    let mods_dir = temp.path().join("mods");
    std::fs::create_dir_all(&mods_dir).unwrap();

    for i in 0..50 {
        let jar_path = mods_dir.join(format!("test_mod_{i}.jar"));
        let spec = SyntheticModSpec {
            id: format!("mod_{i}"),
            name: format!("Synthetic Mod {i}"),
            version: "1.0.0".to_string(),
            authors: vec!["Author".to_string()],
            depends: vec![],
            format: testsupport::ModMetadataFormat::Fabric,
            seed_index: i,
            class_payload_size: 512,
        };
        let bytes = testsupport::build_synthetic_mod_jar(&spec);
        std::fs::write(&jar_path, bytes).unwrap();
    }

    for threads in [1, 2, 4] {
        group.bench_with_input(BenchmarkId::new("threads", threads), &threads, |b, &t| {
            let pool = Arc::new(ComputePool::new(t));
            let cancel = Arc::new(CancelEpoch::new());

            b.iter(|| {
                rt.block_on(async {
                    let rep = scan_mods_directory(black_box(&mods_dir), &pool, &cancel.snapshot())
                        .await
                        .unwrap();
                    assert_eq!(rep.mods.len(), 50);
                });
            });
        });
    }
    group.finish();
}

criterion_group!(benches, bench_parallel_hash_verification, bench_mod_scanner);
criterion_main!(benches);
