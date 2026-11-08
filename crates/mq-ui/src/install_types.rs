use serde::{Deserialize, Serialize};

/// Installation stages defined in CONTRACTS.md.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InstallStage {
    /// Loading and parsing official Mojang version manifest
    Manifest,
    /// Loading and parsing version.json (vanilla / inheritsFrom)
    VersionJson,
    /// Checking system Java and downloading required JRE (Mojang / Adoptium)
    JavaRuntime,
    /// Downloading game client client.jar
    ClientJar,
    /// Downloading Maven libraries
    Libraries,
    /// Extracting and preparing native libraries (.so / .dll)
    Natives,
    /// Downloading game assets and resources
    Assets,
    /// Validating modloader (Fabric / Quilt / Forge / NeoForge)
    ModLoader,
    /// Hash verification (SHA-1 / SHA-512) and integrity check
    Verification,
    /// State recording, saving install_state.json and finalizing
    Finalize,
}

impl InstallStage {
    /// The primary 7 milestone stages rendered in the stepper UI:
    /// «Манифест версии → Java → Клиент → Библиотеки → Нативные библиотеки → Ассеты → Проверка»
    pub const ALL_DISPLAY_STAGES: [Self; 7] = [
        Self::Manifest,
        Self::JavaRuntime,
        Self::ClientJar,
        Self::Libraries,
        Self::Natives,
        Self::Assets,
        Self::Verification,
    ];

    /// Localized name matching CONTRACTS.md.
    pub fn localized_name(&self) -> &'static str {
        match self {
            Self::Manifest => "Манифест версии",
            Self::VersionJson => "Конфигурация версии",
            Self::JavaRuntime => "Java",
            Self::ClientJar => "Клиент",
            Self::Libraries => "Библиотеки",
            Self::Natives => "Нативные библиотеки",
            Self::Assets => "Ассеты",
            Self::ModLoader => "Модлоадер",
            Self::Verification => "Проверка",
            Self::Finalize => "Финализация",
        }
    }

    /// Full descriptive localized name.
    pub fn full_localized_name(&self) -> &'static str {
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

    /// Numeric index representing progression in the 7-milestone stepper.
    pub fn pipeline_index(&self) -> usize {
        match self {
            Self::Manifest | Self::VersionJson => 0,
            Self::JavaRuntime => 1,
            Self::ClientJar => 2,
            Self::Libraries | Self::ModLoader => 3,
            Self::Natives => 4,
            Self::Assets => 5,
            Self::Verification => 6,
            Self::Finalize => 7,
        }
    }

    /// Maps a pipeline phase string to its corresponding `InstallStage`.
    pub fn from_phase_str(phase: &str) -> Self {
        let p = phase.to_lowercase();
        if p.contains("asset") {
            Self::Assets
        } else if p.contains("native") {
            Self::Natives
        } else if p.contains("lib") {
            Self::Libraries
        } else if p.contains("java") || p.contains("jre") {
            Self::JavaRuntime
        } else if p.contains("client") {
            Self::ClientJar
        } else if p.contains("verify") || p.contains("check") {
            Self::Verification
        } else if p.contains("manifest") || p.contains("metadata") {
            Self::Manifest
        } else if p.contains("version") {
            Self::VersionJson
        } else if p.contains("loader")
            || p.contains("fabric")
            || p.contains("quilt")
            || p.contains("forge")
        {
            Self::ModLoader
        } else if p.contains("finalize") || p.contains("ready") {
            Self::Finalize
        } else {
            Self::Manifest
        }
    }
}

/// Structured installation error conforming to CONTRACTS.md.
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

impl InstallError {
    /// Creates a structured `InstallError` by classifying the raw error message.
    pub fn from_error_str(stage: InstallStage, error_str: &str) -> Self {
        let raw = error_str.trim().to_string();
        let (title, details, target_url, http_status, is_retryable) =
            Self::classify_error(&stage, &raw);

        Self {
            stage,
            localized_title: title,
            localized_details: details,
            target_url,
            file_path: Self::extract_path_from_text(&raw),
            http_status,
            is_retryable,
            raw_cause: raw,
        }
    }

