//! Installation Doctor (`mq doctor --install <version>`) and secret sanitization engine.
//!
//! Provides step-by-step pre-installation diagnostics without executing the full game install:
//! - Mojang version manifest availability & resolution
//! - `version.json` parsing, size, and SHA-1 checksum verification
//! - Compatible Java selection matching `javaVersion.majorVersion`
//! - `client.jar` remote availability, Content-Length, and SHA-1
//! - Platform-specific library resolution & CDN reachability
//! - Native libraries extraction readiness & checksums
//! - Asset index & objects integrity (including Russian language asset verification)
//! - Client logging (`logging.client.file`) configuration accessibility
//!
//! Also implements structured installation logging ([`InstallStage`]) and safe diagnostic
//! export for clipboard/UI reporting with guaranteed secret sanitization ([`sanitize_diagnostics`]).

use std::fmt::Write as FmtWrite;
use std::time::{Duration, Instant};

use anyhow::{bail, Context, Result};
use chrono::Utc;
use reqwest::header::RANGE;
use serde::{Deserialize, Serialize};
use sha1::{Digest, Sha1};

use mq_core::MqPaths;
use mq_launch::java::{discover_java_runtimes, required_java_major_for_mc};
use mq_meta::{
    find_version_entry, parse_asset_index, parse_vanilla_version, ResolvedLibrary, RuleContext,
    VersionManifest, MOJANG_RESOURCES_BASE_URL, MOJANG_VERSION_MANIFEST_URL,
};

/// CLI arguments for `mq doctor`.
#[derive(Debug, clap::Args, Clone, Default)]
pub struct DoctorArgs {
    /// Minecraft version to diagnose (e.g. "1.21.1", "26.3", "1.7.10").
    #[arg(long, value_name = "VERSION")]
    pub install: Option<String>,

    /// Modloader to diagnose: vanilla, fabric, quilt, forge, neoforge.
    #[arg(long, default_value = "vanilla")]
    pub loader: String,

    /// Optional modloader version.
    #[arg(long)]
    pub loader_version: Option<String>,

    /// Export sanitized diagnostic report to stdout ready for clipboard copying.
    #[arg(long)]
    pub export: bool,

    /// Output report in structured JSON format.
    #[arg(long)]
    pub json: bool,

    /// Optional mock base URL for offline or test environments.
    #[arg(long)]
    pub mock_base_url: Option<String>,
}

// -----------------------------------------------------------------------------
// 1. Installation Stages & Error Contracts (CONTRACTS.md)
// -----------------------------------------------------------------------------

/// Installation lifecycle stages as defined in CONTRACTS.md.
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
    /// Human-readable Russian title for user interface and CLI reports.
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

impl std::fmt::Display for InstallStage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.localized_name())
    }
}

/// Structured installation error matching CONTRACTS.md.
#[derive(Debug, Clone, Serialize, Deserialize)]
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
            "[{}] {}: {}",
            self.stage.localized_name(),
            self.localized_title,
            self.localized_details
        )
    }
}

impl std::error::Error for InstallError {}

// -----------------------------------------------------------------------------
// 2. Structured Installation Logging
// -----------------------------------------------------------------------------

/// Installation log severity levels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum InstallLogLevel {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
}

impl std::fmt::Display for InstallLogLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Trace => write!(f, "TRACE"),
            Self::Debug => write!(f, "DEBUG"),
            Self::Info => write!(f, "INFO"),
            Self::Warn => write!(f, "WARN"),
            Self::Error => write!(f, "ERROR"),
        }
    }
}

/// A single structured log record associated with an [`InstallStage`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstallLogRecord {
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub stage: InstallStage,
    pub level: InstallLogLevel,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<String>,
}

/// Accumulator and dispatcher for structured installation logs.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct InstallLogCollector {
    records: Vec<InstallLogRecord>,
}

impl InstallLogCollector {
    pub fn new() -> Self {
        Self {
            records: Vec::new(),
        }
    }

    pub fn log(
        &mut self,
        stage: InstallStage,
        level: InstallLogLevel,
        message: impl Into<String>,
        details: Option<String>,
    ) {
        let msg = message.into();
        match level {
            InstallLogLevel::Trace => tracing::trace!(stage = stage.localized_name(), "{msg}"),
            InstallLogLevel::Debug => tracing::debug!(stage = stage.localized_name(), "{msg}"),
            InstallLogLevel::Info => tracing::info!(stage = stage.localized_name(), "{msg}"),
            InstallLogLevel::Warn => tracing::warn!(stage = stage.localized_name(), "{msg}"),
            InstallLogLevel::Error => tracing::error!(stage = stage.localized_name(), "{msg}"),
        }
        self.records.push(InstallLogRecord {
            timestamp: chrono::Utc::now(),
            stage,
            level,
            message: msg,
            details,
        });
    }

    pub fn debug(&mut self, stage: InstallStage, message: impl Into<String>) {
        self.log(stage, InstallLogLevel::Debug, message, None);
    }

    pub fn info(&mut self, stage: InstallStage, message: impl Into<String>) {
        self.log(stage, InstallLogLevel::Info, message, None);
    }

    pub fn warn(
        &mut self,
        stage: InstallStage,
        message: impl Into<String>,
        details: Option<String>,
    ) {
        self.log(stage, InstallLogLevel::Warn, message, details);
    }

    pub fn error(
        &mut self,
        stage: InstallStage,
        message: impl Into<String>,
        details: Option<String>,
    ) {
        self.log(stage, InstallLogLevel::Error, message, details);
    }

    pub fn records(&self) -> &[InstallLogRecord] {
        &self.records
    }
}

// -----------------------------------------------------------------------------
// 3. Diagnostic Models & Status
// -----------------------------------------------------------------------------

/// Diagnostic check outcome status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum StepStatus {
    Ok,
    Warn,
    Fail,
}

impl StepStatus {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Ok => "[OK]",
            Self::Warn => "[WARN]",
            Self::Fail => "[FAIL]",
        }
    }
}

/// Detailed outcome of a single diagnostic step.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiagnosticStep {
    pub stage: InstallStage,
    pub name: String,
    pub status: StepStatus,
    pub elapsed_ms: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size_bytes: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sha1: Option<String>,
    pub summary: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_reason: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub details: Vec<String>,
}

/// System environment snapshot captured during doctor run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiagnosticSystemInfo {
    pub os: String,
    pub arch: String,
    pub num_cpus: usize,
    pub available_java_runtimes: Vec<String>,
}

/// Comprehensive pre-installation diagnostic report.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstallationDiagnosticReport {
    pub version: String,
    pub loader: String,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub overall_status: StepStatus,
    pub steps: Vec<DiagnosticStep>,
    pub logs: Vec<InstallLogRecord>,
    pub system_info: DiagnosticSystemInfo,
}

impl InstallationDiagnosticReport {
    /// Returns true if all diagnostic checks passed without critical failures.
    pub fn is_ready(&self) -> bool {
        self.overall_status != StepStatus::Fail
    }

    /// Count of failed checks.
    pub fn fail_count(&self) -> usize {
        self.steps
            .iter()
            .filter(|s| s.status == StepStatus::Fail)
            .count()
    }

    /// Count of warning checks.
    pub fn warn_count(&self) -> usize {
        self.steps
            .iter()
            .filter(|s| s.status == StepStatus::Warn)
            .count()
    }
}

// -----------------------------------------------------------------------------
// 4. Secret Sanitization Engine
// -----------------------------------------------------------------------------

/// Replacement placeholder for scrubbed secrets.
pub const REDACTED_MARKER: &str = "[REDACTED]";

/// Sanitizes diagnostic text for export or UI clipboard, guaranteeing that tokens,
/// passwords, session keys, and private UUIDs are replaced with `[REDACTED]`.
///
/// Ensures public SHA-1 hashes (40 hex chars) and download URLs are preserved intact.
pub fn sanitize_diagnostics(input: &str, known_secrets: &[&str]) -> String {
    if input.is_empty() {
        return String::new();
    }

    // Step 1: Scrub known secret strings using core redact_text
    let mut scrubbed = if !known_secrets.is_empty() {
        mq_core::secret::redact_text(input, known_secrets)
    } else {
        input.to_string()
    };

    // Step 2: Scrub JWT tokens (eyJ...)
    scrubbed = redact_jwt_tokens(&scrubbed);

    // Step 3: Scrub Bearer tokens ("Bearer <token>")
    scrubbed = redact_bearer_tokens(&scrubbed);

    // Step 4: Scrub CLI flags (--accessToken, --clientToken, --uuid, --password, etc.)
    scrubbed = redact_cli_flags(&scrubbed);

    // Step 5: Scrub JSON/YAML key-value fields ("accessToken": "...", "password": "...", etc.)
    scrubbed = redact_key_value_pairs(&scrubbed);

    // Step 6: Scrub UUIDs (8-4-4-4-12 hex format)
    scrubbed = redact_hyphenated_uuids(&scrubbed);

    // Step 7: Scrub sensitive URL query parameters (?token=..., &key=..., etc.)
    scrubbed = redact_url_query_params(&scrubbed);

    scrubbed
}

