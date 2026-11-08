//! Crash reporting, secret-scrubbed panic hook (`crash-<YYYY-MM-DDTHH-MM-SS>.log`),
//! and persistent installation state journal (`install.journal`).

use std::fmt::Write as FmtWrite;
use std::fs;
use std::panic::{self, PanicHookInfo};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, Once};

use chrono::Utc;
use serde::{Deserialize, Serialize};

use crate::error::MqResult;
use crate::fs_util::{remove_file_with_retry, rename_with_retry};
use crate::paths::MqPaths;
use crate::secret::REDACTED_PLACEHOLDER;

static PANIC_HOOK_PATHS: Mutex<Option<MqPaths>> = Mutex::new(None);
static PANIC_HOOK_ONCE: Once = Once::new();

/// Persistent marker and stage journal for in-flight game installations (`<data_dir>/install.journal`).
///
/// Remains `active: true` while an installation is running. If the launcher terminates
/// abnormally (panic, driver abort, OOM, power loss), the next launch detects `active == true`
/// via [`InstallJournal::load_unclosed`] and offers a recovery dialog.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct InstallJournal {
    pub active: bool,
    pub instance_id: String,
    pub mc_version: String,
    pub loader: String,
    pub stage: String,
    pub started_at_utc: String,
    pub updated_at_utc: String,
    pub pid: u32,
    pub breadcrumbs: Vec<String>,
    pub last_error: Option<String>,
}

impl InstallJournal {
    /// Creates and atomically persists a new active [`InstallJournal`] at the start of an installation.
    pub fn start(
        paths: &MqPaths,
        instance_id: &str,
        mc_version: &str,
        loader: &str,
        initial_stage: &str,
    ) -> MqResult<Self> {
        let now = Utc::now().to_rfc3339();
        let initial_crumb = format!(
            "[{}] Старт установки {} ({}): {}",
            now,
            sanitize_crash_text(mc_version),
            sanitize_crash_text(loader),
            sanitize_crash_text(initial_stage)
        );
        let journal = Self {
            active: true,
            instance_id: instance_id.to_owned(),
            mc_version: mc_version.to_owned(),
            loader: loader.to_owned(),
            stage: initial_stage.to_owned(),
            started_at_utc: now.clone(),
            updated_at_utc: now,
            pid: std::process::id(),
            breadcrumbs: vec![initial_crumb],
            last_error: None,
        };
        journal.save(paths)?;
        Ok(journal)
    }

    /// Updates the current stage and appends a sanitized breadcrumb, saving atomically to disk.
    pub fn update_stage(
        &mut self,
        paths: &MqPaths,
        stage: &str,
        breadcrumb: impl Into<String>,
    ) -> MqResult<()> {
        let now = Utc::now().to_rfc3339();
        self.stage = stage.to_owned();
        self.updated_at_utc = now.clone();
        let crumb_raw = breadcrumb.into();
        let crumb_clean = sanitize_crash_text(&crumb_raw);
        self.breadcrumbs
            .push(format!("[{now}] [{stage}] {crumb_clean}"));
        self.save(paths)
    }

    /// Alias for [`InstallJournal::update_stage`].
    #[inline]
    pub fn record_stage(
        &mut self,
        paths: &MqPaths,
        stage: &str,
        breadcrumb: impl Into<String>,
    ) -> MqResult<()> {
        self.update_stage(paths, stage, breadcrumb)
    }

    /// Records a sanitized error message in `last_error` without closing or opening the journal.
    pub fn record_error(&mut self, paths: &MqPaths, error: impl Into<String>) -> MqResult<()> {
        let now = Utc::now().to_rfc3339();
        let clean = sanitize_crash_text(&error.into());
        self.updated_at_utc = now.clone();
        self.breadcrumbs.push(format!("[{now}] ОШИБКА: {clean}"));
        self.last_error = Some(clean);
        self.save(paths)
    }