    /// Classifies an error into a user-friendly Russian diagnosis.
    fn classify_error(
        stage: &InstallStage,
        raw: &str,
    ) -> (String, String, Option<String>, Option<u16>, bool) {
        let raw_lower = raw.to_lowercase();
        let url = Self::extract_url_from_text(raw);

        // 1. HTTP 404 Not Found
        if raw_lower.contains("404") || raw_lower.contains("not found") {
            return (
                "Файл не найден на сервере (HTTP 404)".to_string(),
                "Запрашиваемый ресурс отсутствует на удаленном сервере. Проверьте правильность версии игры или используйте другое зеркало.".to_string(),
                url,
                Some(404),
                false,
            );
        }

        // 2. HTTP 429 Too Many Requests / Rate limit
        if raw_lower.contains("429")
            || raw_lower.contains("too many requests")
            || raw_lower.contains("rate limit")
        {
            return (
                "Превышен лимит запросов к серверу (HTTP 429)".to_string(),
                "Сервер Mojang временно ограничил частоту обращений. Рекомендуется подождать 30–60 секунд и повторить попытку.".to_string(),
                url,
                Some(429),
                true,
            );
        }

        // 3. HTTP 5xx Server Error
        if raw_lower.contains("500")
            || raw_lower.contains("502")
            || raw_lower.contains("503")
            || raw_lower.contains("504")
            || raw_lower.contains("internal server error")
        {
            let status = if raw_lower.contains("503") {
                503
            } else if raw_lower.contains("502") {
                502
            } else if raw_lower.contains("504") {
                504
            } else {
                500
            };
            return (
                format!("Сервер временно недоступен (HTTP {})", status),
                "Сервер удалённого репозитория испытывает временные сбои или перегружен. Повторите попытку через минуту.".to_string(),
                url,
                Some(status),
                true,
            );
        }

        // 4. No space left on device
        if raw_lower.contains("no space left")
            || raw_lower.contains("disk full")
            || raw_lower.contains("enospc")
        {
            return (
                "Недостаточно места на диске".to_string(),
                "На накопителе закончилось свободное дисковое пространство. Освободите память на диске и повторите установку.".to_string(),
                None,
                None,
                false,
            );
        }

        // 5. Permission denied
        if raw_lower.contains("permission denied")
            || raw_lower.contains("access denied")
            || raw_lower.contains("eacces")
            || raw_lower.contains("eperm")
        {
            return (
                "Ошибка доступа к файлам (Permission Denied)".to_string(),
                "Отказано в доступе при записи файлов инстанса или библиотек. Проверьте права пользователя на каталог игры.".to_string(),
                None,
                None,
                false,
            );
        }

        // 6. Network connectivity issues
        if raw_lower.contains("failed to lookup")
            || raw_lower.contains("connection refused")
            || raw_lower.contains("network is unreachable")
            || raw_lower.contains("timed out")
            || raw_lower.contains("timeout")
            || raw_lower.contains("dns error")
            || raw_lower.contains("broken pipe")
            || raw_lower.contains("connection reset")
        {
            return (
                "Ошибка сетевого соединения".to_string(),
                "Не удалось установить стабильное соединение с сервером загрузки. Проверьте подключение к Интернету или настройки прокси/VPN.".to_string(),
                url,
                None,
                true,
            );
        }

        // 7. Checksum / Hash verification failure
        if raw_lower.contains("checksum")
            || raw_lower.contains("sha1")
            || raw_lower.contains("sha512")
            || raw_lower.contains("hash mismatch")
            || raw_lower.contains("corrupted")
            || raw_lower.contains("integrity")
        {
            return (
                "Не совпадает контрольная сумма (битый хеш)".to_string(),
                "Загруженный файл повреждён или не совпадает с контрольной суммой в официальном манифесте Mojang.".to_string(),
                url,
                None,
                true,
            );
        }

        // 8. Missing or incompatible Java
        if raw_lower.contains("java") || raw_lower.contains("jvm") || raw_lower.contains("jre") {
            return (
                "Не найдена требуемая версия Java".to_string(),
                "Для данной версии Minecraft необходима совместимая среда выполнения Java. Проверьте настройки Java или установите рекомендуемую версию.".to_string(),
                None,
                None,
                true,
            );
        }

        // 9. Fallback based on stage
        let default_title = format!("Сбой на этапе «{}»", stage.localized_name());
        let default_details = format!("Не удалось завершить операцию: {raw}");
        (default_title, default_details, url, None, true)
    }