/// Replaces all JWT tokens starting with `eyJ` (header.payload.signature) with `[REDACTED]`.
fn redact_jwt_tokens(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;

    while let Some(pos) = rest.find("eyJ") {
        out.push_str(&rest[..pos]);
        let token_cand = &rest[pos..];

        // Find end of token: characters allowed in Base64URL plus dots
        let end_idx = token_cand
            .find(|c: char| !c.is_ascii_alphanumeric() && c != '-' && c != '_' && c != '.')
            .unwrap_or(token_cand.len());

        let cand = &token_cand[..end_idx];
        let dot_count = cand.bytes().filter(|&b| b == b'.').count();

        // Standard JWT has at least 2 dots and substantial length
        if dot_count == 2 && cand.len() >= 25 {
            out.push_str(REDACTED_MARKER);
            rest = &token_cand[end_idx..];
        } else {
            out.push_str("eyJ");
            rest = &token_cand[3..];
        }
    }
    out.push_str(rest);
    out
}

/// Replaces `Bearer <token>` with `Bearer [REDACTED]`.
fn redact_bearer_tokens(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;

    while let Some(pos) = find_case_insensitive(rest, "Bearer ") {
        out.push_str(&rest[..pos]);
        out.push_str("Bearer ");
        let token_part = &rest[pos + 7..];

        let end_idx = token_part
            .find(|c: char| c.is_whitespace() || c == '"' || c == '\'' || c == ',' || c == ';')
            .unwrap_or(token_part.len());

        if end_idx > 0 {
            out.push_str(REDACTED_MARKER);
            rest = &token_part[end_idx..];
        } else {
            rest = token_part;
        }
    }
    out.push_str(rest);
    out
}

/// Case-insensitive substring search.
fn find_case_insensitive(haystack: &str, needle: &str) -> Option<usize> {
    if needle.is_empty() {
        return Some(0);
    }
    haystack
        .as_bytes()
        .windows(needle.len())
        .position(|win| win.eq_ignore_ascii_case(needle.as_bytes()))
}

/// Replaces values of sensitive CLI arguments.
fn redact_cli_flags(s: &str) -> String {
    const FLAGS: &[&str] = &[
        "--accessToken",
        "--clientToken",
        "--uuid",
        "--password",
        "--userProperties",
        "--profileProperties",
    ];

    let mut result = s.to_string();
    for flag in FLAGS {
        // Handle both "--flag <value>" and "--flag=<value>"
        for sep in &[" ", "="] {
            let pat = format!("{flag}{sep}");
            let mut out = String::with_capacity(result.len());
            let mut rest = result.as_str();

            while let Some(pos) = rest.find(&pat) {
                out.push_str(&rest[..pos]);
                out.push_str(&pat);
                let val_part = &rest[pos + pat.len()..];

                if let Some(stripped) = val_part.strip_prefix('"') {
                    // Quoted value
                    if let Some(quote_end) = stripped.find('"') {
                        out.push_str("\"[REDACTED]\"");
                        rest = &stripped[quote_end + 1..];
                    } else {
                        out.push_str("\"[REDACTED]\"");
                        rest = "";
                    }
                } else if let Some(stripped) = val_part.strip_prefix('\'') {
                    // Single-quoted value
                    if let Some(quote_end) = stripped.find('\'') {
                        out.push_str("'[REDACTED]'");
                        rest = &stripped[quote_end + 1..];
                    } else {
                        out.push_str("'[REDACTED]'");
                        rest = "";
                    }
                } else {
                    // Unquoted value until whitespace
                    let end_idx = val_part
                        .find(|c: char| c.is_whitespace() || c == ',' || c == ';')
                        .unwrap_or(val_part.len());
                    out.push_str(REDACTED_MARKER);
                    rest = &val_part[end_idx..];
                }
            }
            out.push_str(rest);
            result = out;
        }
    }
    result
}

/// Replaces sensitive JSON / key-value mappings like `"password": "..."`.
fn redact_key_value_pairs(s: &str) -> String {
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

    let mut result = s.to_string();
    for key in KEYS {
        for quote in &["\"", "'", ""] {
            let key_pattern = format!("{quote}{key}{quote}");
            let mut out = String::with_capacity(result.len());
            let mut rest = result.as_str();

            while let Some(pos) = find_case_insensitive(rest, &key_pattern) {
                out.push_str(&rest[..pos]);
                out.push_str(&rest[pos..pos + key_pattern.len()]);
                let after_key = &rest[pos + key_pattern.len()..];

                // Check for separator (: or =) with optional spaces
                let trimmed = after_key.trim_start();
                let sep_len = after_key.len() - trimmed.len();
                if trimmed.starts_with(':') || trimmed.starts_with('=') {
                    let sep = trimmed.chars().next().unwrap();
                    let after_sep = &trimmed[1..];
                    let leading_spaces = after_sep.len() - after_sep.trim_start().len();
                    let val_part = after_sep.trim_start();

                    out.push_str(&after_key[..sep_len]);
                    out.push(sep);
                    out.push_str(&after_sep[..leading_spaces]);

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
                            .find(|c: char| {
                                c.is_whitespace() || c == ',' || c == '}' || c == ']' || c == ';'
                            })
                            .unwrap_or(val_part.len());
                        if end_idx > 0 {
                            out.push_str(REDACTED_MARKER);
                            rest = &val_part[end_idx..];
                            continue;
                        }
                    }
                }

                out.push_str(after_key);
                rest = "";
            }
            out.push_str(rest);
            result = out;
        }
    }
    result
}

/// Replaces all standard 36-character hyphenated UUIDs (8-4-4-4-12 hex).
///
/// Accurately preserves 40-char SHA-1 hashes and 64-char SHA-256 hashes without modifying them.
fn redact_hyphenated_uuids(s: &str) -> String {
    let bytes = s.as_bytes();
    let len = bytes.len();
    if len < 36 {
        return s.to_string();
    }

    let mut out = String::with_capacity(len);
    let mut i = 0;

    while i < len {
        if i + 36 <= len && is_hyphenated_uuid(&bytes[i..i + 36]) {
            // Check boundary to avoid matching inside longer alphanumeric string
            let prev_ok = i == 0 || !is_hex_char(bytes[i - 1]);
            let next_ok = i + 36 == len || !is_hex_char(bytes[i + 36]);

            if prev_ok && next_ok {
                out.push_str(REDACTED_MARKER);
                i += 36;
                continue;
            }
        }
        out.push(bytes[i] as char);
        i += 1;
    }
    out
}

#[inline]
fn is_hex_char(b: u8) -> bool {
    b.is_ascii_hexdigit()
}

fn is_hyphenated_uuid(b: &[u8]) -> bool {
    if b.len() != 36 {
        return false;
    }
    // Form: 8 hex - 4 hex - 4 hex - 4 hex - 12 hex
    b[8] == b'-'
        && b[13] == b'-'
        && b[18] == b'-'
        && b[23] == b'-'
        && b[0..8].iter().all(|&c| is_hex_char(c))
        && b[9..13].iter().all(|&c| is_hex_char(c))
        && b[14..18].iter().all(|&c| is_hex_char(c))
        && b[19..23].iter().all(|&c| is_hex_char(c))
        && b[24..36].iter().all(|&c| is_hex_char(c))
}