    /// Marks the installation journal as cleanly closed (`active = false`) and persists it atomically.
    pub fn mark_closed(&mut self, paths: &MqPaths) -> MqResult<()> {
        let now = Utc::now().to_rfc3339();
        self.active = false;
        self.updated_at_utc = now.clone();
        self.breadcrumbs
            .push(format!("[{now}] Установка завершена штатно (active=false)"));
        self.save(paths)
    }

    /// Marks any existing `install.journal` on disk as closed (`active = false`).
    /// If no journal exists on disk, returns `Ok(())`.
    pub fn mark_closed_at(paths: &MqPaths) -> MqResult<()> {
        if let Some(mut journal) = Self::load_any(paths) {
            if journal.active {
                journal.mark_closed(paths)?;
            }
        }
        Ok(())
    }

    /// Loads `install.journal` if it exists on disk and has `active == true`
    /// (indicating an unclosed / interrupted previous installation).
    pub fn load_unclosed(paths: &MqPaths) -> Option<Self> {
        Self::load_any(paths).filter(|j| j.active)
    }

    /// Loads `install.journal` from disk regardless of its `active` flag.
    pub fn load_any(paths: &MqPaths) -> Option<Self> {
        let path = paths.install_journal_path();
        let bytes = fs::read(path).ok()?;
        serde_json::from_slice::<Self>(&bytes).ok()
    }

    /// Atomically persists the journal to `<data_dir>/install.journal` using a temporary file
    /// and [`rename_with_retry`].
    pub fn save(&self, paths: &MqPaths) -> MqResult<()> {
        let target = paths.install_journal_path();
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)?;
        }
        let payload = serde_json::to_vec_pretty(self)?;
        let tmp_path = target.with_extension(format!("journal.tmp.{}", std::process::id()));
        fs::write(&tmp_path, &payload)?;
        if let Err(err) = rename_with_retry(&tmp_path, &target) {
            let _ = remove_file_with_retry(&tmp_path);
            return Err(err.into());
        }
        Ok(())
    }

    /// Formats a Russian, secret-sanitized diagnostic summary suitable for clipboard export
    /// («Скопировать диагностику»).
    pub fn format_diagnostic_summary(&self, paths: &MqPaths) -> String {
        let mut out = String::new();
        let _ = writeln!(
            out,
            "=== Диагностический отчёт MQ Launcher v{} ===",
            env!("CARGO_PKG_VERSION")
        );
        let _ = writeln!(
            out,
            "ОС / Архитектура: {} ({})",
            std::env::consts::OS,
            std::env::consts::ARCH
        );
        let _ = writeln!(out, "Инстанс: {}", self.instance_id);
        let _ = writeln!(out, "Версия Minecraft: {}", self.mc_version);
        let _ = writeln!(out, "Модлоадер: {}", self.loader);
        let _ = writeln!(out, "Этап прерывания: {}", self.stage);
        let _ = writeln!(out, "PID процесса: {}", self.pid);
        let _ = writeln!(out, "Время старта (UTC): {}", self.started_at_utc);
        let _ = writeln!(out, "Последнее обновление (UTC): {}", self.updated_at_utc);
        if let Some(err) = &self.last_error {
            let _ = writeln!(out, "Последняя ошибка: {err}");
        }

        if !self.breadcrumbs.is_empty() {
            let _ = writeln!(out, "\nХронология этапов (breadcrumbs):");
            for crumb in &self.breadcrumbs {
                let _ = writeln!(out, "  - {crumb}");
            }
        }

        let latest_crash = paths.logs_dir().join("crash-latest.log");
        if let Ok(crash_text) = fs::read_to_string(&latest_crash) {
            let tail: Vec<&str> = crash_text.lines().rev().take(40).collect();
            if !tail.is_empty() {
                let _ = writeln!(out, "\nПоследние строки журнала аварии (crash-latest.log):");
                for line in tail.into_iter().rev() {
                    let _ = writeln!(out, "  {line}");
                }
            }
        }

        sanitize_crash_text(&out)
    }

    /// Alias for [`InstallJournal::format_diagnostic_summary`].
    #[inline]
    pub fn format_diagnostic_report(&self, paths: &MqPaths) -> String {
        self.format_diagnostic_summary(paths)
    }
}

