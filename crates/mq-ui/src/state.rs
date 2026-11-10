#![deny(clippy::await_holding_lock)]
//! Shared application state and background worker coordinator for `mq-ui`.
//!
//! Enforces non-blocking UI design (Lesson E1):
//! - UI thread NEVER performs network/disk I/O, hash verification, or JSON/ZIP parsing.
//! - Background tasks communicate via bounded `flume` channels.
//! - Frame times are monitored by `UiWatchdog` (>8 ms logs warning).

use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::sync::Arc;

use mq_auth::AccountStore;
use mq_core::{
    AppConfig, CpuSnapshot, Event, EventBus, GameLogEntry, GameLogLevel, InstanceId, ModLoaderKind,
    MqPaths, MqResult, PhaseProgress,
};
use mq_install::CasStore;
use mq_instances::{InstanceManager, InstanceManifest};
use mq_mods::{InstalledModEntry, ModConflict, SearchQuery};
use mq_net::NetEngine;
use mq_sched::{CancelEpoch, ComputePool, UiWatchdog};

use crate::install_types::{InstallError, InstallStage};

/// Max lines retained in memory for live game log console (10,000 lines).
pub const MAX_GAME_LOG_LINES: usize = 10000;

/// Background responses delivered back to UI thread without blocking.
pub enum UiWorkerResponse {
    CatalogResults(crate::catalog_state::CatalogPage),
    IconLoaded {
        url: String,
        image: Option<egui::ColorImage>,
    },
    ProjectLoaded {
        request_id: u64,
        result: Result<Box<crate::catalog_flow::ProjectPage>, String>,
    },
    PlanReady {
        request_id: u64,
        result: Result<crate::catalog_ops::PlanOutcome, String>,
    },
    ModpackPrepared {
        request_id: u64,
        result: Result<crate::catalog_flow::ModpackPreview, String>,
    },
    JobProgress {
        job_id: u64,
        done: usize,
        total: usize,
    },
    JobFinished {
        job_id: u64,
        result: Result<JobOutput, String>,
    },
    UpdatesChecked {
        request_id: u64,
        result: Result<Vec<crate::catalog_flow::UpdateCandidate>, String>,
    },
    ManualProgress {
        pending: Vec<mq_mods::BlockedDownload>,
        newly_done: Vec<String>,
    },
    ScanModsResults {
        mods: Vec<InstalledModEntry>,
        conflicts: Vec<ModConflict>,
    },
    UnusedDepsCleaned {
        removed: usize,
        error: Option<String>,
    },
    VersionManifestFetched(Vec<(String, String)>),
    InstanceInstallStarted(InstanceId),
    InstanceInstallCompleted(InstanceId),
    InstanceError {
        instance_id: InstanceId,
        error: String,
    },
    GameStarted(InstanceId),
    GameExited {
        instance_id: InstanceId,
        exit_code: i32,
    },
}

/// Successful result of an install job.
pub struct JobOutput {
    /// Instance the content ended up in (new instance for modpacks).
    pub instance_id: Option<InstanceId>,
    /// Files the user still has to download by hand.
    pub blocked: Vec<mq_mods::BlockedDownload>,
}