    /// Extracts http/https URL if present in the error string.
    fn extract_url_from_text(text: &str) -> Option<String> {
        for word in text.split_whitespace() {
            let clean = word.trim_matches(|c| {
                c == '\'' || c == '"' || c == '(' || c == ')' || c == '<' || c == '>'
            });
            if clean.starts_with("http://") || clean.starts_with("https://") {
                return Some(clean.to_string());
            }
        }
        None
    }

    /// Extracts file path if present in the error string.
    fn extract_path_from_text(text: &str) -> Option<String> {
        for word in text.split_whitespace() {
            let clean = word.trim_matches(|c| {
                c == '\'' || c == '"' || c == '(' || c == ')' || c == ':' || c == ','
            });
            if (clean.starts_with('/')
                || clean.contains("/.local/")
                || clean.contains("/libraries/")
                || clean.contains("/assets/"))
                && clean.len() > 3
            {
                return Some(clean.to_string());
            }
        }
        None
    }

    /// Formats the error details for clipboard copying without any secrets.
    pub fn format_for_clipboard(&self) -> String {
        let mut out = String::new();
        out.push_str("=== MQ Launcher — Ошибка установки ===\n");
        out.push_str(&format!("Этап: {}\n", self.stage.full_localized_name()));
        out.push_str(&format!("Заголовок: {}\n", self.localized_title));
        out.push_str(&format!("Описание: {}\n", self.localized_details));
        if let Some(status) = self.http_status {
            out.push_str(&format!("HTTP статус: {}\n", status));
        }
        if let Some(ref url) = self.target_url {
            out.push_str(&format!("URL: {}\n", url));
        }
        if let Some(ref path) = self.file_path {
            out.push_str(&format!("Файл: {}\n", path));
        }
        out.push_str(&format!(
            "Повторяемая: {}\n",
            if self.is_retryable { "Да" } else { "Нет" }
        ));
        out.push_str(&format!("Техническая причина:\n{}\n", self.raw_cause));
        out.push_str("======================================\n");
        mq_core::redact_text(&out, &[])
    }
}

