#![deny(clippy::await_holding_lock)]
//! Main `eframe::App` implementation for MQ Launcher. Zero emojis!

use egui::{CentralPanel, SidePanel, TopBottomPanel};
use mq_core::GuiBackend;

use crate::accounts_view::AccountsView;
use crate::catalog_view::CatalogView;
use crate::fonts::configure_system_fonts;
use crate::hud::{is_zero_viewport, render_bottom_hud, BottomHudData};
use crate::instances_view::InstancesView;
use crate::perf_view::PerfView;
use crate::settings_view::SettingsView;
use crate::sidebar::{render_sidebar, sidebar_width, NavTab, SidebarAction};
use crate::state::{UiState, UiWorkerCommand, UiWorkerResponse};
use crate::theme::{apply_theme, solid_sidebar_frame, Tokens};

#[cfg(any(feature = "test-hooks", test))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TestAction {
    Install {
        mc_version: String,
        loader: mq_core::ModLoaderKind,
    },
}

#[cfg(any(feature = "test-hooks", test))]
pub fn parse_test_action(raw: &str) -> Option<TestAction> {
    let trimmed = raw.trim();
    let rest = trimmed.strip_prefix("install:")?;
    let mut parts = rest.split(':');
    let mc_version = parts.next()?.trim();
    if mc_version.is_empty() {
        return None;
    }
    let loader = match parts.next() {
        Some(ldr_str) if !ldr_str.trim().is_empty() => {
            ldr_str.trim().parse::<mq_core::ModLoaderKind>().ok()?
        }
        _ => mq_core::ModLoaderKind::Vanilla,
    };
    if parts.next().is_some() {
        return None;
    }
    Some(TestAction::Install {
        mc_version: mc_version.to_string(),
        loader,
    })
}

#[cfg(feature = "test-hooks")]
#[derive(Debug, Clone)]
struct TestHookRuntimeState {
    initialized: bool,
    action_active: bool,
    started_at: Option<std::time::Instant>,
    cancel_after_ms: Option<u64>,
    cancel_sent: bool,
    exit_on_finish: bool,
    finish_frames: u32,
}

#[cfg(feature = "test-hooks")]
impl Default for TestHookRuntimeState {
    fn default() -> Self {
        Self {
            initialized: false,
            action_active: false,
            started_at: None,
            cancel_after_ms: None,
            cancel_sent: false,
            exit_on_finish: false,
            finish_frames: 0,
        }
    }
}

#[inline]
fn configured_mock_base_url() -> Option<String> {
    #[cfg(feature = "test-hooks")]
    {
        std::env::var("MQ_TEST_MOCK_URL")
            .ok()
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty())
    }
    #[cfg(not(feature = "test-hooks"))]
    {
        None
    }
}

pub struct MqApp {
    pub state: UiState,
    pub current_tab: NavTab,
    pub instances_view: InstancesView,
    pub catalog_view: CatalogView,
    pub accounts_view: AccountsView,
    pub perf_view: PerfView,
    pub settings_view: SettingsView,
    pub recovery_journal: Option<mq_core::crash::InstallJournal>,
    pub frame_crash_message: Option<String>,
}

/// Spawns a background worker future and monitors it for panics or crashes.
///
/// If the task panics or aborts abnormally:
/// 1. Formats and persists a crash report via [`mq_core::crash::write_crash_report_for_panic`].
/// 2. Converts the card state to [`UiWorkerResponse::InstanceError`] with localized title
///    «Установка прервана из-за внутренней ошибки».
fn spawn_monitored_task<F>(
    task_name: &'static str,
    paths: mq_core::MqPaths,
    instance_id: mq_core::InstanceId,
    resp_tx: flume::Sender<UiWorkerResponse>,
    fut: F,
) where
    F: std::future::Future<Output = ()> + Send + 'static,
{
    let inst_id = instance_id.clone();
    let handle = tokio::spawn(async move {
        fut.await;
    });

    tokio::spawn(async move {
        if let Err(join_err) = handle.await {
            let panic_msg = if join_err.is_panic() {
                let panic_obj = join_err.into_panic();
                if let Some(s) = panic_obj.downcast_ref::<&str>() {
                    (*s).to_string()
                } else if let Some(s) = panic_obj.downcast_ref::<String>() {
                    s.clone()
                } else {
                    format!("Паника в фоновом потоке {task_name}")
                }
            } else {
                format!("Фоновая задача {task_name} отменена: {join_err}")
            };

            let backtrace = std::backtrace::Backtrace::capture().to_string();
            let _ = mq_core::crash::write_crash_report_for_panic(
                &paths, &panic_msg, None, task_name, &backtrace,
            );

            let error_text = format!("Установка прервана из-за внутренней ошибки: {panic_msg}");
            let _ = resp_tx.send(UiWorkerResponse::InstanceError {
                instance_id: inst_id,
                error: error_text,
            });
        }
    });
}

impl MqApp {
    pub fn new(cc: &eframe::CreationContext<'_>, state: UiState) -> Self {
        apply_theme(&cc.egui_ctx, &state.config.theme);
        configure_system_fonts(&cc.egui_ctx);
        Self::from_state(state)
    }

    pub fn from_state(mut state: UiState) -> Self {
        let recovery_journal = mq_core::crash::InstallJournal::load_unclosed(&state.paths);
        let mut current_tab = NavTab::Instances;
        if let Some(path) = state.pending_mrpack_path.take() {
            state.import_local_pack(path);
            current_tab = NavTab::Catalog;
        }
        Self {
            state,
            current_tab,
            instances_view: InstancesView::default(),
            catalog_view: CatalogView::default(),
            accounts_view: AccountsView::default(),
            perf_view: PerfView,
            settings_view: SettingsView::default(),
            recovery_journal,
            frame_crash_message: None,
        }
    }