/// Installs the global panic hook (`std::panic::set_hook`) configured to write sanitized
/// crash logs into `paths.logs_dir()` and update `paths.install_journal_path()` if an
/// installation is currently active.
pub fn install_panic_hook(paths: &MqPaths) {
    if let Ok(mut guard) = PANIC_HOOK_PATHS.lock() {
        *guard = Some(paths.clone());
    }

    PANIC_HOOK_ONCE.call_once(|| {
        let prev_hook = panic::take_hook();
        panic::set_hook(Box::new(move |info: &PanicHookInfo<'_>| {
            handle_panic_hook(info);
            prev_hook(info);
        }));
    });
}

fn handle_panic_hook(info: &PanicHookInfo<'_>) {
    let maybe_paths = PANIC_HOOK_PATHS
        .lock()
        .ok()
        .and_then(|g| g.clone())
        .or_else(|| MqPaths::platform_default().ok());

    let payload_msg = extract_panic_message(info);
    let location = info
        .location()
        .map(|loc| (loc.file(), loc.line(), loc.column()));
    let thread_name = std::thread::current()
        .name()
        .unwrap_or("unnamed")
        .to_owned();
    let backtrace = std::backtrace::Backtrace::force_capture().to_string();

    if let Some(paths) = maybe_paths {
        match write_crash_report_for_panic(&paths, &payload_msg, location, &thread_name, &backtrace)
        {
            Ok(log_path) => {
                eprintln!("[MQ CRASH] Записан отчёт об аварии: {}", log_path.display());
                #[cfg(target_os = "windows")]
                {
                    let sanitized_msg = sanitize_crash_text(&payload_msg);
                    show_windows_crash_dialog(&log_path, &sanitized_msg);
                }
            }
            Err(err) => {
                eprintln!("[MQ CRASH] Не удалось записать лог аварии: {err}");
            }
        }
    }
}

/// Extracts a human-readable string from a panic payload (`&str` or `String`).
pub fn extract_panic_message(info: &PanicHookInfo<'_>) -> String {
    if let Some(s) = info.payload().downcast_ref::<&str>() {
        (*s).to_owned()
    } else if let Some(s) = info.payload().downcast_ref::<String>() {
        s.clone()
    } else {
        "Unknown panic payload".to_owned()
    }
}