/// Background task messages sent from UI to worker threads.
pub enum UiWorkerCommand {
    FetchVersionManifest,
    RefreshInstances,
    RefreshAccounts,
    SearchCatalog {
        request_id: u64,
        latest: Arc<std::sync::atomic::AtomicU64>,
        source: mq_mods::CatalogSourceFilter,
        query: SearchQuery,
    },
    FetchIcon {
        url: String,
        edge: u32,
    },
    LoadProject {
        request_id: u64,
        source: mq_mods::CatalogSource,
        project_id: String,
        kind: mq_mods::ProjectKind,
    },
    PlanInstall {
        request_id: u64,
        req: crate::catalog_ops::PlanRequest,
    },
    PrepareModpack {
        request_id: u64,
        version: mq_mods::ProjectVersion,
    },
    CheckUpdates {
        request_id: u64,
        instance_id: InstanceId,
        mc_version: String,
        loader: ModLoaderKind,
    },
    ApplyUpdate {
        job_id: u64,
        instance_id: InstanceId,
        candidate: Box<crate::catalog_flow::UpdateCandidate>,
    },
    PreviewLocalPack {
        request_id: u64,
        path: std::path::PathBuf,
    },
    StartContentJob {
        job_id: u64,
        cancel: Arc<std::sync::atomic::AtomicBool>,
        instance_id: InstanceId,
        kind: mq_mods::ProjectKind,
        source: mq_mods::CatalogSource,
        downloads: Vec<mq_mods::ResolvedModDownload>,
        blocked: Vec<mq_mods::BlockedDownload>,
    },
    StartModpackJob {
        job_id: u64,
        cancel: Arc<std::sync::atomic::AtomicBool>,
        instance_name: String,
        preview: crate::catalog_flow::ModpackPreview,
    },
    PollManual {
        dropped: Vec<std::path::PathBuf>,
    },
    InstallMod {
        instance_id: InstanceId,
        project_id_or_slug: String,
        mc_version: String,
        loader: ModLoaderKind,
    },
    InstallModpack {
        path_or_slug: String,
        instance_name: Option<String>,
    },
    ScanInstanceMods {
        instance_id: InstanceId,
    },
    CleanupUnusedDeps {
        instance_id: InstanceId,
    },
    ToggleMod {
        instance_id: InstanceId,
        file_name: String,
    },
    DeleteMod {
        instance_id: InstanceId,
        file_name: String,
    },
    VerifyInstance {
        instance_id: InstanceId,
    },
    VerifyAndRepairInstance {
        instance_id: InstanceId,
    },
    ReinstallInstance {
        instance_id: InstanceId,
    },
    InstallInstance {
        instance_id: InstanceId,
    },
    CancelDownload,
    LaunchInstance {
        instance_id: InstanceId,
        dry_run: bool,
    },
}

/// Central state model consumed by `eframe::App` in `mq-ui`.
pub struct UiState {
    pub paths: MqPaths,
    pub config: AppConfig,
    pub accounts: AccountStore,
    pub instances: Vec<InstanceManifest>,
    pub selected_instance_id: Option<InstanceId>,

    // Subsystems
    pub instance_mgr: InstanceManager,
    pub net: Arc<NetEngine>,
    pub cas: CasStore,
    pub compute_pool: Arc<ComputePool>,
    pub cancel_epoch: Arc<CancelEpoch>,
    pub event_bus: EventBus,
    pub watchdog: Arc<UiWatchdog>,

    // Channels
    pub cmd_sender: flume::Sender<UiWorkerCommand>,
    pub cmd_receiver: flume::Receiver<UiWorkerCommand>,
    pub resp_sender: flume::Sender<UiWorkerResponse>,
    pub resp_receiver: flume::Receiver<UiWorkerResponse>,

    // Telemetry & Status
    pub latest_cpu: CpuSnapshot,
    pub active_phase: Option<String>,
    pub phase_progress: Option<PhaseProgress>,
    pub status_message: Option<(String, bool)>, // (msg, is_error)

    // Game Console & Execution
    pub game_running: bool,
    pub running_instance: Option<InstanceId>,
    pub is_launching: bool,
    pub installing_instance: Option<InstanceId>,
    pub install_error: Option<String>,
    pub detailed_install_error: Option<InstallError>,
    pub current_install_stage: InstallStage,
    pub install_retries_count: u32,
    pub install_eta_secs: Option<u64>,
    pub last_game_exit_code: Option<i32>,
    pub game_logs: VecDeque<GameLogEntry>,
    pub log_filter: Option<GameLogLevel>,

    // Mods Screen
    pub installed_mods: Vec<InstalledModEntry>,
    pub mod_conflicts: Vec<ModConflict>,
    pub is_scanning_mods: bool,