    fn handle_worker_commands(&mut self) {
        while let Ok(cmd) = self.state.cmd_receiver.try_recv() {
            match cmd {
                UiWorkerCommand::FetchVersionManifest => {
                    let resp_tx = self.state.resp_sender.clone();
                    let bus = self.state.event_bus.clone();
                    tokio::spawn(async move {
                        let client = reqwest::Client::builder()
                            .timeout(std::time::Duration::from_secs(12))
                            .build()
                            .unwrap_or_default();
                        if let Ok(manifest) = mq_meta::fetch_version_manifest(&client, None).await {
                            let versions = manifest
                                .versions
                                .into_iter()
                                .map(|v| (v.id, v.release_type))
                                .collect();
                            let _ =
                                resp_tx.send(UiWorkerResponse::VersionManifestFetched(versions));
                            bus.publish(mq_core::Event::PhaseCompleted {
                                phase: "manifest_fetch".to_string(),
                                elapsed_ms: 0,
                            });
                        }
                    });
                }
                UiWorkerCommand::RefreshInstances => {
                    if let Ok(instances) = self.state.instance_mgr.list_instances() {
                        self.state.instances = instances;
                    }
                }
                UiWorkerCommand::RefreshAccounts => {
                    if let Ok(accs) = mq_auth::AccountStore::load_or_default(&self.state.paths) {
                        self.state.accounts = accs;
                    }
                }
                UiWorkerCommand::ScanInstanceMods { instance_id } => {
                    let mods_dir = self
                        .state
                        .paths
                        .instance_dir(instance_id.as_str())
                        .join("mods");
                    let pool = self.state.compute_pool.clone();
                    let guard = self.state.cancel_epoch.snapshot();
                    let bus = self.state.event_bus.clone();
                    let resp_tx = self.state.resp_sender.clone();

                    tokio::spawn(async move {
                        if let Ok(rep) =
                            mq_mods::scan_mods_directory(&mods_dir, &pool, &guard).await
                        {
                            let _ = resp_tx.send(UiWorkerResponse::ScanModsResults {
                                mods: rep.mods,
                                conflicts: rep.conflicts,
                            });
                            bus.publish(mq_core::Event::PhaseCompleted {
                                phase: "mods_scan".to_string(),
                                elapsed_ms: rep.scan_duration_ms,
                            });
                        }
                    });
                }
                UiWorkerCommand::CleanupUnusedDeps { instance_id } => {
                    let inst_dir = self.state.paths.instance_dir(instance_id.as_str());
                    let mods_dir = inst_dir.join("mods");
                    let pool = self.state.compute_pool.clone();
                    let guard = self.state.cancel_epoch.snapshot();
                    let resp_tx = self.state.resp_sender.clone();
                    tokio::spawn(async move {
                        let result = tokio::task::spawn_blocking({
                            let inst_dir = inst_dir.clone();
                            let mods_dir = mods_dir.clone();
                            move || -> Result<usize, String> {
                                let mut lock = mq_mods::ModLockFile::load_for_instance(&inst_dir);
                                let mut removed = 0usize;
                                for key in lock.find_unused_dependencies() {
                                    let Some(filename) =
                                        lock.entries.get(&key).map(|e| e.filename.clone())
                                    else {
                                        continue;
                                    };
                                    for path in [
                                        mods_dir.join(&filename),
                                        mods_dir.join(format!("{filename}.disabled")),
                                    ] {
                                        if path.exists() {
                                            let _ = std::fs::remove_file(&path);
                                        }
                                    }
                                    if lock.remove_entry(&key).is_some() {
                                        removed += 1;
                                    }
                                }
                                if removed > 0 {
                                    lock.save_for_instance(&inst_dir)
                                        .map_err(|e| e.to_string())?;
                                }
                                Ok(removed)
                            }
                        })
                        .await;
                        let (removed, error) = match result {
                            Ok(Ok(n)) => (n, None),
                            Ok(Err(e)) => (0, Some(e)),
                            Err(e) => (0, Some(e.to_string())),
                        };

                        if error.is_none() {
                            if let Ok(rep) =
                                mq_mods::scan_mods_directory(&mods_dir, &pool, &guard).await
                            {
                                let _ = resp_tx.send(UiWorkerResponse::ScanModsResults {
                                    mods: rep.mods,
                                    conflicts: rep.conflicts,
                                });
                            }
                        }
                        let _ =
                            resp_tx.send(UiWorkerResponse::UnusedDepsCleaned { removed, error });
                    });
                }
                UiWorkerCommand::ToggleMod {
                    instance_id,
                    file_name,
                } => {
                    let mods_dir = self
                        .state
                        .paths
                        .instance_dir(instance_id.as_str())
                        .join("mods");
                    let pool = self.state.compute_pool.clone();
                    let guard = self.state.cancel_epoch.snapshot();
                    let resp_tx = self.state.resp_sender.clone();
                    tokio::spawn(async move {
                        let _ = mq_mods::toggle_mod(&mods_dir, &file_name);
                        if let Ok(rep) =
                            mq_mods::scan_mods_directory(&mods_dir, &pool, &guard).await
                        {
                            let _ = resp_tx.send(UiWorkerResponse::ScanModsResults {
                                mods: rep.mods,
                                conflicts: rep.conflicts,
                            });
                        }
                    });
                }
                UiWorkerCommand::DeleteMod {
                    instance_id,
                    file_name,
                } => {
                    let mods_dir = self
                        .state
                        .paths
                        .instance_dir(instance_id.as_str())
                        .join("mods");
                    let pool = self.state.compute_pool.clone();
                    let guard = self.state.cancel_epoch.snapshot();
                    let resp_tx = self.state.resp_sender.clone();
                    tokio::spawn(async move {
                        let _ = mq_mods::delete_mod(&mods_dir, &file_name);
                        if let Ok(rep) =
                            mq_mods::scan_mods_directory(&mods_dir, &pool, &guard).await
                        {
                            let _ = resp_tx.send(UiWorkerResponse::ScanModsResults {
                                mods: rep.mods,
                                conflicts: rep.conflicts,
                            });
                        }
                    });
                }
                UiWorkerCommand::VerifyInstance { instance_id }
                | UiWorkerCommand::VerifyAndRepairInstance { instance_id } => {
                    let inst_dir = self.state.paths.instance_dir(instance_id.as_str());
                    let resolved_path = inst_dir.join("resolved_version.json");
                    let paths = self.state.paths.clone();
                    let net = self.state.net.clone();
                    let cas = self.state.cas.clone();
                    let pool = self.state.compute_pool.clone();
                    let guard = self.state.cancel_epoch.snapshot();
                    let bus = self.state.event_bus.clone();
                    let resp_tx = self.state.resp_sender.clone();
                    let inst_opt = self
                        .state
                        .instances
                        .iter()
                        .find(|i| i.id == instance_id)
                        .cloned();

                    let _ = resp_tx.send(UiWorkerResponse::InstanceInstallStarted(
                        instance_id.clone(),
                    ));

                    spawn_monitored_task(
                        "verify_instance",
                        paths.clone(),
                        instance_id.clone(),
                        resp_tx.clone(),
                        async move {
                            bus.publish(mq_core::Event::PhaseStarted {
                                phase: "verify".to_string(),
                                total_items: 100,
                            });

                            let mut verified_ok = false;
                            if let Ok(bytes) = std::fs::read(&resolved_path) {
                                if let Ok(ver) =
                                    serde_json::from_slice::<mq_meta::ResolvedVersion>(&bytes)
                                {
                                    let asset_index_path = paths
                                        .assets_dir()
                                        .join("indexes")
                                        .join(format!("{}.json", ver.asset_index.id));
                                    if let Ok(idx_bytes) = std::fs::read(&asset_index_path) {
                                        if let Ok(idx) = serde_json::from_slice::<mq_meta::AssetIndex>(
                                            &idx_bytes,
                                        ) {
                                            let targets =
                                                mq_install::collect_instance_verify_targets(
                                                    &paths, &inst_dir, &ver, &idx,
                                                );
                                            match mq_install::verify_files_parallel(
                                                &targets,
                                                &pool,
                                                &guard,
                                                Some(&bus),
                                            )
                                            .await
                                            {
                                                Ok(report) => {
                                                    if report.is_clean() {
                                                        bus.publish(
                                                            mq_core::Event::PhaseCompleted {
                                                                phase: "verify".to_string(),
                                                                elapsed_ms: report.elapsed_ms,
                                                            },
                                                        );
                                                        let _ = resp_tx.send(
                                                        UiWorkerResponse::InstanceInstallCompleted(
                                                            instance_id.clone(),
                                                        ),
                                                    );
                                                        verified_ok = true;
                                                    } else {
                                                        for corrupt in &report.corrupted_files {
                                                            let _ = std::fs::remove_file(corrupt);
                                                        }
                                                    }
                                                }
                                                Err(e) => {
                                                    bus.publish(mq_core::Event::TaskError {
                                                        context: "Проверка файлов".to_string(),
                                                        message: e.to_string(),
                                                    });
                                                }
                                            }
                                        }
                                    }
                                }
                            }

                            if !verified_ok {
                                if let Some(inst) = inst_opt {
                                    let req = mq_install::InstallRequest {
                                        mc_version: inst.mc_version.clone(),
                                        loader: inst.loader,
                                        loader_version: inst.loader_version.clone(),
                                        instance_dir: inst_dir.clone(),
                                        mock_base_url: configured_mock_base_url(),
                                    };

                                    match mq_install::install_game(
                                        &req,
                                        &paths,
                                        &net,
                                        &cas,
                                        &pool,
                                        &guard,
                                        Some(&bus),
                                    )
                                    .await
                                    {
                                        Ok(summary) => {
                                            bus.publish(mq_core::Event::PhaseCompleted {
                                                phase: "verify_and_repair".to_string(),
                                                elapsed_ms: summary.elapsed_ms,
                                            });
                                            let _ = resp_tx.send(
                                                UiWorkerResponse::InstanceInstallCompleted(
                                                    instance_id.clone(),
                                                ),
                                            );
                                        }
                                        Err(e) => {
                                            let err_msg =
                                                format!("Ошибка починки {}: {e}", inst.name);
                                            bus.publish(mq_core::Event::TaskError {
                                                context: format!("Починка {}", inst.name),
                                                message: e.to_string(),
                                            });
                                            let _ = resp_tx.send(UiWorkerResponse::InstanceError {
                                                instance_id,
                                                error: err_msg,
                                            });
                                        }
                                    }
                                } else {
                                    let _ = resp_tx.send(
                                        UiWorkerResponse::InstanceInstallCompleted(instance_id),
                                    );
                                }
                            }
                        },
                    );
                }
                UiWorkerCommand::ReinstallInstance { instance_id } => {
                    let inst_dir = self.state.paths.instance_dir(instance_id.as_str());
                    let paths = self.state.paths.clone();
                    let bus = self.state.event_bus.clone();
                    let net = self.state.net.clone();
                    let cas = self.state.cas.clone();
                    let pool = self.state.compute_pool.clone();
                    let guard = self.state.cancel_epoch.snapshot();
                    let resp_tx = self.state.resp_sender.clone();

                    if let Some(inst) = self
                        .state
                        .instances
                        .iter()
                        .find(|i| i.id == instance_id)
                        .cloned()
                    {
                        // Safely remove game binaries and temporary directories while preserving
                        // user saves/, options.txt, resourcepacks/, shaderpacks/, and servers.dat!
                        let _ = std::fs::remove_file(inst_dir.join("client.jar"));
                        let _ = std::fs::remove_file(inst_dir.join("resolved_version.json"));
                        let _ = std::fs::remove_file(inst_dir.join("install_state.json"));
                        let _ = std::fs::remove_dir_all(inst_dir.join("natives"));
                        let _ = std::fs::remove_dir_all(inst_dir.join(".installing"));

                        let _ = resp_tx.send(UiWorkerResponse::InstanceInstallStarted(
                            instance_id.clone(),
                        ));

                        spawn_monitored_task(
                            "reinstall_instance",
                            paths.clone(),
                            instance_id.clone(),
                            resp_tx.clone(),
                            async move {
                                let req = mq_install::InstallRequest {
                                    mc_version: inst.mc_version.clone(),
                                    loader: inst.loader,
                                    loader_version: inst.loader_version.clone(),
                                    instance_dir: inst_dir.clone(),
                                    mock_base_url: configured_mock_base_url(),
                                };

                                match mq_install::install_game(
                                    &req,
                                    &paths,
                                    &net,
                                    &cas,
                                    &pool,
                                    &guard,
                                    Some(&bus),
                                )
                                .await
                                {
                                    Ok(summary) => {
                                        bus.publish(mq_core::Event::PhaseCompleted {
                                            phase: "reinstall".to_string(),
                                            elapsed_ms: summary.elapsed_ms,
                                        });
                                        let _ = resp_tx.send(
                                            UiWorkerResponse::InstanceInstallCompleted(instance_id),
                                        );
                                    }
                                    Err(e) => {
                                        let err_msg =
                                            format!("Ошибка переустановки {}: {e}", inst.name);
                                        bus.publish(mq_core::Event::TaskError {
                                            context: format!("Переустановка {}", inst.name),
                                            message: e.to_string(),
                                        });
                                        let _ = resp_tx.send(UiWorkerResponse::InstanceError {
                                            instance_id,
                                            error: err_msg,
                                        });
                                    }
                                }
                            },
                        );
                    }
                }
                UiWorkerCommand::CancelDownload => {
                    self.state.cancel_epoch.cancel_and_advance();
                    self.state.installing_instance = None;
                    self.state.active_phase = None;
                    self.state.phase_progress = None;
                    self.state.status_message =
                        Some(("Загрузка отменена пользователем".to_string(), false));
                }
                UiWorkerCommand::InstallInstance { instance_id } => {
                    let inst_dir = self.state.paths.instance_dir(instance_id.as_str());
                    let paths = self.state.paths.clone();
                    let bus = self.state.event_bus.clone();
                    let net = self.state.net.clone();
                    let cas = self.state.cas.clone();
                    let pool = self.state.compute_pool.clone();
                    let guard = self.state.cancel_epoch.snapshot();
                    let resp_tx = self.state.resp_sender.clone();

                    if let Some(inst) = self
                        .state
                        .instances
                        .iter()
                        .find(|i| i.id == instance_id)
                        .cloned()
                    {
                        let _ = resp_tx.send(UiWorkerResponse::InstanceInstallStarted(
                            instance_id.clone(),
                        ));
                        spawn_monitored_task(
                            "install_instance",
                            paths.clone(),
                            instance_id.clone(),
                            resp_tx.clone(),
                            async move {
                                let req = mq_install::InstallRequest {
                                    mc_version: inst.mc_version.clone(),
                                    loader: inst.loader,
                                    loader_version: inst.loader_version.clone(),
                                    instance_dir: inst_dir.clone(),
                                    mock_base_url: configured_mock_base_url(),
                                };

                                match mq_install::install_game(
                                    &req,
                                    &paths,
                                    &net,
                                    &cas,
                                    &pool,
                                    &guard,
                                    Some(&bus),
                                )
                                .await
                                {
                                    Ok(summary) => {
                                        bus.publish(mq_core::Event::PhaseCompleted {
                                            phase: "install_game".to_string(),
                                            elapsed_ms: summary.elapsed_ms,
                                        });
                                        let _ = resp_tx.send(
                                            UiWorkerResponse::InstanceInstallCompleted(instance_id),
                                        );
                                    }
                                    Err(e) => {
                                        let err_msg =
                                            format!("Ошибка установки {}: {e}", inst.name);
                                        bus.publish(mq_core::Event::TaskError {
                                            context: format!("Установка {}", inst.name),
                                            message: e.to_string(),
                                        });
                                        let _ = resp_tx.send(UiWorkerResponse::InstanceError {
                                            instance_id,
                                            error: err_msg,
                                        });
                                    }
                                }
                            },
                        );
                    }
                }
                UiWorkerCommand::LaunchInstance {
                    instance_id,
                    dry_run,
                } => {
                    let inst_dir = self.state.paths.instance_dir(instance_id.as_str());
                    let paths = self.state.paths.clone();
                    let bus = self.state.event_bus.clone();
                    let net = self.state.net.clone();
                    let cas = self.state.cas.clone();
                    let pool = self.state.compute_pool.clone();
                    let guard = self.state.cancel_epoch.snapshot();
                    let resp_tx = self.state.resp_sender.clone();
                    let default_mc_loc = self.state.config.default_mc_locale.clone();

                    if let Some(inst) = self
                        .state
                        .instances
                        .iter()
                        .find(|i| i.id == instance_id)
                        .cloned()
                    {
                        let active_acc = self
                            .state
                            .accounts
                            .active_account()
                            .cloned()
                            .unwrap_or_else(|| mq_auth::create_offline_account("Player").unwrap());

                        spawn_monitored_task(
                            "launch_instance",
                            paths.clone(),
                            instance_id.clone(),
                            resp_tx.clone(),
                            async move {
                                // 1. Check if installation is needed
                                let resolved_path = inst_dir.join("resolved_version.json");
                                let client_jar_path = inst_dir.join("client.jar");
                                let is_installed =
                                    resolved_path.exists() && client_jar_path.exists();

                                if !is_installed {
                                    let _ = resp_tx.send(UiWorkerResponse::InstanceInstallStarted(
                                        instance_id.clone(),
                                    ));
                                    let req = mq_install::InstallRequest {
                                        mc_version: inst.mc_version.clone(),
                                        loader: inst.loader,
                                        loader_version: inst.loader_version.clone(),
                                        instance_dir: inst_dir.clone(),
                                        mock_base_url: configured_mock_base_url(),
                                    };

                                    match mq_install::install_game(
                                        &req,
                                        &paths,
                                        &net,
                                        &cas,
                                        &pool,
                                        &guard,
                                        Some(&bus),
                                    )
                                    .await
                                    {
                                        Ok(summary) => {
                                            bus.publish(mq_core::Event::PhaseCompleted {
                                                phase: "install_game".to_string(),
                                                elapsed_ms: summary.elapsed_ms,
                                            });
                                            let _ = resp_tx.send(
                                                UiWorkerResponse::InstanceInstallCompleted(
                                                    instance_id.clone(),
                                                ),
                                            );
                                        }
                                        Err(e) => {
                                            let err_msg = format!(
                                                "Ошибка скачивания инстанса {}: {e}",
                                                inst.name
                                            );
                                            bus.publish(mq_core::Event::TaskError {
                                                context: format!("Установка {}", inst.name),
                                                message: e.to_string(),
                                            });
                                            let _ = resp_tx.send(UiWorkerResponse::InstanceError {
                                                instance_id,
                                                error: err_msg,
                                            });
                                            return;
                                        }
                                    }
                                }

                                // 2. Read resolved version metadata
                                let bytes = match std::fs::read(&resolved_path) {
                                    Ok(b) => b,
                                    Err(e) => {
                                        let err_msg = format!(
                                            "Не удалось прочитать resolved_version.json: {e}"
                                        );
                                        let _ = resp_tx.send(UiWorkerResponse::InstanceError {
                                            instance_id,
                                            error: err_msg,
                                        });
                                        return;
                                    }
                                };

                                let ver = match serde_json::from_slice::<mq_meta::ResolvedVersion>(
                                    &bytes,
                                ) {
                                    Ok(v) => v,
                                    Err(e) => {
                                        let err_msg =
                                            format!("Ошибка структуры resolved_version.json: {e}");
                                        let _ = resp_tx.send(UiWorkerResponse::InstanceError {
                                            instance_id,
                                            error: err_msg,
                                        });
                                        return;
                                    }
                                };

                                // 3. Ensure Ely.by authlib injector if Ely.by account is active
                                let authlib_path = if active_acc.kind == mq_auth::AccountKind::ElyBy
                                {
                                    let client = mq_auth::ElyByClient::new();
                                    match client.ensure_authlib_injector(&paths).await {
                                        Ok(p) => Some(p),
                                        Err(e) => {
                                            bus.publish(mq_core::Event::TaskError {
                                                context: "Ely.by Authlib".to_string(),
                                                message: format!(
                                                    "Не удалось загрузить скин-инжектор: {e}"
                                                ),
                                            });
                                            None
                                        }
                                    }
                                } else {
                                    None
                                };

                                // 4. Discover and select compatible Java Virtual Machine
                                let java_runtimes = mq_launch::discover_java_runtimes();
                                let req_major = mq_launch::required_java_major_for_mc(
                                    &ver.mc_version,
                                    Some(ver.java_version.major_version),
                                );

                                let selected_java = match mq_launch::select_java_runtime(
                                    &java_runtimes,
                                    req_major,
                                    inst.java_path.as_deref(),
                                ) {
                                    Ok(j) => j,
                                    Err(e) => {
                                        let err_msg = format!(
                                        "Подходящая версия Java не найдена (требуется Java {req_major}): {e}"
                                    );
                                        bus.publish(mq_core::Event::TaskError {
                                            context: "Поиск Java".to_string(),
                                            message: err_msg.clone(),
                                        });
                                        let _ = resp_tx.send(UiWorkerResponse::InstanceError {
                                            instance_id,
                                            error: err_msg,
                                        });
                                        return;
                                    }
                                };

                                // 5. Construct launch context and arguments
                                let launch_ctx = mq_launch::LaunchContext {
                                    paths,
                                    instance: inst.clone(),
                                    version: ver,
                                    account: active_acc,
                                    java_bin: selected_java.path.clone(),
                                    authlib_injector_jar: authlib_path,
                                    extra_secrets_to_redact: Vec::new(),
                                };

                                let built_cmd = match mq_launch::build_launch_command(&launch_ctx) {
                                    Ok(cmd) => cmd,
                                    Err(e) => {
                                        let err_msg =
                                            format!("Ошибка построения аргументов JVM: {e}");
                                        bus.publish(mq_core::Event::TaskError {
                                            context: "Запуск JVM".to_string(),
                                            message: err_msg.clone(),
                                        });
                                        let _ = resp_tx.send(UiWorkerResponse::InstanceError {
                                            instance_id,
                                            error: err_msg,
                                        });
                                        return;
                                    }
                                };

                                if dry_run {
                                    bus.publish(mq_core::Event::GameLog(mq_core::GameLogEntry {
                                        instance: instance_id.to_string(),
                                        timestamp_ms: chrono::Utc::now().timestamp_millis(),
                                        level: mq_core::GameLogLevel::Info,
                                        line: format!(
                                            "DRY RUN COMMAND:\n{}",
                                            built_cmd.to_shell_command_line()
                                        ),
                                    }));
                                } else {
                                    let _ = resp_tx
                                        .send(UiWorkerResponse::GameStarted(instance_id.clone()));

                                    // Ensure valid assets/indexes/<id>.json and lang assets before game starts
                                    let _ = mq_launch::ensure_prelaunch_assets(
                                        &launch_ctx.paths,
                                        &inst_dir,
                                        &launch_ctx.version,
                                    )
                                    .await;

                                    // Auto-synchronize options.txt language
                                    let target_mc_lang = inst
                                        .mc_locale
                                        .as_deref()
                                        .unwrap_or(default_mc_loc.as_str());
                                    let _ = mq_launch::sync_minecraft_options_lang(
                                        &inst_dir,
                                        target_mc_lang,
                                    );

                                    bus.publish(mq_core::Event::GameLog(mq_core::GameLogEntry {
                                        instance: instance_id.to_string(),
                                        timestamp_ms: chrono::Utc::now().timestamp_millis(),
                                        level: mq_core::GameLogLevel::Info,
                                        line: format!(
                                            "[MQ] Запуск игры '{}' (MC {}, Java {}, Язык: {})...",
                                            inst.name,
                                            inst.mc_version,
                                            selected_java.full_version,
                                            target_mc_lang
                                        ),
                                    }));

                                    let exit_code = match mq_launch::spawn_and_stream_game(
                                        &built_cmd,
                                        instance_id.as_str(),
                                        Vec::new(),
                                        Some(bus.clone()),
                                    )
                                    .await
                                    {
                                        Ok(status) => status.code().unwrap_or(0),
                                        Err(e) => {
                                            bus.publish(mq_core::Event::TaskError {
                                                context: "Процесс игры".to_string(),
                                                message: format!("Ошибка процесса: {e}"),
                                            });
                                            -1
                                        }
                                    };

                                    let _ = resp_tx.send(UiWorkerResponse::GameExited {
                                        instance_id,
                                        exit_code,
                                    });
                                }
                            },
                        );
                    }
                }
                UiWorkerCommand::SearchCatalog {
                    request_id,
                    latest,
                    source,
                    query,
                } => {
                    let bus = self.state.event_bus.clone();
                    let config = self.state.config.clone();
                    let resp_tx = self.state.resp_sender.clone();
                    tokio::spawn(async move {
                        let started = std::time::Instant::now();
                        let page = crate::catalog_worker::run_search(
                            &config, request_id, &latest, source, &query,
                        )
                        .await;
                        // Cancelled by a newer request: drop silently.
                        let Some(page) = page else { return };
                        let _ = resp_tx.send(UiWorkerResponse::CatalogResults(page));
                        bus.publish(mq_core::Event::PhaseCompleted {
                            phase: "catalog_search".to_string(),
                            elapsed_ms: started.elapsed().as_millis() as u64,
                        });
                    });
                }
                UiWorkerCommand::FetchIcon { url, edge } => {
                    let resp_tx = self.state.resp_sender.clone();
                    let cache_dir = self.state.paths.cache_dir.join("catalog-icons");
                    tokio::spawn(async move {
                        let image = crate::catalog_worker::load_icon(&cache_dir, &url, edge).await;
                        let _ = resp_tx.send(UiWorkerResponse::IconLoaded { url, image });
                    });
                }
                UiWorkerCommand::LoadProject {
                    request_id,
                    source,
                    project_id,
                    kind,
                } => {
                    let config = self.state.config.clone();
                    let resp_tx = self.state.resp_sender.clone();
                    tokio::spawn(async move {
                        let result =
                            crate::catalog_ops::load_project(&config, source, &project_id, kind)
                                .await;
                        let _ = resp_tx.send(UiWorkerResponse::ProjectLoaded {
                            request_id,
                            result: result.map(Box::new),
                        });
                    });
                }
                UiWorkerCommand::PlanInstall { request_id, req } => {
                    let config = self.state.config.clone();
                    let resp_tx = self.state.resp_sender.clone();
                    tokio::spawn(async move {
                        let result = crate::catalog_ops::plan_content(&config, &req).await;
                        let _ = resp_tx.send(UiWorkerResponse::PlanReady { request_id, result });
                    });
                }
                UiWorkerCommand::PrepareModpack {
                    request_id,
                    version,
                } => {
                    let ctx = self.ops_ctx();
                    let resp_tx = self.state.resp_sender.clone();
                    tokio::spawn(async move {
                        let result = crate::catalog_ops::prepare_modpack(&ctx, &version).await;
                        let _ =
                            resp_tx.send(UiWorkerResponse::ModpackPrepared { request_id, result });
                    });
                }
                UiWorkerCommand::CheckUpdates {
                    request_id,
                    instance_id,
                    mc_version,
                    loader,
                } => {
                    let config = self.state.config.clone();
                    let dir = self.state.paths.instance_dir(instance_id.as_str());
                    let resp_tx = self.state.resp_sender.clone();
                    tokio::spawn(async move {
                        let result =
                            crate::catalog_ops::check_updates(&config, &dir, &mc_version, loader)
                                .await;
                        let _ =
                            resp_tx.send(UiWorkerResponse::UpdatesChecked { request_id, result });
                    });
                }
                UiWorkerCommand::ApplyUpdate {
                    job_id,
                    instance_id,
                    candidate,
                } => {
                    let ctx = self.ops_ctx();
                    let resp_tx = self.state.resp_sender.clone();
                    tokio::spawn(async move {
                        let result =
                            crate::catalog_ops::apply_update(&ctx, &instance_id, &candidate)
                                .await
                                .map(|()| crate::state::JobOutput {
                                    instance_id: Some(instance_id.clone()),
                                    blocked: Vec::new(),
                                });
                        let _ = resp_tx.send(UiWorkerResponse::JobFinished { job_id, result });
                    });
                }
                UiWorkerCommand::PreviewLocalPack { request_id, path } => {
                    let resp_tx = self.state.resp_sender.clone();
                    tokio::spawn(async move {
                        let result = tokio::task::spawn_blocking(move || {
                            crate::catalog_ops::preview_archive(&path)
                        })
                        .await
                        .map_err(|e| e.to_string())
                        .and_then(|r| r);
                        let _ =
                            resp_tx.send(UiWorkerResponse::ModpackPrepared { request_id, result });
                    });
                }
                UiWorkerCommand::StartContentJob {
                    job_id,
                    cancel,
                    instance_id,
                    kind,
                    source,
                    downloads,
                    blocked,
                } => {
                    let ctx = self.ops_ctx();
                    let resp_tx = self.state.resp_sender.clone();
                    tokio::spawn(async move {
                        let tx = resp_tx.clone();
                        let result = crate::catalog_ops::run_content_job(
                            &ctx,
                            &cancel,
                            &instance_id,
                            kind,
                            source,
                            &downloads,
                            move |done, total| {
                                let _ = tx.send(UiWorkerResponse::JobProgress {
                                    job_id,
                                    done,
                                    total,
                                });
                            },
                        )
                        .await
                        .map(|()| crate::state::JobOutput {
                            instance_id: Some(instance_id.clone()),
                            blocked,
                        });
                        let _ = resp_tx.send(UiWorkerResponse::JobFinished { job_id, result });
                    });
                }
                UiWorkerCommand::StartModpackJob {
                    job_id,
                    cancel,
                    instance_name,
                    preview,
                } => {
                    let ctx = self.ops_ctx();
                    let resp_tx = self.state.resp_sender.clone();
                    tokio::spawn(async move {
                        let result = crate::catalog_ops::run_modpack_job(
                            &ctx,
                            &cancel,
                            &instance_name,
                            &preview,
                        )
                        .await
                        .map(|(id, blocked)| crate::state::JobOutput {
                            instance_id: Some(id),
                            blocked,
                        });
                        let _ = resp_tx.send(UiWorkerResponse::JobFinished { job_id, result });
                    });
                }
                UiWorkerCommand::PollManual { dropped } => {
                    if let Some(watcher) = self.state.manual_watcher.clone() {
                        let resp_tx = self.state.resp_sender.clone();
                        tokio::spawn(async move {
                            let joined = tokio::task::spawn_blocking(move || {
                                crate::catalog_ops::poll_watcher(&watcher, &dropped)
                            })
                            .await;
                            if let Ok((pending, newly_done)) = joined {
                                let _ = resp_tx.send(UiWorkerResponse::ManualProgress {
                                    pending,
                                    newly_done,
                                });
                            }
                        });
                    }
                }
                _ => {}
            }
        }
    }

