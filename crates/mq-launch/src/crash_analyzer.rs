//! Automated game crash analysis and 1-click `mclogs` API integration.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use mq_core::{MqError, MqResult};
use serde::{Deserialize, Serialize};

/// Detailed result of a game crash diagnosis.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CrashAnalysis {
    /// Non-zero process exit code, if known.
    pub exit_code: Option<i32>,
    /// Concise human-readable cause in Russian.
    pub summary: String,
    /// Actionable recommendation in Russian.
    pub recommendation: String,
    /// Names or identifiers of mods suspected of causing the crash.
    pub suspected_mods: Vec<String>,
    /// Relevant error snippet or stacktrace.
    pub raw_snippet: String,
    /// Absolute path to the detected `crash-reports/*.txt` file, if present.
    pub crash_report_file: Option<PathBuf>,
}

/// Analyzes crash reports and trailing console log lines to diagnose game failure.
pub fn analyze_crash(
    instance_dir: &Path,
    exit_code: Option<i32>,
    log_lines: &[String],
) -> CrashAnalysis {
    // 1. Check for recent crash-reports/*.txt
    let crash_dir = instance_dir.join("crash-reports");
    let mut latest_crash_file: Option<(PathBuf, SystemTime)> = None;

    if let Ok(entries) = fs::read_dir(&crash_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() && path.extension().and_then(|s| s.to_str()) == Some("txt") {
                if let Ok(meta) = path.metadata() {
                    if let Ok(mtime) = meta.modified() {
                        if latest_crash_file
                            .as_ref()
                            .map(|(_, t)| mtime > *t)
                            .unwrap_or(true)
                        {
                            latest_crash_file = Some((path, mtime));
                        }
                    }
                }
            }
        }
    }

    let mut crash_text = String::new();
    let mut report_path = None;

    if let Some((path, _)) = latest_crash_file {
        if let Ok(content) = fs::read_to_string(&path) {
            crash_text = content;
            report_path = Some(path);
        }
    }

    if crash_text.is_empty() {
        // Fallback: analyze trailing 500 lines of console output
        let trailing = if log_lines.len() > 500 {
            &log_lines[log_lines.len() - 500..]
        } else {
            log_lines
        };
        crash_text = trailing.join("\n");
    }

    diagnose_text(&crash_text, exit_code, report_path)
}

fn diagnose_text(
    text: &str,
    exit_code: Option<i32>,
    report_file: Option<PathBuf>,
) -> CrashAnalysis {
    let lower = text.to_lowercase();
    let mut suspected_mods = Vec::new();

    // 1. Out of Memory (OOM)
    if lower.contains("outofmemoryerror")
        || lower.contains("java heap space")
        || exit_code == Some(137)
    {
        return CrashAnalysis {
            exit_code,
            summary: "Нехватка оперативной памяти (OutOfMemoryError)".to_string(),
            recommendation: "Выделенной памяти недостаточно для загрузки игры и модов. Перейдите в настройки инстанса и увеличьте максимальное выделение RAM (рекомендуется от 4096 МБ до 6144 МБ).".to_string(),
            suspected_mods: Vec::new(),
            raw_snippet: extract_snippet(text, "OutOfMemoryError", 6),
            crash_report_file: report_file,
        };
    }

    // 2. Incompatible Java version
    if lower.contains("unsupportedclassversionerror")
        || lower.contains("has been compiled by a more recent version of the java runtime")
    {
        return CrashAnalysis {
            exit_code,
            summary: "Неподходящая версия Java (UnsupportedClassVersionError)".to_string(),
            recommendation: "Один или несколько модов скомпилированы для более новой версии Java, чем выбранная. Перейдите в настройки инстанса или воспользуйтесь автоматической загрузкой Temurin JRE 17 / 21.".to_string(),
            suspected_mods: Vec::new(),
            raw_snippet: extract_snippet(text, "UnsupportedClassVersionError", 6),
            crash_report_file: report_file,
        };
    }

    // 3. Duplicate Mods
    if lower.contains("duplicatemodsfoundexception") || lower.contains("duplicate mod id") {
        extract_mod_candidates(text, &mut suspected_mods);
        return CrashAnalysis {
            exit_code,
            summary: "Обнаружены дубликаты модов".to_string(),
            recommendation: "В папке mods установлены разные версии одного и того же мода. Удалите устаревшие копии во вкладке «Моды».".to_string(),
            suspected_mods,
            raw_snippet: extract_snippet(text, "duplicate", 8),
            crash_report_file: report_file,
        };
    }

    // 4. Missing Dependency
    if lower.contains("classnotfoundexception") || lower.contains("noclassdeffounderror") {
        extract_mod_candidates(text, &mut suspected_mods);
        return CrashAnalysis {
            exit_code,
            summary: "Отсутствует обязательная библиотека или зависимость мода".to_string(),
            recommendation: "Один из модов требует библиотеки (например, Fabric API, Cloth Config, Architectury). Проверьте список зависимостей во вкладке «Моды» или в Каталоге.".to_string(),
            suspected_mods,
            raw_snippet: extract_snippet(text, "ClassNotFoundException", 8),
            crash_report_file: report_file,
        };
    }

    // 5. Mixin Transformation Failure
    if lower.contains("mixinapplyerror") || lower.contains("org.spongepowered.asm.mixin") {
        extract_mod_candidates(text, &mut suspected_mods);
        return CrashAnalysis {
            exit_code,
            summary: "Конфликт Mixin-инъекций между модами".to_string(),
            recommendation: "Два или более мода пытаются изменить один и тот же метод в игре несовместимым способом. Попробуйте обновить конфликтующие моды до актуальных версий или отключить подозрительный мод.".to_string(),
            suspected_mods,
            raw_snippet: extract_snippet(text, "mixin", 10),
            crash_report_file: report_file,
        };
    }

    // 6. Shader / Iris / Sodium / OpenGL / GLFW errors
    if lower.contains("net.irisshaders")
        || lower.contains("me.jellysquid.mods.sodium")
        || lower.contains("glfw error")
        || lower.contains("wgl")
        || lower.contains("glcontext")
        || lower.contains("opengl error")
        || lower.contains("gl_invalid_operation")
    {
        extract_mod_candidates(text, &mut suspected_mods);
        return CrashAnalysis {
            exit_code,
            summary: "Сбой графического шейдер-движка или OpenGL/GLFW".to_string(),
            recommendation: "Произошла ошибка инициализации рендерера (Iris/Sodium/OpenGL). Убедитесь, что драйверы видеокарты обновлены (поддержка OpenGL >= 3.3) и установлена совместимая версия шейдеров и Iris/Sodium для вашей версии игры.".to_string(),
            suspected_mods,
            raw_snippet: extract_snippet(text, "gl", 8),
            crash_report_file: report_file,
        };
    }

    // 7. Generic Failure
    extract_mod_candidates(text, &mut suspected_mods);
    let code_str = exit_code.map(|c| format!(" (код {c})")).unwrap_or_default();
    CrashAnalysis {
        exit_code,
        summary: format!("Игра неожиданно завершила работу{code_str}"),
        recommendation: "Проверьте журнал консоли и отчёт об ошибке. Если установлены новые моды, попробуйте отключать их по очереди для локализации сбоя.".to_string(),
        suspected_mods,
        raw_snippet: extract_tail_snippet(text, 12),
        crash_report_file: report_file,
    }
}

