#![deny(clippy::await_holding_lock)]
//! Installation state tracking (`install_state.json`), structured error models (`InstallError`),
//! disk space verification (`statvfs`), and inter-process/in-process installation concurrency locking.

use std::fs::File;
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock};

use dashmap::DashMap;
use mq_core::{MqError, MqResult};
use serde::{Deserialize, Serialize};

/// Installation stages defined in `CONTRACTS.md`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InstallStage {
    /// Загрузка и разбор официального манифеста версий Mojang
    Manifest,
    /// Загрузка и разбор version.json (vanilla / inheritsFrom)
    VersionJson,
    /// Проверка системной Java и загрузка необходимой JRE (Mojang / Adoptium)
    JavaRuntime,
    /// Загрузка исполняемого client.jar
    ClientJar,
    /// Загрузка общих Maven-библиотек
    Libraries,
    /// Распаковка и подготовка нативных динамических библиотек (.so / .dll)
    Natives,
    /// Загрузка игровых ресурсов и ассетов
    Assets,
    /// Проверка модлоадера (Fabric / Quilt / Forge / NeoForge)
    ModLoader,
    /// Верификация хэшей (SHA-1 / SHA-512) и целостности
    Verification,
    /// Фиксация состояния, запись install_state.json и завершение
    Finalize,
}

impl InstallStage {
    #[must_use]
    pub fn localized_name(&self) -> &'static str {
        match self {
            Self::Manifest => "Манифест версии",
            Self::VersionJson => "Конфигурация версии",
            Self::JavaRuntime => "Среда выполнения Java",
            Self::ClientJar => "Игровой клиент",
            Self::Libraries => "Библиотеки",
            Self::Natives => "Нативные библиотеки",
            Self::Assets => "Игровые ресурсы",
            Self::ModLoader => "Модлоадер",
            Self::Verification => "Проверка файлов",
            Self::Finalize => "Финализация",
        }
    }
}

/// Structured installation error contract from `CONTRACTS.md`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct InstallError {
    pub stage: InstallStage,
    pub localized_title: String,
    pub localized_details: String,
    pub target_url: Option<String>,
    pub file_path: Option<String>,
    pub http_status: Option<u16>,
    pub is_retryable: bool,
    pub raw_cause: String,
}

impl std::fmt::Display for InstallError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "[{}] {}: {} (cause: {})",
            self.stage.localized_name(),
            self.localized_title,
            self.localized_details,
            self.raw_cause
        )
    }
}

impl std::error::Error for InstallError {}

impl InstallError {
    #[must_use]
    pub fn from_mq_error(stage: InstallStage, err: &MqError) -> Self {
        let is_retryable = err.is_retryable();
        let (title, details, target_url, http_status) = match err {
            MqError::HttpStatus { status, url, body } => (
                format!("Сетевая ошибка HTTP {status}"),
                format!("Сервер вернул ошибку {status} при обращении к {url}: {body}"),
                Some(url.clone()),
                Some(*status),
            ),
            MqError::HashMismatch {
                target,
                algo,
                expected,
                actual,
            } => (
                "Ошибка контрольной суммы".to_string(),
                format!(
                    "Несоответствие контрольной суммы {algo} для {target}: ожидалось {expected}, получено {actual}"
                ),
                None,
                None,
            ),
            MqError::Network(msg) => (
                "Сетевая ошибка".to_string(),
                format!("Сбой при передаче данных по сети: {msg}"),
                None,
                None,
            ),
            MqError::Cancelled { epoch } => (
                "Установка отменена".to_string(),
                format!("Операция установки была отменена (эпоха {epoch})"),
                None,
                None,
            ),
            MqError::Io(io_err) => (
                "Ошибка дисковой подсистемы".to_string(),
                format!("Ошибка ввода-вывода: {io_err}"),
                None,
                None,
            ),
            _ => (
                "Ошибка установки".to_string(),
                err.to_string(),
                None,
                None,
            ),
        };

        Self {
            stage,
            localized_title: title,
            localized_details: details,
            target_url,
            file_path: None,
            http_status,
            is_retryable,
            raw_cause: err.to_string(),
        }
    }
}

/// Installation status values supported in `install_state.json`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InstallStatus {
    Installing,
    Ready,
    Failed,
    Verifying,
}

