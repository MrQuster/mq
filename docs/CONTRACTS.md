# MQ v2.2.0 Architecture Contracts & Interfaces

Документ описывает интерфейсы взаимодействия между 10 субагентами.

---

## 1. Модуль платформы (`crates/mq-core/src/platform.rs`)

```rust
pub trait Platform {
    fn os_name() -> &'static str; // "linux" | "windows" | "macos"
    fn default_data_dir() -> PathBuf;
    fn default_config_dir() -> PathBuf;
    fn default_cache_dir() -> PathBuf;
    fn classpath_separator() -> char; // ';' on Windows, ':' on Unix
    fn sanitize_filename(name: &str) -> String;
    fn ensure_long_path_prefix(path: &Path) -> PathBuf;
    fn open_folder(path: &Path) -> Result<(), std::io::Error>;
    fn open_url(url: &str) -> Result<(), std::io::Error>;
}
```

---

## 2. API самообновления и подписи (`crates/mq-update`)

```rust
pub struct MinisignVerifier {
    pub public_key_base64: &'static str,
}

impl MinisignVerifier {
    pub fn verify_checksums(checksums_content: &[u8], signature_content: &[u8]) -> Result<bool, UpdateError>;
}

pub enum UpdateTargetMode {
    Portable,
    InstalledInnoSetup { setup_path: PathBuf },
}

pub struct WindowsSelfUpdate;
impl WindowsSelfUpdate {
    pub fn prepare_atomic_swap(current_exe: &Path, new_binary: &Path) -> Result<(), UpdateError>;
    pub fn launch_silent_installer(setup_exe: &Path) -> Result<(), UpdateError>;
}
```

---

## 3. Провайдер аккаунтов и Ely.by OAuth2 PKCE (`crates/mq-auth`)

```rust
pub struct PkcePair {
    pub code_verifier: String,
    pub code_challenge: String,
}

pub struct ElyByOAuthFlow {
    pub client_id: String,
    pub redirect_port: u16,
    pub pkce: PkcePair,
    pub state: String,
}

impl ElyByOAuthFlow {
    pub fn new(client_id: Option<&str>, port: u16) -> Self;
    pub fn authorize_url(&self) -> String;
    pub async fn wait_for_code_and_exchange(self) -> Result<ElyByAccountInfo, MqError>;
}
```

---

## 4. Контракт генерации запуска (`crates/mq-launch`)

- Classpath: слияние через `mq_core::platform::classpath_separator()`.
- JVM аргументы для Windows: добавление `-Dfile.encoding=UTF-8` для стабильной поддержки кириллицы.
- Создание процесса: на Windows выставление флага `CREATE_NO_WINDOW = 0x08000000`.
- Остановка игры: вызов `taskkill /F /T /PID <pid>` для завершения всего дерева дочерних процессов.
