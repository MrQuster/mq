use std::process::Command;

use mq_core::{AppConfig, MqPaths};
use mq_mods::{
    resolve_dependency_plan, CatalogSource, CurseForgeClient, ModrinthClient, ProjectKind,
    ResolveOptions, SearchQuery,
};
use testsupport::{MockConfig, MockServer};

fn mq_bin() -> &'static str {
    env!("CARGO_BIN_EXE_mq")
}

fn setup_env(mock_url: &str) -> (tempfile::TempDir, MqPaths) {
    let dir = tempfile::tempdir().expect("tempdir");
    let paths = MqPaths::with_root(dir.path().to_path_buf());
    paths.ensure_dirs().expect("ensure_dirs");
    let config = AppConfig {
        mock_url: Some(mock_url.to_string()),
        ..Default::default()
    };
    config.save(&paths).expect("save config");
    (dir, paths)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn e2e_cli_search_modrinth_and_curseforge_key_behavior() {
    let server = MockServer::start(MockConfig::default()).await;
    let (dir, _paths) = setup_env(server.base_url());

    // 1. Modrinth search works without any CurseForge key
    let out = Command::new(mq_bin())
        .args(["search", "sodium", "--source", "modrinth"])
        .env("MQ_DATA_DIR", dir.path())
        .env_remove("MQ_CURSEFORGE_API_KEY")
        .env_remove("MQ_CF_API_KEY")
        .output()
        .expect("run mq search modrinth");
    assert!(
        out.status.success(),
        "mq search modrinth failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("Sodium"),
        "expected Sodium in output: {stdout}"
    );

    // 2. CurseForge search without key fails cleanly with an informative message
    let no_key = Command::new(mq_bin())
        .args(["search", "sodium", "--source", "curseforge"])
        .env("MQ_DATA_DIR", dir.path())
        .env_remove("MQ_CURSEFORGE_API_KEY")
        .env_remove("MQ_CF_API_KEY")
        .output()
        .expect("run mq search curseforge without key");
    assert!(
        !no_key.status.success(),
        "expected curseforge search without key to fail"
    );

    // 3. CurseForge search with key succeeds
    let with_key = Command::new(mq_bin())
        .args(["search", "sodium", "--source", "curseforge"])
        .env("MQ_DATA_DIR", dir.path())
        .env("MQ_CURSEFORGE_API_KEY", "test-cf-key-12345678")
        .output()
        .expect("run mq search curseforge with key");
    assert!(
        with_key.status.success(),
        "mq search curseforge failed: {}",
        String::from_utf8_lossy(&with_key.stderr)
    );
    let cf_stdout = String::from_utf8_lossy(&with_key.stdout);
    assert!(
        cf_stdout.contains("Sodium"),
        "expected Sodium in CF output: {cf_stdout}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn e2e_cli_install_modrinth_mod_and_import_packs() {
    let server = MockServer::start(MockConfig::default()).await;
    let (dir, paths) = setup_env(server.base_url());

    // Create a target Fabric 1.21.1 instance
    let create = Command::new(mq_bin())
        .args([
            "instances",
            "create",
            "fabric-test",
            "--version",
            "1.21.1",
            "--loader",
            "fabric",
        ])
        .env("MQ_DATA_DIR", dir.path())
        .output()
        .expect("create instance");
    assert!(
        create.status.success(),
        "create instance failed: {}",
        String::from_utf8_lossy(&create.stderr)
    );

    // Install modrinth:sodium into fabric-test
    let install = Command::new(mq_bin())
        .args(["install", "modrinth:sodium", "--instance", "fabric-test"])
        .env("MQ_DATA_DIR", dir.path())
        .output()
        .expect("install modrinth:sodium");
    assert!(
        install.status.success(),
        "mq install modrinth:sodium failed: stdout={}, stderr={}",
        String::from_utf8_lossy(&install.stdout),
        String::from_utf8_lossy(&install.stderr)
    );
    let inst_dir = paths.instance_dir("fabric-test");
    assert!(
        inst_dir.join("mods/.mq-lock.json").is_file(),
        "expected mods/.mq-lock.json after install"
    );

    // Import local .mrpack
    let mrpack_path = dir.path().join("demo.mrpack");
    std::fs::write(&mrpack_path, server.build_mrpack_bytes()).expect("write mrpack");
    let imp_mr = Command::new(mq_bin())
        .args([
            "import",
            mrpack_path.to_str().unwrap(),
            "--name",
            "imported-mr",
        ])
        .env("MQ_DATA_DIR", dir.path())
        .output()
        .expect("import mrpack");
    assert!(
        imp_mr.status.success(),
        "mq import mrpack failed: stdout={}, stderr={}",
        String::from_utf8_lossy(&imp_mr.stdout),
        String::from_utf8_lossy(&imp_mr.stderr)
    );
    assert!(
        paths
            .instance_dir("imported-mr")
            .join("instance.toml")
            .is_file()
            || paths
                .instance_dir("imported-mr")
                .join("mods/.mq-lock.json")
                .is_file(),
        "expected imported-mr instance dir to contain instance.toml or mods/.mq-lock.json"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn e2e_mock_429_retry_and_curseforge_blocked_download_and_malformed_json() {
    let server = MockServer::start(MockConfig::default()).await;
    let (_dir, paths) = setup_env(server.base_url());
    let mut config = AppConfig::load_or_default(&paths).expect("load config");
    config.curseforge_api_key = Some(mq_core::SecretString::new("test-cf-key-12345678"));

    // 1. 429 on Modrinth search is retried automatically
    server.inject_429(Some("/modrinth/v2/search"), 0, 1);
    let mr = ModrinthClient::from_config(&config).expect("mr client");
    let res = mr
        .search(&SearchQuery {
            query: "sodium".to_string(),
            project_kind: ProjectKind::Mod,
            ..Default::default()
        })
        .await
        .expect("search should succeed after 429 retry");
    assert!(!res.is_empty());

    // 2. CurseForge blocked download (mod 900002) produces BlockedDownload, never edge.forgecdn.net
    let cf = CurseForgeClient::from_config(&config).expect("cf client");
    let plan = resolve_dependency_plan(
        &cf,
        "900002",
        None::<&str>,
        None::<mq_core::ModLoaderKind>,
        &ResolveOptions::default(),
    )
    .await
    .expect("resolve_dependency_plan for blocked cf mod");
    assert!(
        !plan.blocked_downloads.is_empty(),
        "expected blocked_downloads for mod 900002, got: downloads={:?}, blocked={:?}",
        plan.downloads,
        plan.blocked_downloads
    );
    for b in &plan.blocked_downloads {
        assert!(!b.webpage_url.contains("edge.forgecdn.net"));
        assert!(b.webpage_url.contains("/download/"));
    }
    for d in &plan.downloads {
        assert!(!d.url.contains("edge.forgecdn.net"));
    }

    // 3. Malformed JSON / 500 error returns Err cleanly without panic
    let bad_listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind");
    let bad_addr = bad_listener.local_addr().unwrap();
    let bad_task = tokio::spawn(async move {
        if let Ok((mut sock, _)) = bad_listener.accept().await {
            use tokio::io::{AsyncReadExt, AsyncWriteExt};
            let mut buf = [0u8; 1024];
            let _ = sock.read(&mut buf).await;
            let _ = sock
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 11\r\n\r\n{bad json!!")
                .await;
        }
    });
    let bad_mr = ModrinthClient::with_base_url(format!("http://{bad_addr}/modrinth/v2"))
        .expect("bad client");
    let bad_res = bad_mr
        .search(&SearchQuery {
            query: "sodium".to_string(),
            ..Default::default()
        })
        .await;
    assert!(bad_res.is_err(), "malformed JSON must return Err");
    let _ = bad_task.await;
    let _ = CatalogSource::Modrinth;
}