/// Java runtime metadata saved in `install_state.json`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct InstallJavaInfo {
    pub major_version: u32,
    pub path: PathBuf,
    pub source: String,
    pub component: Option<String>,
}

/// Client JAR verification record saved in `install_state.json`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct InstallClientJarInfo {
    pub sha1: String,
    pub size: u64,
}

pub const INSTALL_STATE_SCHEMA_VERSION: u32 = 1;
pub const INSTALL_STATE_FILENAME: &str = "install_state.json";

/// Root structure of `<instance_dir>/install_state.json` adhering to `CONTRACTS.md`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct InstallState {
    pub schema_version: u32,
    pub instance_id: String,
    pub mc_version: String,
    pub loader: String,
    pub loader_version: Option<String>,
    pub status: InstallStatus,
    pub installed_at: Option<String>,
    pub verified_at: Option<String>,
    pub java: Option<InstallJavaInfo>,
    pub client_jar: Option<InstallClientJarInfo>,
    pub libraries_count: usize,
    pub assets_count: usize,
    pub asset_index_id: Option<String>,
    pub natives_count: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_error: Option<InstallError>,
}

impl InstallState {
    #[must_use]
    pub fn new_installing(
        instance_id: impl Into<String>,
        mc_version: impl Into<String>,
        loader: impl Into<String>,
        loader_version: Option<String>,
    ) -> Self {
        Self {
            schema_version: INSTALL_STATE_SCHEMA_VERSION,
            instance_id: instance_id.into(),
            mc_version: mc_version.into(),
            loader: loader.into(),
            loader_version,
            status: InstallStatus::Installing,
            installed_at: None,
            verified_at: None,
            java: None,
            client_jar: None,
            libraries_count: 0,
            assets_count: 0,
            asset_index_id: None,
            natives_count: 0,
            last_error: None,
        }
    }

    /// Loads `<instance_dir>/install_state.json` if it exists and is valid.
    #[must_use]
    pub fn load(instance_dir: &Path) -> Option<Self> {
        let path = instance_dir.join(INSTALL_STATE_FILENAME);
        let content = std::fs::read_to_string(path).ok()?;
        serde_json::from_str(&content).ok()
    }

    /// Atomically persists `<instance_dir>/install_state.json` via a temporary file + rename.
    pub fn save(&self, instance_dir: &Path) -> MqResult<()> {
        let path = instance_dir.join(INSTALL_STATE_FILENAME);
        let pid = std::process::id();
        let tmp_path = instance_dir.join(format!(".{INSTALL_STATE_FILENAME}.{pid}.tmp"));

        let json = serde_json::to_string_pretty(self).map_err(MqError::Json)?;

        std::fs::create_dir_all(instance_dir)?;
        std::fs::write(&tmp_path, json.as_bytes())?;
        std::fs::rename(&tmp_path, &path)?;
        Ok(())
    }

    /// Evaluates instance launch readiness.
    ///
    /// Requires status == `Ready` AND actual presence of:
    /// - `client.jar` (with matching file length when `client_jar.size > 0`)
    /// - `resolved_version.json`
    /// - `natives/` directory
    #[must_use]
    pub fn is_ready(&self, instance_dir: &Path) -> bool {
        if self.status != InstallStatus::Ready {
            return false;
        }

        let client_jar_path = instance_dir.join("client.jar");
        if !client_jar_path.is_file() {
            return false;
        }
        if let Some(ref cjar) = self.client_jar {
            if cjar.size > 0 {
                if let Ok(meta) = client_jar_path.metadata() {
                    if meta.len() != cjar.size {
                        return false;
                    }
                } else {
                    return false;
                }
            }
        }

        let resolved_json = instance_dir.join("resolved_version.json");
        if !resolved_json.is_file() {
            return false;
        }

        let natives_dir = instance_dir.join("natives");
        if !natives_dir.is_dir() {
            return false;
        }

        true
    }
}

