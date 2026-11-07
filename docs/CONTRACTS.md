# MQ Architecture Contracts & API Specifications (v2.1.0)

This document establishes the binding public contracts, data models, and component interfaces across all 17 subagents for parallel development.

---

## 1. UI Design Tokens (`crates/mq-ui/src/theme.rs`)

Colors, spacing, and styling must strictly reference `Tokens` and theme constants. No hardcoded colors outside `theme.rs`.

```rust
pub struct Tokens {
    pub bg_deep: egui::Color32,
    pub bg_surface: egui::Color32,
    pub bg_surface_hover: egui::Color32,
    pub bg_card: egui::Color32,
    pub bg_input: egui::Color32,
    pub border: egui::Color32,
    pub border_strong: egui::Color32,
    pub text: egui::Color32,
    pub text_muted: egui::Color32,
    pub accent: egui::Color32,
    pub accent_hover: egui::Color32,
    pub accent_text: egui::Color32,
    pub success: egui::Color32,
    pub warning: egui::Color32,
    pub danger: egui::Color32,
}

pub const RADIUS_CARD: f32 = 8.0;
pub const RADIUS_BUTTON: f32 = 6.0;
pub const RADIUS_CHIP: f32 = 4.0;
pub const SPACING_XS: f32 = 4.0;
pub const SPACING_SM: f32 = 8.0;
pub const SPACING_MD: f32 = 12.0;
pub const SPACING_LG: f32 = 16.0;
```

---

## 2. Reusable UI Components (`crates/mq-ui/src/components/`)

All interactive elements adhere to fixed 32px standard control height, pixel-snapped coordinates, and sentence-case labels.

### 2.1 `SearchField` (`components/search.rs` / `components/input.rs`)
```rust
pub struct SearchField<'a> {
    pub query: &'a mut String,
    pub placeholder: &'a str,
    pub width: Option<f32>,
    pub focus_on_ctrl_f: bool,
}
impl<'a> SearchField<'a> {
    pub fn new(query: &'a mut String, placeholder: &'a str) -> Self;
    pub fn show(self, ui: &mut egui::Ui) -> egui::Response;
}
```

### 2.2 `Select` / Dropdown (`components/select.rs`)
```rust
pub struct Select<'a, T: PartialEq + Clone> {
    pub label: &'a str,
    pub selected: &'a mut T,
    pub options: Vec<(T, String)>,
}
impl<'a, T: PartialEq + Clone> Select<'a, T> {
    pub fn show(self, ui: &mut egui::Ui) -> egui::Response;
}
```

### 2.3 `Segmented` Switch (`components/segmented.rs`)
```rust
pub struct SegmentedOption<T: PartialEq + Clone> {
    pub value: T,
    pub label: String,
    pub icon: Option<VectorIcon>,
    pub tooltip: Option<String>,
}
pub fn segmented_switch<T: PartialEq + Clone>(
    ui: &mut egui::Ui,
    selected: &mut T,
    options: &[SegmentedOption<T>],
) -> egui::Response;
```

### 2.4 `Chip` Toggle (`components/chip.rs`)
```rust
pub struct FilterChip<'a> {
    pub label: &'a str,
    pub active: bool,
    pub dot_color: Option<egui::Color32>,
    pub count: Option<usize>,
}
impl<'a> FilterChip<'a> {
    pub fn show(self, ui: &mut egui::Ui) -> egui::Response;
}
```

### 2.5 `Tabs` with Underline (`components/tabs.rs`)
```rust
pub fn underline_tabs<T: PartialEq + Clone>(
    ui: &mut egui::Ui,
    selected: &mut T,
    tabs: &[(T, &str, Option<VectorIcon>)],
) -> egui::Response;
```

### 2.6 `EmptyState` & `ErrorState` (`components/empty.rs`)
```rust
pub struct EmptyStateConfig<'a> {
    pub icon: VectorIcon,
    pub title: &'a str,
    pub description: &'a str,
    pub action_label: Option<&'a str>,
}
pub fn render_empty_state(ui: &mut egui::Ui, config: &EmptyStateConfig) -> bool;
```

---

## 3. UI ↔ Core Worker Event Bus Contracts (`crates/mq-core/src/events.rs`)

Worker channels pass strongly-typed asynchronous messages:

```rust
pub enum UiWorkerCommand {
    LaunchInstance { instance_id: InstanceId, offline: bool },
    AbortLaunch { instance_id: InstanceId },
    ScanInstanceMods { instance_id: InstanceId },
    InstallMod { instance_id: InstanceId, project_id: String, version_id: String },
    UninstallMod { instance_id: InstanceId, filename: String },
    ResolveAndInstallMod { instance_id: InstanceId, project_id: String, version_id: String, with_optional: Vec<String> },
    CheckLauncherUpdates { background: bool },
    ApplyLauncherUpdate,
    RollbackLauncherUpdate,
    StartElyByBrowserLogin,
    CancelElyByLogin,
}

pub enum UiWorkerEvent {
    LaunchLog { line: String, level: GameLogLevel },
    LaunchFinished { instance_id: InstanceId, exit_code: i32 },
    CrashDetected { summary: String, recommendation: String, suspected_mods: Vec<String> },
    UpdateCheckFinished(Result<Option<UpdateInfo>, String>),
    UpdateDownloadProgress { downloaded_bytes: u64, total_bytes: Option<u64> },
    UpdateReadyToRestart { new_version: String },
    UpdateFailed(String),
    ElyByBrowserWaiting { auth_url: String },
    ElyBySuccess { username: String, uuid: String },
    ElyByFailed(String),
}
```