    // Catalog Screen
    pub catalog: crate::catalog_state::CatalogState,
    pub project: crate::catalog_flow::ProjectViewState,
    pub install: Option<crate::catalog_flow::InstallDialog>,
    pub jobs: crate::catalog_flow::JobQueue,
    pub updates: crate::catalog_flow::UpdatesState,
    pub manual: crate::catalog_flow::ManualDownloadState,
    pub manual_watcher: Option<crate::catalog_ops::SharedWatcher>,
    pub install_seq: u64,

    // Dialogs & Versions
    pub show_create_instance_dialog: bool,
    pub create_inst_name: String,
    pub create_inst_mc_ver: String,
    pub create_inst_loader: ModLoaderKind,
    pub create_inst_ram_mb: u32,
    pub remote_versions: Vec<(String, String)>, // (id, release_type)
    pub is_fetching_manifest: bool,

    // Crash Diagnostics
    pub crash_analysis: Option<mq_launch::CrashAnalysis>,
    pub mclogs_url: Option<String>,
    pub is_uploading_mclogs: bool,

    // Mod Updates
    pub mod_updates: BTreeMap<String, mq_mods::ProjectVersion>,
    pub is_checking_mod_updates: bool,

    // Live Server Pings
    pub server_pings: HashMap<String, mq_instances::ServerPingResult>,

    // Favorites
    pub favorite_instances: HashSet<String>,

    // Dialogs
    pub show_import_dialog: bool,
    pub import_archive_path: String,
    pub import_instance_name: String,
    pub show_duplicate_dialog: bool,
    pub duplicate_target_name: String,
    pub show_delete_confirm_dialog: bool,
}

impl UiState {
    pub fn new(
        paths: MqPaths,
        config: AppConfig,
        compute_pool: Arc<ComputePool>,
    ) -> MqResult<Self> {
        let accounts = AccountStore::load_or_default(&paths)?;
        let instance_mgr = InstanceManager::new(paths.clone());
        let instances = instance_mgr.list_instances().unwrap_or_default();
        let selected_instance_id = instances.first().map(|i| i.id.clone());

        let cas = CasStore::new(paths.clone());
        let cancel_epoch = Arc::new(CancelEpoch::new());
        let event_bus = EventBus::new();
        let net = Arc::new(NetEngine::with_components(
            mq_net::NetConfig {
                max_connections: config.max_connections,
                ..Default::default()
            },
            Arc::new(mq_sched::BufferPool::new()),
            cancel_epoch.clone(),
            Some(event_bus.clone()),
        )?);

        let (cmd_sender, cmd_receiver) = flume::bounded(128);
        let (resp_sender, resp_receiver) = flume::bounded(128);

        Ok(Self {
            paths,
            config,
            accounts,
            instances,
            selected_instance_id,
            instance_mgr,
            net,
            cas,
            compute_pool,
            cancel_epoch,
            event_bus,
            watchdog: Arc::new(UiWatchdog::default()),
            cmd_sender,
            cmd_receiver,
            resp_sender,
            resp_receiver,
            latest_cpu: CpuSnapshot {
                total_pct: 0.0,
                per_core: Vec::new(),
                rss_mib: 0.0,
            },
            active_phase: None,
            phase_progress: None,
            status_message: Some(("MQ Launcher готов к работе".to_string(), false)),
            game_running: false,
            running_instance: None,
            is_launching: false,
            installing_instance: None,
            install_error: None,
            detailed_install_error: None,
            current_install_stage: InstallStage::Manifest,
            install_retries_count: 0,
            install_eta_secs: None,
            last_game_exit_code: None,
            game_logs: VecDeque::with_capacity(MAX_GAME_LOG_LINES),
            log_filter: None,
            installed_mods: Vec::new(),
            mod_conflicts: Vec::new(),
            is_scanning_mods: false,
            catalog: crate::catalog_state::CatalogState::default(),
            project: crate::catalog_flow::ProjectViewState::default(),
            install: None,
            jobs: crate::catalog_flow::JobQueue::default(),
            updates: crate::catalog_flow::UpdatesState::default(),
            manual: crate::catalog_flow::ManualDownloadState::default(),
            manual_watcher: None,
            install_seq: 0,
            show_create_instance_dialog: false,
            create_inst_name: "1.21.1 Fabric".to_string(),
            create_inst_mc_ver: "1.21.1".to_string(),
            create_inst_loader: ModLoaderKind::Fabric,
            create_inst_ram_mb: 4096,
            remote_versions: Vec::new(),
            is_fetching_manifest: false,
            crash_analysis: None,
            mclogs_url: None,
            is_uploading_mclogs: false,
            mod_updates: BTreeMap::new(),
            is_checking_mod_updates: false,
            server_pings: HashMap::new(),
            favorite_instances: HashSet::new(),
            show_import_dialog: false,
            import_archive_path: String::new(),
            import_instance_name: String::new(),
            show_duplicate_dialog: false,
            duplicate_target_name: String::new(),
            show_delete_confirm_dialog: false,
        })
    }

