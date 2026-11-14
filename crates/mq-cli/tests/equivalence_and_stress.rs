//! End-to-end multi-threading equivalence and chaos cancellation stress tests.

use mq_core::{ModLoaderKind, MqPaths};
use mq_install::{install_game, CasStore, InstallRequest};
use mq_launch::{build_launch_command, LaunchContext};
use mq_net::{NetConfig, NetEngine};
use mq_sched::{BufferPool, CancelEpoch, ComputePool};
use std::sync::Arc;
use tempfile::tempdir;
use testsupport::{MockConfig, MockServer};

#[tokio::test]
async fn test_multithreading_equivalence_single_vs_multi_threaded() {
    let mock_cfg = MockConfig {
        asset_count: 50,
        library_count: 20,
        modpack_mod_count: 20,
        fail_first_n_requests: 0,
    };
    let server = MockServer::start(mock_cfg).await;

    // Run 1: threads = 1
    let temp1 = tempdir().unwrap();
    let paths1 = MqPaths::with_root(temp1.path().join(".mq"));
    paths1.ensure_dirs().unwrap();
    let pool1 = Arc::new(ComputePool::new(1));
    let cancel1 = Arc::new(CancelEpoch::new());
    let net1 = Arc::new(
        NetEngine::with_components(
            NetConfig::default(),
            Arc::new(BufferPool::new()),
            cancel1.clone(),
            None,
        )
        .unwrap(),
    );
    let cas1 = CasStore::new(paths1.clone());
    let inst_dir1 = paths1.instance_dir("inst1");

    let req1 = InstallRequest {
        mc_version: "1.21.1".to_string(),
        loader: ModLoaderKind::Vanilla,
        loader_version: None,
        instance_dir: inst_dir1.clone(),
        mock_base_url: Some(server.base_url().to_string()),
    };
    let sum1 = install_game(
        &req1,
        &paths1,
        &net1,
        &cas1,
        &pool1,
        &cancel1.snapshot(),
        None,
    )
    .await
    .unwrap();

    // Run 2: threads = 4
    let temp2 = tempdir().unwrap();
    let paths2 = MqPaths::with_root(temp2.path().join(".mq"));
    paths2.ensure_dirs().unwrap();
    let pool2 = Arc::new(ComputePool::new(4));
    let cancel2 = Arc::new(CancelEpoch::new());
    let net2 = Arc::new(
        NetEngine::with_components(
            NetConfig::default(),
            Arc::new(BufferPool::new()),
            cancel2.clone(),
            None,
        )
        .unwrap(),
    );
    let cas2 = CasStore::new(paths2.clone());
    let inst_dir2 = paths2.instance_dir("inst2");

    let req2 = InstallRequest {
        mc_version: "1.21.1".to_string(),
        loader: ModLoaderKind::Vanilla,
        loader_version: None,
        instance_dir: inst_dir2.clone(),
        mock_base_url: Some(server.base_url().to_string()),
    };
    let sum2 = install_game(
        &req2,
        &paths2,
        &net2,
        &cas2,
        &pool2,
        &cancel2.snapshot(),
        None,
    )
    .await
    .unwrap();

    // Verification 1: Summary counts match identically
    assert_eq!(sum1.libraries_installed, sum2.libraries_installed);
    assert_eq!(sum1.assets_installed, sum2.assets_installed);
    assert_eq!(sum1.bytes_downloaded, sum2.bytes_downloaded);

    // Verification 2: CAS blobs match identically
    let blobs1: std::collections::BTreeSet<_> = std::fs::read_dir(paths1.cas_dir())
        .unwrap()
        .filter_map(|e| e.ok().map(|e| e.file_name()))
        .collect();
    let blobs2: std::collections::BTreeSet<_> = std::fs::read_dir(paths2.cas_dir())
        .unwrap()
        .filter_map(|e| e.ok().map(|e| e.file_name()))
        .collect();
    assert_eq!(blobs1, blobs2);

    // Verification 3: Launch commands are 100% byte-for-byte identical
    let inst1 =
        mq_instances::InstanceManifest::new("inst1", "1.21.1", ModLoaderKind::Vanilla, None);
    let inst2 =
        mq_instances::InstanceManifest::new("inst2", "1.21.1", ModLoaderKind::Vanilla, None);
    let acc = mq_auth::create_offline_account("Player").unwrap();

    let ctx1 = LaunchContext {
        paths: paths1,
        instance: inst1,
        version: (*sum1.resolved_version).clone(),
        account: acc.clone(),
        java_bin: std::path::PathBuf::from("/usr/bin/java"),
        authlib_injector_jar: None,
        extra_secrets_to_redact: Vec::new(),
    };
    let ctx2 = LaunchContext {
        paths: paths2,
        instance: inst2,
        version: (*sum2.resolved_version).clone(),
        account: acc,
        java_bin: std::path::PathBuf::from("/usr/bin/java"),
        authlib_injector_jar: None,
        extra_secrets_to_redact: Vec::new(),
    };

    let cmd1 = build_launch_command(&ctx1).unwrap();
    let cmd2 = build_launch_command(&ctx2).unwrap();

    let normalize = |args: &[String], root: &std::path::Path, inst: &str| -> Vec<String> {
        let canon = root
            .canonicalize()
            .ok()
            .and_then(|p| p.to_str().map(str::to_owned));
        let raw = root.to_str().unwrap();
        args.iter()
            .map(|s| {
                let s = if let Some(ref c) = canon {
                    s.replace(c.as_str(), "<ROOT>")
                } else {
                    s.clone()
                };
                s.replace(raw, "<ROOT>").replace(inst, "<INST>")
            })
            .collect()
    };

    let norm1 = normalize(&cmd1.args, temp1.path(), "inst1");
    let norm2 = normalize(&cmd2.args, temp2.path(), "inst2");

    assert_eq!(cmd1.program, cmd2.program);
    assert_eq!(norm1, norm2);
    assert_eq!(cmd1.env, cmd2.env);
}