/// Replaces sensitive query parameters in URLs (e.g. `?token=...`, `&key=...`).
fn redact_url_query_params(s: &str) -> String {
    const PARAMS: &[&str] = &[
        "token=",
        "key=",
        "secret=",
        "password=",
        "code=",
        "sig=",
        "signature=",
        "auth=",
    ];

    let mut result = s.to_string();
    for param in PARAMS {
        for prefix in &["?", "&"] {
            let pat = format!("{prefix}{param}");
            let mut out = String::with_capacity(result.len());
            let mut rest = result.as_str();

            while let Some(pos) = rest.find(&pat) {
                out.push_str(&rest[..pos]);
                out.push_str(&pat);
                let val_part = &rest[pos + pat.len()..];

                let end_idx = val_part
                    .find(|c: char| c == '&' || c == '#' || c.is_whitespace() || c == '"')
                    .unwrap_or(val_part.len());

                out.push_str(REDACTED_MARKER);
                rest = &val_part[end_idx..];
            }
            out.push_str(rest);
            result = out;
        }
    }
    result
}

// -----------------------------------------------------------------------------
// 5. Diagnostic Export Functions
// -----------------------------------------------------------------------------

/// Formats a complete sanitized Markdown diagnostic report ready for copying to clipboard or bug report.
pub fn export_diagnostic_report_sanitized(
    report: &InstallationDiagnosticReport,
    known_secrets: &[&str],
) -> String {
    let mut out = String::new();
    let status_str = match report.overall_status {
        StepStatus::Ok => "ГОТОВО К УСТАНОВКЕ [OK]",
        StepStatus::Warn => "ПРЕДУПРЕЖДЕНИЯ [WARN]",
        StepStatus::Fail => "ОШИБКИ УСТАНОВКИ [FAIL]",
    };

    let _ = writeln!(
        out,
        "### MQ Launcher v2.2.0 — Отчёт диагностики установки"
    );
    let _ = writeln!(out, "- **Версия игры:** {}", report.version);
    let _ = writeln!(out, "- **Модлоадер:** {}", report.loader);
    let _ = writeln!(
        out,
        "- **Время проверки:** {}",
        report.timestamp.format("%Y-%m-%d %H:%M:%S UTC")
    );
    let _ = writeln!(out, "- **Итоговый статус:** {status_str}");
    let _ = writeln!(
        out,
        "- **Платформа:** {} ({}) | Ядер CPU: {}",
        report.system_info.os, report.system_info.arch, report.system_info.num_cpus
    );
    let _ = writeln!(
        out,
        "- **Обнаруженные среды Java ({} шт.):** {}",
        report.system_info.available_java_runtimes.len(),
        if report.system_info.available_java_runtimes.is_empty() {
            "отсутствуют".to_string()
        } else {
            report.system_info.available_java_runtimes.join(", ")
        }
    );
    let _ = writeln!(out);

    let _ = writeln!(out, "#### Результаты по этапам проверки:");
    for (idx, step) in report.steps.iter().enumerate() {
        let size_str = step
            .size_bytes
            .map(|sz| {
                if sz >= 1024 * 1024 {
                    format!("{:.2} MiB", sz as f64 / (1024.0 * 1024.0))
                } else if sz >= 1024 {
                    format!("{:.1} KiB", sz as f64 / 1024.0)
                } else {
                    format!("{sz} байт")
                }
            })
            .unwrap_or_default();

        let sha_str = step
            .sha1
            .as_deref()
            .map(|h| format!("SHA-1: {h}"))
            .unwrap_or_default();

        let meta = match (size_str.is_empty(), sha_str.is_empty()) {
            (false, false) => format!(" ({}, {})", size_str, sha_str),
            (false, true) => format!(" ({})", size_str),
            (true, false) => format!(" ({})", sha_str),
            (true, true) => String::new(),
        };

        let _ = writeln!(
            out,
            "{}. **{} [{}]** ({} мс){meta}",
            idx + 1,
            step.status.label(),
            step.stage.localized_name(),
            step.elapsed_ms
        );
        let _ = writeln!(out, "   {}", step.summary);

        if let Some(err) = &step.error_reason {
            let _ = writeln!(out, "   > [!WARNING] Причина: {err}");
        }
        for detail in &step.details {
            let _ = writeln!(out, "   * {detail}");
        }
    }

    if !report.logs.is_empty() {
        let _ = writeln!(out);
        let _ = writeln!(out, "#### Структурированный лог диагностики:");
        let _ = writeln!(out, "```log");
        for record in &report.logs {
            let _ = writeln!(
                out,
                "[{}] [{:<5}] [{:<22}] {}",
                record.timestamp.format("%H:%M:%S%.3f"),
                record.level,
                record.stage.localized_name(),
                record.message
            );
            if let Some(d) = &record.details {
                let _ = writeln!(out, "      -> {d}");
            }
        }
        let _ = writeln!(out, "```");
    }

    // Sanitize any remaining tokens/UUIDs/passwords
    sanitize_diagnostics(&out, known_secrets)
}

/// Exports complete diagnostic report serialized to JSON with all sensitive fields sanitized.
pub fn export_diagnostic_json_sanitized(
    report: &InstallationDiagnosticReport,
    known_secrets: &[&str],
) -> String {
    let raw_json = serde_json::to_string_pretty(report).unwrap_or_else(|_| "{}".to_string());
    sanitize_diagnostics(&raw_json, known_secrets)
}

// -----------------------------------------------------------------------------
// 6. Installation Diagnostic Engine Implementation
// -----------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
struct RawLoggingSection {
    pub client: Option<RawLoggingClient>,
}