    /// Handles shared with background catalog operations.
    fn ops_ctx(&self) -> crate::catalog_ops::OpsCtx {
        crate::catalog_ops::OpsCtx {
            config: self.state.config.clone(),
            paths: self.state.paths.clone(),
            net: self.state.net.clone(),
            epoch: self.state.cancel_epoch.clone(),
            pool: self.state.compute_pool.clone(),
            bus: self.state.event_bus.clone(),
        }
    }

    /// Headless/offscreen frame rendering for visual QA and snapshots.
    pub fn render_frame_for_snapshot(&mut self, ctx: &egui::Context) {
        let accounts_list = self.state.accounts.list_accounts().to_vec();
        let active_id = self
            .state
            .accounts
            .active_account()
            .map(|a| a.id.to_string());

        let launcher_locale = self.state.config.launcher_locale.clone();
        let tokens = Tokens::from_config(&self.state.config.theme);

        SidePanel::left("mq_sidebar")
            .resizable(false)
            .exact_width(sidebar_width(ctx))
            .frame(solid_sidebar_frame(&tokens))
            .show(ctx, |ui| {
                let _ = render_sidebar(
                    ui,
                    self.current_tab,
                    &accounts_list,
                    active_id.as_deref(),
                    &launcher_locale,
                    &tokens,
                );
            });

        let hud_data = BottomHudData {
            active_phase: self.state.active_phase.clone(),
            progress_fraction: self
                .state
                .phase_progress
                .as_ref()
                .map(|p| crate::state::safe_progress_ratio(p.completed_items, p.total_items)),
            files_per_sec: self
                .state
                .phase_progress
                .as_ref()
                .map(|p| p.files_per_sec)
                .unwrap_or(0.0),
            mib_per_sec: self
                .state
                .phase_progress
                .as_ref()
                .map(|p| p.mib_per_sec)
                .unwrap_or(0.0),
            active_dag_tasks: self
                .state
                .phase_progress
                .as_ref()
                .map(|p| p.active_tasks)
                .unwrap_or(0),
            cpu_snapshot: self.state.latest_cpu.clone(),
            watchdog_violations: self.state.watchdog.violation_count(),
        };

        TopBottomPanel::bottom("mq_hud").show(ctx, |ui| {
            render_bottom_hud(ui, &hud_data);
        });

        CentralPanel::default().show(ctx, |ui| match self.current_tab {
            NavTab::Instances => self.instances_view.ui(ui, &mut self.state),
            NavTab::Catalog => self.catalog_view.ui(ui, &mut self.state),
            NavTab::Accounts => self.accounts_view.ui(ui, &mut self.state),
            NavTab::Performance => self.perf_view.ui(ui, &self.state),
            NavTab::Settings => self.settings_view.ui(ui, &mut self.state),
        });
    }
    #[cfg(feature = "test-hooks")]
    pub fn step_test_hooks(&mut self, ctx: &egui::Context) {
        let hook_id = egui::Id::new("mq_test_hook_state");
        let mut hook = ctx.data_mut(|d| {
            d.get_temp::<TestHookRuntimeState>(hook_id)
                .unwrap_or_default()
        });

        if !hook.initialized {
            hook.initialized = true;
            hook.cancel_after_ms = std::env::var("MQ_TEST_CANCEL_AFTER_MS")
                .ok()
                .and_then(|v| v.trim().parse::<u64>().ok());
            hook.exit_on_finish = std::env::var("MQ_TEST_EXIT_ON_FINISH")
                .ok()
                .map(|v| matches!(v.trim().to_ascii_lowercase().as_str(), "1" | "true" | "yes"))
                .unwrap_or(false);

            if let Ok(raw_action) = std::env::var("MQ_TEST_ACTION") {
                if let Some(TestAction::Install { mc_version, loader }) =
                    parse_test_action(&raw_action)
                {
                    let existing_id = self
                        .state
                        .instances
                        .iter()
                        .find(|i| i.mc_version == mc_version && i.loader == loader)
                        .map(|i| i.id.clone());

                    let inst_id_opt = if let Some(id) = existing_id {
                        Some(id)
                    } else {
                        let inst_name = format!("{} {}", mc_version, loader.as_str());
                        match self.state.instance_mgr.create_instance(
                            &inst_name,
                            &mc_version,
                            loader,
                            None,
                        ) {
                            Ok(mut manifest) => {
                                manifest.memory_max_mb = self.state.create_inst_ram_mb;
                                let _ = self.state.instance_mgr.save_instance(&manifest);
                                let id = manifest.id.clone();
                                self.state.instances.push(manifest);
                                Some(id)
                            }
                            Err(e) => {
                                tracing::error!("MQ_TEST_ACTION failed to create instance: {e}");
                                self.state.status_message = Some((
                                    format!("Ошибка создания инстанса (MQ_TEST_ACTION): {e}"),
                                    true,
                                ));
                                None
                            }
                        }
                    };

                    if let Some(inst_id) = inst_id_opt {
                        self.state.selected_instance_id = Some(inst_id.clone());
                        self.state.installing_instance = Some(inst_id.clone());
                        self.state.install_error = None;
                        self.state.detailed_install_error = None;
                        self.state.current_install_stage =
                            crate::install_types::InstallStage::Manifest;
                        self.state.install_retries_count = 0;
                        self.state.install_eta_secs = None;
                        self.state.status_message = Some((
                            format!(
                                "Инстанс '{}' выбран. Запущена автоматическая установка...",
                                inst_id
                            ),
                            false,
                        ));
                        let _ = self
                            .state
                            .cmd_sender
                            .send(UiWorkerCommand::InstallInstance {
                                instance_id: inst_id,
                            });
                        hook.action_active = true;
                        hook.started_at = Some(std::time::Instant::now());
                    }
                }
            }
        }

        if hook.action_active {
            if let (Some(cancel_ms), Some(started_at)) = (hook.cancel_after_ms, hook.started_at) {
                if !hook.cancel_sent
                    && started_at.elapsed() >= std::time::Duration::from_millis(cancel_ms)
                {
                    hook.cancel_sent = true;
                    let _ = self.state.cmd_sender.send(UiWorkerCommand::CancelDownload);
                }
            }
        }

        ctx.data_mut(|d| d.insert_temp(hook_id, hook));
    }

