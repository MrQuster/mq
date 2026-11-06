#![deny(clippy::await_holding_lock)]
//! `mq` — Command Line Interface and Desktop GUI entrypoint.
//!
//! Provides subcommands for:
//! - `mq` (default without args): launch desktop GUI (Wayland/X11, Vulkan/OpenGL fallback).
//! - `mq install <version>`: multi-threaded DAG installation into CAS and instance.
//! - `mq verify <instance>`: parallel SHA-1 / SHA-512 integrity check across all cores.
//! - `mq launch <instance>`: launch Minecraft (offline / Ely.by skins / Microsoft) or `--dry-run`.
//! - `mq plan <instance>`: DAG critical path analysis and stage breakdown.
//! - `mq auth`: manage offline, Ely.by, and Microsoft accounts.
//! - `mq mods`: local jar scanner, conflict detector, catalog search and dependency installer.
//! - `mq modpack install`: `.mrpack` and CurseForge `.zip` installer.
//! - `mq bench`: Amdahl law scaling verification and `/proc/stat` core saturation benchmark.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use anyhow::{bail, Context, Result};
use clap::{Args, Parser, Subcommand, ValueEnum};
use mq_auth::{
    create_offline_account, resolve_ms_client_id, AccountKind, AccountStore, ElyByClient, MsaClient,
};
use mq_core::{AppConfig, Event, EventBus, GuiBackend, ModLoaderKind, MqPaths};
use mq_install::{
    build_game_install_dag_plan, collect_instance_verify_targets, install_game,
    verify_files_parallel, CasStore, InstallRequest,
};
use mq_instances::InstanceManager;
use mq_launch::{
    build_launch_command, discover_java_runtimes, required_java_major_for_mc, select_java_runtime,
    spawn_and_stream_game, LaunchContext,
};
use mq_mods::{
    delete_mod, install_curseforge_zip, install_mrpack, resolve_mod_with_dependencies,
    scan_mods_directory, toggle_mod, CurseForgeClient, ModConflict, ModrinthClient, ProjectKind,
    SearchQuery,
};
use mq_net::{DownloadSpec, NetConfig, NetEngine};
use mq_sched::{
    compute_speedup, estimate_serial_fraction, read_peak_rss_mib, read_proc_stat, thread_ladder,
    BufferPool, CancelEpoch, ComputePool,
};
use mq_ui::{run_gui, UiState};
use testsupport::{MockConfig, MockServer};

#[derive(Parser)]
#[command(
    name = "mq",
    author = "MQ Systems Team",
    version,
    about = "Autonomous multithreaded Minecraft: Java Edition launcher for Linux",
    long_about = "MQ — high-performance, multithreaded Minecraft launcher for Arch Linux.\n\
                  Supports Offline mode, Ely.by skins with authlib-injector, Modrinth, and CurseForge.\n\
                  Run 'mq' without arguments to start the GUI, or use subcommands for CLI operations."
)]
struct Cli {
    /// Graphical rendering backend for the UI.
    #[arg(long, value_enum, default_value = "auto")]
    backend: BackendChoice,

    /// Number of worker threads for Rayon CPU pool (defaults to logical core count).
    #[arg(short = 't', long)]
    threads: Option<usize>,

    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, ValueEnum, Debug)]
enum BackendChoice {
    Auto,
    Vulkan,
    Glow,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Launch the game or inspect launch arguments without starting the process.
    Launch(LaunchArgs),

    /// Install Minecraft version and loader into an instance.
    Install(InstallArgs),

    /// Verify SHA-1 / SHA-512 hashes of all files in an instance using all CPU cores.
    Verify(VerifyArgs),

    /// Display DAG execution plan and Critical Path Analysis for installation.
    Plan(PlanArgs),

    /// Manage authentication profiles (Offline, Ely.by, Microsoft).
    Auth(AuthArgs),

    /// Manage mods: scan, search, install, toggle, or delete.
    Mods(ModsArgs),

    /// Install a Modrinth (.mrpack) or CurseForge (.zip) modpack.
    Modpack(ModpackArgs),

    /// Benchmark multi-core scaling, Amdahl speedup, and per-core CPU utilization.
    Bench(BenchArgs),
}

#[derive(Args, Debug)]
struct LaunchArgs {
    /// Instance identifier or name.
    instance: String,

    /// Print command line and environment without launching the game process.
    #[arg(long)]
    dry_run: bool,

    /// Run with an offline profile name (UUID v3 deterministic MD5).
    #[arg(long)]
    offline: Option<String>,

    /// Run with an Ely.by account username.
    #[arg(long)]
    ely: Option<String>,
}

#[derive(Args, Debug)]
struct InstallArgs {
    /// Minecraft Java Edition version (e.g. "1.21.1", "1.20.1", "1.16.5").
    version: String,

    /// Mod loader kind (vanilla, fabric, quilt, forge, neoforge).
    #[arg(long, default_value = "vanilla")]
    loader: String,

    /// Explicit loader version (defaults to latest recommended for the MC version).
    #[arg(long)]
    loader_version: Option<String>,