fn extract_snippet(text: &str, keyword: &str, line_count: usize) -> String {
    let lower_kw = keyword.to_lowercase();
    let lines: Vec<&str> = text.lines().collect();
    for (i, line) in lines.iter().enumerate() {
        if line.to_lowercase().contains(&lower_kw) {
            let start = i.saturating_sub(2);
            let end = (start + line_count).min(lines.len());
            return lines[start..end].join("\n");
        }
    }
    extract_tail_snippet(text, line_count)
}

fn extract_tail_snippet(text: &str, line_count: usize) -> String {
    let lines: Vec<&str> = text.lines().collect();
    if lines.len() <= line_count {
        return text.to_string();
    }
    lines[lines.len() - line_count..].join("\n")
}

fn extract_mod_candidates(text: &str, out: &mut Vec<String>) {
    for line in text.lines() {
        if line.contains(".jar") {
            for token in line.split_whitespace() {
                if token.ends_with(".jar") || token.contains(".jar:") {
                    let clean = token.trim_matches(|c: char| {
                        !c.is_alphanumeric() && c != '.' && c != '-' && c != '_'
                    });
                    if !clean.is_empty() && !out.contains(&clean.to_string()) && out.len() < 5 {
                        out.push(clean.to_string());
                    }
                }
            }
        }
    }
}

/// Uploads a crash log or console log to the public `mclogs` API (`https://api.mclogs.com/1/log`).
///
/// Returns the shareable URL (e.g. `https://mclogs.com/xxxxxxx`).
pub async fn upload_to_mclogs(client: &reqwest::Client, log_content: &str) -> MqResult<String> {
    if log_content.trim().is_empty() {
        return Err(MqError::Internal("log content is empty".to_string()));
    }

    let url = "https://api.mclogs.com/1/log";
    let params = [("content", log_content)];

    let resp = client
        .post(url)
        .form(&params)
        .send()
        .await
        .map_err(|e| MqError::Network(format!("failed to connect to mclogs API: {e}")))?;

    if !resp.status().is_success() {
        return Err(MqError::Network(format!(
            "mclogs API returned HTTP status {}",
            resp.status()
        )));
    }

    let json_val: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| MqError::Network(format!("failed parsing mclogs API response: {e}")))?;

    if json_val.get("success").and_then(|v| v.as_bool()) == Some(true) {
        if let Some(log_url) = json_val.get("url").and_then(|v| v.as_str()) {
            return Ok(log_url.to_string());
        }
    }

    let err_msg = json_val
        .get("error")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown error from mclogs API");

    Err(MqError::Network(format!("mclogs upload error: {err_msg}")))
}