    #[cfg(feature = "test-hooks")]
    fn post_frame_test_hooks(&mut self, ctx: &egui::Context) {
        let hook_id = egui::Id::new("mq_test_hook_state");
        let mut hook = ctx.data_mut(|d| {
            d.get_temp::<TestHookRuntimeState>(hook_id)
                .unwrap_or_default()
        });

        if hook.action_active {
            ctx.request_repaint();
            let waiting_for_cancel = hook.cancel_after_ms.is_some() && !hook.cancel_sent;
            if self.state.installing_instance.is_none() && !waiting_for_cancel {
                hook.finish_frames = hook.finish_frames.saturating_add(1);
                if hook.exit_on_finish && hook.finish_frames >= 3 {
                    hook.action_active = false;
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            }
        }

        ctx.data_mut(|d| d.insert_temp(hook_id, hook));
    }

    /// Renders recovery dialog if previous launcher run terminated unexpectedly with an active install journal.
    pub fn render_recovery_dialog(&mut self, ctx: &egui::Context) {
        let Some(journal) = &self.recovery_journal else {
            return;
        };

        let tokens = Tokens::from_config(&self.state.config.theme);
        let mut close_dialog = false;
        let mut action_continue = false;
        let mut action_clean_reinstall = false;
        let mut action_copy_diag = false;

        let version_str = journal.mc_version.clone();
        let stage_str = journal.stage.clone();
        let inst_id_str = journal.instance_id.clone();
        let diag_text = journal.format_diagnostic_summary(&self.state.paths);

        egui::Window::new("Восстановление после аварийного завершения")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .frame(
                egui::Frame::new()
                    .fill(tokens.bg_surface)
                    .corner_radius(egui::CornerRadius::same(8))
                    .stroke(egui::Stroke::new(1.0_f32, tokens.border))
                    .inner_margin(egui::Margin::same(20)),
            )
            .show(ctx, |ui| {
                ui.set_max_width(540.0);
                ui.vertical(|ui| {
                    ui.label(
                        egui::RichText::new("Обнаружено прерывание предыдущей установки")
                            .font(egui::FontId::proportional(16.0))
                            .color(tokens.warning),
                    );
                    ui.add_space(8.0);
                    let prompt_text = format!(
                        "В прошлый раз лаунчер завершился аварийно при установке {}, этап {}",
                        version_str, stage_str
                    );
                    ui.label(
                        egui::RichText::new(prompt_text)
                            .font(egui::FontId::proportional(13.5))
                            .color(tokens.text),
                    );
                    ui.add_space(18.0);

                    ui.horizontal_wrapped(|ui| {
                        ui.spacing_mut().item_spacing = egui::Vec2::new(10.0, 8.0);
                        if crate::components::button_primary(ui, &tokens, None, "Продолжить")
                            .on_hover_text("Возобновить процесс установки")
                            .clicked()
                        {
                            action_continue = true;
                            close_dialog = true;
                        }

                        if crate::components::button_secondary(
                            ui,
                            &tokens,
                            None,
                            "Очистить и начать заново",
                        )
                        .on_hover_text(
                            "Удалить повреждённые временные файлы и начать чистую установку",
                        )
                        .clicked()
                        {
                            action_clean_reinstall = true;
                            close_dialog = true;
                        }

                        if crate::components::button_secondary(
                            ui,
                            &tokens,
                            None,
                            "Скопировать диагностику",
                        )
                        .on_hover_text("Скопировать отчёт об аварии в буфер обмена")
                        .clicked()
                        {
                            action_copy_diag = true;
                            close_dialog = true;
                        }
                    });
                });
            });

        if action_copy_diag {
            ctx.copy_text(diag_text);
            self.state.status_message = Some((
                "Диагностический отчёт скопирован в буфер обмена".to_string(),
                false,
            ));
        }

        if action_continue {
            if let Ok(inst_id) = mq_core::InstanceId::new(&inst_id_str) {
                self.state.installing_instance = Some(inst_id.clone());
                let _ = self
                    .state
                    .cmd_sender
                    .send(UiWorkerCommand::InstallInstance {
                        instance_id: inst_id,
                    });
            }
        }

        if action_clean_reinstall {
            if let Ok(inst_id) = mq_core::InstanceId::new(&inst_id_str) {
                self.state.installing_instance = Some(inst_id.clone());
                let inst_dir = self.state.paths.instance_dir(inst_id.as_str());
                let _ = std::fs::remove_dir_all(inst_dir.join(".installing"));
                let _ = std::fs::remove_dir_all(inst_dir.join("natives"));
                let _ = std::fs::remove_file(inst_dir.join("client.jar"));
                let _ = std::fs::remove_file(inst_dir.join("resolved_version.json"));
                let _ = std::fs::remove_file(inst_dir.join("install_state.json"));
                let _ = self
                    .state
                    .cmd_sender
                    .send(UiWorkerCommand::ReinstallInstance {
                        instance_id: inst_id,
                    });
            }
        }

        if close_dialog {
            let _ = mq_core::crash::InstallJournal::mark_closed_at(&self.state.paths);
            self.recovery_journal = None;
        }
    }

    /// Internal per-frame rendering logic executed under `catch_unwind`.
    fn render_frame_inner(&mut self, ctx: &egui::Context) {
        #[cfg(feature = "test-hooks")]
        self.step_test_hooks(ctx);

        // Drain background worker events
        self.state.drain_events();
        self.handle_worker_commands();

        if let Some(rx) = self.state.mrpack_ipc_receiver.clone() {
            let mut received = Vec::new();
            if let Ok(guard) = rx.lock() {
                while let Ok(path) = guard.try_recv() {
                    received.push(path);
                }
            }
            if !received.is_empty() {
                for path in received {
                    self.state.import_local_pack(path);
                }
                self.current_tab = NavTab::Catalog;
                ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
            }
        }

        // 1. Sidebar (Left)
        let accounts_list = self.state.accounts.list_accounts().to_vec();
        let active_id = self
            .state
            .accounts
            .active_account()
            .map(|a| a.id.to_string());

        let mut sidebar_action = None;
        let launcher_locale = self.state.config.launcher_locale.clone();
        let tokens = Tokens::from_config(&self.state.config.theme);
        SidePanel::left("mq_sidebar")
            .resizable(false)
            .exact_width(sidebar_width(ctx))
            .frame(solid_sidebar_frame(&tokens))
            .show(ctx, |ui| {
                sidebar_action = render_sidebar(
                    ui,
                    self.current_tab,
                    &accounts_list,
                    active_id.as_deref(),
                    &launcher_locale,
                    &tokens,
                );
            });

        if let Some(act) = sidebar_action {
            match act {
                SidebarAction::SelectTab(t) => self.current_tab = t,
                SidebarAction::SwitchAccount(id) => {
                    let _ = self.state.accounts.select_account(&id);
                    let _ = self.state.accounts.save(&self.state.paths);
                }
                SidebarAction::OpenAccountsTab => self.current_tab = NavTab::Accounts,
                SidebarAction::ChangeLanguage(code) => {
                    self.state.config.launcher_locale = code;
                    let _ = self.state.config.save(&self.state.paths);
                }
            }
        }

        // 2. Bottom HUD (Bottom)
        let hud_data = BottomHudData {
            active_phase: self.state.active_phase.clone(),
            progress_fraction: self
                .state
                .phase_progress
                .as_ref()
                .map(|p| crate::state::safe_progress_ratio(p.completed_items, p.total_items)),
            files_per_sec: self
                .state
                .phase_progress
                .as_ref()
                .map(|p| p.files_per_sec)
                .unwrap_or(0.0),
            mib_per_sec: self
                .state
                .phase_progress
                .as_ref()
                .map(|p| p.mib_per_sec)
                .unwrap_or(0.0),
            active_dag_tasks: self
                .state
                .phase_progress
                .as_ref()
                .map(|p| p.active_tasks)
                .unwrap_or(0),
            cpu_snapshot: self.state.latest_cpu.clone(),
            watchdog_violations: self.state.watchdog.violation_count(),
        };

        TopBottomPanel::bottom("mq_hud").show(ctx, |ui| {
            render_bottom_hud(ui, &hud_data);
        });

        // 3. Central Panel (Active view)
        CentralPanel::default().show(ctx, |ui| match self.current_tab {
            NavTab::Instances => self.instances_view.ui(ui, &mut self.state),
            NavTab::Catalog => self.catalog_view.ui(ui, &mut self.state),
            NavTab::Accounts => self.accounts_view.ui(ui, &mut self.state),
            NavTab::Performance => self.perf_view.ui(ui, &self.state),
            NavTab::Settings => self.settings_view.ui(ui, &mut self.state),
        });

        // 4. Modal Recovery Dialog (if previous installation terminated unexpectedly)
        self.render_recovery_dialog(ctx);

        // Request next frame repaint if an operation is active
        if self.state.active_phase.is_some() || self.state.game_running {
            ctx.request_repaint();
        }

        #[cfg(feature = "test-hooks")]
        self.post_frame_test_hooks(ctx);
    }

    /// Primary entry point for rendering a complete frame with catch_unwind and fallback screen.
    pub fn render_frame(&mut self, ctx: &egui::Context) {
        let watchdog = self.state.watchdog.clone();
        let _guard = watchdog.guard("mq_frame_update");

        // Safe 0x0 handling on tiling WMs like Hyprland
        if is_zero_viewport(ctx) {
            return;
        }

        // If a previous frame panic occurred, display the fallback recovery screen
        if let Some(err_msg) = &self.frame_crash_message {
            let tokens = Tokens::from_config(&self.state.config.theme);
            let mut reset_screen = false;
            let mut copy_report = false;
            let err_text = err_msg.clone();

            CentralPanel::default().show(ctx, |ui| {
                ui.vertical_centered(|ui| {
                    ui.add_space(60.0);
                    ui.label(
                        egui::RichText::new("Сбой при отображении интерфейса")
                            .font(egui::FontId::proportional(22.0))
                            .color(tokens.danger),
                    );
                    ui.add_space(16.0);
                    ui.label(
                        egui::RichText::new("Произошла ошибка при отрисовке кадра:")
                            .font(egui::FontId::proportional(14.0))
                            .color(tokens.text),
                    );
                    ui.add_space(12.0);

                    let err_frame = egui::Frame::new()
                        .fill(tokens.bg_surface)
                        .corner_radius(egui::CornerRadius::same(6))
                        .stroke(egui::Stroke::new(1.0_f32, tokens.border))
                        .inner_margin(egui::Margin::same(16));
                    err_frame.show(ui, |ui| {
                        ui.label(
                            egui::RichText::new(&err_text)
                                .font(egui::FontId::monospace(12.0))
                                .color(tokens.text_muted),
                        );
                    });
                    ui.add_space(24.0);

                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing = egui::Vec2::new(12.0, 0.0);
                        if crate::components::button_primary(ui, &tokens, None, "Сбросить экран")
                            .on_hover_text("Вернуться к стандартному экрану инстансов")
                            .clicked()
                        {
                            reset_screen = true;
                        }
                        if crate::components::button_secondary(
                            ui,
                            &tokens,
                            None,
                            "Скопировать отчёт",
                        )
                        .on_hover_text("Скопировать сведения об аварии в буфер обмена")
                        .clicked()
                        {
                            copy_report = true;
                        }
                    });
                });
            });

            if reset_screen {
                self.frame_crash_message = None;
                self.current_tab = NavTab::Instances;
            }
            if copy_report {
                let report = format!(
                    "=== Отчёт об аварии интерфейса MQ Launcher v{} ===\n{}\n",
                    env!("CARGO_PKG_VERSION"),
                    err_text
                );
                ctx.copy_text(report);
                self.state.status_message =
                    Some(("Отчёт скопирован в буфер обмена".to_string(), false));
            }
            return;
        }