    /// Custom instance name (defaults to "<version> <loader>").
    #[arg(long)]
    name: Option<String>,
}

#[derive(Args, Debug)]
struct VerifyArgs {
    /// Instance identifier or name to verify.
    instance: String,
}

#[derive(Args, Debug)]
struct PlanArgs {
    /// Instance identifier or name.
    instance: String,
}

#[derive(Args, Debug)]
struct AuthArgs {
    #[command(subcommand)]
    action: AuthSubcommand,
}

#[derive(Subcommand, Debug)]
enum AuthSubcommand {
    /// List all configured accounts.
    List,
    /// Create or select an offline player profile.
    Offline { username: String },
    /// Add or login to an account.
    Login(LoginArgs),
    /// Select the active account.
    Select { id: String },
    /// Remove an account.
    Remove { id: String },
}

#[derive(Args, Debug)]
struct LoginArgs {
    /// Create or select an offline player profile.
    #[arg(long)]
    offline: Option<String>,

    /// Authenticate via Ely.by.
    #[arg(long)]
    ely: bool,

    /// Ely.by username or email.
    #[arg(long)]
    username: Option<String>,

    /// Ely.by password.
    #[arg(long)]
    password: Option<String>,

    /// Authenticate via Microsoft Device Code Flow.
    #[arg(long)]
    microsoft: bool,
}

#[derive(Args, Debug)]
struct ModsArgs {
    #[command(subcommand)]
    action: ModsSubcommand,
}

#[derive(Subcommand, Debug)]
enum ModsSubcommand {
    /// Scan instance mods directory with Rayon and display metadata & conflicts.
    Scan { instance: String },

    /// Search for mods in Modrinth or CurseForge catalogs.
    Search {
        query: String,
        #[arg(long, default_value = "modrinth")]
        source: String,
        #[arg(long)]
        loader: Option<String>,
        #[arg(long)]
        version: Option<String>,
    },

    /// Install a mod and its required dependencies into an instance.
    Install {
        instance: String,
        project: String,
        #[arg(long)]
        loader: Option<String>,
        #[arg(long)]
        version: Option<String>,
    },

    /// Enable or disable a mod jar (renaming .jar <-> .jar.disabled).
    Toggle { instance: String, file: String },

    /// Delete a mod jar from an instance.
    Delete { instance: String, file: String },
}

#[derive(Args, Debug)]
struct ModpackArgs {
    #[command(subcommand)]
    action: ModpackSubcommand,
}

#[derive(Subcommand, Debug)]
enum ModpackSubcommand {
    /// Install a .mrpack or CurseForge .zip modpack into an instance.
    Install {
        /// File path or catalog slug.
        path_or_slug: String,

        /// Catalog source (file, modrinth, curseforge).
        #[arg(long, default_value = "file")]
        source: String,

        /// Name for the new instance.
        #[arg(long)]
        name: Option<String>,
    },
}

#[derive(Args, Debug)]
struct BenchArgs {
    /// Explicit thread count to benchmark (defaults to 1..N physical cores ladder).
    #[arg(long)]
    threads: Option<usize>,

    /// Quick mode for smoke tests.
    #[arg(long)]
    quick: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize structured logging with token redaction
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info,mq=info")),
        )
        .init();

    let cli = Cli::parse();
    let paths = MqPaths::xdg_default().context("Failed to initialize XDG directories")?;
    paths
        .ensure_dirs()
        .context("Failed to create XDG directories")?;

    match cli.command {
        None => run_desktop_gui(cli.backend, cli.threads, paths),
        Some(Commands::Launch(args)) => cmd_launch(args, paths).await,
        Some(Commands::Install(args)) => cmd_install(args, paths, cli.threads).await,
        Some(Commands::Verify(args)) => cmd_verify(args, paths, cli.threads).await,
        Some(Commands::Plan(args)) => cmd_plan(args, paths).await,
        Some(Commands::Auth(args)) => cmd_auth(args, paths).await,
        Some(Commands::Mods(args)) => cmd_mods(args, paths, cli.threads).await,
        Some(Commands::Modpack(args)) => cmd_modpack(args, paths, cli.threads).await,
        Some(Commands::Bench(args)) => cmd_bench(args).await,
    }
}

fn default_cpu_threads() -> usize {
    std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4)
}

