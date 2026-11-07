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
use mq_mods::{CatalogHit, InstalledModEntry, ModConflict, ProjectKind, SearchQuery};
use mq_net::NetEngine;
use mq_sched::{CancelEpoch, ComputePool, UiWatchdog};

/// Max lines retained in memory for live game log console (10,000 lines).
pub const MAX_GAME_LOG_LINES: usize = 10000;

/// Background responses delivered back to UI thread without blocking.
pub enum UiWorkerResponse {
    CatalogResults(Vec<CatalogHit>),
    ScanModsResults {
        mods: Vec<InstalledModEntry>,
        conflicts: Vec<ModConflict>,
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

/// Background task messages sent from UI to worker threads.
pub enum UiWorkerCommand {
    FetchVersionManifest,
    RefreshInstances,
    RefreshAccounts,
    SearchCatalog {
        source: mq_mods::CatalogSource,
        query: SearchQuery,
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
    pub last_game_exit_code: Option<i32>,
    pub game_logs: VecDeque<GameLogEntry>,
    pub log_filter: Option<GameLogLevel>,

    // Mods Screen
    pub installed_mods: Vec<InstalledModEntry>,
    pub mod_conflicts: Vec<ModConflict>,
    pub is_scanning_mods: bool,

    // Catalog Screen
    pub catalog_hits: Vec<CatalogHit>,
    pub catalog_searching: bool,
    pub catalog_query_text: String,
    pub catalog_source: mq_mods::CatalogSource,
    pub catalog_kind: ProjectKind,
    pub catalog_filter_version: String,

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
            last_game_exit_code: None,
            game_logs: VecDeque::with_capacity(MAX_GAME_LOG_LINES),
            log_filter: None,
            installed_mods: Vec::new(),
            mod_conflicts: Vec::new(),
            is_scanning_mods: false,
            catalog_hits: Vec::new(),
            catalog_searching: false,
            catalog_query_text: String::new(),
            catalog_source: mq_mods::CatalogSource::Modrinth,
            catalog_kind: ProjectKind::Mod,
            catalog_filter_version: "1.21.1".to_string(),
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

    /// Non-blocking drain of background worker events (up to 256 events per frame).
    pub fn drain_events(&mut self) {
        while let Ok(resp) = self.resp_receiver.try_recv() {
            match resp {
                UiWorkerResponse::CatalogResults(hits) => {
                    self.catalog_hits = hits;
                    self.catalog_searching = false;
                }
                UiWorkerResponse::ScanModsResults { mods, conflicts } => {
                    self.installed_mods = mods;
                    self.mod_conflicts = conflicts;
                    self.is_scanning_mods = false;
                }
                UiWorkerResponse::VersionManifestFetched(versions) => {
                    self.remote_versions = versions;
                    self.is_fetching_manifest = false;
                }
                UiWorkerResponse::InstanceInstallStarted(id) => {
                    self.installing_instance = Some(id);
                    self.install_error = None;
                }
                UiWorkerResponse::InstanceInstallCompleted(id) => {
                    if self.installing_instance.as_ref() == Some(&id) {
                        self.installing_instance = None;
                    }
                    self.status_message = Some(("Инстанс успешно установлен".to_string(), false));
                }
                UiWorkerResponse::InstanceError {
                    instance_id: _,
                    error,
                } => {
                    self.installing_instance = None;
                    self.is_launching = false;
                    self.install_error = Some(error.clone());
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
                    self.active_phase = Some(phase);
                }
                Event::Progress(prog) => {
                    self.active_phase = Some(prog.phase.clone());
                    self.phase_progress = Some(prog);
                }
                Event::PhaseCompleted { phase, elapsed_ms } => {
                    self.active_phase = None;
                    self.phase_progress = None;
                    self.status_message = Some((
                        format!("Фаза '{phase}' успешно завершена за {elapsed_ms} мс"),
                        false,
                    ));
                }
                Event::TaskError { context, message } => {
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
        inst_dir.join("resolved_version.json").exists() && inst_dir.join("client.jar").exists()
    }
}