        // Catch any panics during main frame rendering
        let panic_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            self.render_frame_inner(ctx);
        }));

        if let Err(panic_payload) = panic_result {
            let panic_msg = if let Some(s) = panic_payload.downcast_ref::<&str>() {
                (*s).to_string()
            } else if let Some(s) = panic_payload.downcast_ref::<String>() {
                s.clone()
            } else {
                "Неизвестная ошибка рендеринга кадра".to_string()
            };

            let backtrace = std::backtrace::Backtrace::capture().to_string();
            let _ = mq_core::crash::write_crash_report_for_panic(
                &self.state.paths,
                &panic_msg,
                None,
                "ui-render-thread",
                &backtrace,
            );

            self.frame_crash_message = Some(panic_msg);
            ctx.request_repaint();
        }
    }
}

impl eframe::App for MqApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.render_frame(ctx);
    }
}

/// Launches the desktop GUI window.
pub fn run_gui(state: UiState, backend: GuiBackend) -> Result<(), eframe::Error> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("MQ Launcher")
            .with_inner_size([1180.0, 740.0])
            .with_min_inner_size([960.0, 600.0])
            .with_transparent(false)
            .with_decorations(true),
        renderer: match backend {
            GuiBackend::Vulkan => eframe::Renderer::Wgpu,
            GuiBackend::Gl => eframe::Renderer::Glow,
            GuiBackend::Auto => eframe::Renderer::Wgpu,
        },
        ..Default::default()
    };

    eframe::run_native(
        "MQ Launcher",
        options,
        Box::new(|cc| Ok(Box::new(MqApp::new(cc, state)))),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use mq_core::ModLoaderKind;

    #[test]
    fn parse_test_action_supports_version_and_loader_variants() {
        assert_eq!(
            parse_test_action("install:26.3"),
            Some(TestAction::Install {
                mc_version: "26.3".to_string(),
                loader: ModLoaderKind::Vanilla,
            })
        );
        assert_eq!(
            parse_test_action("install:26.3:vanilla"),
            Some(TestAction::Install {
                mc_version: "26.3".to_string(),
                loader: ModLoaderKind::Vanilla,
            })
        );
        assert_eq!(
            parse_test_action("  install:1.20.1:fabric  "),
            Some(TestAction::Install {
                mc_version: "1.20.1".to_string(),
                loader: ModLoaderKind::Fabric,
            })
        );
        assert_eq!(
            parse_test_action("install:1.21.4:neoforge"),
            Some(TestAction::Install {
                mc_version: "1.21.4".to_string(),
                loader: ModLoaderKind::NeoForge,
            })
        );
        assert_eq!(parse_test_action(""), None);
        assert_eq!(parse_test_action("install:"), None);
        assert_eq!(parse_test_action("install:1.20.1:unknown"), None);
        assert_eq!(parse_test_action("install:1.20.1:fabric:extra"), None);
        assert_eq!(parse_test_action("launch:1.20.1"), None);
    }

    #[cfg(feature = "test-hooks")]
    #[test]
    fn step_test_hooks_creates_instance_and_queues_install_once() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let paths = mq_core::MqPaths::with_root(tmp.path().to_path_buf());
        let config = mq_core::AppConfig::default();
        let pool = std::sync::Arc::new(mq_sched::ComputePool::new(2));
        let state = UiState::new(paths, config, pool).expect("ui state");

        let mut app = MqApp::from_state(state);

        std::env::set_var("MQ_TEST_ACTION", "install:26.3:vanilla");
        let ctx = egui::Context::default();

        app.step_test_hooks(&ctx);
        assert_eq!(app.state.instances.len(), 1);
        assert_eq!(app.state.instances[0].mc_version, "26.3");
        assert_eq!(app.state.instances[0].loader, ModLoaderKind::Vanilla);
        let created_id = app.state.instances[0].id.clone();
        assert_eq!(app.state.selected_instance_id, Some(created_id.clone()));
        assert_eq!(app.state.installing_instance, Some(created_id.clone()));

        match app.state.cmd_receiver.try_recv() {
            Ok(UiWorkerCommand::InstallInstance { instance_id }) => {
                assert_eq!(instance_id, created_id);
            }
            other => panic!("expected InstallInstance command, got {:?}", other.is_ok()),
        }

        // Calling step_test_hooks a second time on the same Context must not duplicate the command
        app.step_test_hooks(&ctx);
        assert!(app.state.cmd_receiver.try_recv().is_err());

        std::env::remove_var("MQ_TEST_ACTION");
    }

    #[test]
    fn test_recovery_dialog_detects_unclosed_journal() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let paths = mq_core::MqPaths::with_root(tmp.path().to_path_buf());

        // Create an unclosed journal as if a previous launch panicked during client_jar stage
        let journal = mq_core::crash::InstallJournal::start(
            &paths,
            "inst-1",
            "1.21.1",
            "fabric",
            "client_jar",
        )
        .expect("start journal");
        assert!(journal.active);

        let config = mq_core::AppConfig::default();
        let pool = std::sync::Arc::new(mq_sched::ComputePool::new(2));
        let state = UiState::new(paths.clone(), config, pool).expect("ui state");

        let mut app = MqApp::from_state(state);
        assert!(app.recovery_journal.is_some());
        let rec = app.recovery_journal.as_ref().unwrap();
        assert_eq!(rec.mc_version, "1.21.1");
        assert_eq!(rec.stage, "client_jar");

        // Verify diagnostic summary formatting
        let diag = rec.format_diagnostic_summary(&paths);
        assert!(diag.contains("1.21.1"));
        assert!(diag.contains("client_jar"));

        // Render frame with recovery dialog active
        let ctx = egui::Context::default();
        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            app.render_recovery_dialog(ctx);
        });

        // Closing the journal manually closes it on disk
        mq_core::crash::InstallJournal::mark_closed_at(&paths).expect("mark closed");
        assert!(mq_core::crash::InstallJournal::load_unclosed(&paths).is_none());
    }

    #[test]
    fn test_frame_crash_fallback_screen_renders_safely() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let paths = mq_core::MqPaths::with_root(tmp.path().to_path_buf());
        let config = mq_core::AppConfig::default();
        let pool = std::sync::Arc::new(mq_sched::ComputePool::new(2));
        let state = UiState::new(paths, config, pool).expect("ui state");
        let mut app = MqApp::from_state(state);

        app.frame_crash_message = Some("Паника рендеринга кадра: division by zero".to_string());
        let ctx = egui::Context::default();

        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            app.render_frame(ctx);
        });

        // Resetting the crash screen
        app.frame_crash_message = None;
        assert!(app.frame_crash_message.is_none());
    }

    #[tokio::test]
    async fn test_spawn_monitored_task_catches_panic_and_reports_error() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let paths = mq_core::MqPaths::with_root(tmp.path().to_path_buf());
        let (resp_tx, resp_rx) = flume::bounded(16);
        let inst_id = mq_core::InstanceId::new("panicking-instance").expect("valid id");

        spawn_monitored_task(
            "test_panicking_task",
            paths.clone(),
            inst_id.clone(),
            resp_tx,
            async move {
                panic!("Умышленная ошибка внутри фоновой задачи");
            },
        );

        // Await the worker error response
        let resp = tokio::time::timeout(std::time::Duration::from_secs(3), resp_rx.recv_async())
            .await
            .expect("timeout waiting for monitored task response")
            .expect("channel recv error");

        match resp {
            UiWorkerResponse::InstanceError { instance_id, error } => {
                assert_eq!(instance_id, inst_id);
                assert!(
                    error.contains("Установка прервана из-за внутренней ошибки"),
                    "unexpected error format: {error}"
                );
                assert!(
                    error.contains("Умышленная ошибка внутри фоновой задачи"),
                    "panic message not retained: {error}"
                );
            }
            _other => panic!("expected InstanceError, got response"),
        }

        // Verify crash log was written
        let crash_latest = paths.logs_dir().join("crash-latest.log");
        assert!(
            crash_latest.is_file(),
            "crash-latest.log should be created on panic"
        );
        let crash_text = std::fs::read_to_string(&crash_latest).expect("read crash log");
        assert!(crash_text.contains("Умышленная ошибка внутри фоновой задачи"));
    }
}