/// Fallback-aware GUI launcher with headless environment detection.
fn run_desktop_gui(backend: BackendChoice, threads: Option<usize>, paths: MqPaths) -> Result<()> {
    let has_wayland = std::env::var("WAYLAND_DISPLAY").is_ok();
    let has_x11 = std::env::var("DISPLAY").is_ok();

    if !has_wayland && !has_x11 {
        eprintln!(
            "--------------------------------------------------------------------------------"
        );
        eprintln!("Ошибка: Графический интерфейс MQ требует активного дисплея (Wayland или X11).");
        eprintln!(
            "Переменные WAYLAND_DISPLAY и DISPLAY не установлены (работа по SSH / Headless)."
        );
        eprintln!();
        eprintln!("Для работы в терминале используйте подкоманды CLI:");
        eprintln!("  mq --help");
        eprintln!("  mq install <version> [--loader <fabric|quilt|forge|neoforge>]");
        eprintln!("  mq launch <instance> [--dry-run]");
        eprintln!("  mq verify <instance>");
        eprintln!("  mq mods scan <instance>");
        eprintln!("  mq bench");
        eprintln!(
            "--------------------------------------------------------------------------------"
        );
        std::process::exit(1);
    }

    let config = AppConfig::load_or_default(&paths)?;
    let num_t = threads.unwrap_or_else(default_cpu_threads);
    let pool = Arc::new(ComputePool::new(num_t));

    let state = UiState::new(paths, config, pool).context("Failed to initialize UI state")?;
    let gui_backend = match backend {
        BackendChoice::Auto => GuiBackend::Auto,
        BackendChoice::Vulkan => GuiBackend::Vulkan,
        BackendChoice::Glow => GuiBackend::Gl,
    };

    println!(
        "[MQ] Запуск графического интерфейса MQ Launcher (Backend: {:?})...",
        gui_backend
    );
    run_gui(state, gui_backend).map_err(|e| anyhow::anyhow!("eframe error: {e}"))
}

async fn cmd_launch(args: LaunchArgs, paths: MqPaths) -> Result<()> {
    let mgr = InstanceManager::new(paths.clone());
    let instances = mgr.list_instances().context("Failed to read instances")?;
    let inst = instances
        .into_iter()
        .find(|i| i.id.as_str() == args.instance || i.name == args.instance)
        .ok_or_else(|| anyhow::anyhow!("Инстанс '{}' не найден", args.instance))?;

    let resolved_path = paths
        .instance_dir(inst.id.as_str())
        .join("resolved_version.json");
    if !resolved_path.exists() {
        bail!(
            "Инстанс '{}' ещё не установлен. Выполните: mq install {}",
            inst.name,
            inst.mc_version
        );
    }
    let ver_bytes = std::fs::read(&resolved_path)?;
    let resolved_ver: mq_meta::ResolvedVersion = serde_json::from_slice(&ver_bytes)?;

    let account_store = AccountStore::load_or_default(&paths)?;
    let account = if let Some(nick) = args.offline {
        create_offline_account(&nick)?
    } else if let Some(ely_nick) = args.ely {
        account_store
            .list_accounts()
            .iter()
            .find(|a| a.username == ely_nick)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("Аккаунт Ely.by '{}' не найден в хранилище", ely_nick))?
    } else if let Some(active) = account_store.active_account() {
        active.clone()
    } else {
        println!("[MQ] Активный аккаунт не найден. Создан оффлайн-профиль 'Player'.");
        create_offline_account("Player")?
    };

    let java_runtimes = discover_java_runtimes();
    let req_major = required_java_major_for_mc(
        &inst.mc_version,
        Some(resolved_ver.java_version.major_version),
    );
    let selected_java = select_java_runtime(&java_runtimes, req_major, inst.java_path.as_deref())
        .context("Подходящая версия Java не найдена")?;

    let authlib_path = if account.kind == AccountKind::ElyBy {
        let client = ElyByClient::new();
        println!("[MQ] Проверка и загрузка authlib-injector для скинов Ely.by...");
        let path = client.ensure_authlib_injector(&paths).await?;
        Some(path)
    } else {
        None
    };

    let launch_ctx = LaunchContext {
        paths: paths.clone(),
        instance: inst.clone(),
        version: resolved_ver,
        account,
        java_bin: selected_java.path,
        authlib_injector_jar: authlib_path,
        extra_secrets_to_redact: Vec::new(),
    };

    let built_cmd = build_launch_command(&launch_ctx)?;

    if args.dry_run {
        println!(
            "--------------------------------------------------------------------------------"
        );
        println!("[MQ DRY RUN] Сформированная командная строка JVM:");
        println!("{}", built_cmd.to_shell_command_line());
        println!(
            "--------------------------------------------------------------------------------"
        );
        println!(
            "[MQ DRY RUN] Рабочая директория: {}",
            built_cmd.working_dir.display()
        );
        return Ok(());
    }

    println!(
        "[MQ] Запуск Minecraft ({}) с помощью Java {}...",
        inst.name, selected_java.full_version
    );
    let event_bus = EventBus::new();
    let rx = event_bus.receiver().clone();
    tokio::spawn(async move {
        while let Ok(evt) = rx.recv_async().await {
            if let Event::GameLog(entry) = evt {
                println!("[GAME] {}", entry.line);
            }
        }
    });

    let exit_code =
        spawn_and_stream_game(&built_cmd, inst.id.as_str(), Vec::new(), Some(event_bus)).await?;
    println!("[MQ] Игра завершилась с кодом выхода: {}", exit_code);
    Ok(())
}