/// Formats and writes a sanitized crash report to `<logs_dir>/crash-<YYYY-MM-DDTHH-MM-SS>.log`
/// and `<logs_dir>/crash-latest.log`, and records `PANIC: <msg>` in any active `install.journal`
/// (preserving `active: true` so the recovery dialog triggers on next startup).
pub fn write_crash_report_for_panic(
    paths: &MqPaths,
    payload_msg: &str,
    location: Option<(&str, u32, u32)>,
    thread_name: &str,
    backtrace: &str,
) -> MqResult<PathBuf> {
    let now = Utc::now();
    let timestamp_iso = now.to_rfc3339();
    let timestamp_file = now.format("%Y-%m-%dT%H-%M-%S").to_string();

    let logs_dir = paths.logs_dir();
    fs::create_dir_all(&logs_dir)?;

    let sanitized_msg = sanitize_crash_text(payload_msg);
    let loc_str = match location {
        Some((file, line, col)) => sanitize_crash_text(&format!("{file}:{line}:{col}")),
        None => "<unknown location>".to_owned(),
    };

    // If an installation journal is currently active, record the panic in `last_error`
    // while keeping `active = true` so the next launch displays the crash recovery dialog.
    let active_journal = InstallJournal::load_unclosed(paths);
    if let Some(mut journal) = active_journal.clone() {
        let panic_note = format!("PANIC at {loc_str}: {sanitized_msg}");
        journal.last_error = Some(panic_note.clone());
        journal.updated_at_utc = timestamp_iso.clone();
        journal
            .breadcrumbs
            .push(format!("[{timestamp_iso}] {panic_note}"));
        let _ = journal.save(paths);
    }

    let mut report = String::with_capacity(4096);
    let _ = writeln!(
        report,
        "================================================================================"
    );
    let _ = writeln!(
        report,
        "MQ Launcher Crash Report (v{})",
        env!("CARGO_PKG_VERSION")
    );
    let _ = writeln!(report, "Timestamp (UTC): {timestamp_iso}");
    let _ = writeln!(
        report,
        "OS / Arch:       {} / {}",
        std::env::consts::OS,
        std::env::consts::ARCH
    );
    let _ = writeln!(
        report,
        "Thread:          {} ({:?})",
        thread_name,
        std::thread::current().id()
    );
    let _ = writeln!(report, "Location:        {loc_str}");
    let _ = writeln!(report, "Panic Message:   {sanitized_msg}");
    let _ = writeln!(
        report,
        "================================================================================"
    );

    if let Some(journal) = active_journal {
        let _ = writeln!(
            report,
            "\n--- Active Installation Journal (install.journal) ---"
        );
        let _ = writeln!(report, "Instance ID:     {}", journal.instance_id);
        let _ = writeln!(report, "MC Version:      {}", journal.mc_version);
        let _ = writeln!(report, "Loader:          {}", journal.loader);
        let _ = writeln!(report, "Stage:           {}", journal.stage);
        let _ = writeln!(report, "Started (UTC):   {}", journal.started_at_utc);
        let _ = writeln!(report, "Breadcrumbs:");
        for crumb in &journal.breadcrumbs {
            let _ = writeln!(report, "  * {crumb}");
        }
    }

    let _ = writeln!(report, "\n--- Stack Backtrace ---");
    let _ = writeln!(report, "{backtrace}");

    let sanitized_report = sanitize_crash_text(&report);

    let crash_filename = format!("crash-{timestamp_file}.log");
    let crash_path = logs_dir.join(crash_filename);
    let latest_path = logs_dir.join("crash-latest.log");

    write_file_atomically(&crash_path, sanitized_report.as_bytes())?;
    write_file_atomically(&latest_path, sanitized_report.as_bytes())?;

    Ok(crash_path)
}

fn write_file_atomically(path: &Path, content: &[u8]) -> MqResult<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let tmp_path = path.with_extension(format!("tmp.{}", std::process::id()));
    fs::write(&tmp_path, content)?;
    if let Err(err) = rename_with_retry(&tmp_path, path) {
        let _ = remove_file_with_retry(&tmp_path);
        return Err(err.into());
    }
    Ok(())
}

#[cfg(target_os = "windows")]
fn show_windows_crash_dialog(crash_log_path: &Path, summary: &str) {
    // Never pop up modal GUI dialogs during automated CI runs or unit tests.
    if std::env::var_os("CI").is_some()
        || std::env::var_os("MQ_NO_CRASH_DIALOG").is_some()
        || std::env::var_os("RUST_TEST_THREADS").is_some()
    {
        return;
    }

    if std::thread::current().name() != Some("main") {
        return;
    }

    let body = format!(
        "MQ Launcher завершился из-за внутренней ошибки.\n\nПричина: {}\n\nОтчёт об аварии сохранён в:\n{}",
        summary,
        crash_log_path.display()
    );
    let escaped_body = body.replace('\'', "''");
    let ps_cmd = format!(
        "Add-Type -AssemblyName PresentationFramework; [System.Windows.MessageBox]::Show('{}', 'MQ Launcher — Ошибка', 'OK', 'Error') | Out-Null",
        escaped_body
    );
    let _ = std::process::Command::new("powershell")
        .args(["-NoProfile", "-WindowStyle", "Hidden", "-Command", &ps_cmd])
        .spawn();
}