    /// Completes a queue job and, if needed, opens the manual-download dialog.
    pub fn finish_job(&mut self, job_id: u64, result: Result<JobOutput, String>) {
        match result {
            Ok(out) => {
                self.jobs.finish(job_id, Ok(()));
                let _ = self.cmd_sender.send(UiWorkerCommand::RefreshInstances);
                if let Some(id) = self.selected_instance_id.clone() {
                    let _ = self
                        .cmd_sender
                        .send(UiWorkerCommand::ScanInstanceMods { instance_id: id });
                }
                if let Some(id) = out.instance_id {
                    if !out.blocked.is_empty() {
                        self.open_manual_dialog(&id, out.blocked);
                    }
                }
            }
            Err(e) => self.jobs.finish(job_id, Err(e)),
        }
    }

    /// Opens the manual-download dialog for `blocked` files of `instance_id`.
    pub fn open_manual_dialog(
        &mut self,
        instance_id: &InstanceId,
        blocked: Vec<mq_mods::BlockedDownload>,
    ) {
        let instance_dir = self.paths.instance_dir(instance_id.as_str());
        let watch_dir =
            crate::catalog_flow::default_downloads_dir().unwrap_or_else(|| self.paths.temp_dir());
        self.manual = crate::catalog_flow::ManualDownloadState {
            open: true,
            instance_label: instance_id.to_string(),
            pending: blocked.clone(),
            watch_dir_label: watch_dir.display().to_string(),
            ..Default::default()
        };
        self.manual_watcher = Some(Arc::new(parking_lot::Mutex::new(
            mq_mods::BlockedDownloadsWatcher::new(instance_dir, watch_dir, blocked),
        )));
    }