async fn cmd_install(args: InstallArgs, paths: MqPaths, threads: Option<usize>) -> Result<()> {
    let loader = match args.loader.to_lowercase().as_str() {
        "fabric" => ModLoaderKind::Fabric,
        "quilt" => ModLoaderKind::Quilt,
        "forge" => ModLoaderKind::Forge,
        "neoforge" => ModLoaderKind::NeoForge,
        _ => ModLoaderKind::Vanilla,
    };

    let mgr = InstanceManager::new(paths.clone());
    let inst_name = args
        .name
        .unwrap_or_else(|| format!("{} {}", args.version, loader));
    let inst = mgr.create_instance(
        &inst_name,
        &args.version,
        loader,
        args.loader_version.clone(),
    )?;

    println!(
        "[MQ] Установка инстанса '{}' (MC: {}, Loader: {:?})...",
        inst.name, inst.mc_version, inst.loader
    );
    let inst_dir = paths.instance_dir(inst.id.as_str());

    let num_t = threads.unwrap_or_else(default_cpu_threads);
    let pool = Arc::new(ComputePool::new(num_t));
    let cancel = Arc::new(CancelEpoch::new());
    let bus = EventBus::new();
    let net = Arc::new(NetEngine::with_components(
        NetConfig::default(),
        Arc::new(BufferPool::new()),
        cancel.clone(),
        Some(bus.clone()),
    )?);
    let cas = CasStore::new(paths.clone());

    let req = InstallRequest {
        mc_version: args.version,
        loader,
        loader_version: args.loader_version,
        instance_dir: inst_dir.clone(),
        mock_base_url: None,
    };

    let started = Instant::now();
    let summary = install_game(
        &req,
        &paths,
        &net,
        &cas,
        &pool,
        &cancel.snapshot(),
        Some(&bus),
    )
    .await?;

    println!("--------------------------------------------------------------------------------");
    println!(
        "[MQ] Установка успешно завершена за {:.2} с!",
        started.elapsed().as_secs_f64()
    );
    println!("  Установлено библиотек: {}", summary.libraries_installed);
    println!("  Установлено ассетов:    {}", summary.assets_installed);
    println!("  Распаковано нативов:    {}", summary.natives_extracted);
    println!("  Создано hardlink-ов:    {}", summary.hardlinks_created);
    println!(
        "  Загружено байт:         {:.2} MiB",
        summary.bytes_downloaded as f64 / (1024.0 * 1024.0)
    );
    println!("--------------------------------------------------------------------------------");
    Ok(())
}

async fn cmd_verify(args: VerifyArgs, paths: MqPaths, threads: Option<usize>) -> Result<()> {
    let mgr = InstanceManager::new(paths.clone());
    let instances = mgr.list_instances()?;
    let inst = instances
        .into_iter()
        .find(|i| i.id.as_str() == args.instance || i.name == args.instance)
        .ok_or_else(|| anyhow::anyhow!("Инстанс '{}' не найден", args.instance))?;

    let inst_dir = paths.instance_dir(inst.id.as_str());
    let ver_path = inst_dir.join("resolved_version.json");
    if !ver_path.exists() {
        bail!("Файл resolved_version.json не найден. Сначала выполните mq install.");
    }
    let ver_bytes = std::fs::read(&ver_path)?;
    let ver: mq_meta::ResolvedVersion = serde_json::from_slice(&ver_bytes)?;

    let asset_index_path = paths
        .assets_dir()
        .join("indexes")
        .join(format!("{}.json", ver.asset_index.id));
    let asset_bytes = std::fs::read(&asset_index_path)?;
    let asset_index: mq_meta::AssetIndex = serde_json::from_slice(&asset_bytes)?;

    let targets = collect_instance_verify_targets(&paths, &inst_dir, &ver, &asset_index);
    println!(
        "[MQ] Найдено {} файлов для глубокой проверки хэшей (SHA-1 / SHA-512)...",
        targets.len()
    );

    let num_t = threads.unwrap_or_else(default_cpu_threads);
    let pool = Arc::new(ComputePool::new(num_t));
    let cancel = Arc::new(CancelEpoch::new());
    let bus = EventBus::new();

    let started = Instant::now();
    let report = verify_files_parallel(&targets, &pool, &cancel.snapshot(), Some(&bus)).await?;

    let elapsed = started.elapsed().as_secs_f64();
    let total_mib = report.total_bytes_hashed as f64 / (1024.0 * 1024.0);
    println!("--------------------------------------------------------------------------------");
    println!("[MQ] Верификация хэшей завершена за {:.2} с!", elapsed);
    println!("  Проверено файлов:      {}", report.total_files);
    println!("  Успешно подтверждено:  {}", report.verified_ok);
    println!("  Повреждено:            {}", report.corrupted_files.len());
    println!("  Отсутствует:           {}", report.missing_files.len());
    println!(
        "  Объём данных:          {:.2} MiB ({:.2} MiB/s)",
        total_mib,
        total_mib / elapsed
    );
    println!("--------------------------------------------------------------------------------");
    if !report.corrupted_files.is_empty() || !report.missing_files.is_empty() {
        bail!("Обнаружены повреждённые или недостающие файлы! Запустите mq install для восстановления.");
    }
    Ok(())
}