---

## 4. Mod Lockfile Format (`mods/.mq-lock.json`)

Stored inside each instance folder: `<instance_dir>/mods/.mq-lock.json`.

```json
{
  "version": 1,
  "updated_at": "2026-11-07T12:00:00Z",
  "entries": {
    "fabric-api": {
      "project_id": "P7dR8mSH",
      "version_id": "9t1W5wY2",
      "filename": "fabric-api-0.100.0+1.21.1.jar",
      "sha512": "3a7b...",
      "installed_as_dependency": true,
      "required_by": ["sodium", "iris"]
    },
    "sodium": {
      "project_id": "AANobbMI",
      "version_id": "5Y8T2o1q",
      "filename": "sodium-fabric-0.5.11+mc1.21.1.jar",
      "sha512": "9f2c...",
      "installed_as_dependency": false,
      "required_by": []
    }
  }
}
```

Rust struct representation (`crates/mq-mods/src/lockfile.rs`):
```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModLockFile {
    pub version: u32,
    pub updated_at: chrono::DateTime<chrono::Utc>,
    pub entries: HashMap<String, ModLockEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModLockEntry {
    pub project_id: String,
    pub version_id: String,
    pub filename: String,
    pub sha512: String,
    pub installed_as_dependency: bool,
    pub required_by: Vec<String>,
}
```

---

## 5. Options.txt API (`crates/mq-launch/src/options.rs`)

Single-owner atomic mutations for `options.txt` preserving non-managed keys:

```rust
pub struct MinecraftOptionsManager;

impl MinecraftOptionsManager {
    /// Preserves user choice if `lang_initialized` flag is set.
    pub fn sync_lang(options_path: &Path, lang: &str, force: bool) -> MqResult<()>;

    /// Updates resource packs list atomically: `resourcePacks:[...]` and `incompatibleResourcePacks:[...]`.
    pub fn update_resource_packs(options_path: &Path, packs: &[String]) -> MqResult<()>;

    /// Reads active resource pack ordering.
    pub fn read_resource_packs(options_path: &Path) -> MqResult<Vec<String>>;
}
```

---

## 6. Self-Update Engine (`crates/mq-update/src/lib.rs`)

```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReleaseAssetInfo {
    pub version: String,
    pub release_notes: String,
    pub binary_url: String,
    pub binary_sha256: String,
    pub tarball_url: String,
    pub published_at: chrono::DateTime<chrono::Utc>,
    pub is_prerelease: bool,
}

#[derive(Debug, Clone)]
pub enum UpdateState {
    Idle,
    Checking,
    UpdateAvailable(ReleaseAssetInfo),
    Downloading { progress: f32, bytes: u64, total: Option<u64> },
    Verifying,
    ReadyToRestart { exe_path: PathBuf, old_path: PathBuf },
    PackageManagerManaged { message: String, release_url: String },
    Error(String),
}

pub struct UpdateManager {
    repo_owner: String,
    repo_name: String,
}

impl UpdateManager {
    pub fn new(repo_owner: &str, repo_name: &str) -> Self;
    pub async fn check_latest(&self, current_version: &str, include_prereleases: bool) -> MqResult<Option<ReleaseAssetInfo>>;
    pub async fn download_and_install<F>(&self, asset: &ReleaseAssetInfo, on_progress: F) -> MqResult<PathBuf>
    where F: Fn(u64, Option<u64>) + Send + 'static;
    pub fn rollback() -> MqResult<()>;
    pub fn is_packaged_install() -> bool;
    pub fn restart_process(args: &[String]) -> MqResult<()>;
}
```

---

## 7. Ely.by OAuth2 Browser-Only API (`crates/mq-auth/src/elyby.rs`)

Strictly browser-based authentication flow. No credential fields in UI or CLI.

```rust
pub struct ElyByOAuthConfig {
    pub client_id: Option<String>,
    pub client_secret: Option<String>,
    pub redirect_port: u16,
}

#[derive(Debug, Clone)]
pub enum ElyByLoginState {
    Idle,
    MissingClientId { help_url: String },
    Listening { auth_url: String, port: u16, timeout_secs: u64 },
    ExchangingCode,
    Completed { username: String, uuid: String },
    Failed(String),
}

pub struct ElyByOAuthFlow;

impl ElyByOAuthFlow {
    pub fn generate_auth_url(client_id: &str, port: u16, state: &str) -> String;
    pub async fn start_loopback_listener(port: u16, expected_state: String) -> MqResult<String>;
    pub async fn exchange_code_for_token(client: &reqwest::Client, client_id: &str, client_secret: Option<&str>, code: &str, redirect_uri: &str) -> MqResult<ElyByTokenResponse>;
    pub async fn fetch_account_info(client: &reqwest::Client, access_token: &str) -> MqResult<ElyByAccountInfo>;
    pub async fn refresh_token(client: &reqwest::Client, client_id: &str, client_secret: Option<&str>, refresh_token: &str) -> MqResult<ElyByTokenResponse>;
}
```
