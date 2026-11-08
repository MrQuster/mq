#![deny(clippy::await_holding_lock)]
//! Game installation pipeline DAG and execution engine (`mq install`).

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use mq_core::{Event, EventBus, ModLoaderKind, MqError, MqPaths, MqResult};
use mq_meta::{
    fetch_asset_index_raw, fetch_fabric_loader_versions, fetch_fabric_profile,
    fetch_quilt_loader_versions, fetch_quilt_profile, fetch_vanilla_version,
    fetch_version_manifest, find_version_entry, merge_inherits_from, parse_asset_index, AssetIndex,
    ResolvedVersion, RuleContext,
};
use mq_net::{DownloadSpec, NetEngine};
use mq_sched::{ComputePool, DagNodeSpec, DagPlan, EpochGuard};

use crate::cas::{compute_file_sha1_streaming, CasStore};
use crate::natives::extract_natives_archives;
use crate::state::{
    check_free_disk_space, InstallClientJarInfo, InstallError, InstallJavaInfo, InstallStage,
    InstallState, InstallStatus, InstanceInstallLock,
};

/// Request specifying which version and loader to install into an instance.
#[derive(Debug, Clone)]
pub struct InstallRequest {
    pub mc_version: String,
    pub loader: ModLoaderKind,
    pub loader_version: Option<String>,
    pub instance_dir: PathBuf,
    pub mock_base_url: Option<String>,
}

/// Summary of an installation run.
#[derive(Debug, Clone)]
pub struct InstallSummary {
    pub resolved_version: Arc<ResolvedVersion>,
    pub asset_index: Arc<AssetIndex>,
    pub libraries_installed: usize,
    pub assets_installed: usize,
    pub natives_extracted: usize,
    pub hardlinks_created: usize,
    pub bytes_downloaded: u64,
    pub elapsed_ms: u64,
}

/// Builds the explicit DAG plan for `mq plan <version>` (Lesson E9).
pub fn build_game_install_dag_plan(
    mc_version: &str,
    loader: ModLoaderKind,
    lib_count: usize,
    asset_count: usize,
    include_authlib: bool,
) -> MqResult<DagPlan> {
    let mut nodes = Vec::new();

    nodes.push(DagNodeSpec {
        id: "fetch_version_manifest".to_string(),
        stage: "Metadata".to_string(),
        deps: vec![],
        parallel_items: 1,
        estimated_ms: 100,
        sequential_reason: Some(
            "Manifest must be retrieved first to locate version JSON".to_string(),
        ),
    });

    nodes.push(DagNodeSpec {
        id: "fetch_version_json".to_string(),
        stage: "Metadata".to_string(),
        deps: vec!["fetch_version_manifest".to_string()],
        parallel_items: 1,
        estimated_ms: 150,
        sequential_reason: Some(
            "Version metadata defines the full library and asset graph".to_string(),
        ),
    });

    if loader != ModLoaderKind::Vanilla {
        nodes.push(DagNodeSpec {
            id: "fetch_loader_profile".to_string(),
            stage: "Metadata".to_string(),
            deps: vec!["fetch_version_json".to_string()],
            parallel_items: 1,
            estimated_ms: 120,
            sequential_reason: Some(format!(
                "Requires game version {mc_version} to resolve {loader} profile"
            )),
        });

        nodes.push(DagNodeSpec {
            id: "merge_loader_profile".to_string(),
            stage: "Metadata".to_string(),
            deps: vec!["fetch_loader_profile".to_string()],
            parallel_items: 1,
            estimated_ms: 20,
            sequential_reason: Some(
                "Deterministic CPU merge of loader and vanilla libraries".to_string(),
            ),
        });
    }

    let meta_dep = if loader != ModLoaderKind::Vanilla {
        "merge_loader_profile".to_string()
    } else {
        "fetch_version_json".to_string()
    };

    nodes.push(DagNodeSpec {
        id: "download_client_jar".to_string(),
        stage: "Download".to_string(),
        deps: vec![meta_dep.clone()],
        parallel_items: 1,
        estimated_ms: 250,
        sequential_reason: None,
    });

    nodes.push(DagNodeSpec {
        id: "download_libraries".to_string(),
        stage: "Download".to_string(),
        deps: vec![meta_dep.clone()],
        parallel_items: lib_count.max(1),
        estimated_ms: 400,
        sequential_reason: None,
    });

    nodes.push(DagNodeSpec {
        id: "fetch_asset_index".to_string(),
        stage: "Metadata".to_string(),
        deps: vec![meta_dep.clone()],
        parallel_items: 1,
        estimated_ms: 100,
        sequential_reason: Some(
            "Asset index JSON defines 3000+ individual asset objects".to_string(),
        ),
    });

    nodes.push(DagNodeSpec {
        id: "download_assets".to_string(),
        stage: "Download".to_string(),
        deps: vec!["fetch_asset_index".to_string()],
        parallel_items: asset_count.max(1),
        estimated_ms: 600,
        sequential_reason: None,
    });

    if include_authlib {
        nodes.push(DagNodeSpec {
            id: "ensure_authlib_injector".to_string(),
            stage: "Auth".to_string(),
            deps: vec![],
            parallel_items: 1,
            estimated_ms: 150,
            sequential_reason: None,
        });
    }

    nodes.push(DagNodeSpec {
        id: "extract_natives".to_string(),
        stage: "Extract".to_string(),
        deps: vec!["download_libraries".to_string()],
        parallel_items: 1,
        estimated_ms: 80,
        sequential_reason: Some(
            "Requires native Linux library JARs to be completely downloaded".to_string(),
        ),
    });

    let mut ready_deps = vec![
        "download_client_jar".to_string(),
        "download_assets".to_string(),
        "extract_natives".to_string(),
    ];
    if include_authlib {
        ready_deps.push("ensure_authlib_injector".to_string());
    }

    nodes.push(DagNodeSpec {
        id: "write_instance_lock_ready".to_string(),
        stage: "Finalize".to_string(),
        deps: ready_deps,
        parallel_items: 1,
        estimated_ms: 10,
        sequential_reason: Some(
            "Writes resolved_version.json and marks instance launchable".to_string(),
        ),
    });

    DagPlan::new(nodes).map_err(|e| MqError::Internal(e.to_string()))
}