/// Sanitizes crash reports, logs, and breadcrumbs by redacting:
/// - JWT tokens (`eyJ...`)
/// - HTTP `Bearer <token>` headers
/// - Sensitive CLI flags (`--accessToken`, `--clientToken`, `--uuid`, `--password`, `--refresh_token`)
/// - Sensitive JSON / key-value fields (`accessToken`, `clientToken`, `refresh_token`, `password`, `apiKey`, `secret`)
/// - User home directory paths (`$HOME`, `%USERPROFILE%`) -> `~`
pub fn sanitize_crash_text(input: &str) -> String {
    if input.is_empty() {
        return String::new();
    }

    let mut out = redact_jwt(input);
    out = redact_bearer(&out);
    out = redact_cli_flags(&out);
    out = redact_kv_secrets(&out);
    out = redact_home_dirs(&out);
    out
}

fn redact_jwt(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;

    while let Some(pos) = rest.find("eyJ") {
        out.push_str(&rest[..pos]);
        let cand_slice = &rest[pos..];
        let end_idx = cand_slice
            .find(|c: char| !c.is_ascii_alphanumeric() && c != '-' && c != '_' && c != '.')
            .unwrap_or(cand_slice.len());
        let cand = &cand_slice[..end_idx];
        let dots = cand.bytes().filter(|&b| b == b'.').count();
        if dots >= 2 && cand.len() >= 20 {
            out.push_str(REDACTED_PLACEHOLDER);
            rest = &cand_slice[end_idx..];
        } else {
            out.push_str("eyJ");
            rest = &cand_slice[3..];
        }
    }
    out.push_str(rest);
    out
}

fn redact_bearer(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;

    while let Some(pos) = find_ascii_case_insensitive(rest, "Bearer ") {
        out.push_str(&rest[..pos]);
        out.push_str("Bearer ");
        let after = &rest[pos + 7..];
        let end_idx = after
            .find(|c: char| c.is_whitespace() || matches!(c, '"' | '\'' | ',' | ';' | ')'))
            .unwrap_or(after.len());
        if end_idx > 0 {
            out.push_str(REDACTED_PLACEHOLDER);
            rest = &after[end_idx..];
        } else {
            rest = after;
        }
    }
    out.push_str(rest);
    out
}

fn redact_cli_flags(s: &str) -> String {
    const FLAGS: &[&str] = &[
        "--accessToken",
        "--clientToken",
        "--refresh_token",
        "--refreshToken",
        "--uuid",
        "--password",
        "--userProperties",
        "--profileProperties",
    ];

    let mut current = s.to_owned();
    for flag in FLAGS {
        for sep in [" ", "="] {
            let pat = format!("{flag}{sep}");
            let mut out = String::with_capacity(current.len());
            let mut rest = current.as_str();
            while let Some(pos) = find_ascii_case_insensitive(rest, &pat) {
                out.push_str(&rest[..pos]);
                out.push_str(&rest[pos..pos + pat.len()]);
                let val_part = &rest[pos + pat.len()..];
                if let Some(stripped) = val_part.strip_prefix('"') {
                    if let Some(end_q) = stripped.find('"') {
                        out.push_str("\"[REDACTED]\"");
                        rest = &stripped[end_q + 1..];
                    } else {
                        out.push_str("\"[REDACTED]\"");
                        rest = "";
                    }
                } else {
                    let end_idx = val_part
                        .find(|c: char| c.is_whitespace() || matches!(c, ',' | ';'))
                        .unwrap_or(val_part.len());
                    out.push_str(REDACTED_PLACEHOLDER);
                    rest = &val_part[end_idx..];
                }
            }
            out.push_str(rest);
            current = out;
        }
    }
    current
}