async fn cmd_plan(args: PlanArgs, paths: MqPaths) -> Result<()> {
    let mgr = InstanceManager::new(paths);
    let instances = mgr.list_instances()?;
    let inst = instances
        .into_iter()
        .find(|i| i.id.as_str() == args.instance || i.name == args.instance)
        .ok_or_else(|| anyhow::anyhow!("Инстанс '{}' не найден", args.instance))?;

    let plan = build_game_install_dag_plan(&inst.mc_version, inst.loader, 110, 3200, true)?;

    println!("--------------------------------------------------------------------------------");
    println!(
        "[MQ DAG PLAN] Анализ критического пути установки инстанса '{}':",
        inst.name
    );
    println!("  Версия Minecraft:     {}", inst.mc_version);
    println!("  Модлоадер:            {:?}", inst.loader);
    println!("  Всего узлов в DAG:    {}", plan.nodes().len());
    println!("  Топологических волн:  {}", plan.waves().len());
    println!("  Критический путь (волны выполнения):");
    for (i, wave) in plan.waves().iter().enumerate() {
        println!("    Волна {}: {:?}", i, wave);
    }
    println!("--------------------------------------------------------------------------------");
    Ok(())
}

async fn cmd_auth(args: AuthArgs, paths: MqPaths) -> Result<()> {
    let mut store = AccountStore::load_or_default(&paths)?;
    let config = AppConfig::load_or_default(&paths)?;

    match args.action {
        AuthSubcommand::Offline { username } => {
            let acc = create_offline_account(&username)?;
            store.upsert_account(acc.clone(), true);
            store.save(&paths)?;
            println!(
                "[MQ] Создан и выбран оффлайн-профиль: '{}' (UUID: {})",
                acc.username, acc.uuid
            );
        }
        AuthSubcommand::List => {
            let accounts = store.list_accounts();
            println!(
                "--------------------------------------------------------------------------------"
            );
            println!("Сохранённые аккаунты MQ ({}) :", accounts.len());
            for acc in accounts {
                let active_marker = if store
                    .active_account()
                    .map(|a| a.id == acc.id)
                    .unwrap_or(false)
                {
                    "[АКТИВЕН]"
                } else {
                    "        "
                };
                println!(
                    "  {} {} | {:<9} | UUID: {}",
                    active_marker,
                    acc.username,
                    format!("{:?}", acc.kind),
                    acc.uuid
                );
            }
            println!(
                "--------------------------------------------------------------------------------"
            );
        }
        AuthSubcommand::Login(login) => {
            if let Some(nick) = login.offline {
                let acc = create_offline_account(&nick)?;
                store.upsert_account(acc.clone(), true);
                store.save(&paths)?;
                println!(
                    "[MQ] Создан и выбран оффлайн-профиль: '{}' (UUID: {})",
                    acc.username, acc.uuid
                );
            } else if login.ely {
                let user = login
                    .username
                    .ok_or_else(|| anyhow::anyhow!("Укажите --username для Ely.by"))?;
                let pass = login
                    .password
                    .ok_or_else(|| anyhow::anyhow!("Укажите --password для Ely.by"))?;
                println!("[MQ] Авторизация на authserver.ely.by...");
                let client = ElyByClient::new();
                let acc = client.authenticate(&user, &pass, None, None).await?;
                store.upsert_account(acc.clone(), true);
                store.save(&paths)?;
                println!(
                    "[MQ] Успешный вход через Ely.by! Выбран аккаунт '{}'.",
                    acc.username
                );
            } else if login.microsoft {
                let client_id = resolve_ms_client_id(&config)?;
                println!("[MQ] Запуск Microsoft Device Code Flow авторизации...");
                let client = MsaClient::new()?;
                let prompt = client.start_device_code_flow(&client_id).await?;
                println!("--------------------------------------------------------------------------------");
                println!("1. Откройте в браузере: {}", prompt.verification_uri);
                println!("2. Введите код:         {}", prompt.user_code);
                println!("Ожидание подтверждения авторизации...");
                println!("--------------------------------------------------------------------------------");
                let acc = client.poll_and_complete_login(&client_id, &prompt).await?;
                store.upsert_account(acc.clone(), true);
                store.save(&paths)?;
                println!(
                    "[MQ] Вход через Microsoft выполнен успешно! Аккаунт '{}' сохранён.",
                    acc.username
                );
            } else {
                println!("Укажите метод авторизации: --offline <nick>, --ely, или --microsoft");
            }
        }
        AuthSubcommand::Select { id } => {
            store.select_account(&id)?;
            store.save(&paths)?;
            println!("[MQ] Активный аккаунт переключен на: {}", id);
        }
        AuthSubcommand::Remove { id } => {
            store.remove_account(&id);
            store.save(&paths)?;
            println!("[MQ] Аккаунт '{}' удалён.", id);
        }
    }
    Ok(())
}