/// Installation state contract matching CONTRACTS.md Section 3.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstanceInstallState {
    pub schema_version: u32,
    pub instance_id: String,
    pub mc_version: String,
    pub loader: String,
    pub loader_version: Option<String>,
    pub status: String, // "installing", "ready", "failed", "verifying"
    pub installed_at: Option<String>,
    pub verified_at: Option<String>,
    pub last_error: Option<InstallError>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_install_stages_display_pipeline() {
        assert_eq!(InstallStage::ALL_DISPLAY_STAGES.len(), 7);
        assert_eq!(InstallStage::ALL_DISPLAY_STAGES[0], InstallStage::Manifest);
        assert_eq!(
            InstallStage::ALL_DISPLAY_STAGES[1],
            InstallStage::JavaRuntime
        );
        assert_eq!(InstallStage::ALL_DISPLAY_STAGES[2], InstallStage::ClientJar);
        assert_eq!(InstallStage::ALL_DISPLAY_STAGES[3], InstallStage::Libraries);
        assert_eq!(InstallStage::ALL_DISPLAY_STAGES[4], InstallStage::Natives);
        assert_eq!(InstallStage::ALL_DISPLAY_STAGES[5], InstallStage::Assets);
        assert_eq!(
            InstallStage::ALL_DISPLAY_STAGES[6],
            InstallStage::Verification
        );
    }

    #[test]
    fn test_install_stage_names_and_progression() {
        assert_eq!(InstallStage::Manifest.localized_name(), "Манифест версии");
        assert_eq!(InstallStage::JavaRuntime.localized_name(), "Java");
        assert_eq!(InstallStage::ClientJar.localized_name(), "Клиент");
        assert_eq!(InstallStage::Libraries.localized_name(), "Библиотеки");
        assert_eq!(
            InstallStage::Natives.localized_name(),
            "Нативные библиотеки"
        );
        assert_eq!(InstallStage::Assets.localized_name(), "Ассеты");
        assert_eq!(InstallStage::Verification.localized_name(), "Проверка");

        // Pipeline indices should be monotonic
        assert!(
            InstallStage::Manifest.pipeline_index() < InstallStage::JavaRuntime.pipeline_index()
        );
        assert!(
            InstallStage::JavaRuntime.pipeline_index() < InstallStage::ClientJar.pipeline_index()
        );
        assert!(
            InstallStage::ClientJar.pipeline_index() < InstallStage::Libraries.pipeline_index()
        );
        assert!(InstallStage::Libraries.pipeline_index() < InstallStage::Natives.pipeline_index());
        assert!(InstallStage::Natives.pipeline_index() < InstallStage::Assets.pipeline_index());
        assert!(
            InstallStage::Assets.pipeline_index() < InstallStage::Verification.pipeline_index()
        );
    }

    #[test]
    fn test_stage_from_phase_str() {
        assert_eq!(
            InstallStage::from_phase_str("fetch_version_manifest"),
            InstallStage::Manifest
        );
        assert_eq!(
            InstallStage::from_phase_str("metadata"),
            InstallStage::Manifest
        );
        assert_eq!(
            InstallStage::from_phase_str("java_runtime"),
            InstallStage::JavaRuntime
        );
        assert_eq!(
            InstallStage::from_phase_str("download_client_jar"),
            InstallStage::ClientJar
        );
        assert_eq!(
            InstallStage::from_phase_str("libraries"),
            InstallStage::Libraries
        );
        assert_eq!(
            InstallStage::from_phase_str("extract_natives"),
            InstallStage::Natives
        );
        assert_eq!(
            InstallStage::from_phase_str("assets_download"),
            InstallStage::Assets
        );
        assert_eq!(
            InstallStage::from_phase_str("verify_files"),
            InstallStage::Verification
        );
    }

    #[test]
    fn test_install_error_classification() {
        // 404
        let err404 = InstallError::from_error_str(
            InstallStage::ClientJar,
            "HTTP 404 Not Found while fetching https://piston-data.mojang.com/v1/objects/test.jar",
        );
        assert_eq!(err404.http_status, Some(404));
        assert!(err404.localized_title.contains("404"));
        assert_eq!(
            err404.target_url.as_deref(),
            Some("https://piston-data.mojang.com/v1/objects/test.jar")
        );

        // No space
        let err_space = InstallError::from_error_str(
            InstallStage::Assets,
            "OS error: No space left on device (ENOSPC)",
        );
        assert!(err_space.localized_title.contains("места на диске"));
        assert!(!err_space.is_retryable);

        // Network
        let err_net = InstallError::from_error_str(
            InstallStage::Libraries,
            "Connection refused: failed to lookup address info for repo.maven.apache.org",
        );
        assert!(err_net.localized_title.contains("сетевого соединения"));
        assert!(err_net.is_retryable);

        // Hash mismatch
        let err_hash = InstallError::from_error_str(
            InstallStage::Verification,
            "SHA1 checksum mismatch for file /home/honey/.local/share/mq/libraries/test.jar",
        );
        assert!(err_hash.localized_title.contains("контрольная сумма"));
        assert_eq!(
            err_hash.file_path.as_deref(),
            Some("/home/honey/.local/share/mq/libraries/test.jar")
        );

        // Java missing
        let err_java = InstallError::from_error_str(
            InstallStage::JavaRuntime,
            "JavaNotFound: no compatible JRE detected for Minecraft 1.21.1",
        );
        assert!(err_java.localized_title.contains("версия Java"));

        // Clipboard formatting
        let clip = err404.format_for_clipboard();
        assert!(clip.contains("MQ Launcher — Ошибка установки"));
        assert!(clip.contains("HTTP статус: 404"));
        assert!(clip.contains("URL: https://piston-data.mojang.com/v1/objects/test.jar"));
    }

    #[test]
    fn test_install_error_json_serialization() {
        let err = InstallError::from_error_str(
            InstallStage::Manifest,
            "Network timeout after 30000ms: https://launchermeta.mojang.com/mc/game/version_manifest_v2.json",
        );
        let json = serde_json::to_string(&err).unwrap();
        let deserialized: InstallError = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.stage, InstallStage::Manifest);
        assert!(deserialized.is_retryable);
    }
}