    /// Non-blocking drain of background worker events (up to 256 events per frame).
    pub fn drain_events(&mut self) {
        while let Ok(resp) = self.resp_receiver.try_recv() {
            match resp {
                UiWorkerResponse::CatalogResults(page) => {
                    self.catalog.apply_page(page);
                }
                UiWorkerResponse::ProjectLoaded { request_id, result } => {
                    self.project.apply(request_id, result.map(|p| *p));
                }
                UiWorkerResponse::PlanReady { request_id, result } => {
                    if let Some(d) = self.install.as_mut() {
                        if d.request_id == request_id {
                            d.loading = false;
                            match result {
                                Ok(o) => {
                                    d.error = None;
                                    d.titles.extend(o.titles);
                                    d.plan = Some(o.plan);
                                }
                                Err(e) => {
                                    d.plan = None;
                                    d.error = Some(e);
                                }
                            }
                        }
                    }
                }
                UiWorkerResponse::ModpackPrepared { request_id, result } => {
                    if let Some(d) = self.install.as_mut() {
                        if d.request_id == request_id {
                            d.loading = false;
                            match result {
                                Ok(p) => d.preview = Some(p),
                                Err(e) => d.error = Some(e),
                            }
                        }
                    }
                }
                UiWorkerResponse::JobProgress {
                    job_id,
                    done,
                    total,
                } => self.jobs.progress(job_id, done, total),
                UiWorkerResponse::UpdatesChecked { request_id, result } => {
                    if request_id == self.updates.request_id {
                        self.updates.checking = false;
                        self.updates.checked = true;
                        match result {
                            Ok(items) => self.updates.items = items,
                            Err(e) => self.updates.error = Some(e),
                        }
                    }
                }
                UiWorkerResponse::JobFinished { job_id, result } => {
                    self.finish_job(job_id, result);
                }
                UiWorkerResponse::ManualProgress {
                    pending,
                    newly_done,
                } => {
                    self.manual.apply_progress(pending, newly_done);
                }
                UiWorkerResponse::IconLoaded { url, image } => {
                    self.catalog.icons.insert(
                        url,
                        match image {
                            Some(img) => crate::catalog_state::IconSlot::Ready(img),
                            None => crate::catalog_state::IconSlot::Failed,
                        },
                    );
                }
                UiWorkerResponse::ScanModsResults { mods, conflicts } => {
                    self.installed_mods = mods;
                    self.mod_conflicts = conflicts;
                    self.is_scanning_mods = false;
                }
                UiWorkerResponse::UnusedDepsCleaned { removed, error } => {
                    self.status_message = Some(match error {
                        Some(e) => (format!("Не удалось очистить зависимости: {e}"), true),
                        None if removed == 0 => {
                            ("Неиспользуемых зависимостей не найдено".to_string(), false)
                        }
                        None => (
                            format!("Удалено неиспользуемых зависимостей: {removed}"),
                            false,
                        ),
                    });
                }
                UiWorkerResponse::VersionManifestFetched(versions) => {
                    self.remote_versions = versions;
                    self.is_fetching_manifest = false;
                }
                UiWorkerResponse::InstanceInstallStarted(id) => {
                    self.installing_instance = Some(id);
                    self.install_error = None;
                    self.detailed_install_error = None;
                    self.install_retries_count = 0;
                    self.install_eta_secs = None;
                    self.current_install_stage = InstallStage::Manifest;
                }
                UiWorkerResponse::InstanceInstallCompleted(id) => {
                    if self.installing_instance.as_ref() == Some(&id) {
                        self.installing_instance = None;
                    }
                    self.install_eta_secs = None;
                    self.active_phase = None;
                    self.phase_progress = None;
                    self.status_message = Some(("Инстанс успешно установлен".to_string(), false));
                }
                UiWorkerResponse::InstanceError {
                    instance_id: _,
                    error,
                } => {
                    self.installing_instance = None;
                    self.is_launching = false;
                    self.install_error = Some(error.clone());
                    let stage = self.current_install_stage;
                    self.detailed_install_error = Some(InstallError::from_error_str(stage, &error));
                    self.status_message = Some((error, true));
                }
                UiWorkerResponse::GameStarted(id) => {
                    self.is_launching = false;
                    self.game_running = true;
                    self.running_instance = Some(id);
                }
                UiWorkerResponse::GameExited {
                    instance_id,
                    exit_code,
                } => {
                    self.game_running = false;
                    self.running_instance = None;
                    self.last_game_exit_code = Some(exit_code);
                    self.status_message = Some((
                        format!("Игра завершилась с кодом выхода {}", exit_code),
                        exit_code != 0,
                    ));
                    if exit_code != 0 {
                        let inst_dir = self.paths.instance_dir(instance_id.as_str());
                        let lines: Vec<String> =
                            self.game_logs.iter().map(|l| l.line.clone()).collect();
                        let analysis = mq_launch::analyze_crash(&inst_dir, Some(exit_code), &lines);
                        self.crash_analysis = Some(analysis);
                    } else {
                        self.crash_analysis = None;
                    }
                }
            }
        }

        for event in self.event_bus.try_drain(256) {
            match event {
                Event::PhaseStarted { phase, .. } => {
                    self.active_phase = Some(phase.clone());
                    self.current_install_stage = InstallStage::from_phase_str(&phase);
                }
                Event::Progress(prog) => {
                    self.active_phase = Some(prog.phase.clone());
                    self.current_install_stage = InstallStage::from_phase_str(&prog.phase);
                    self.install_eta_secs =
                        calculate_eta_secs(prog.bytes_done, prog.bytes_total, prog.mib_per_sec);
                    self.phase_progress = Some(prog);
                }
                Event::PhaseCompleted { phase, elapsed_ms } => {
                    self.active_phase = None;
                    self.phase_progress = None;
                    self.install_eta_secs = None;
                    self.status_message = Some((
                        format!("Фаза '{phase}' успешно завершена за {elapsed_ms} мс"),
                        false,
                    ));
                }
                Event::TaskError { context, message } => {
                    if self.installing_instance.is_some() {
                        self.install_retries_count = self.install_retries_count.saturating_add(1);
                    }
                    self.status_message = Some((format!("Ошибка [{context}]: {message}"), true));
                }
                Event::CpuTelemetry(snap) => {
                    self.latest_cpu = snap;
                }
                Event::GameLog(log) => {
                    if self.game_logs.len() >= MAX_GAME_LOG_LINES {
                        self.game_logs.pop_front();
                    }
                    self.game_logs.push_back(log);
                }
                Event::WatchdogWarning {
                    operation,
                    elapsed_ms,
                } => {
                    tracing::warn!(
                        operation,
                        elapsed_ms,
                        "UI Watchdog warning received in state"
                    );
                }
                _ => {}
            }
        }
    }