fn redact_kv_secrets(s: &str) -> String {
    const KEYS: &[&str] = &[
        "accessToken",
        "access_token",
        "clientToken",
        "client_token",
        "refreshToken",
        "refresh_token",
        "sessionToken",
        "session_token",
        "password",
        "passwd",
        "secret",
        "apiKey",
        "api_key",
    ];

    let mut current = s.to_owned();
    for key in KEYS {
        for quote in ["\"", "'", ""] {
            let key_pat = format!("{quote}{key}{quote}");
            let mut out = String::with_capacity(current.len());
            let mut rest = current.as_str();
            while let Some(pos) = find_ascii_case_insensitive(rest, &key_pat) {
                out.push_str(&rest[..pos]);
                out.push_str(&rest[pos..pos + key_pat.len()]);
                let after_key = &rest[pos + key_pat.len()..];
                let trimmed = after_key.trim_start();
                let ws_before = after_key.len() - trimmed.len();
                if trimmed.starts_with(':') || trimmed.starts_with('=') {
                    let sep = trimmed.chars().next().unwrap();
                    let after_sep = &trimmed[1..];
                    let ws_after = after_sep.len() - after_sep.trim_start().len();
                    let val_part = after_sep.trim_start();

                    out.push_str(&after_key[..ws_before]);
                    out.push(sep);
                    out.push_str(&after_sep[..ws_after]);

                    if let Some(stripped) = val_part.strip_prefix('"') {
                        if let Some(end_q) = stripped.find('"') {
                            out.push_str("\"[REDACTED]\"");
                            rest = &stripped[end_q + 1..];
                            continue;
                        }
                    } else if let Some(stripped) = val_part.strip_prefix('\'') {
                        if let Some(end_q) = stripped.find('\'') {
                            out.push_str("'[REDACTED]'");
                            rest = &stripped[end_q + 1..];
                            continue;
                        }
                    } else {
                        let end_idx = val_part
                            .find(|c: char| c.is_whitespace() || matches!(c, ',' | '}' | ']' | ';'))
                            .unwrap_or(val_part.len());
                        if end_idx > 0 {
                            out.push_str(REDACTED_PLACEHOLDER);
                            rest = &val_part[end_idx..];
                            continue;
                        }
                    }
                }
                out.push_str(after_key);
                rest = "";
            }
            out.push_str(rest);
            current = out;
        }
    }
    current
}

fn redact_home_dirs(s: &str) -> String {
    let mut out = s.to_owned();
    for var in ["HOME", "USERPROFILE"] {
        if let Ok(home) = std::env::var(var) {
            let trimmed = home.trim_end_matches(['/', '\\']);
            if trimmed.len() >= 3 {
                out = out.replace(trimmed, "~");
            }
        }
    }
    out
}