async fn cmd_mods(args: ModsArgs, paths: MqPaths, threads: Option<usize>) -> Result<()> {
    let num_t = threads.unwrap_or_else(default_cpu_threads);
    let pool = Arc::new(ComputePool::new(num_t));
    let cancel = Arc::new(CancelEpoch::new());

    match args.action {
        ModsSubcommand::Scan { instance } => {
            let mods_dir = paths.instance_dir(&instance).join("mods");
            if !mods_dir.exists() {
                bail!("Директория модов не существует: {}", mods_dir.display());
            }
            println!(
                "[MQ] Многопоточное Rayon-сканирование модов в {}...",
                mods_dir.display()
            );
            let report = scan_mods_directory(&mods_dir, &pool, &cancel.snapshot()).await?;

            println!(
                "--------------------------------------------------------------------------------"
            );
            println!(
                "Обнаружено модов: {} (за {:.2} мс)",
                report.mods.len(),
                report.scan_duration_ms
            );
            for m in &report.mods {
                let status = if m.enabled { "ВКЛ " } else { "ВЫКЛ" };
                println!(
                    "  [{}] {:<28} v{:<10} ({})",
                    status, m.name, m.version, m.file_name
                );
            }
            if !report.conflicts.is_empty() {
                println!("\nВнимание! Обнаружены конфликты / дубликаты:");
                for c in &report.conflicts {
                    match c {
                        ModConflict::DuplicateModId { mod_id, files } => {
                            println!("  Дубликат mod_id '{}' в файлах: {:?}", mod_id, files);
                        }
                        ModConflict::MissingDependency {
                            mod_id,
                            file_name,
                            required_dep,
                        } => {
                            println!(
                                "  Мод '{}' ({}) требует отсутствующий: {}",
                                mod_id, file_name, required_dep
                            );
                        }
                    }
                }
            }
            println!(
                "--------------------------------------------------------------------------------"
            );
        }
        ModsSubcommand::Search {
            query,
            source,
            loader,
            version,
        } => {
            println!("[MQ] Поиск '{}' в каталоге {}...", query, source);
            let mut s_query = SearchQuery::new(&query).with_project_kind(ProjectKind::Mod);
            if let Some(ver) = version {
                s_query = s_query.with_mc_version(ver);
            }
            if let Some(ldr) = loader.and_then(|l| match l.to_lowercase().as_str() {
                "fabric" => Some(ModLoaderKind::Fabric),
                "quilt" => Some(ModLoaderKind::Quilt),
                "forge" => Some(ModLoaderKind::Forge),
                "neoforge" => Some(ModLoaderKind::NeoForge),
                _ => None,
            }) {
                s_query = s_query.with_loader(ldr);
            }

            let hits = if source.eq_ignore_ascii_case("modrinth") {
                let client = ModrinthClient::new()?;
                client.search(&s_query).await?
            } else {
                let config = AppConfig::load_or_default(&paths)?;
                let cf_client = CurseForgeClient::from_config(&config)?;
                cf_client.search(&s_query).await?
            };

            println!(
                "--------------------------------------------------------------------------------"
            );
            for hit in hits {
                println!("* {} ({})", hit.title, hit.slug);
                println!("  {}", hit.description);
                println!("  Загрузок: {} | Автор: {}", hit.downloads, hit.author);
            }
            println!(
                "--------------------------------------------------------------------------------"
            );
        }
        ModsSubcommand::Install {
            instance,
            project,
            loader,
            version,
        } => {
            let inst_dir = paths.instance_dir(&instance);
            let mc_ver = version.unwrap_or_else(|| "1.21.1".to_string());
            let mod_loader = loader
                .map(|l| match l.to_lowercase().as_str() {
                    "quilt" => ModLoaderKind::Quilt,
                    "forge" => ModLoaderKind::Forge,
                    "neoforge" => ModLoaderKind::NeoForge,
                    _ => ModLoaderKind::Fabric,
                })
                .unwrap_or(ModLoaderKind::Fabric);

            println!(
                "[MQ] Авто-разрешение зависимостей и установка мода '{}'...",
                project
            );
            let client = ModrinthClient::new()?;
            let downloads =
                resolve_mod_with_dependencies(&client, &project, mc_ver.as_str(), mod_loader)
                    .await?;

            let net = Arc::new(NetEngine::new(NetConfig::default())?);
            let mods_dir = inst_dir.join("mods");
            std::fs::create_dir_all(&mods_dir)?;

            let mut specs = Vec::new();
            for d in &downloads {
                let mut spec = DownloadSpec::new(&d.filename, &d.url, mods_dir.join(&d.filename));
                spec.expected_sha1 = d.sha1;
                spec.expected_sha512 = d.sha512;
                spec.expected_size = Some(d.size);
                specs.push(spec);
            }

            let outcomes = net
                .download_batch(specs, "install_mods", &cancel.snapshot())
                .await?;
            let successful_count = outcomes.len();

            println!(
                "--------------------------------------------------------------------------------"
            );
            println!(
                "[MQ] Успешно установлено модов (с зависимостями): {}",
                successful_count
            );
            for d in &downloads {
                let role = if d.is_dependency {
                    "Зависимость"
                } else {
                    "Основной мод"
                };
                println!("  + [{}] {} ({})", role, d.filename, d.project_id);
            }
            println!(
                "--------------------------------------------------------------------------------"
            );
        }
        ModsSubcommand::Toggle { instance, file } => {
            let mods_dir = paths.instance_dir(&instance).join("mods");
            let new_name = toggle_mod(&mods_dir, &file)?;
            println!("[MQ] Мод переключен: {} -> {}", file, new_name.display());
        }
        ModsSubcommand::Delete { instance, file } => {
            let mods_dir = paths.instance_dir(&instance).join("mods");
            delete_mod(&mods_dir, &file)?;
            println!("[MQ] Мод '{}' удален.", file);
        }
    }
    Ok(())
}

