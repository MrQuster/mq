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
use crate::sidebar::{render_sidebar, NavTab, SidebarAction};
use crate::state::{UiState, UiWorkerCommand, UiWorkerResponse};
use crate::theme::apply_dark_glass_theme;

pub struct MqApp {
    pub state: UiState,
    pub current_tab: NavTab,
    pub instances_view: InstancesView,
    pub catalog_view: CatalogView,
    pub accounts_view: AccountsView,
    pub perf_view: PerfView,
    pub settings_view: SettingsView,
}

impl MqApp {
    pub fn new(cc: &eframe::CreationContext<'_>, state: UiState) -> Self {
        apply_dark_glass_theme(&cc.egui_ctx);
        configure_system_fonts(&cc.egui_ctx);

        Self {
            state,
            current_tab: NavTab::Instances,
            instances_view: InstancesView::default(),
            catalog_view: CatalogView::default(),
            accounts_view: AccountsView::default(),
            perf_view: PerfView,
            settings_view: SettingsView::default(),
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
                UiWorkerCommand::VerifyInstance { instance_id } => {
                    let inst_dir = self.state.paths.instance_dir(instance_id.as_str());
                    let resolved_path = inst_dir.join("resolved_version.json");
                    let paths = self.state.paths.clone();
                    let pool = self.state.compute_pool.clone();
                    let guard = self.state.cancel_epoch.snapshot();
                    let bus = self.state.event_bus.clone();

                    tokio::spawn(async move {
                        if let Ok(bytes) = std::fs::read(&resolved_path) {
                            if let Ok(ver) =
                                serde_json::from_slice::<mq_meta::ResolvedVersion>(&bytes)
                            {
                                let asset_index_path = paths
                                    .assets_dir()
                                    .join("indexes")
                                    .join(format!("{}.json", ver.asset_index.id));
                                if let Ok(idx_bytes) = std::fs::read(&asset_index_path) {
                                    if let Ok(idx) =
                                        serde_json::from_slice::<mq_meta::AssetIndex>(&idx_bytes)
                                    {
                                        let targets = mq_install::collect_instance_verify_targets(
                                            &paths, &inst_dir, &ver, &idx,
                                        );
                                        let _ = mq_install::verify_files_parallel(
                                            &targets,
                                            &pool,
                                            &guard,
                                            Some(&bus),
                                        )
                                        .await;
                                    }
                                }
                            }
                        }
                    });
                }
                UiWorkerCommand::LaunchInstance {
                    instance_id,
                    dry_run,
                } => {
                    let inst_dir = self.state.paths.instance_dir(instance_id.as_str());
                    let resolved_path = inst_dir.join("resolved_version.json");
                    let paths = self.state.paths.clone();
                    let bus = self.state.event_bus.clone();

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
                            .unwrap_or_else(|| {
                                mq_auth::create_offline_account("Mr_Quster").unwrap()
                            });

                        tokio::spawn(async move {
                            if let Ok(bytes) = std::fs::read(&resolved_path) {
                                if let Ok(ver) =
                                    serde_json::from_slice::<mq_meta::ResolvedVersion>(&bytes)
                                {
                                    let java_runtimes = mq_launch::discover_java_runtimes();
                                    let req_major = mq_launch::required_java_major_for_mc(
                                        &ver.mc_version,
                                        Some(ver.java_version.major_version),
                                    );
                                    if let Ok(selected_java) = mq_launch::select_java_runtime(
                                        &java_runtimes,
                                        req_major,
                                        inst.java_path.as_deref(),
                                    ) {
                                        let launch_ctx = mq_launch::LaunchContext {
                                            paths,
                                            instance: inst,
                                            version: ver,
                                            account: active_acc,
                                            java_bin: selected_java.path,
                                            authlib_injector_jar: None,
                                            extra_secrets_to_redact: Vec::new(),
                                        };

                                        if let Ok(built_cmd) =
                                            mq_launch::build_launch_command(&launch_ctx)
                                        {
                                            if dry_run {
                                                bus.publish(mq_core::Event::GameLog(
                                                    mq_core::GameLogEntry {
                                                        instance: instance_id.to_string(),
                                                        timestamp_ms: chrono::Utc::now()
                                                            .timestamp_millis(),
                                                        level: mq_core::GameLogLevel::Info,
                                                        line: format!(
                                                            "DRY RUN COMMAND:\n{}",
                                                            built_cmd.to_shell_command_line()
                                                        ),
                                                    },
                                                ));
                                            } else {
                                                let _ = mq_launch::spawn_and_stream_game(
                                                    &built_cmd,
                                                    instance_id.as_str(),
                                                    Vec::new(),
                                                    Some(bus),
                                                )
                                                .await;
                                            }
                                        }
                                    }
                                }
                            }
                        });
                    }
                }
                UiWorkerCommand::SearchCatalog { source, query } => {
                    let bus = self.state.event_bus.clone();
                    let config = self.state.config.clone();
                    let resp_tx = self.state.resp_sender.clone();
                    tokio::spawn(async move {
                        let hits = match source {
                            mq_mods::CatalogSource::Modrinth => {
                                match mq_mods::ModrinthClient::from_config(&config) {
                                    Ok(m_client) => {
                                        m_client.search(&query).await.unwrap_or_default()
                                    }
                                    Err(_) => Vec::new(),
                                }
                            }
                            mq_mods::CatalogSource::CurseForge => Vec::new(),
                        };
                        let _ = resp_tx.send(UiWorkerResponse::CatalogResults(hits));
                        bus.publish(mq_core::Event::PhaseCompleted {
                            phase: "catalog_search".to_string(),
                            elapsed_ms: 50,
                        });
                    });
                }
                _ => {}
            }
        }
    }
}