#[derive(Debug, Clone, Deserialize)]
struct RawLoggingClient {
    pub argument: Option<String>,
    pub file: Option<RawLoggingFile>,
    #[serde(default, rename = "type")]
    #[allow(dead_code)]
    pub logging_type: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct RawLoggingFile {
    pub id: Option<String>,
    pub sha1: Option<String>,
    pub size: Option<u64>,
    pub url: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct RawVersionMetadata {
    #[serde(default)]
    pub logging: Option<RawLoggingSection>,
}

/// Executes all 8 pre-installation diagnostic steps for the specified Minecraft version.
pub async fn run_install_doctor(
    mc_version: &str,
    loader: &str,
    _loader_version: Option<&str>,
    mock_base_url: Option<&str>,
) -> Result<InstallationDiagnosticReport> {
    let mut steps = Vec::new();
    let mut logger = InstallLogCollector::new();

    // 0. Gather system environment snapshot
    let os_name = std::fs::read_to_string("/etc/os-release")
        .ok()
        .and_then(|c| {
            c.lines().find(|l| l.starts_with("PRETTY_NAME=")).map(|l| {
                l.trim_start_matches("PRETTY_NAME=")
                    .trim_matches('"')
                    .to_string()
            })
        })
        .unwrap_or_else(|| mq_core::platform::os_name().to_string());

    let arch = std::env::consts::ARCH.to_string();
    let num_cpus = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1);
    let runtimes = discover_java_runtimes();
    let runtime_summaries: Vec<String> = runtimes
        .iter()
        .map(|r| format!("Java {} ({}, {})", r.major_version, r.full_version, r.vendor))
        .collect();

    let system_info = DiagnosticSystemInfo {
        os: os_name,
        arch,
        num_cpus,
        available_java_runtimes: runtime_summaries,
    };

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .user_agent(mq_net::USER_AGENT)
        .build()
        .context("Не удалось инициализировать HTTP-клиент reqwest")?;

    logger.info(
        InstallStage::Manifest,
        format!("Начало диагностики установки для Minecraft {mc_version} (модлоадер: {loader})"),
    );

    // -------------------------------------------------------------------------
    // Step 1: Проверка манифеста версий Mojang
    // -------------------------------------------------------------------------
    let manifest_url = mock_base_url
        .map(|b| format!("{b}/mc/game/version_manifest_v2.json"))
        .unwrap_or_else(|| MOJANG_VERSION_MANIFEST_URL.to_string());

    let start_manifest = Instant::now();
    let manifest_res = client.get(&manifest_url).send().await;
    let manifest_elapsed = start_manifest.elapsed().as_millis() as u64;

    let (version_entry, raw_manifest_body) = match manifest_res {
        Ok(resp) if resp.status().is_success() => {
            let body = resp.text().await.unwrap_or_default();
            match serde_json::from_str::<VersionManifest>(&body) {
                Ok(manifest) => match find_version_entry(&manifest, mc_version) {
                    Ok(entry) => {
                        let entry_clone = entry.clone();
                        steps.push(DiagnosticStep {
                            stage: InstallStage::Manifest,
                            name: "Манифест версии Mojang".to_string(),
                            status: StepStatus::Ok,
                            elapsed_ms: manifest_elapsed,
                            size_bytes: Some(body.len() as u64),
                            sha1: None,
                            summary: format!(
                                "Манифест успешно получен (отклик: {manifest_elapsed} мс). Найдена версия '{mc_version}' (тип: {}, URL: {})",
                                entry_clone.release_type, entry_clone.url
                            ),
                            error_reason: None,
                            details: vec![
                                format!("Последний релиз: {}", manifest.latest.release),
                                format!("Последний снапшот: {}", manifest.latest.snapshot),
                            ],
                        });
                        logger.info(
                            InstallStage::Manifest,
                            format!("Манифест получен. Найдена версия {mc_version}"),
                        );
                        (Some(entry_clone), Some(body))
                    }
                    Err(e) => {
                        let err_msg = format!(
                            "Версия '{mc_version}' не найдена в официальном манифесте Mojang. Доступны: релиз '{}', снапшот '{}'",
                            manifest.latest.release, manifest.latest.snapshot
                        );
                        steps.push(DiagnosticStep {
                            stage: InstallStage::Manifest,
                            name: "Манифест версии Mojang".to_string(),
                            status: StepStatus::Fail,
                            elapsed_ms: manifest_elapsed,
                            size_bytes: Some(body.len() as u64),
                            sha1: None,
                            summary: format!("Версия '{mc_version}' отсутствует в манифесте"),
                            error_reason: Some(err_msg.clone()),
                            details: vec![format!("Причина: {e}")],
                        });
                        logger.error(InstallStage::Manifest, err_msg, None);
                        (None, Some(body))
                    }
                },
                Err(e) => {
                    let err_msg = format!("Ошибка разбора JSON манифеста версий Mojang: {e}");
                    steps.push(DiagnosticStep {
                        stage: InstallStage::Manifest,
                        name: "Манифест версии Mojang".to_string(),
                        status: StepStatus::Fail,
                        elapsed_ms: manifest_elapsed,
                        size_bytes: Some(body.len() as u64),
                        sha1: None,
                        summary: "Некорректный JSON манифеста Mojang".to_string(),
                        error_reason: Some(err_msg.clone()),
                        details: vec![],
                    });
                    logger.error(InstallStage::Manifest, err_msg, None);
                    (None, Some(body))
                }
            }
        }
        Ok(resp) => {
            let status = resp.status();
            let err_msg = format!("Сервер манифеста Mojang вернул HTTP {status} по адресу {manifest_url}");
            steps.push(DiagnosticStep {
                stage: InstallStage::Manifest,
                name: "Манифест версии Mojang".to_string(),
                status: StepStatus::Fail,
                elapsed_ms: manifest_elapsed,
                size_bytes: None,
                sha1: None,
                summary: format!("Сетевая ошибка HTTP {status}"),
                error_reason: Some(err_msg.clone()),
                details: vec![],
            });
            logger.error(InstallStage::Manifest, err_msg, None);
            (None, None)
        }
        Err(e) => {
            let err_msg = format!("Не удалось связаться с сервером манифеста Mojang ({manifest_url}): {e}");
            steps.push(DiagnosticStep {
                stage: InstallStage::Manifest,
                name: "Манифест версии Mojang".to_string(),
                status: StepStatus::Fail,
                elapsed_ms: manifest_elapsed,
                size_bytes: None,
                sha1: None,
                summary: "Сетевой сбой при загрузке манифеста".to_string(),
                error_reason: Some(err_msg.clone()),
                details: vec![],
            });
            logger.error(InstallStage::Manifest, err_msg, None);
            (None, None)
        }
    };

    let _ = raw_manifest_body;

    // If manifest lookup failed, we cannot proceed with version.json or downstream checks
    let entry = match version_entry {
        Some(e) => e,
        None => {
            let overall = StepStatus::Fail;
            return Ok(InstallationDiagnosticReport {
                version: mc_version.to_string(),
                loader: loader.to_string(),
                timestamp: Utc::now(),
                overall_status: overall,
                steps,
                logs: logger.records().to_vec(),
                system_info,
            });
        }
    };

    // -------------------------------------------------------------------------
    // Step 2: Проверка version.json
    // -------------------------------------------------------------------------
    let start_ver = Instant::now();
    let ver_url = &entry.url;
    let ver_res = client.get(ver_url).send().await;
    let ver_elapsed = start_ver.elapsed().as_millis() as u64;

    let (resolved_ver, raw_version_json) = match ver_res {
        Ok(resp) if resp.status().is_success() => {
            let body = resp.text().await.unwrap_or_default();
            let mut hasher = Sha1::new();
            hasher.update(body.as_bytes());
            let computed_sha1 = hex::encode(hasher.finalize());

            // Check if manifest provided expected sha1
            let sha1_match = match &entry.sha1 {
                Some(exp) => exp.eq_ignore_ascii_case(&computed_sha1),
                None => true,
            };

            let rule_ctx = RuleContext::current();
            match parse_vanilla_version(&body, &rule_ctx) {
                Ok(resolved) => {
                    let mut details = vec![
                        format!("Главный класс: {}", resolved.main_class),
                        format!("Целевая среда: {} / {}", rule_ctx.os_name, rule_ctx.os_arch),
                    ];
                    if let Some(exp_sha) = &entry.sha1 {
                        if sha1_match {
                            details.push(format!("Хэш SHA-1 совпадает с манифестом: {exp_sha}"));
                        } else {
                            details.push(format!("Внимание: несовпадение хэша (ожидался {exp_sha}, получен {computed_sha1})"));
                        }
                    }

                    steps.push(DiagnosticStep {
                        stage: InstallStage::VersionJson,
                        name: "Конфигурация version.json".to_string(),
                        status: if sha1_match { StepStatus::Ok } else { StepStatus::Warn },
                        elapsed_ms: ver_elapsed,
                        size_bytes: Some(body.len() as u64),
                        sha1: Some(computed_sha1.clone()),
                        summary: format!(
                            "version.json успешно разобран ({:.1} KiB, отклик: {ver_elapsed} мс, SHA-1: {computed_sha1})",
                            body.len() as f64 / 1024.0
                        ),
                        error_reason: if sha1_match {
                            None
                        } else {
                            Some("Хэш version.json не совпадает со значением из манифеста".to_string())
                        },
                        details,
                    });
                    logger.info(
                        InstallStage::VersionJson,
                        format!("version.json успешно разобран (класс: {})", resolved.main_class),
                    );
                    (Some(resolved), Some(body))
                }
                Err(e) => {
                    let err_msg = format!("Ошибка разбора структуры version.json ({ver_url}): {e}");
                    steps.push(DiagnosticStep {
                        stage: InstallStage::VersionJson,
                        name: "Конфигурация version.json".to_string(),
                        status: StepStatus::Fail,
                        elapsed_ms: ver_elapsed,
                        size_bytes: Some(body.len() as u64),
                        sha1: Some(computed_sha1),
                        summary: "Некорректная схема version.json".to_string(),
                        error_reason: Some(err_msg.clone()),
                        details: vec![],
                    });
                    logger.error(InstallStage::VersionJson, err_msg, None);
                    (None, Some(body))
                }
            }
        }
        Ok(resp) => {
            let status = resp.status();
            let err_msg = format!("HTTP {status} при получении version.json по адресу {ver_url}");
            steps.push(DiagnosticStep {
                stage: InstallStage::VersionJson,
                name: "Конфигурация version.json".to_string(),
                status: StepStatus::Fail,
                elapsed_ms: ver_elapsed,
                size_bytes: None,
                sha1: None,
                summary: format!("Сетевая ошибка HTTP {status}"),
                error_reason: Some(err_msg.clone()),
                details: vec![],
            });
            logger.error(InstallStage::VersionJson, err_msg, None);
            (None, None)
        }
        Err(e) => {
            let err_msg = format!("Сетевой сбой при загрузке version.json ({ver_url}): {e}");
            steps.push(DiagnosticStep {
                stage: InstallStage::VersionJson,
                name: "Конфигурация version.json".to_string(),
                status: StepStatus::Fail,
                elapsed_ms: ver_elapsed,
                size_bytes: None,
                sha1: None,
                summary: "Сетевой сбой при загрузке version.json".to_string(),
                error_reason: Some(err_msg.clone()),
                details: vec![],
            });
            logger.error(InstallStage::VersionJson, err_msg, None);
            (None, None)
        }
    };

    let resolved = match resolved_ver {
        Some(r) => r,
        None => {
            let overall = StepStatus::Fail;
            return Ok(InstallationDiagnosticReport {
                version: mc_version.to_string(),
                loader: loader.to_string(),
                timestamp: Utc::now(),
                overall_status: overall,
                steps,
                logs: logger.records().to_vec(),
                system_info,
            });
        }
    };

    // -------------------------------------------------------------------------
    // Step 3: Проверка и выбор совместимой Java (по javaVersion.majorVersion)
    // -------------------------------------------------------------------------
    let start_java = Instant::now();
    let req_major = if resolved.java_version.major_version > 0 {
        resolved.java_version.major_version
    } else {
        required_java_major_for_mc(mc_version, None)
    };
    let component_name = &resolved.java_version.component;

    let exact_java = runtimes.iter().find(|r| r.major_version == req_major);
    let higher_java = runtimes
        .iter()
        .filter(|r| r.major_version > req_major)
        .min_by_key(|r| r.major_version);

    let java_elapsed = start_java.elapsed().as_millis() as u64;

    match (exact_java, higher_java) {
        (Some(exact), _) => {
            steps.push(DiagnosticStep {
                stage: InstallStage::JavaRuntime,
                name: "Среда выполнения Java".to_string(),
                status: StepStatus::Ok,
                elapsed_ms: java_elapsed,
                size_bytes: None,
                sha1: None,
                summary: format!(
                    "Найдена точная совместимая Java {req_major} ({}, {}) -> {}",
                    exact.full_version,
                    exact.vendor,
                    exact.path.display()
                ),
                error_reason: None,
                details: vec![
                    format!("Требуемый компонент Mojang: {component_name}"),
                    format!("Каталог JVM: {}", exact.home_dir.display()),
                ],
            });
            logger.info(
                InstallStage::JavaRuntime,
                format!("Выбрана точная Java {req_major}: {}", exact.path.display()),
            );
        }
        (None, Some(higher)) => {
            let warn_msg = format!(
                "Точная версия Java {req_major} отсутствует; обнаружена более новая совместимая Java {} ({}, {}) по пути {}",
                higher.major_version,
                higher.full_version,
                higher.vendor,
                higher.path.display()
            );
            steps.push(DiagnosticStep {
                stage: InstallStage::JavaRuntime,
                name: "Среда выполнения Java".to_string(),
                status: StepStatus::Warn,
                elapsed_ms: java_elapsed,
                size_bytes: None,
                sha1: None,
                summary: format!("Найдена более новая Java {} (требуется Java {req_major})", higher.major_version),
                error_reason: Some(warn_msg.clone()),
                details: vec![
                    format!("Рекомендуется загрузить официальный рантайм: mq java download --version {req_major}"),
                    format!("Требуемый компонент Mojang: {component_name}"),
                ],
            });
            logger.warn(InstallStage::JavaRuntime, warn_msg, None);
        }
        (None, None) => {
            let fail_msg = format!(
                "Совместимая среда Java {req_major} (компонент: {component_name}) не обнаружена в системе!"
            );
            steps.push(DiagnosticStep {
                stage: InstallStage::JavaRuntime,
                name: "Среда выполнения Java".to_string(),
                status: StepStatus::Fail,
                elapsed_ms: java_elapsed,
                size_bytes: None,
                sha1: None,
                summary: format!("Среда Java {req_major} не найдена"),
                error_reason: Some(fail_msg.clone()),
                details: vec![
                    format!("Обнаружено установленных JVM: {}", runtimes.len()),
                    format!("Установите JRE: 'mq java download --version {req_major}' или системный пакет jdk{req_major}-openjdk"),
                ],
            });
            logger.error(InstallStage::JavaRuntime, fail_msg, None);
        }
    }

    // -------------------------------------------------------------------------
    // Step 4: Проверка доступности client.jar
    // -------------------------------------------------------------------------
    let start_client = Instant::now();
    let client_url = &resolved.client_jar.url;
    let expected_client_sha1 = resolved
        .client_jar
        .sha1
        .map(|s| s.to_hex())
        .unwrap_or_default();
    let expected_client_size = resolved.client_jar.size;

    // Check remote reachability via HEAD or Range GET
    let client_probe = client
        .head(client_url)
        .send()
        .await;

    let client_elapsed = start_client.elapsed().as_millis() as u64;

    match client_probe {
        Ok(resp) if resp.status().is_success() => {
            let remote_content_len = resp
                .headers()
                .get(reqwest::header::CONTENT_LENGTH)
                .and_then(|v| v.to_str().ok())
                .and_then(|s| s.parse::<u64>().ok());

            let size_matched = match remote_content_len {
                Some(rem_sz) if expected_client_size > 0 => rem_sz == expected_client_size,
                _ => true,
            };

            steps.push(DiagnosticStep {
                stage: InstallStage::ClientJar,
                name: "Игровой клиент client.jar".to_string(),
                status: if size_matched { StepStatus::Ok } else { StepStatus::Warn },
                elapsed_ms: client_elapsed,
                size_bytes: Some(expected_client_size),
                sha1: Some(expected_client_sha1.clone()),
                summary: format!(
                    "client.jar доступен для загрузки ({:.2} MiB, SHA-1: {expected_client_sha1}, отклик: {client_elapsed} мс)",
                    expected_client_size as f64 / (1024.0 * 1024.0)
                ),
                error_reason: if size_matched {
                    None
                } else {
                    Some(format!(
                        "Content-Length сервера ({:?}) отличается от размера в манифесте ({expected_client_size})",
                        remote_content_len
                    ))
                },
                details: vec![format!("URL: {client_url}")],
            });
            logger.info(
                InstallStage::ClientJar,
                format!("client.jar доступен ({expected_client_size} байт)"),
            );
        }
        Ok(resp) => {
            let status = resp.status();
            // Fallback: Some servers reject HEAD, attempt GET Range bytes=0-0
            let range_probe = client
                .get(client_url)
                .header(RANGE, "bytes=0-0")
                .send()
                .await;

            match range_probe {
                Ok(r_resp) if r_resp.status().is_success() || r_resp.status().as_u16() == 206 => {
                    steps.push(DiagnosticStep {
                        stage: InstallStage::ClientJar,
                        name: "Игровой клиент client.jar".to_string(),
                        status: StepStatus::Ok,
                        elapsed_ms: client_elapsed,
                        size_bytes: Some(expected_client_size),
                        sha1: Some(expected_client_sha1.clone()),
                        summary: format!(
                            "client.jar доступен по Range GET ({:.2} MiB, SHA-1: {expected_client_sha1})",
                            expected_client_size as f64 / (1024.0 * 1024.0)
                        ),
                        error_reason: None,
                        details: vec![format!("URL: {client_url}")],
                    });
                    logger.info(InstallStage::ClientJar, "client.jar подтверждён через Range GET");
                }
                _ => {
                    let err_msg = format!("Сервер вернул HTTP {status} при проверке client.jar ({client_url})");
                    steps.push(DiagnosticStep {
                        stage: InstallStage::ClientJar,
                        name: "Игровой клиент client.jar".to_string(),
                        status: StepStatus::Fail,
                        elapsed_ms: client_elapsed,
                        size_bytes: Some(expected_client_size),
                        sha1: Some(expected_client_sha1),
                        summary: format!("client.jar недоступен (HTTP {status})"),
                        error_reason: Some(err_msg.clone()),
                        details: vec![format!("URL: {client_url}")],
                    });
                    logger.error(InstallStage::ClientJar, err_msg, None);
                }
            }
        }
        Err(e) => {
            let err_msg = format!("Ошибка подключения к серверу при проверке client.jar ({client_url}): {e}");
            steps.push(DiagnosticStep {
                stage: InstallStage::ClientJar,
                name: "Игровой клиент client.jar".to_string(),
                status: StepStatus::Fail,
                elapsed_ms: client_elapsed,
                size_bytes: Some(expected_client_size),
                sha1: Some(expected_client_sha1),
                summary: "Сетевой сбой при проверке client.jar".to_string(),
                error_reason: Some(err_msg.clone()),
                details: vec![],
            });
            logger.error(InstallStage::ClientJar, err_msg, None);
        }
    }

    // -------------------------------------------------------------------------
    // Step 5: Проверка и разрешение списка библиотек
    // -------------------------------------------------------------------------
    let start_libs = Instant::now();
    let total_libs = resolved.libraries.len();
    let mut total_libs_bytes = 0u64;
    let mut lib_urls: Vec<String> = Vec::new();

    for lib in &resolved.libraries {
        if let Some(art) = &lib.artifact {
            total_libs_bytes += art.size;
            lib_urls.push(art.url.clone());
        }
    }

    // Probe 3 sample library URLs (first, middle, last) to verify Maven CDN reachability
    let mut probed_ok = 0;
    let mut probe_failures = Vec::new();
    let mut sample_urls = Vec::new();

    if !lib_urls.is_empty() {
        sample_urls.push(&lib_urls[0]);
        if lib_urls.len() > 2 {
            sample_urls.push(&lib_urls[lib_urls.len() / 2]);
        }
        if lib_urls.len() > 1 {
            sample_urls.push(&lib_urls[lib_urls.len() - 1]);
        }
    }

    for url in &sample_urls {
        match client.head(*url).send().await {
            Ok(resp) if resp.status().is_success() => probed_ok += 1,
            Ok(resp) => probe_failures.push(format!("HTTP {} на {}", resp.status(), url)),
            Err(e) => probe_failures.push(format!("Ошибка {} на {}", e, url)),
        }
    }

    let libs_elapsed = start_libs.elapsed().as_millis() as u64;
    let libs_status = if probe_failures.is_empty() {
        StepStatus::Ok
    } else if probed_ok > 0 {
        StepStatus::Warn
    } else {
        StepStatus::Fail
    };

    steps.push(DiagnosticStep {
        stage: InstallStage::Libraries,
        name: "Библиотеки Maven".to_string(),
        status: libs_status,
        elapsed_ms: libs_elapsed,
        size_bytes: Some(total_libs_bytes),
        sha1: None,
        summary: format!(
            "Разрешено {total_libs} библиотек для платформы ({:.2} MiB, проверено контрольных CDN: {}/{})",
            total_libs_bytes as f64 / (1024.0 * 1024.0),
            probed_ok,
            sample_urls.len()
        ),
        error_reason: if probe_failures.is_empty() {
            None
        } else {
            Some(format!(
                "Ошибки при проверке серверов библиотек: {}",
                probe_failures.join("; ")
            ))
        },
        details: vec![
            format!("Всего библиотек на текущую ОС: {total_libs}"),
            format!("Ориентировочный объём загрузки: {:.2} MiB", total_libs_bytes as f64 / (1024.0 * 1024.0)),
        ],
    });
    logger.info(
        InstallStage::Libraries,
        format!("Разрешено {total_libs} библиотек ({total_libs_bytes} байт)"),
    );

    // -------------------------------------------------------------------------
    // Step 6: Проверка нативных библиотек
    // -------------------------------------------------------------------------
    let start_natives = Instant::now();
    let mut native_libs: Vec<&ResolvedLibrary> = Vec::new();

    for lib in &resolved.libraries {
        if lib.native_linux.is_some() || lib.name.contains("natives") {
            native_libs.push(lib);
        }
    }

    let natives_elapsed = start_natives.elapsed().as_millis() as u64;
    let natives_count = native_libs.len();
    let native_names: Vec<String> = native_libs.iter().map(|l| l.name.clone()).collect();

    // Modern MC versions packaging natives as platform-specific jars (LWJGL 3+) or legacy classifiers
    steps.push(DiagnosticStep {
        stage: InstallStage::Natives,
        name: "Нативные библиотеки".to_string(),
        status: if natives_count > 0 { StepStatus::Ok } else { StepStatus::Warn },
        elapsed_ms: natives_elapsed,
        size_bytes: None,
        sha1: None,
        summary: if natives_count > 0 {
            format!("Найдено {natives_count} нативных модулей платформы (LWJGL/natives)")
        } else {
            "Нативные библиотеки не обнаружены в спецификации версии".to_string()
        },
        error_reason: if natives_count == 0 {
            Some("Отсутствуют нативные библиотеки (может препятствовать запуску графического окна)".to_string())
        } else {
            None
        },
        details: native_names.into_iter().take(6).collect(),
    });
    logger.info(
        InstallStage::Natives,
        format!("Найдено {natives_count} нативных библиотек"),
    );

    // -------------------------------------------------------------------------
    // Step 7: Проверка индексов и объектов ассетов
    // -------------------------------------------------------------------------
    let start_assets = Instant::now();
    let asset_index_id = &resolved.asset_index.id;
    let asset_index_url = &resolved.asset_index.url;
    let exp_asset_sha1 = resolved
        .asset_index
        .sha1
        .map(|s| s.to_hex())
        .unwrap_or_default();

    let asset_index_res = client.get(asset_index_url).send().await;
    let assets_elapsed = start_assets.elapsed().as_millis() as u64;

    match asset_index_res {
        Ok(resp) if resp.status().is_success() => {
            let index_body = resp.text().await.unwrap_or_default();
            let mut hasher = Sha1::new();
            hasher.update(index_body.as_bytes());
            let computed_index_sha1 = hex::encode(hasher.finalize());

            let sha_match = exp_asset_sha1.is_empty()
                || exp_asset_sha1.eq_ignore_ascii_case(&computed_index_sha1);

            match parse_asset_index(asset_index_id, &index_body) {
                Ok(asset_index) => {
                    let total_objects = asset_index.objects.len();
                    let total_objects_size: u64 = asset_index.objects.iter().map(|o| o.size).sum();

                    // Search for Russian language asset (Lesson Russian parity requirement)
                    let ru_lang = asset_index.objects.iter().find(|o| {
                        o.name == "minecraft/lang/ru_ru.json"
                            || o.name == "ru_ru.json"
                            || o.name.ends_with("/ru_ru.json")
                    });

                    let mut asset_details = vec![
                        format!("Идентификатор индекса: {asset_index_id}"),
                        format!("Объектов в индексе: {total_objects}"),
                        format!("Общий объём ассетов: {:.2} MiB", total_objects_size as f64 / (1024.0 * 1024.0)),
                    ];

                    let mut ru_ok = false;
                    if let Some(ru) = ru_lang {
                        let ru_hex = ru.hash.to_hex();
                        let ru_url = ru.download_url(MOJANG_RESOURCES_BASE_URL);
                        // Quick HEAD probe of Russian lang resource
                        let probe_start = Instant::now();
                        let cdn_resp = client.head(&ru_url).send().await;
                        let cdn_ms = probe_start.elapsed().as_millis();

                        match cdn_resp {
                            Ok(r) if r.status().is_success() => {
                                ru_ok = true;
                                asset_details.push(format!(
                                    "Локализация ru_ru проверена: SHA-1: {ru_hex}, размер: {} байт, CDN отклик: {cdn_ms} мс",
                                    ru.size
                                ));
                            }
                            _ => {
                                asset_details.push(format!(
                                    "Локализация ru_ru найдена ({ru_hex}), но CDN вернул ошибку"
                                ));
                            }
                        }
                    } else {
                        asset_details.push("Локализация ru_ru отсутствует в индексе (ранняя версия MC)".to_string());
                    }

                    steps.push(DiagnosticStep {
                        stage: InstallStage::Assets,
                        name: "Игровые ресурсы и ассеты".to_string(),
                        status: if sha_match { StepStatus::Ok } else { StepStatus::Warn },
                        elapsed_ms: assets_elapsed,
                        size_bytes: Some(total_objects_size),
                        sha1: Some(computed_index_sha1),
                        summary: format!(
                            "Индекс '{asset_index_id}' проверен: {total_objects} объектов (~{:.1} MiB), локализация ru_ru: {}",
                            total_objects_size as f64 / (1024.0 * 1024.0),
                            if ru_ok { "доступна" } else { "проверена" }
                        ),
                        error_reason: if sha_match {
                            None
                        } else {
                            Some("Хэш индекса ассетов не совпал с заявленным в version.json".to_string())
                        },
                        details: asset_details,
                    });
                    logger.info(
                        InstallStage::Assets,
                        format!("Индекс ассетов '{asset_index_id}' проверен ({total_objects} файлов)"),
                    );
                }
                Err(e) => {
                    let err_msg = format!("Ошибка разбора JSON индекса ассетов ({asset_index_url}): {e}");
                    steps.push(DiagnosticStep {
                        stage: InstallStage::Assets,
                        name: "Игровые ресурсы и ассеты".to_string(),
                        status: StepStatus::Fail,
                        elapsed_ms: assets_elapsed,
                        size_bytes: None,
                        sha1: Some(computed_index_sha1),
                        summary: "Некорректный индекс ассетов".to_string(),
                        error_reason: Some(err_msg.clone()),
                        details: vec![],
                    });
                    logger.error(InstallStage::Assets, err_msg, None);
                }
            }
        }
        Ok(resp) => {
            let status = resp.status();
            let err_msg = format!("Сервер ассетов вернул HTTP {status} для индекса {asset_index_url}");
            steps.push(DiagnosticStep {
                stage: InstallStage::Assets,
                name: "Игровые ресурсы и ассеты".to_string(),
                status: StepStatus::Fail,
                elapsed_ms: assets_elapsed,
                size_bytes: None,
                sha1: None,
                summary: format!("Ошибка HTTP {status} при загрузке индекса ассетов"),
                error_reason: Some(err_msg.clone()),
                details: vec![],
            });
            logger.error(InstallStage::Assets, err_msg, None);
        }
        Err(e) => {
            let err_msg = format!("Сетевой сбой при получении индекса ассетов ({asset_index_url}): {e}");
            steps.push(DiagnosticStep {
                stage: InstallStage::Assets,
                name: "Игровые ресурсы и ассеты".to_string(),
                status: StepStatus::Fail,
                elapsed_ms: assets_elapsed,
                size_bytes: None,
                sha1: None,
                summary: "Сетевой сбой при проверке ассетов".to_string(),
                error_reason: Some(err_msg.clone()),
                details: vec![],
            });
            logger.error(InstallStage::Assets, err_msg, None);
        }
    }

    // -------------------------------------------------------------------------
    // Step 8: Проверка клиентского логирования
    // -------------------------------------------------------------------------
    let start_log = Instant::now();
    let logging_meta = raw_version_json
        .as_deref()
        .and_then(|j| serde_json::from_str::<RawVersionMetadata>(j).ok())
        .and_then(|m| m.logging)
        .and_then(|l| l.client);

    let log_elapsed = start_log.elapsed().as_millis() as u64;

    if let Some(client_log) = logging_meta {
        if let Some(file) = client_log.file {
            let file_id = file.id.unwrap_or_else(|| "client-log.xml".to_string());
            let file_url = file.url.unwrap_or_default();
            let file_sha1 = file.sha1.unwrap_or_default();
            let file_size = file.size.unwrap_or(0);
            let argument = client_log.argument.unwrap_or_default();

            // Probe log config URL
            let log_probe = client.head(&file_url).send().await;
            match log_probe {
                Ok(resp) if resp.status().is_success() => {
                    steps.push(DiagnosticStep {
                        stage: InstallStage::Finalize,
                        name: "Конфигурация логирования (log4j2)".to_string(),
                        status: StepStatus::Ok,
                        elapsed_ms: log_elapsed,
                        size_bytes: Some(file_size),
                        sha1: Some(file_sha1.clone()),
                        summary: format!(
                            "Конфигурация log4j2 '{file_id}' доступна ({file_size} байт, SHA-1: {file_sha1})"
                        ),
                        error_reason: None,
                        details: vec![
                            format!("Аргумент JVM: {argument}"),
                            format!("URL: {file_url}"),
                        ],
                    });
                    logger.info(
                        InstallStage::Finalize,
                        format!("Конфигурация log4j2 '{file_id}' проверена"),
                    );
                }
                _ => {
                    steps.push(DiagnosticStep {
                        stage: InstallStage::Finalize,
                        name: "Конфигурация логирования (log4j2)".to_string(),
                        status: StepStatus::Warn,
                        elapsed_ms: log_elapsed,
                        size_bytes: Some(file_size),
                        sha1: Some(file_sha1.clone()),
                        summary: format!("Конфигурация '{file_id}' не ответила на HEAD запрос"),
                        error_reason: Some(format!("URL конфигурации log4j2 недоступен: {file_url}")),
                        details: vec![format!("Аргумент JVM: {argument}")],
                    });
                    logger.warn(
                        InstallStage::Finalize,
                        format!("URL конфигурации log4j2 недоступен: {file_url}"),
                        None,
                    );
                }
            }
        } else {
            steps.push(DiagnosticStep {
                stage: InstallStage::Finalize,
                name: "Конфигурация логирования".to_string(),
                status: StepStatus::Ok,
                elapsed_ms: log_elapsed,
                size_bytes: None,
                sha1: None,
                summary: "Стандартный вывод консоли (без отдельного файла log4j2)".to_string(),
                error_reason: None,
                details: vec![],
            });
            logger.info(InstallStage::Finalize, "Стандартная конфигурация логирования");
        }
    } else {
        steps.push(DiagnosticStep {
            stage: InstallStage::Finalize,
            name: "Конфигурация логирования".to_string(),
            status: StepStatus::Ok,
            elapsed_ms: log_elapsed,
            size_bytes: None,
            sha1: None,
            summary: "Стандартный консольный лог (версия не требует отдельного XML-конфига log4j2)"
                .to_string(),
            error_reason: None,
            details: vec![],
        });
        logger.info(InstallStage::Finalize, "XML-файл log4j2 не требуется версией");
    }

    // Determine overall diagnostic status
    let has_fail = steps.iter().any(|s| s.status == StepStatus::Fail);
    let has_warn = steps.iter().any(|s| s.status == StepStatus::Warn);
    let overall_status = if has_fail {
        StepStatus::Fail
    } else if has_warn {
        StepStatus::Warn
    } else {
        StepStatus::Ok
    };

    Ok(InstallationDiagnosticReport {
        version: mc_version.to_string(),
        loader: loader.to_string(),
        timestamp: Utc::now(),
        overall_status,
        steps,
        logs: logger.records().to_vec(),
        system_info,
    })
}

/// CLI runner for `mq doctor --install <version>`.
pub async fn cmd_doctor_install(
    version: &str,
    args: &DoctorArgs,
    _paths: &MqPaths,
) -> Result<()> {
    println!("================================================================================");
    println!("          MQ LAUNCHER v2.2.0 — ДИАГНОСТИКА УСТАНОВКИ (DOCTOR INSTALL)");
    println!("================================================================================");
    println!(
        "[ПАРАМЕТРЫ] Версия Minecraft: {} | Модлоадер: {}",
        version, args.loader
    );

    let report = run_install_doctor(
        version,
        &args.loader,
        args.loader_version.as_deref(),
        args.mock_base_url.as_deref(),
    )
    .await?;

    println!(
        "[СИСТЕМА]   ОС: {} | Архитектура: {} | Ядер CPU: {}",
        report.system_info.os, report.system_info.arch, report.system_info.num_cpus
    );
    println!("--------------------------------------------------------------------------------");

    for (idx, step) in report.steps.iter().enumerate() {
        println!(
            "[{}/{}] {:<6} [{}] {}",
            idx + 1,
            report.steps.len(),
            step.status.label(),
            step.stage.localized_name(),
            step.summary
        );
        if let Some(err) = &step.error_reason {
            println!("             [!] Ошибка/Причина: {err}");
        }
        for detail in &step.details {
            println!("             * {detail}");
        }
    }

    println!("--------------------------------------------------------------------------------");
    match report.overall_status {
        StepStatus::Ok => {
            println!(
                "[ИТОГ]      [OK] Диагностика версии '{}' завершена: ВСЕ ПРОВЕРКИ ПРОЙДЕНЫ ({}/{}).",
                version,
                report.steps.len(),
                report.steps.len()
            );
            println!("            Версия готова к быстрой и надёжной установке: 'mq install --version {}'", version);
        }
        StepStatus::Warn => {
            println!(
                "[ИТОГ]      [WARN] Диагностика завершена с предупреждениями (предупреждений: {}).",
                report.warn_count()
            );
            println!("            Установка возможна, но обратите внимание на замечания выше.");
        }
        StepStatus::Fail => {
            println!(
                "[ИТОГ]      [FAIL] Диагностика выявила критические проблемы (ошибок: {}).",
                report.fail_count()
            );
            println!("            Установка данной версии в текущем окружении завершится сбоем.");
            println!("            Устраните указанные ошибки (например, загрузите требуемую версию Java).");
        }
    }
    println!("================================================================================");

    if args.json {
        println!("\n--- [JSON ЭКСПОРТ ДИАГНОСТИКИ (САНИТИЗИРОВАН)] ---");
        println!("{}", export_diagnostic_json_sanitized(&report, &[]));
    } else if args.export {
        println!("\n--- [ЭКСПОРТ ДИАГНОСТИКИ ДЛЯ БАГ-РЕПОРТА (САНИТИЗИРОВАН)] ---");
        println!("{}", export_diagnostic_report_sanitized(&report, &[]));
    }

    if report.overall_status == StepStatus::Fail {
        bail!("Диагностика установки Minecraft '{version}' выявила критические ошибки.");
    }

    Ok(())
}

// -----------------------------------------------------------------------------
// 7. Unit Tests
// -----------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_install_stage_localized_names_and_contracts() {
        assert_eq!(InstallStage::Manifest.localized_name(), "Манифест версии");
        assert_eq!(InstallStage::VersionJson.localized_name(), "Конфигурация версии");
        assert_eq!(InstallStage::JavaRuntime.localized_name(), "Среда выполнения Java");
        assert_eq!(InstallStage::ClientJar.localized_name(), "Игровой клиент");
        assert_eq!(InstallStage::Libraries.localized_name(), "Библиотеки");
        assert_eq!(InstallStage::Natives.localized_name(), "Нативные библиотеки");
        assert_eq!(InstallStage::Assets.localized_name(), "Игровые ресурсы");
        assert_eq!(InstallStage::ModLoader.localized_name(), "Модлоадер");
        assert_eq!(InstallStage::Verification.localized_name(), "Проверка файлов");
        assert_eq!(InstallStage::Finalize.localized_name(), "Финализация");