fn find_ascii_case_insensitive(haystack: &str, needle: &str) -> Option<usize> {
    if needle.is_empty() {
        return Some(0);
    }
    haystack
        .as_bytes()
        .windows(needle.len())
        .position(|w| w.eq_ignore_ascii_case(needle.as_bytes()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn install_journal_lifecycle_start_update_unclosed_and_close() {
        let tmp = tempdir().expect("tempdir");
        let paths = MqPaths::with_root(tmp.path());
        paths.ensure_dirs().expect("ensure dirs");

        // Initially no unclosed journal exists
        assert!(InstallJournal::load_unclosed(&paths).is_none());

        // Start installation
        let mut journal =
            InstallJournal::start(&paths, "26.3-vanilla", "26.3", "vanilla", "Манифест версии")
                .expect("start journal");
        assert!(journal.active);
        assert_eq!(journal.mc_version, "26.3");
        assert_eq!(journal.stage, "Манифест версии");

        // Update stage
        journal
            .update_stage(&paths, "Игровые ресурсы", "Загрузка 5214 ассетов")
            .expect("update stage");

        // Simulating a crash: load_unclosed must detect the active journal
        let loaded = InstallJournal::load_unclosed(&paths).expect("must find unclosed journal");
        assert!(loaded.active);
        assert_eq!(loaded.instance_id, "26.3-vanilla");
        assert_eq!(loaded.stage, "Игровые ресурсы");
        assert_eq!(loaded.breadcrumbs.len(), 2);

        // Mark closed cleanly
        journal.mark_closed(&paths).expect("mark closed");
        assert!(!journal.active);
        assert!(
            InstallJournal::load_unclosed(&paths).is_none(),
            "closed journal must not be returned by load_unclosed"
        );
    }

    #[test]
    fn sanitize_crash_text_scrubs_bearer_jwt_tokens_and_home_dir() {
        let raw = "Error with header Authorization: Bearer secret_oauth_token_999 \
                   and jwt eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiIxMjM0NTY3ODkwIn0.SflKxwRJSMeKKF2QT4fwpMeJf36POk6yJV_adQssw5c \
                   and --accessToken my_mc_token_12345 \
                   and {\"refresh_token\": \"rt_secret_abc\", \"clientToken\": \"ct_secret_xyz\"}";

        let clean = sanitize_crash_text(raw);
        assert!(
            !clean.contains("secret_oauth_token_999"),
            "Bearer leaked: {clean}"
        );
        assert!(!clean.contains("eyJhbGci"), "JWT leaked: {clean}");
        assert!(
            !clean.contains("my_mc_token_12345"),
            "accessToken leaked: {clean}"
        );
        assert!(
            !clean.contains("rt_secret_abc"),
            "refresh_token leaked: {clean}"
        );
        assert!(
            !clean.contains("ct_secret_xyz"),
            "clientToken leaked: {clean}"
        );
        assert!(clean.contains("Bearer [REDACTED]"));
    }

    #[test]
    fn crash_report_writes_timestamped_and_latest_logs_and_updates_active_journal() {
        let tmp = tempdir().expect("tempdir");
        let paths = MqPaths::with_root(tmp.path());
        paths.ensure_dirs().expect("ensure dirs");

        let _journal =
            InstallJournal::start(&paths, "1.20.1-fabric", "1.20.1", "fabric", "Библиотеки")
                .expect("start journal");

        let crash_path = write_crash_report_for_panic(
            &paths,
            "called `Option::unwrap()` on a `None` value with Bearer super_secret_token_777",
            Some(("crates/mq-ui/src/hud.rs", 99, 13)),
            "main",
            "   0: mq_core::crash::tests\n   1: main",
        )
        .expect("write crash report");

        assert!(crash_path.is_file());
        let latest_path = paths.logs_dir().join("crash-latest.log");
        assert!(latest_path.is_file());

        let content = fs::read_to_string(&crash_path).expect("read crash report");
        assert!(content.contains("MQ Launcher Crash Report"));
        assert!(content.contains("crates/mq-ui/src/hud.rs:99:13"));
        assert!(content.contains("1.20.1-fabric"));
        assert!(content.contains("Библиотеки"));
        assert!(!content.contains("super_secret_token_777"));
        assert!(content.contains("Bearer [REDACTED]"));

        // Verify that the active journal remains active=true and records the PANIC in last_error
        let unclosed =
            InstallJournal::load_unclosed(&paths).expect("journal must remain active after panic");
        assert!(unclosed.active);
        let last_err = unclosed.last_error.as_deref().unwrap_or_default();
        assert!(last_err.starts_with("PANIC"));
        assert!(!last_err.contains("super_secret_token_777"));

        // Verify diagnostic summary formatting
        let summary = unclosed.format_diagnostic_summary(&paths);
        assert!(summary.contains("1.20.1-fabric"));
        assert!(summary.contains("Библиотеки"));
        assert!(summary.contains("crash-latest.log"));
        assert!(!summary.contains("super_secret_token_777"));
    }

    #[test]
    fn global_panic_hook_captures_thread_panic_to_crash_latest_log() {
        let tmp = tempdir().expect("tempdir");
        let paths = MqPaths::with_root(tmp.path());
        paths.ensure_dirs().expect("ensure dirs");

        install_panic_hook(&paths);

        let handle = std::thread::Builder::new()
            .name("test-install-worker".to_owned())
            .spawn(|| {
                panic!("simulated worker crash with --accessToken tok_secret_555");
            })
            .expect("spawn thread");

        let join_res = handle.join();
        assert!(join_res.is_err());

        let latest_path = paths.logs_dir().join("crash-latest.log");
        assert!(
            latest_path.is_file(),
            "panic hook must write crash-latest.log"
        );
        let report = fs::read_to_string(&latest_path).expect("read crash-latest.log");
        assert!(report.contains("test-install-worker"));
        assert!(report.contains("simulated worker crash"));
        assert!(!report.contains("tok_secret_555"));
        assert!(report.contains("--accessToken [REDACTED]"));
    }
}