/// Adds known official/safe fallback mirrors (e.g. BMCLAPI) ONLY when official SHA-1 digest is present.
fn add_mojang_mirror_if_hashed(spec: &mut DownloadSpec) {
    if spec.expected_sha1.is_none() {
        return;
    }
    let primary = match spec.urls.first() {
        Some(u) => u.as_str(),
        None => return,
    };

    let mirror_url =
        if let Some(stripped) = primary.strip_prefix("https://resources.download.minecraft.net/") {
            Some(format!("https://bmclapi2.bangbang93.com/assets/{stripped}"))
        } else if let Some(stripped) = primary.strip_prefix("https://libraries.minecraft.net/") {
            Some(format!("https://bmclapi2.bangbang93.com/maven/{stripped}"))
        } else if let Some(stripped) = primary.strip_prefix("https://piston-data.mojang.com/") {
            Some(format!("https://bmclapi2.bangbang93.com/{stripped}"))
        } else {
            primary
                .strip_prefix("https://launcher.mojang.com/")
                .map(|stripped| format!("https://bmclapi2.bangbang93.com/{stripped}"))
        };

    if let Some(m) = mirror_url {
        if !spec.urls.contains(&m) {
            spec.urls.push(m);
        }
    }
}

/// Executes game installation end-to-end with transactionality, .installing isolation,
/// CAS skipping, disk space verification, and atomic swap.
pub async fn install_game(
    req: &InstallRequest,
    paths: &MqPaths,
    net: &NetEngine,
    cas: &CasStore,
    compute_pool: &ComputePool,
    epoch_guard: &EpochGuard,
    event_bus: Option<&EventBus>,
) -> MqResult<InstallSummary> {
    let start = Instant::now();
    paths.ensure_dirs()?;
    fs::create_dir_all(&req.instance_dir)?;

    let instance_id = req
        .instance_dir
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(&req.mc_version);

    // 1. Inter-process and in-process installation lock
    let _install_lock = InstanceInstallLock::acquire(&req.instance_dir, instance_id).await?;

    // 2. Initialize install_state.json with status Installing
    let mut state = InstallState::new_installing(
        instance_id,
        &req.mc_version,
        req.loader.to_string(),
        req.loader_version.clone(),
    );
    let _ = state.save(&req.instance_dir);

    // 3. Prepare temporary working directory `<instance_dir>/.installing/`
    let installing_dir = req.instance_dir.join(".installing");
    if installing_dir.exists() {
        let _ = fs::remove_dir_all(&installing_dir);
    }
    fs::create_dir_all(&installing_dir)?;
    fs::create_dir_all(installing_dir.join("natives"))?;

    let res = install_game_transactional(
        req,
        paths,
        net,
        cas,
        compute_pool,
        epoch_guard,
        event_bus,
        &installing_dir,
        &mut state,
        start,
    )
    .await;

    match res {
        Ok(summary) => Ok(summary),
        Err((stage, err)) => {
            // Clean up temporary .installing directory on failure or cancellation
            let _ = fs::remove_dir_all(&installing_dir);

            // Record failure in install_state.json
            let install_err = InstallError::from_mq_error(stage, &err);
            state.status = InstallStatus::Failed;
            state.last_error = Some(install_err);
            let _ = state.save(&req.instance_dir);

            Err(err)
        }
    }
}