    pub fn selected_instance(&self) -> Option<&InstanceManifest> {
        let sel_id = self.selected_instance_id.as_ref()?;
        self.instances.iter().find(|i| i.id == *sel_id)
    }

    /// Checks whether an instance has already been installed into the local CAS and instance directory.
    pub fn is_instance_installed(&self, id: &InstanceId) -> bool {
        let inst_dir = self.paths.instance_dir(id.as_str());
        let state_file = inst_dir.join("install_state.json");
        if state_file.is_file() {
            if let Ok(content) = std::fs::read_to_string(&state_file) {
                if let Ok(st) = serde_json::from_str::<serde_json::Value>(&content) {
                    if let Some(status) = st.get("status").and_then(|v| v.as_str()) {
                        if status == "ready" {
                            return true;
                        } else if status == "failed" || status == "installing" {
                            return false;
                        }
                    }
                }
            }
        }
        inst_dir.join("resolved_version.json").exists() && inst_dir.join("client.jar").exists()
    }
}

/// Clamps `val` between `min` and `max` without panicking under any circumstances:
/// - Inverted bounds (`min > max`) are normalized safely without asserting.
/// - `NaN` values for `val`, `min`, or `max` are safely replaced without panicking.
/// - `+Inf` and `-Inf` are clamped to finite bounds when bounds are finite.
#[inline]
pub fn safe_clamp_f32(val: f32, min: f32, max: f32) -> f32 {
    let min = if min.is_nan() { 0.0 } else { min };
    let max = if max.is_nan() { min } else { max };
    let (effective_min, effective_max) = if min > max { (max, min) } else { (min, max) };
    if val.is_nan() {
        return effective_min;
    }
    if val < effective_min {
        effective_min
    } else if val > effective_max {
        effective_max
    } else {
        val
    }
}

/// Clamps any floating point progress value to the safe range `[0.0, 1.0]`.
/// Returns `0.0` for `NaN` and negative values, and `1.0` for values >= 1.0 or `+Inf`.
#[inline]
pub fn safe_progress_clamp(val: f32) -> f32 {
    safe_clamp_f32(val, 0.0, 1.0)
}

/// Safely computes the completion ratio `done / total` as an `f32` in `[0.0, 1.0]`.
///
/// - If `total == 0`, returns `0.0`.
/// - If `done >= total`, returns `1.0`.
/// - Uses `u64` bounds, checked divisions, and avoids any floating-point overflow or NaN.
#[inline]
pub fn safe_progress_ratio(done: u64, total: u64) -> f32 {
    if total == 0 {
        return 0.0;
    }
    if done >= total {
        return 1.0;
    }
    let frac = (done as f64) / (total as f64);
    if !frac.is_finite() || frac <= 0.0 {
        0.0
    } else if frac >= 1.0 {
        1.0
    } else {
        frac as f32
    }
}