        // Serde roundtrip matching snake_case contract in CONTRACTS.md
        let json = serde_json::to_string(&InstallStage::JavaRuntime).unwrap();
        assert_eq!(json, "\"java_runtime\"");
        let parsed: InstallStage = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, InstallStage::JavaRuntime);
    }

    #[test]
    fn test_structured_installation_logging() {
        let mut collector = InstallLogCollector::new();
        collector.info(InstallStage::Manifest, "Манифест загружен");
        collector.warn(
            InstallStage::JavaRuntime,
            "Используется более новая Java",
            Some("Java 25 вместо Java 21".to_string()),
        );
        collector.error(InstallStage::ClientJar, "404 Not Found", None);

        let records = collector.records();
        assert_eq!(records.len(), 3);
        assert_eq!(records[0].stage, InstallStage::Manifest);
        assert_eq!(records[0].level, InstallLogLevel::Info);
        assert_eq!(records[1].stage, InstallStage::JavaRuntime);
        assert_eq!(records[1].level, InstallLogLevel::Warn);
        assert_eq!(records[2].stage, InstallStage::ClientJar);
        assert_eq!(records[2].level, InstallLogLevel::Error);
    }

    #[test]
    fn test_secret_sanitization_jwt_tokens() {
        let raw = "Authorization: Bearer eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiIxMjM0NTY3ODkwIiwibmFtZSI6IkpvaG4gRG9lIn0.SflKxwRJSMeKKF2QT4fwpMeJf36POk6yJV_adQssw5c";
        let scrubbed = sanitize_diagnostics(raw, &[]);
        assert!(!scrubbed.contains("eyJhbGci"));
        assert!(!scrubbed.contains("SflKxwRJS"));
        assert_eq!(scrubbed, "Authorization: Bearer [REDACTED]");
    }

    #[test]
    fn test_secret_sanitization_bearer_and_tokens() {
        let raw = "GET /profile HTTP/1.1\nAuthorization: Bearer secret_session_token_12345\nX-Other: normal";
        let scrubbed = sanitize_diagnostics(raw, &[]);
        assert!(!scrubbed.contains("secret_session_token_12345"));
        assert!(scrubbed.contains("Authorization: Bearer [REDACTED]"));
        assert!(scrubbed.contains("X-Other: normal"));
    }

    #[test]
    fn test_secret_sanitization_cli_flags() {
        let cmd = "java -jar client.jar --accessToken eyJabc123.def456.ghi789 --clientToken 99887766-aaaa-bbbb-cccc-ddddeeeeffff --uuid 12345678-1234-1234-1234-123456789abc --password MySecretPass123! --username Steve";
        let scrubbed = sanitize_diagnostics(cmd, &[]);
        assert!(!scrubbed.contains("MySecretPass123!"));
        assert!(!scrubbed.contains("12345678-1234-1234-1234-123456789abc"));
        assert!(!scrubbed.contains("99887766-aaaa-bbbb-cccc-ddddeeeeffff"));
        assert!(!scrubbed.contains("eyJabc123"));
        assert!(scrubbed.contains("--accessToken [REDACTED]"));
        assert!(scrubbed.contains("--clientToken [REDACTED]"));
        assert!(scrubbed.contains("--uuid [REDACTED]"));
        assert!(scrubbed.contains("--password [REDACTED]"));
        assert!(scrubbed.contains("--username Steve"));
    }

    #[test]
    fn test_secret_sanitization_json_key_value_pairs() {
        let json_body = r#"{
            "username": "Player1",
            "accessToken": "ms_token_998877665544332211",
            "clientToken": "cf_token_secret_1234",
            "password": "SuperSecretPassword!",
            "sha1": "34f8a912b74044cd93c289adfae10113289abc12"
        }"#;

        let scrubbed = sanitize_diagnostics(json_body, &[]);
        assert!(!scrubbed.contains("ms_token_998877665544332211"));
        assert!(!scrubbed.contains("cf_token_secret_1234"));
        assert!(!scrubbed.contains("SuperSecretPassword!"));
        assert!(scrubbed.contains("\"username\": \"Player1\""));
        assert!(scrubbed.contains("\"accessToken\": \"[REDACTED]\""));
        assert!(scrubbed.contains("\"password\": \"[REDACTED]\""));
        // SHA-1 checksum MUST be preserved!
        assert!(scrubbed.contains("34f8a912b74044cd93c289adfae10113289abc12"));
    }

    #[test]
    fn test_secret_sanitization_preserves_sha1_and_urls() {
        let text = "Downloaded client.jar from https://piston-data.mojang.com/v1/objects/34f8a912b74044cd93c289adfae10113289abc12/client.jar (SHA-1: 34f8a912b74044cd93c289adfae10113289abc12)";
        let scrubbed = sanitize_diagnostics(text, &[]);
        assert_eq!(scrubbed, text);
    }

    #[test]
    fn test_secret_sanitization_url_query_params() {
        let url = "https://example.com/api?token=secret_param_token_999&auth=private_key_xyz&version=1.21.1";
        let scrubbed = sanitize_diagnostics(url, &[]);
        assert!(!scrubbed.contains("secret_param_token_999"));
        assert!(!scrubbed.contains("private_key_xyz"));
        assert!(scrubbed.contains("?token=[REDACTED]"));
        assert!(scrubbed.contains("&auth=[REDACTED]"));
        assert!(scrubbed.contains("&version=1.21.1"));
    }

    #[test]
    fn test_diagnostic_report_export_and_copy() {
        let step1 = DiagnosticStep {
            stage: InstallStage::Manifest,
            name: "Манифест версии".to_string(),
            status: StepStatus::Ok,
            elapsed_ms: 120,
            size_bytes: Some(1024),
            sha1: None,
            summary: "Манифест Mojang получен за 120 мс".to_string(),
            error_reason: None,
            details: vec!["Версия 1.21.1 найдена".to_string()],
        };

        let report = InstallationDiagnosticReport {
            version: "1.21.1".to_string(),
            loader: "vanilla".to_string(),
            timestamp: Utc::now(),
            overall_status: StepStatus::Ok,
            steps: vec![step1],
            logs: vec![],
            system_info: DiagnosticSystemInfo {
                os: "Linux x86_64".to_string(),
                arch: "x86_64".to_string(),
                num_cpus: 8,
                available_java_runtimes: vec!["Java 21".to_string()],
            },
        };

        let markdown = export_diagnostic_report_sanitized(&report, &[]);
        assert!(markdown.contains("### MQ Launcher v2.2.0 — Отчёт диагностики установки"));
        assert!(markdown.contains("Версия игры: 1.21.1"));
        assert!(markdown.contains("ГОТОВО К УСТАНОВКЕ [OK]"));
        assert!(markdown.contains("Манифест Mojang получен за 120 мс"));

        let json = export_diagnostic_json_sanitized(&report, &[]);
        assert!(json.contains("\"version\": \"1.21.1\""));
        assert!(json.contains("\"overall_status\": \"OK\""));
    }
}