async fn cmd_modpack(args: ModpackArgs, paths: MqPaths, threads: Option<usize>) -> Result<()> {
    let num_t = threads.unwrap_or_else(default_cpu_threads);
    let pool = Arc::new(ComputePool::new(num_t));
    let cancel = Arc::new(CancelEpoch::new());
    let bus = EventBus::new();
    let net = Arc::new(NetEngine::with_components(
        NetConfig::default(),
        Arc::new(BufferPool::new()),
        cancel.clone(),
        Some(bus.clone()),
    )?);

    match args.action {
        ModpackSubcommand::Install {
            path_or_slug,
            source: _,
            name,
        } => {
            let pack_path = PathBuf::from(&path_or_slug);
            if !pack_path.exists() {
                bail!("Файл модпака не найден: {}", pack_path.display());
            }

            let inst_name = name.unwrap_or_else(|| {
                pack_path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("Modpack")
                    .to_string()
            });
            let inst_dir = paths.instance_dir(&inst_name);

            println!("[MQ] Установка сборки модпака '{}'...", inst_name);
            let started = Instant::now();

            if pack_path.extension().and_then(|s| s.to_str()) == Some("mrpack") {
                let outcome = install_mrpack(
                    &pack_path,
                    &inst_dir,
                    &paths,
                    &net,
                    &pool,
                    &cancel.snapshot(),
                    Some(&bus),
                )
                .await?;

                println!("--------------------------------------------------------------------------------");
                println!(
                    "[MQ] Сборка Modrinth .mrpack успешно установлена за {:.2} с!",
                    started.elapsed().as_secs_f64()
                );
                println!("  Название:         {}", outcome.index.name);
                println!("  Версия сборки:    {}", outcome.index.version_id);
                println!("  Версия Minecraft: {}", outcome.minecraft_version);
                println!("  Файлов загружено: {}", outcome.installed_files.len());
                println!("  Распаковано overrides: {}", outcome.overrides_extracted);
                println!("--------------------------------------------------------------------------------");
            } else {
                let outcome = install_curseforge_zip(
                    &pack_path,
                    &inst_dir,
                    &paths,
                    &net,
                    None,
                    &pool,
                    &cancel.snapshot(),
                )
                .await?;

                println!("--------------------------------------------------------------------------------");
                println!(
                    "[MQ] Сборка CurseForge .zip успешно установлена за {:.2} с!",
                    started.elapsed().as_secs_f64()
                );
                println!("  Файлов установлено: {}", outcome);
                println!("--------------------------------------------------------------------------------");
            }
        }
    }
    Ok(())
}

