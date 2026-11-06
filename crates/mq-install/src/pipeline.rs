#![deny(clippy::await_holding_lock)]
//! Game installation pipeline DAG and execution engine (`mq install`).

use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use mq_core::{Event, EventBus, ModLoaderKind, MqError, MqPaths, MqResult};
use mq_meta::{
    fetch_asset_index, fetch_fabric_loader_versions, fetch_fabric_profile,
    fetch_quilt_loader_versions, fetch_quilt_profile, fetch_vanilla_version,
    fetch_version_manifest, find_version_entry, merge_inherits_from, AssetIndex, ResolvedVersion,
    RuleContext,
};
use mq_net::{DownloadSpec, NetEngine};
use mq_sched::{ComputePool, DagNodeSpec, DagPlan, EpochGuard};

use crate::cas::CasStore;
use crate::natives::extract_natives_archives;

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

/// Executes game installation end-to-end.
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
    fs::create_dir_all(req.instance_dir.join("natives"))?;

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
        .map_err(|e| MqError::Network(e.to_string()))?;

    let manifest = fetch_version_manifest(&client, req.mock_base_url.as_deref()).await?;
    let entry = find_version_entry(&manifest, &req.mc_version)?;

    // 2. Fetch Vanilla version.json
    let rule_ctx = RuleContext::linux_x86_64();
    let mut resolved_ver = fetch_vanilla_version(&client, entry, &rule_ctx).await?;

    // 3. ModLoader resolution if requested
    if req.loader != ModLoaderKind::Vanilla {
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
                        .await?;
                        versions.first().cloned().ok_or_else(|| {
                            MqError::Metadata(format!(
                                "no Fabric loader version found for {}",
                                req.mc_version
                            ))
                        })?
                    }
                };
                let profile = fetch_fabric_profile(
                    &client,
                    req.mock_base_url.as_deref(),
                    &req.mc_version,
                    &loader_ver,
                )
                .await?;
                resolved_ver = merge_inherits_from(&resolved_ver, &profile)?;
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
                        .await?;
                        versions.first().cloned().ok_or_else(|| {
                            MqError::Metadata(format!(
                                "no Quilt loader version found for {}",
                                req.mc_version
                            ))
                        })?
                    }
                };
                let profile = fetch_quilt_profile(
                    &client,
                    req.mock_base_url.as_deref(),
                    &req.mc_version,
                    &loader_ver,
                )
                .await?;
                resolved_ver = merge_inherits_from(&resolved_ver, &profile)?;
            }
            _ => {}
        }
    }

    // 4. Fetch Asset Index
    let asset_index = fetch_asset_index(&client, &resolved_ver.asset_index).await?;

    // Save asset index JSON
    let asset_index_path = paths
        .assets_dir()
        .join("indexes")
        .join(format!("{}.json", resolved_ver.asset_index.id));
    if let Some(p) = asset_index_path.parent() {
        fs::create_dir_all(p)?;
    }
    fs::write(&asset_index_path, serde_json::to_vec_pretty(&asset_index)?)?;

    // 5. Build Download Specs
    let mut download_specs = Vec::new();
    let mut hardlink_targets: Vec<(PathBuf, PathBuf)> = Vec::new();

    // 5a. Client JAR
    let client_jar_dest = req.instance_dir.join("client.jar");
    let client_jar_blob = if let Some(sha1) = resolved_ver.client_jar.sha1 {
        cas.blob_path(&sha1)
    } else {
        client_jar_dest.clone()
    };
    download_specs.push(DownloadSpec {
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
    });
    if client_jar_blob != client_jar_dest {
        hardlink_targets.push((client_jar_blob.clone(), client_jar_dest));
    }
    let ver_jar_dest = paths
        .versions_dir()
        .join(&resolved_ver.mc_version)
        .join(format!("{}.jar", resolved_ver.mc_version));
    if client_jar_blob != ver_jar_dest {
        hardlink_targets.push((client_jar_blob, ver_jar_dest));
    }

    // 5b. Libraries
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

            download_specs.push(DownloadSpec {
                id: format!("lib_{}", lib.name),
                urls: vec![art.url.clone()],
                dest_path: blob_dest.clone(),
                expected_sha1: art.sha1,
                expected_sha512: None,
                expected_size: if art.size > 0 { Some(art.size) } else { None },
            });
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

            download_specs.push(DownloadSpec {
                id: format!("native_{}", lib.name),
                urls: vec![nat.url.clone()],
                dest_path: blob_dest.clone(),
                expected_sha1: nat.sha1,
                expected_sha512: None,
                expected_size: if nat.size > 0 { Some(nat.size) } else { None },
            });
            native_jars.push(target.clone());
            native_excludes.push(lib.extract_exclude.clone());

            if blob_dest != target {
                hardlink_targets.push((blob_dest, target));
            }
        }
    }

    // 5c. Assets (Prioritize all minecraft/lang/*.json files at the very top of the download batch)
    let resources_base = req
        .mock_base_url
        .as_deref()
        .unwrap_or(mq_meta::MOJANG_RESOURCES_BASE_URL);
    let objects_base = paths.assets_dir().join("objects");

    let all_unique = asset_index.unique_objects_by_hash();
    let (lang_objs, mut other_objs): (Vec<_>, Vec<_>) = all_unique
        .into_iter()
        .partition(|o| o.name.starts_with("minecraft/lang/"));

    let mut prioritized_objs = lang_objs;
    prioritized_objs.append(&mut other_objs);

    for obj in prioritized_objs {
        let rel_path = obj.relative_object_path();
        let target = objects_base.join(&rel_path);
        let blob_dest = cas.blob_path(&obj.hash);

        download_specs.push(DownloadSpec {
            id: format!("asset_{}", obj.hash.to_hex()),
            urls: vec![obj.download_url(resources_base)],
            dest_path: blob_dest.clone(),
            expected_sha1: Some(obj.hash),
            expected_sha512: None,
            expected_size: if obj.size > 0 { Some(obj.size) } else { None },
        });

        if blob_dest != target {
            hardlink_targets.push((blob_dest, target));
        }
    }

    // 6. Download all files in parallel via NetEngine
    let outcomes = net
        .download_batch(download_specs, "install_game", epoch_guard)
        .await?;
    let mut total_bytes = 0u64;
    for o in &outcomes {
        total_bytes += o.bytes_written;
    }

    // 7. Materialize Hardlinks
    let mut hardlinks_created = 0usize;
    for (src, dst) in hardlink_targets {
        if let Some(p) = dst.parent() {
            fs::create_dir_all(p)?;
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

    // 8. Extract Natives
    let natives_dir = req.instance_dir.join("natives");
    let extracted_natives = extract_natives_archives(
        &native_jars,
        &native_excludes,
        &natives_dir,
        compute_pool,
        epoch_guard,
    )
    .await?;

    // 9. Save resolved_version.json in instance directory for fast offline launches
    let resolved_json_path = req.instance_dir.join("resolved_version.json");
    fs::write(
        &resolved_json_path,
        serde_json::to_vec_pretty(&resolved_ver)?,
    )?;

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