/// Queries the filesystem for available unprivileged disk space using `rustix::fs::statvfs`.
pub fn available_disk_space(path: &Path) -> MqResult<u64> {
    let mut check_path = path;
    while !check_path.exists() {
        match check_path.parent() {
            Some(p) if !p.as_os_str().is_empty() => check_path = p,
            _ => {
                check_path = Path::new(".");
                break;
            }
        }
    }

    let stat = rustix::fs::statvfs(check_path).map_err(|e| {
        MqError::Io(std::io::Error::new(
            std::io::ErrorKind::Other,
            format!(
                "failed to query disk space for '{}': {e}",
                check_path.display()
            ),
        ))
    })?;

    let block_size = if stat.f_frsize > 0 {
        stat.f_frsize as u64
    } else {
        stat.f_bsize as u64
    };

    Ok(stat.f_bavail as u64 * block_size)
}

/// Checks that the filesystem hosting `path` has at least `required_bytes` free space available.
pub fn check_free_disk_space(path: &Path, required_bytes: u64) -> MqResult<()> {
    let available = available_disk_space(path)?;
    if available < required_bytes {
        let req_mib = required_bytes as f64 / (1024.0 * 1024.0);
        let avail_mib = available as f64 / (1024.0 * 1024.0);
        return Err(MqError::Io(std::io::Error::new(
            std::io::ErrorKind::Other,
            format!(
                "Недостаточно свободного места на диске для установки: требуется {:.1} МиБ, доступно только {:.1} МиБ (путь: {})",
                req_mib,
                avail_mib,
                path.display()
            ),
        )));
    }
    Ok(())
}

static IN_PROCESS_LOCKS: LazyLock<DashMap<PathBuf, Arc<tokio::sync::Mutex<()>>>> =
    LazyLock::new(DashMap::new);

/// RAII lock preventing concurrent installations of the same instance across threads and processes.
#[derive(Debug)]
pub struct InstanceInstallLock {
    lock_file_path: PathBuf,
    file: File,
    _permit: tokio::sync::OwnedMutexGuard<()>,
}

impl InstanceInstallLock {
    pub async fn acquire(instance_dir: &Path, instance_id: &str) -> MqResult<Self> {
        std::fs::create_dir_all(instance_dir)?;
        let canonical_dir = std::fs::canonicalize(instance_dir).unwrap_or_else(|_| instance_dir.to_path_buf());

        // 1. In-process locking
        let mutex = IN_PROCESS_LOCKS
            .entry(canonical_dir)
            .or_insert_with(|| Arc::new(tokio::sync::Mutex::new(())))
            .clone();

        let permit = match mutex.clone().try_lock_owned() {
            Ok(guard) => guard,
            Err(_) => {
                return Err(MqError::Internal(format!(
                    "Установка инстанса '{instance_id}' уже выполняется в другом потоке приложения"
                )));
            }
        };

        // 2. Inter-process file locking via flock
        let lock_file_path = instance_dir.join(".install.lock");
        let file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&lock_file_path)?;

        match rustix::fs::flock(&file, rustix::fs::FlockOperation::NonBlockingLockExclusive) {
            Ok(()) => {}
            Err(err) => {
                drop(permit);
                return Err(MqError::Internal(format!(
                    "Установка инстанса '{instance_id}' уже выполняется другим процессом ({err})"
                )));
            }
        }

        Ok(Self {
            lock_file_path,
            file,
            _permit: permit,
        })
    }
}