/// Runs empirical multi-threading benchmark and prints Amdahl scaling table (Lesson E3).
async fn cmd_bench(args: BenchArgs) -> Result<()> {
    println!("================================================================================");
    println!("   MQ AUTONOMOUS MULTITHREADED BENCHMARK (Amdahl Law & Core Saturation)");
    println!("================================================================================");

    let num_cpus = default_cpu_threads();
    let thread_counts = match args.threads {
        Some(t) => vec![t],
        None => thread_ladder(num_cpus),
    };

    println!("[MQ BENCH] Доступно логических ядер CPU: {}", num_cpus);
    println!("[MQ BENCH] Сетка замеров потоков: {:?}", thread_counts);

    let mock_cfg = if args.quick {
        MockConfig {
            asset_count: 500,
            library_count: 30,
            modpack_mod_count: 40,
            fail_first_n_requests: 0,
        }
    } else {
        MockConfig::default() // 3200 assets, 110 libs, 220 modpack mods
    };

    println!("[MQ BENCH] Запуск локального HTTP mock-сервера (axum, zero network latency)...");
    let server = MockServer::start(mock_cfg).await;
    println!("[MQ BENCH] Сервер запущен: {}", server.base_url());

    struct BenchResult {
        threads: usize,
        elapsed_sec: f64,
        speedup: f64,
        serial_fraction: f64,
        total_cpu_pct: f32,
        per_core_cpu: Vec<f32>,
        peak_rss_mib: f64,
    }

    let mut results = Vec::new();
    let mut baseline_time = None;

    for &threads in &thread_counts {
        println!("\n>>> Тестирование при threads = {}...", threads);

        let temp_dir = tempfile::tempdir()?;
        let b_paths = MqPaths::with_root(temp_dir.path().join(".mq"));
        b_paths.ensure_dirs()?;

        let pool = Arc::new(ComputePool::new(threads));
        let cancel = Arc::new(CancelEpoch::new());
        let bus = EventBus::new();
        let net = Arc::new(NetEngine::with_components(
            NetConfig {
                max_connections: 64,
                ..Default::default()
            },
            Arc::new(BufferPool::new()),
            cancel.clone(),
            Some(bus.clone()),
        )?);
        let cas = CasStore::new(b_paths.clone());

        // Snapshot initial CPU counters
        let stat_before = read_proc_stat().unwrap_or_default();
        let bench_start = Instant::now();

        // 1. Install Game (3200+ assets, 110+ libs)
        let inst_dir = b_paths.instance_dir("bench_inst");
        let req = InstallRequest {
            mc_version: "1.21.1".to_string(),
            loader: ModLoaderKind::Vanilla,
            loader_version: None,
            instance_dir: inst_dir.clone(),
            mock_base_url: Some(server.base_url().to_string()),
        };
        let _install_summary = install_game(
            &req,
            &b_paths,
            &net,
            &cas,
            &pool,
            &cancel.snapshot(),
            Some(&bus),
        )
        .await?;

        // 2. Install Modpack (220+ mods)
        let mrpack_bytes = server.build_mrpack_bytes();
        let mrpack_file = temp_dir.path().join("bench.mrpack");
        std::fs::write(&mrpack_file, mrpack_bytes)?;
        let _modpack_summary = install_mrpack(
            &mrpack_file,
            &inst_dir,
            &b_paths,
            &net,
            &pool,
            &cancel.snapshot(),
            Some(&bus),
        )
        .await?;

        // 3. Deep Hash Verification (Verify all files in parallel via Rayon)
        let ver_path = inst_dir.join("resolved_version.json");
        let ver_bytes = std::fs::read(&ver_path)?;
        let ver: mq_meta::ResolvedVersion = serde_json::from_slice(&ver_bytes)?;

        let asset_index_path = b_paths
            .assets_dir()
            .join("indexes")
            .join(format!("{}.json", ver.asset_index.id));
        let asset_bytes = std::fs::read(&asset_index_path)?;
        let asset_index: mq_meta::AssetIndex = serde_json::from_slice(&asset_bytes)?;

        let targets = collect_instance_verify_targets(&b_paths, &inst_dir, &ver, &asset_index);
        let _verify_report =
            verify_files_parallel(&targets, &pool, &cancel.snapshot(), Some(&bus)).await?;

        // 4. Rayon Mod Jar Scanner (220+ mods metadata scan)
        let mods_dir = inst_dir.join("mods");
        let _scan_report = scan_mods_directory(&mods_dir, &pool, &cancel.snapshot()).await?;

        let elapsed_sec = bench_start.elapsed().as_secs_f64();

        // Sample final CPU utilization and RSS
        let stat_after = read_proc_stat().unwrap_or_default();
        let (total_cpu_pct, per_core_cpu) = stat_after.utilization_delta(&stat_before);
        let peak_rss_mib = read_peak_rss_mib();

        let t1 = *baseline_time.get_or_insert(elapsed_sec);
        let speedup = compute_speedup(t1, elapsed_sec);
        let serial = estimate_serial_fraction(speedup, threads);

        println!(
            "  -> Завершено за {:.3} с | Ускорение: {:.2}x | CPU Total: {:.1}% | Peak RSS: {:.1} MiB",
            elapsed_sec, speedup, total_cpu_pct, peak_rss_mib
        );

        results.push(BenchResult {
            threads,
            elapsed_sec,
            speedup,
            serial_fraction: serial,
            total_cpu_pct,
            per_core_cpu,
            peak_rss_mib,
        });
    }

    println!("\n================================================================================");
    println!("                   ИТОГОВАЯ ТАБЛИЦА МАСШТАБИРУЕМОСТИ MQ");
    println!("================================================================================");
    println!(
        "| Потоки (p) | Время (с) | Ускорение S(p) | Амдал s  | CPU Загрузка | Peak RSS (MiB) |"
    );
    println!(
        "|------------|-----------|----------------|----------|--------------|----------------|"
    );
    for r in &results {
        println!(
            "| {:<10} | {:<9.3} | {:<14.2} | {:<8.3} | {:<12.1}% | {:<14.1} |",
            r.threads, r.elapsed_sec, r.speedup, r.serial_fraction, r.total_cpu_pct, r.peak_rss_mib
        );
    }
    println!("================================================================================");
    println!("\nПоядерная утилизация CPU (/proc/stat):");
    for r in &results {
        print!("  threads={:<2}: [", r.threads);
        for (i, core) in r.per_core_cpu.iter().enumerate() {
            if i > 0 {
                print!(", ");
            }
            print!("cpu{}: {:.1}%", i, core);
        }
        println!("]");
    }
    println!("================================================================================");

    Ok(())
}