/// Safely calculates remaining ETA in seconds given bytes completed, total bytes, and transfer speed in MiB/s.
/// Uses `saturating_sub` and validates against NaN/Inf/zero speeds.
#[inline]
pub fn calculate_eta_secs(bytes_done: u64, bytes_total: u64, mib_per_sec: f64) -> Option<u64> {
    if !mib_per_sec.is_finite() || mib_per_sec <= 0.001 {
        return None;
    }
    let remaining = bytes_total.saturating_sub(bytes_done);
    if remaining == 0 {
        return None;
    }
    let speed_bytes = mib_per_sec * 1024.0 * 1024.0;
    if !speed_bytes.is_finite() || speed_bytes <= 0.0 {
        return None;
    }
    let secs = remaining as f64 / speed_bytes;
    if !secs.is_finite() || secs < 0.0 {
        return None;
    }
    if secs > (u64::MAX as f64) {
        Some(u64::MAX)
    } else {
        Some(secs as u64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_safe_clamp_f32_edges() {
        // Normal in-bounds
        assert_eq!(safe_clamp_f32(0.5, 0.0, 1.0), 0.5);
        // Exceeds max
        assert_eq!(safe_clamp_f32(1.5, 0.0, 1.0), 1.0);
        // Below min
        assert_eq!(safe_clamp_f32(-0.5, 0.0, 1.0), 0.0);
        // Inverted min > max (e.g. hud.rs:99 case max_bar_h < 1.5)
        assert_eq!(safe_clamp_f32(0.8, 1.5, 1.0), 1.0);
        assert_eq!(safe_clamp_f32(2.0, 1.5, 1.0), 1.5);
        assert_eq!(safe_clamp_f32(1.2, 1.5, 1.0), 1.2);
        // NaNs
        assert_eq!(safe_clamp_f32(f32::NAN, 0.0, 1.0), 0.0);
        assert_eq!(safe_clamp_f32(0.5, f32::NAN, 1.0), 0.5);
        assert_eq!(safe_clamp_f32(0.5, 0.0, f32::NAN), 0.0);
        assert_eq!(safe_clamp_f32(f32::NAN, f32::NAN, f32::NAN), 0.0);
        // Infinities
        assert_eq!(safe_clamp_f32(f32::INFINITY, 0.0, 1.0), 1.0);
        assert_eq!(safe_clamp_f32(f32::NEG_INFINITY, 0.0, 1.0), 0.0);
    }

    #[test]
    fn test_safe_progress_ratio_edges() {
        assert_eq!(safe_progress_ratio(0, 0), 0.0);
        assert_eq!(safe_progress_ratio(50, 0), 0.0);
        assert_eq!(safe_progress_ratio(0, 100), 0.0);
        assert_eq!(safe_progress_ratio(50, 100), 0.5);
        assert_eq!(safe_progress_ratio(100, 100), 1.0);
        assert_eq!(safe_progress_ratio(150, 100), 1.0);
        assert_eq!(safe_progress_ratio(u64::MAX, u64::MAX), 1.0);
        assert_eq!(safe_progress_ratio(u64::MAX, 10), 1.0);
        assert_eq!(safe_progress_ratio(0, u64::MAX), 0.0);
    }

    #[test]
    fn test_calculate_eta_secs_edges() {
        assert_eq!(calculate_eta_secs(0, 0, 10.0), None);
        assert_eq!(calculate_eta_secs(100, 50, 10.0), None);
        assert_eq!(calculate_eta_secs(50, 100, 0.0), None);
        assert_eq!(calculate_eta_secs(50, 100, -5.0), None);
        assert_eq!(calculate_eta_secs(50, 100, f64::NAN), None);
        assert_eq!(calculate_eta_secs(50, 100, f64::INFINITY), None);

        // 1 MiB remaining at 1 MiB/s = 1 second
        let one_mib = 1024 * 1024;
        assert_eq!(calculate_eta_secs(0, one_mib, 1.0), Some(1));
        // Saturating subtraction when done > total
        assert_eq!(calculate_eta_secs(u64::MAX, 100, 10.0), None);
    }

    #[test]
    fn test_progress_ten_thousand_extreme_states_never_panic() {
        let extreme_u64 = [
            0,
            1,
            2,
            100,
            1000,
            u32::MAX as u64,
            (u32::MAX as u64) + 1,
            u64::MAX / 2,
            u64::MAX - 1,
            u64::MAX,
        ];

        let extreme_f32 = [
            0.0,
            -0.0,
            1.0,
            -1.0,
            0.5,
            -0.5,
            1.5,
            100.0,
            -100.0,
            f32::MIN_POSITIVE,
            f32::EPSILON,
            f32::MAX,
            f32::MIN,
            f32::NAN,
            f32::INFINITY,
            f32::NEG_INFINITY,
        ];

        let extreme_f64 = [
            0.0,
            -0.0,
            0.0001,
            0.001,
            1.0,
            -1.0,
            50.0,
            f64::MIN_POSITIVE,
            f64::MAX,
            f64::MIN,
            f64::NAN,
            f64::INFINITY,
            f64::NEG_INFINITY,
        ];

        let string_samples = [
            "",
            "   ",
            "manifest",
            "assets",
            "natives",
            "libraries",
            "java",
            "client",
            "verify",
            "unknown_phase_xyz",
            "404 Not Found",
            "Connection reset",
            "No space left on device",
            "Permission denied",
            "SHA1 checksum mismatch",
            "паника в потоке",
            "Установка прервана из-за внутренней ошибки",
            "\0\r\n\t",
            "https://piston-meta.mojang.com/v1/packages/test.json",
        ];

        let mut seed: u64 = 0xDEADBEEF_CAFE1234;
        let mut next_rand = || {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
            seed
        };

        for i in 0..10_000 {
            let done = if i < extreme_u64.len() {
                extreme_u64[i]
            } else {
                let r = next_rand();
                match r % 4 {
                    0 => extreme_u64[(r as usize) % extreme_u64.len()],
                    1 => r,
                    2 => r % 10_000,
                    _ => 0,
                }
            };

            let total = if i < extreme_u64.len() {
                extreme_u64[extreme_u64.len() - 1 - i]
            } else {
                let r = next_rand();
                match r % 4 {
                    0 => extreme_u64[(r as usize) % extreme_u64.len()],
                    1 => r,
                    2 => r % 10_000,
                    _ => 0,
                }
            };

            let ratio = safe_progress_ratio(done, total);
            assert!(
                (0.0..=1.0).contains(&ratio),
                "ratio out of bounds: {ratio} for done={done}, total={total}"
            );
            assert!(
                ratio.is_finite(),
                "ratio is not finite for done={done}, total={total}"
            );

            let val = extreme_f32[(next_rand() as usize) % extreme_f32.len()];
            let min = extreme_f32[(next_rand() as usize) % extreme_f32.len()];
            let max = extreme_f32[(next_rand() as usize) % extreme_f32.len()];
            let clamped = safe_clamp_f32(val, min, max);
            assert!(
                !clamped.is_nan(),
                "clamped returned NaN for val={val}, min={min}, max={max}"
            );

            let unit_clamped = safe_progress_clamp(val);
            assert!(
                (0.0..=1.0).contains(&unit_clamped),
                "unit clamped out of range: {unit_clamped} for val={val}"
            );

            let speed = extreme_f64[(next_rand() as usize) % extreme_f64.len()];
            let eta = calculate_eta_secs(done, total, speed);
            if let Some(secs) = eta {
                let _ = secs;
            }

            let phase_str = string_samples[(next_rand() as usize) % string_samples.len()];
            let stage = InstallStage::from_phase_str(phase_str);
            let _ = stage.localized_name();
            let _ = stage.pipeline_index();

            let err_str = string_samples[(next_rand() as usize) % string_samples.len()];
            let err = InstallError::from_error_str(stage, err_str);
            assert!(!err.localized_title.is_empty());
            assert!(!err.localized_details.is_empty());
            let clip = err.format_for_clipboard();
            assert!(!clip.is_empty());
        }
    }
}