impl Drop for InstanceInstallLock {
    fn drop(&mut self) {
        let _ = rustix::fs::flock(&self.file, rustix::fs::FlockOperation::Unlock);
        let _ = std::fs::remove_file(&self.lock_file_path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn install_state_serialization_conforms_to_contracts_schema() {
        let sample_json = r#"{
  "schema_version": 1,
  "instance_id": "26.3-vanilla",
  "mc_version": "26.3",
  "loader": "vanilla",
  "loader_version": null,
  "status": "ready",
  "installed_at": "2026-11-07T22:54:33Z",
  "verified_at": "2026-11-07T22:54:33Z",
  "java": {
    "major_version": 25,
    "path": "/home/honey/.local/share/mq/java/mojang-java-runtime-epsilon/bin/java",
    "source": "mojang",
    "component": "java-runtime-epsilon"
  },
  "client_jar": {
    "sha1": "abcdef1234567890abcdef1234567890abcdef12",
    "size": 34123456
  },
  "libraries_count": 78,
  "assets_count": 5114,
  "asset_index_id": "34",
  "natives_count": 0
}"#;

        let parsed: InstallState =
            serde_json::from_str(sample_json).expect("deserialize CONTRACTS.md JSON");
        assert_eq!(parsed.schema_version, 1);
        assert_eq!(parsed.instance_id, "26.3-vanilla");
        assert_eq!(parsed.mc_version, "26.3");
        assert_eq!(parsed.status, InstallStatus::Ready);
        assert_eq!(parsed.libraries_count, 78);
        assert_eq!(parsed.assets_count, 5114);
        assert_eq!(parsed.asset_index_id.as_deref(), Some("34"));
        assert!(parsed.last_error.is_none());

        let java = parsed.java.as_ref().expect("java info");
        assert_eq!(java.major_version, 25);
        assert_eq!(java.source, "mojang");
        assert_eq!(java.component.as_deref(), Some("java-runtime-epsilon"));

        let client = parsed.client_jar.as_ref().expect("client jar info");
        assert_eq!(client.size, 34123456);
        assert_eq!(client.sha1, "abcdef1234567890abcdef1234567890abcdef12");

        // Roundtrip serialization
        let re_serialized = serde_json::to_string(&parsed).expect("serialize");
        let re_parsed: InstallState = serde_json::from_str(&re_serialized).expect("re-parse");
        assert_eq!(parsed, re_parsed);
    }

    #[test]
    fn install_state_is_ready_checks_files_and_status() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let inst_dir = tmp.path();

        let mut state = InstallState::new_installing("test-inst", "1.21.4", "vanilla", None);
        assert!(!state.is_ready(inst_dir));

        // Mark ready but files not created yet -> false
        state.status = InstallStatus::Ready;
        state.client_jar = Some(InstallClientJarInfo {
            sha1: "abc".into(),
            size: 100,
        });
        assert!(!state.is_ready(inst_dir));

        // Create client.jar with wrong size -> false
        std::fs::write(inst_dir.join("client.jar"), vec![0u8; 50]).unwrap();
        assert!(!state.is_ready(inst_dir));

        // Fix client.jar size -> still missing resolved_version.json and natives -> false
        std::fs::write(inst_dir.join("client.jar"), vec![0u8; 100]).unwrap();
        assert!(!state.is_ready(inst_dir));

        // Create resolved_version.json -> still missing natives -> false
        std::fs::write(inst_dir.join("resolved_version.json"), b"{}").unwrap();
        assert!(!state.is_ready(inst_dir));

        // Create natives directory -> now is_ready is true!
        std::fs::create_dir_all(inst_dir.join("natives")).unwrap();
        assert!(state.is_ready(inst_dir));

        // Save and reload from disk
        state.save(inst_dir).expect("save state");
        let loaded = InstallState::load(inst_dir).expect("load state");
        assert_eq!(loaded.status, InstallStatus::Ready);
        assert!(loaded.is_ready(inst_dir));
    }

    #[test]
    fn check_free_disk_space_validates_realistic_and_unrealistic_sizes() {
        let tmp = tempfile::tempdir().expect("tempdir");
        // Realistic request (100 KiB) must succeed
        assert!(check_free_disk_space(tmp.path(), 100 * 1024).is_ok());

        // Unrealistic request (100 Petabytes) must fail with informative error
        let huge_bytes = 100 * 1024 * 1024 * 1024 * 1024 * 1024u64;
        let err = check_free_disk_space(tmp.path(), huge_bytes).expect_err("should fail");
        let msg = err.to_string();
        assert!(msg.contains("Недостаточно свободного места на диске"));
        assert!(msg.contains("МиБ"));
    }

    #[tokio::test]
    async fn instance_install_lock_prevents_concurrent_runs() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let inst_dir = tmp.path();

        let lock1 = InstanceInstallLock::acquire(inst_dir, "test-instance")
            .await
            .expect("first lock must succeed");

        // Concurrent acquire on the same instance directory must fail
        let lock2_err = InstanceInstallLock::acquire(inst_dir, "test-instance").await;
        assert!(lock2_err.is_err());
        let err_msg = lock2_err.unwrap_err().to_string();
        assert!(err_msg.contains("уже выполняется"));

        // Dropping lock1 releases both in-process mutex and flock
        drop(lock1);

        let lock3 = InstanceInstallLock::acquire(inst_dir, "test-instance")
            .await
            .expect("re-acquire after drop must succeed");
        drop(lock3);
    }
}