impl eframe::App for MqApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let watchdog = self.state.watchdog.clone();
        let _guard = watchdog.guard("mq_frame_update");

        // Safe 0x0 handling on tiling WMs like Hyprland
        if is_zero_viewport(ctx) {
            return;
        }

        // Drain background worker events
        self.state.drain_events();
        self.handle_worker_commands();

        // 1. Sidebar (Left)
        let accounts_list = self.state.accounts.list_accounts().to_vec();
        let active_id = self
            .state
            .accounts
            .active_account()
            .map(|a| a.id.to_string());

        let mut sidebar_action = None;
        SidePanel::left("mq_sidebar")
            .resizable(false)
            .default_width(220.0)
            .show(ctx, |ui| {
                sidebar_action =
                    render_sidebar(ui, self.current_tab, &accounts_list, active_id.as_deref());
            });

        if let Some(act) = sidebar_action {
            match act {
                SidebarAction::SelectTab(t) => self.current_tab = t,
                SidebarAction::SwitchAccount(id) => {
                    let _ = self.state.accounts.select_account(&id);
                    let _ = self.state.accounts.save(&self.state.paths);
                }
                SidebarAction::OpenAccountsTab => self.current_tab = NavTab::Accounts,
            }
        }

        // 2. Bottom HUD (Bottom)
        let hud_data = BottomHudData {
            active_phase: self.state.active_phase.clone(),
            progress_fraction: self.state.phase_progress.as_ref().map(|p| {
                if p.total_items > 0 {
                    (p.completed_items as f32) / (p.total_items as f32)
                } else {
                    0.0
                }
            }),
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

        // Request next frame repaint if an operation is active
        if self.state.active_phase.is_some() || self.state.game_running {
            ctx.request_repaint();
        }
    }
}

/// Launches the desktop GUI window.
pub fn run_gui(state: UiState, backend: GuiBackend) -> Result<(), eframe::Error> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("MQ Launcher")
            .with_inner_size([1120.0, 720.0])
            .with_min_inner_size([800.0, 500.0]),
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