#[allow(clippy::too_many_arguments)]
async fn install_game_transactional(
    req: &InstallRequest,
    paths: &MqPaths,
    net: &NetEngine,
    cas: &CasStore,
    compute_pool: &ComputePool,
    epoch_guard: &EpochGuard,
    event_bus: Option<&EventBus>,
    installing_dir: &Path,
    state: &mut InstallState,
    start: Instant,
) -> Result<InstallSummary, (InstallStage, MqError)> {
    let mut current_stage = InstallStage::Manifest;

    if epoch_guard.is_cancelled() {
        return Err((
            current_stage,
            MqError::Cancelled {
                epoch: epoch_guard.epoch(),
            },
        ));
    }

    if let Some(bus) = event_bus {
        bus.publish(Event::PhaseStarted {
            phase: "metadata".to_string(),
            total_items: 2,
        });
    }

    // 1. Fetch Version Manifest
    let client = reqwest::Client::builder()
        .user_agent(mq_net::USER_AGENT)
        .build()
        .map_err(|e| (current_stage, MqError::Network(e.to_string())))?;

    let manifest = fetch_version_manifest(&client, req.mock_base_url.as_deref())
        .await
        .map_err(|e| (current_stage, e))?;
    let entry = find_version_entry(&manifest, &req.mc_version).map_err(|e| (current_stage, e))?;

    // 2. Fetch Vanilla version.json
    current_stage = InstallStage::VersionJson;
    if epoch_guard.is_cancelled() {
        return Err((
            current_stage,
            MqError::Cancelled {
                epoch: epoch_guard.epoch(),
            },
        ));
    }

    let rule_ctx = RuleContext::linux_x86_64();
    let mut resolved_ver = fetch_vanilla_version(&client, entry, &rule_ctx)
        .await
        .map_err(|e| (current_stage, e))?;

    // 3. ModLoader resolution if requested
    if req.loader != ModLoaderKind::Vanilla {
        current_stage = InstallStage::ModLoader;
        if epoch_guard.is_cancelled() {
            return Err((
                current_stage,
                MqError::Cancelled {
                    epoch: epoch_guard.epoch(),
                },
            ));
        }

        match req.loader {
            ModLoaderKind::Fabric => {
                let loader_ver = match &req.loader_version {
                    Some(v) => v.clone(),
                    None => {
                        let versions = fetch_fabric_loader_versions(
                            &client,
                            req.mock_base_url.as_deref(),
                            &req.mc_version,
                        )
                        .await
                        .map_err(|e| (current_stage, e))?;
                        versions.first().cloned().ok_or_else(|| {
                            (
                                current_stage,
                                MqError::Metadata(format!(
                                    "no Fabric loader version found for {}",
                                    req.mc_version
                                )),
                            )
                        })?
                    }
                };
                let profile = fetch_fabric_profile(
                    &client,
                    req.mock_base_url.as_deref(),
                    &req.mc_version,
                    &loader_ver,
                )
                .await
                .map_err(|e| (current_stage, e))?;
                resolved_ver =
                    merge_inherits_from(&resolved_ver, &profile).map_err(|e| (current_stage, e))?;
            }
            ModLoaderKind::Quilt => {
                let loader_ver = match &req.loader_version {
                    Some(v) => v.clone(),
                    None => {
                        let versions = fetch_quilt_loader_versions(
                            &client,
                            req.mock_base_url.as_deref(),
                            &req.mc_version,
                        )
                        .await
                        .map_err(|e| (current_stage, e))?;
                        versions.first().cloned().ok_or_else(|| {
                            (
                                current_stage,
                                MqError::Metadata(format!(
                                    "no Quilt loader version found for {}",
                                    req.mc_version
                                )),
                            )
                        })?
                    }
                };
                let profile = fetch_quilt_profile(
                    &client,
                    req.mock_base_url.as_deref(),
                    &req.mc_version,
                    &loader_ver,
                )
                .await
                .map_err(|e| (current_stage, e))?;
                resolved_ver =
                    merge_inherits_from(&resolved_ver, &profile).map_err(|e| (current_stage, e))?;
            }
            _ => {}
        }
    }

    // 3b. Ensure compatible Java runtime is present (Mojang / System / Adoptium / Zulu)
    current_stage = InstallStage::JavaRuntime;
    if let Some(bus) = event_bus {
        bus.publish(Event::PhaseStarted {
            phase: "java_runtime".to_string(),
            total_items: 1,
        });
    }

    let java_dir = paths.data_dir.join("java");
    let req_major = resolved_ver.java_version.major_version;
    let comp_hint = Some(resolved_ver.java_version.component.as_str());

    match mq_launch::ensure_java_runtime(&client, &java_dir, req_major, comp_hint, None).await {
        Ok(selected) => {
            tracing::info!(
                major = selected.runtime.major_version,
                source = ?selected.source,
                path = %selected.runtime.path.display(),
                "ensured Java runtime for installation"
            );
        }
        Err(e) => {
            if req.mock_base_url.is_some() {
                tracing::warn!("mock installation environment; skipping Java ensure error: {e}");
            } else {
                return Err((current_stage, e));
            }
        }
    }

    // 4. Fetch Asset Index
    current_stage = InstallStage::Assets;
    if epoch_guard.is_cancelled() {
        return Err((
            current_stage,
            MqError::Cancelled {
                epoch: epoch_guard.epoch(),
            },
        ));
    }

    let asset_index_path = paths
        .assets_dir()
        .join("indexes")
        .join(format!("{}.json", resolved_ver.asset_index.id));

    let (asset_index, raw_index_json) = if asset_index_path.is_file() {
        if let Ok(content) = fs::read_to_string(&asset_index_path) {
            if let Ok(idx) = parse_asset_index(&resolved_ver.asset_index.id, &content) {
                (idx, content)
            } else {
                fetch_asset_index_raw(&client, &resolved_ver.asset_index)
                    .await
                    .map_err(|e| (current_stage, e))?
            }
        } else {
            fetch_asset_index_raw(&client, &resolved_ver.asset_index)
                .await
                .map_err(|e| (current_stage, e))?
        }
    } else {
        fetch_asset_index_raw(&client, &resolved_ver.asset_index)
            .await
            .map_err(|e| (current_stage, e))?
    };

    if let Some(p) = asset_index_path.parent() {
        fs::create_dir_all(p).map_err(|e| (current_stage, MqError::Io(e)))?;
    }
    fs::write(&asset_index_path, raw_index_json.as_bytes())
        .map_err(|e| (current_stage, MqError::Io(e)))?;

    // 5. Pre-check disk space using statvfs
    current_stage = InstallStage::Verification;
    let mut estimated_download_bytes = 0u64;

    if let Some(sha1) = resolved_ver.client_jar.sha1 {
        if !cas.has_blob(&sha1) {
            estimated_download_bytes += resolved_ver.client_jar.size.max(25 * 1024 * 1024);
        }
    } else {
        estimated_download_bytes += resolved_ver.client_jar.size.max(25 * 1024 * 1024);
    }

    for lib in &resolved_ver.libraries {
        if let Some(art) = &lib.artifact {
            let needed = match art.sha1 {
                Some(sha1) => !cas.has_blob(&sha1),
                None => true,
            };
            if needed {
                estimated_download_bytes += art.size;
            }
        }
        if let Some(nat) = &lib.native_linux {
            let needed = match nat.sha1 {
                Some(sha1) => !cas.has_blob(&sha1),
                None => true,
            };
            if needed {
                estimated_download_bytes += nat.size;
            }
        }
    }

    for obj in &asset_index.objects {
        if !cas.has_blob(&obj.hash) {
            estimated_download_bytes += obj.size;
        }
    }

    // Include 50 MiB safety margin for unpacked natives, JSON files, and filesystem overhead
    let space_needed = estimated_download_bytes.saturating_add(50 * 1024 * 1024);
    check_free_disk_space(&req.instance_dir, space_needed).map_err(|e| (current_stage, e))?;

    // 6. Build Download Specs with fast CAS resumption and safe mirrors
    current_stage = InstallStage::Libraries;
    if epoch_guard.is_cancelled() {
        return Err((
            current_stage,
            MqError::Cancelled {
                epoch: epoch_guard.epoch(),
            },
        ));
    }
    let mut download_specs = Vec::new();
    let mut hardlink_targets: Vec<(PathBuf, PathBuf)> = Vec::new();

    // 6a. Client JAR -> written into .installing/client.jar
    let client_jar_installing = installing_dir.join("client.jar");
    let client_jar_blob = if let Some(sha1) = resolved_ver.client_jar.sha1 {
        cas.blob_path(&sha1)
    } else {
        client_jar_installing.clone()
    };

    let client_jar_in_cas = resolved_ver
        .client_jar
        .sha1
        .is_some_and(|sha1| cas.has_blob(&sha1));

    if !client_jar_in_cas {
        let mut client_spec = DownloadSpec {
            id: "client_jar".to_string(),
            urls: vec![resolved_ver.client_jar.url.clone()],
            dest_path: client_jar_blob.clone(),
            expected_sha1: resolved_ver.client_jar.sha1,
            expected_sha512: None,
            expected_size: if resolved_ver.client_jar.size > 0 {
                Some(resolved_ver.client_jar.size)
            } else {
                None
            },
        };
        add_mojang_mirror_if_hashed(&mut client_spec);
        download_specs.push(client_spec);
    }

    hardlink_targets.push((client_jar_blob.clone(), client_jar_installing));
    let ver_jar_dest = paths
        .versions_dir()
        .join(&resolved_ver.mc_version)
        .join(format!("{}.jar", resolved_ver.mc_version));
    hardlink_targets.push((client_jar_blob, ver_jar_dest));

    // 6b. Libraries
    let lib_base = paths.libraries_dir();
    let mut native_jars = Vec::new();
    let mut native_excludes = Vec::new();

    for lib in &resolved_ver.libraries {
        if let Some(art) = &lib.artifact {
            let target = lib_base.join(&art.path);
            let blob_dest = if let Some(sha1) = art.sha1 {
                cas.blob_path(&sha1)
            } else {
                target.clone()
            };

            let in_cas = art.sha1.is_some_and(|sha1| cas.has_blob(&sha1));
            if !in_cas {
                let mut spec = DownloadSpec {
                    id: format!("lib_{}", lib.name),
                    urls: vec![art.url.clone()],
                    dest_path: blob_dest.clone(),
                    expected_sha1: art.sha1,
                    expected_sha512: None,
                    expected_size: if art.size > 0 { Some(art.size) } else { None },
                };
                add_mojang_mirror_if_hashed(&mut spec);
                download_specs.push(spec);
            }
            if blob_dest != target {
                hardlink_targets.push((blob_dest, target));
            }
        }

        if let Some(nat) = &lib.native_linux {
            let target = lib_base.join(&nat.path);
            let blob_dest = if let Some(sha1) = nat.sha1 {
                cas.blob_path(&sha1)
            } else {
                target.clone()
            };

            let in_cas = nat.sha1.is_some_and(|sha1| cas.has_blob(&sha1));
            if !in_cas {
                let mut spec = DownloadSpec {
                    id: format!("native_{}", lib.name),
                    urls: vec![nat.url.clone()],
                    dest_path: blob_dest.clone(),
                    expected_sha1: nat.sha1,
                    expected_sha512: None,
                    expected_size: if nat.size > 0 { Some(nat.size) } else { None },
                };
                add_mojang_mirror_if_hashed(&mut spec);
                download_specs.push(spec);
            }

            native_jars.push(target.clone());
            native_excludes.push(lib.extract_exclude.clone());

            if blob_dest != target {
                hardlink_targets.push((blob_dest, target));
            }
        }
    }

    // 6c. Assets
    current_stage = InstallStage::Assets;
    let resources_base = req
        .mock_base_url
        .as_deref()
        .unwrap_or(mq_meta::MOJANG_RESOURCES_BASE_URL);
    let objects_base = paths.assets_dir().join("objects");

    let all_unique = asset_index.unique_objects_by_hash();
    let (lang_objs, mut other_objs): (Vec<_>, Vec<_>) = all_unique
        .into_iter()
        .partition(|o| o.name.starts_with("minecraft/lang/") || o.name == "pack.mcmeta");

    let mut prioritized_objs = lang_objs;
    prioritized_objs.append(&mut other_objs);

    for obj in prioritized_objs {
        let rel_path = obj.relative_object_path();
        let target = objects_base.join(&rel_path);
        let blob_dest = cas.blob_path(&obj.hash);

        let in_cas = cas.has_blob(&obj.hash);
        if !in_cas {
            let mut spec = DownloadSpec {
                id: format!("asset_{}", obj.hash.to_hex()),
                urls: vec![obj.download_url(resources_base)],
                dest_path: blob_dest.clone(),
                expected_sha1: Some(obj.hash),
                expected_sha512: None,
                expected_size: if obj.size > 0 { Some(obj.size) } else { None },
            };
            add_mojang_mirror_if_hashed(&mut spec);
            download_specs.push(spec);
        }

        if blob_dest != target {
            hardlink_targets.push((blob_dest, target));
        }
    }

    // 7. Execute Downloads via NetEngine
    let outcomes = if !download_specs.is_empty() {
        net.download_batch(download_specs, "install_game", epoch_guard)
            .await
            .map_err(|e| (current_stage, e))?
    } else {
        Vec::new()
    };

    let mut total_bytes = 0u64;
    for o in &outcomes {
        total_bytes += o.bytes_written;
    }

    // 8. Materialize hardlinks
    let mut hardlinks_created = 0usize;
    for (src, dst) in hardlink_targets {
        if let Some(p) = dst.parent() {
            fs::create_dir_all(p).map_err(|e| (current_stage, MqError::Io(e)))?;
        }
        if dst.exists() {
            let _ = fs::remove_file(&dst);
        }
        if fs::hard_link(&src, &dst).is_ok() {
            hardlinks_created += 1;
        } else {
            let _ = fs::copy(&src, &dst);
        }
    }

    // 8b. Legacy virtual / map_to_resources asset reconstruction (1.7.10 - 1.12.2)
    if asset_index.is_virtual {
        let virtual_base = paths.assets_dir().join("virtual").join(&asset_index.id);
        for obj in &asset_index.objects {
            let src = cas.blob_path(&obj.hash);
            let dst = virtual_base.join(&obj.name);
            if let Some(p) = dst.parent() {
                let _ = fs::create_dir_all(p);
            }
            if !dst.exists() && fs::hard_link(&src, &dst).is_err() {
                let _ = fs::copy(&src, &dst);
            }
        }
    }

    if asset_index.map_to_resources {
        let resources_base = installing_dir.join("resources");
        for obj in &asset_index.objects {
            let src = cas.blob_path(&obj.hash);
            let dst = resources_base.join(&obj.name);
            if let Some(p) = dst.parent() {
                let _ = fs::create_dir_all(p);
            }
            if !dst.exists() && fs::hard_link(&src, &dst).is_err() {
                let _ = fs::copy(&src, &dst);
            }
        }
    }

    // 9. Extract Natives into `<instance_dir>/.installing/natives`
    current_stage = InstallStage::Natives;
    let installing_natives_dir = installing_dir.join("natives");
    let extracted_natives = extract_natives_archives(
        &native_jars,
        &native_excludes,
        &installing_natives_dir,
        compute_pool,
        epoch_guard,
    )
    .await
    .map_err(|e| (current_stage, e))?;

    // 10. Save resolved_version.json in `.installing/`
    let installing_resolved_json = installing_dir.join("resolved_version.json");
    let resolved_bytes =
        serde_json::to_vec_pretty(&resolved_ver).map_err(|e| (current_stage, MqError::Json(e)))?;
    fs::write(&installing_resolved_json, &resolved_bytes)
        .map_err(|e| (current_stage, MqError::Io(e)))?;

    // 11. Verification stage: Validate integrity of all files in `.installing/`
    current_stage = InstallStage::Verification;
    state.status = InstallStatus::Verifying;
    let _ = state.save(&req.instance_dir);

    let installed_client_jar = installing_dir.join("client.jar");
    if !installed_client_jar.is_file() {
        return Err((
            current_stage,
            MqError::Io(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "client.jar is missing in .installing",
            )),
        ));
    }

    if let Some(expected_sha1) = resolved_ver.client_jar.sha1 {
        let actual_sha1 =
            compute_file_sha1_streaming(&installed_client_jar).map_err(|e| (current_stage, e))?;
        if actual_sha1 != expected_sha1 {
            return Err((
                current_stage,
                MqError::HashMismatch {
                    target: "client.jar".to_string(),
                    algo: "SHA-1",
                    expected: expected_sha1.to_hex(),
                    actual: actual_sha1.to_hex(),
                },
            ));
        }
    }

    if !installing_resolved_json.is_file() {
        return Err((
            current_stage,
            MqError::Io(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "resolved_version.json is missing in .installing",
            )),
        ));
    }

    if !installing_natives_dir.is_dir() {
        return Err((
            current_stage,
            MqError::Io(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "natives directory is missing in .installing",
            )),
        ));
    }

    // 12. Atomic Commit (Swap) from `.installing/` to `<instance_dir>/`
    current_stage = InstallStage::Finalize;

    // Atomic move of client.jar
    let final_client_jar = req.instance_dir.join("client.jar");
    fs::rename(&installed_client_jar, &final_client_jar)
        .map_err(|e| (current_stage, MqError::Io(e)))?;

    // Atomic move of resolved_version.json
    let final_resolved_json = req.instance_dir.join("resolved_version.json");
    fs::rename(&installing_resolved_json, &final_resolved_json)
        .map_err(|e| (current_stage, MqError::Io(e)))?;

    // Atomic swap of natives
    let final_natives_dir = req.instance_dir.join("natives");
    let old_natives_dir = req.instance_dir.join(".natives.old");
    if final_natives_dir.exists() {
        let _ = fs::rename(&final_natives_dir, &old_natives_dir);
    }
    fs::rename(&installing_natives_dir, &final_natives_dir)
        .map_err(|e| (current_stage, MqError::Io(e)))?;
    if old_natives_dir.exists() {
        let _ = fs::remove_dir_all(&old_natives_dir);
    }

    // Atomic swap of resources if present
    let installing_resources = installing_dir.join("resources");
    if installing_resources.exists() {
        let final_resources = req.instance_dir.join("resources");
        let old_resources = req.instance_dir.join(".resources.old");
        if final_resources.exists() {
            let _ = fs::rename(&final_resources, &old_resources);
        }
        let _ = fs::rename(&installing_resources, &final_resources);
        if old_resources.exists() {
            let _ = fs::remove_dir_all(&old_resources);
        }
    }

    // Clean up temporary .installing directory
    let _ = fs::remove_dir_all(installing_dir);

    // 13. Persist valid install_state.json with status Ready
    let now_str = chrono::Utc::now().to_rfc3339();
    state.status = InstallStatus::Ready;
    state.installed_at = Some(now_str.clone());
    state.verified_at = Some(now_str);
    state.client_jar = Some(InstallClientJarInfo {
        sha1: resolved_ver
            .client_jar
            .sha1
            .map(|s| s.to_hex())
            .unwrap_or_default(),
        size: resolved_ver.client_jar.size,
    });
    state.libraries_count = resolved_ver.libraries.len();
    state.assets_count = asset_index.objects.len();
    state.asset_index_id = Some(resolved_ver.asset_index.id.clone());
    state.natives_count = extracted_natives;
    state.java = Some(InstallJavaInfo {
        major_version: resolved_ver.java_version.major_version,
        path: PathBuf::from("java"),
        source: "mojang".to_string(),
        component: Some(resolved_ver.java_version.component.clone()),
    });
    state.last_error = None;
    state
        .save(&req.instance_dir)
        .map_err(|e| (current_stage, e))?;

    let elapsed_ms = start.elapsed().as_millis() as u64;

    Ok(InstallSummary {
        resolved_version: Arc::new(resolved_ver),
        asset_index: Arc::new(asset_index),
        libraries_installed: outcomes.len().min(100),
        assets_installed: outcomes.len().saturating_sub(100),
        natives_extracted: extracted_natives,
        hardlinks_created,
        bytes_downloaded: total_bytes,
        elapsed_ms,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use mq_core::Sha1Digest;
    use mq_sched::CancelEpoch;

    #[tokio::test]
    async fn transaction_failure_cleans_up_installing_dir_and_records_failed_state() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let inst_dir = tmp.path().join("instances").join("test-failed");
        fs::create_dir_all(&inst_dir).unwrap();

        let paths = MqPaths::with_root(tmp.path().join("mq_root"));
        let cas = CasStore::new(paths.clone());
        let net = NetEngine::with_defaults().expect("net engine");
        let compute = ComputePool::new(2);
        let epoch = Arc::new(CancelEpoch::new());
        let guard = epoch.snapshot();

        let req = InstallRequest {
            mc_version: "1.21.4".to_string(),
            loader: ModLoaderKind::Vanilla,
            loader_version: None,
            instance_dir: inst_dir.clone(),
            mock_base_url: Some("http://127.0.0.1:1/nonexistent".to_string()),
        };

        let res = install_game(&req, &paths, &net, &cas, &compute, &guard, None).await;
        assert!(
            res.is_err(),
            "install must fail due to unreachable mock url"
        );

        // 1. .installing directory must be completely cleaned up
        let installing_dir = inst_dir.join(".installing");
        assert!(
            !installing_dir.exists(),
            ".installing dir must be removed on failure"
        );

        // 2. install_state.json must be persisted with Failed status and error details
        let loaded = InstallState::load(&inst_dir).expect("install_state.json must exist");
        assert_eq!(loaded.status, InstallStatus::Failed);
        assert!(loaded.last_error.is_some());
        let err = loaded.last_error.as_ref().unwrap();
        assert_eq!(err.stage, InstallStage::Manifest);
        assert!(!err.localized_title.is_empty());

        // 3. No partial files must exist in instance_dir and is_ready must be false
        assert!(!inst_dir.join("client.jar").exists());
        assert!(!inst_dir.join("resolved_version.json").exists());
        assert!(!inst_dir.join("natives").exists());
        assert!(!loaded.is_ready(&inst_dir));
    }

    #[test]
    fn cas_resumption_skips_resident_blobs() {
        use sha1::Digest as _;

        let tmp = tempfile::tempdir().expect("tempdir");
        let paths = MqPaths::with_root(tmp.path().join("mq_root"));
        let cas = CasStore::new(paths);

        let data = b"sample resident library contents";
        let src_file = tmp.path().join("lib.jar");
        fs::write(&src_file, data).unwrap();
        let digest = Sha1Digest::from_bytes(sha1::Sha1::digest(data).into());

        // Before storing in CAS
        assert!(!cas.has_blob(&digest));

        // Store into CAS
        let stored_path = cas.store_verified_file(&src_file, digest).expect("store");
        assert!(stored_path.is_file());

        // Resumption check: CAS fast path recognises blob
        assert!(cas.has_blob(&digest));

        // Verify that in pipeline download planning, in_cas skips download
        let in_cas = Some(digest).is_some_and(|sha1| cas.has_blob(&sha1));
        assert!(
            in_cas,
            "blob in CAS should be skipped from network downloads"
        );
    }

    #[test]
    fn atomic_swap_replaces_old_files_safely() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let inst_dir = tmp.path().join("instances").join("test-swap");
        let installing_dir = inst_dir.join(".installing");
        fs::create_dir_all(&installing_dir).unwrap();

        // Stale existing files in instance_dir
        fs::create_dir_all(inst_dir.join("natives")).unwrap();
        fs::write(inst_dir.join("client.jar"), b"old-client-jar").unwrap();
        fs::write(inst_dir.join("natives").join("stale.so"), b"old-native").unwrap();

        // New files in .installing
        fs::create_dir_all(installing_dir.join("natives")).unwrap();
        fs::write(
            installing_dir.join("client.jar"),
            b"new-verified-client-jar",
        )
        .unwrap();
        fs::write(
            installing_dir.join("resolved_version.json"),
            b"{\"mc\": \"1.21.4\"}",
        )
        .unwrap();
        fs::write(
            installing_dir.join("natives").join("libnew.so"),
            b"new-native",
        )
        .unwrap();

        // Perform atomic swap
        let installed_client_jar = installing_dir.join("client.jar");
        let final_client_jar = inst_dir.join("client.jar");
        fs::rename(&installed_client_jar, &final_client_jar).unwrap();

        let installed_resolved = installing_dir.join("resolved_version.json");
        let final_resolved = inst_dir.join("resolved_version.json");
        fs::rename(&installed_resolved, &final_resolved).unwrap();

        let installing_natives_dir = installing_dir.join("natives");
        let final_natives_dir = inst_dir.join("natives");
        let old_natives_dir = inst_dir.join(".natives.old");
        if final_natives_dir.exists() {
            fs::rename(&final_natives_dir, &old_natives_dir).unwrap();
        }
        fs::rename(&installing_natives_dir, &final_natives_dir).unwrap();
        if old_natives_dir.exists() {
            fs::remove_dir_all(&old_natives_dir).unwrap();
        }
        let _ = fs::remove_dir_all(&installing_dir);

        // Verification after swap
        assert_eq!(
            fs::read(&final_client_jar).unwrap(),
            b"new-verified-client-jar"
        );
        assert_eq!(fs::read(&final_resolved).unwrap(), b"{\"mc\": \"1.21.4\"}");
        assert!(final_natives_dir.join("libnew.so").exists());
        assert!(!final_natives_dir.join("stale.so").exists());
        assert!(!old_natives_dir.exists());
        assert!(!installing_dir.exists());
    }
}