fn check_no_part_files(dir: &std::path::Path) {
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let p = entry.path();
            if p.is_dir() {
                check_no_part_files(&p);
            } else {
                let name = p.file_name().unwrap().to_string_lossy();
                assert!(
                    !name.ends_with(".part"),
                    "Found dangling part file: {:?}",
                    p
                );
            }
        }
    }
}

#[tokio::test]
async fn test_chaos_cancellation_cleans_up_part_files_and_avoids_deadlock() {
    let mock_cfg = MockConfig {
        asset_count: 100,
        library_count: 50,
        modpack_mod_count: 50,
        fail_first_n_requests: 0,
    };
    let server = MockServer::start(mock_cfg).await;

    let temp = tempdir().unwrap();
    let paths = MqPaths::with_root(temp.path().join(".mq"));
    paths.ensure_dirs().unwrap();
    let pool = Arc::new(ComputePool::new(4));
    let cancel = Arc::new(CancelEpoch::new());
    let net = Arc::new(
        NetEngine::with_components(
            NetConfig::default(),
            Arc::new(BufferPool::new()),
            cancel.clone(),
            None,
        )
        .unwrap(),
    );
    let cas = CasStore::new(paths.clone());
    let inst_dir = paths.instance_dir("chaos_inst");

    let req = InstallRequest {
        mc_version: "1.21.1".to_string(),
        loader: ModLoaderKind::Vanilla,
        loader_version: None,
        instance_dir: inst_dir.clone(),
        mock_base_url: Some(server.base_url().to_string()),
    };

    // Trigger cancellation mid-flight
    let cancel_clone = cancel.clone();
    tokio::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(30)).await;
        cancel_clone.cancel_and_advance();
    });

    let res = install_game(&req, &paths, &net, &cas, &pool, &cancel.snapshot(), None).await;
    assert!(res.is_err(), "Installation must abort on cancellation");

    // Check no dangling .part files in instances or cache
    check_no_part_files(temp.path());

    // Verify subsequent attempt with a fresh epoch succeeds cleanly
    let fresh_cancel = Arc::new(CancelEpoch::new());
    let net_fresh = Arc::new(
        NetEngine::with_components(
            NetConfig::default(),
            Arc::new(BufferPool::new()),
            fresh_cancel.clone(),
            None,
        )
        .unwrap(),
    );

    let retry_res = install_game(
        &req,
        &paths,
        &net_fresh,
        &cas,
        &pool,
        &fresh_cancel.snapshot(),
        None,
    )
    .await;
    assert!(retry_res.is_ok(), "Retry with new epoch must succeed");
}
