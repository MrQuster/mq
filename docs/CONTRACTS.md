# MQ — Архитектурные контракты подсистемы установки (CONTRACTS.md)

Документ фиксирует общие типы данных, контракты этапов установки, форматы событий и схему `install_state.json`. Все 7 агентов обязаны строго соблюдать эти контракты.

---

## 1. Этапы установки (`InstallStage`)

Перечисление этапов установки в `crates/mq-core/src/types.rs` или `crates/mq-install/src/types.rs`:

```rust
use serde::{Deserialize, Serialize};

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
```

---

## 2. Структурированная ошибка установки (`InstallError`)

```rust
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
```

---

## 3. Схема состояния установки инстанса (`install_state.json`)

Файл создается в каталоге инстанса: `<instance_dir>/install_state.json`.

```json
{
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
    "sha1": "...",
    "size": 34123456
  },
  "libraries_count": 78,
  "assets_count": 5114,
  "asset_index_id": "34",
  "natives_count": 0
}
```

Возможные значения `status`:
* `"installing"` — процесс установки активен во временном каталоге `.installing`
* `"ready"` — все файлы успешно скачаны, проверены по SHA-1 и перенесены в рабочую папку
* `"failed"` — установка завершилась ошибкой, содержит поле `last_error`
* `"verifying"` — выполняется повторная сверка файлов

---

## 4. Контракт транзакционности установки
1. Установка создает временный рабочий каталог `<instance_dir>/.installing/`.
2. Скачивание всех артефактов (`client.jar`, `resolved_version.json`, распакованные нативы) производится в `.installing/`.
3. Библиотеки и ассеты сохраняются в CAS (`<data_dir>/cas/sha1/...`) и хардлинкуются в `<data_dir>/libraries/` и `<data_dir>/assets/`.
4. После успешной проверки всех SHA-1 контрольных сумм:
   * Файлы атомарно перемещаются из `.installing/` в `<instance_dir>/`.
   * Записывается валидный `<instance_dir>/install_state.json` со статусом `"ready"`.
   * Каталог `.installing/` удаляется.
5. При отмене или ошибке:
   * Каталог `.installing/` очищается.
   * `install_state.json` помечается статусом `"failed"` с сохранением `InstallError`.
   * При повторной попытке установка безопасно возобновляется с докачкой только недостающего (благодаря CAS).
