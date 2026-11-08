# Руководство по участию в разработке (CONTRIBUTING)

Благодарим за интерес к развитию **MQ Launcher**. Проект развивается как высокопроизводительный, надежный и открытый инструмент для запуска Minecraft: Java Edition на Linux и Windows.

---

## 1. Архитектурные принципы

При внесении любых изменений соблюдайте ключевые архитектурные правила проекта:

1. **Разделение доменов потоков:**
   - **UI Domain (`eframe` / `egui`):** Никаких блокирующих вызовов I/O, синхронных сетевых запросов или тяжелых вычислений в потоке рендеринга.
   - **Async I/O Domain (`tokio`):** Асинхронные сетевые загрузки и работа с API. Сетевой пул ограничен семафором `Semaphore(32)`.
   - **CPU Compute Domain (`rayon`):** Параллельное хеширование (SHA-1, SHA-512), распаковка ZIP/JAR архивов, разбор манифестов. Потоки управляются через глобальный пул Rayon (`--threads N`).
2. **Дизайн без визуального шума (Anti-Slop):**
   - Запрещено использование эмодзи в интерфейсе лаунчера. Все иконки рисуются векторно через `egui::Painter`.
   - Окно лаунчера полностью непрозрачно (`with_transparent(false)`), соблюдается строгая 4/8-пиксельная сетка.
   - В интерфейсе используется строгий русский Sentence case.
3. **Безопасность и приватность:**
   - Строгая нулевая телеметрия: лаунчер не отправляет аналитику, идентификаторы оборудования или телеметрию падений на промежуточные серверы.
   - Токены авторизации сохраняются в системном хранилище ключей (Secret Service API / Windows Credential Manager) либо в файлах с маской `0600`.
   - Никаких форм ввода паролей внутри лаунчера: Microsoft и Ely.by работают через браузерные OAuth2-потоки.

---

## 2. Системные требования для сборки

Для локальной сборки необходимы:

* **Rust:** актуальная стабильная версия (1.80+), Rust 2021 edition.
* **Linux:**
  * Пакеты разработки графической подсистемы X11/Wayland:
    * Arch Linux: `sudo pacman -S base-devel pkg-config libx11 libxcursor libxrandr libxi libxkbcommon wayland mesa`
    * Ubuntu / Debian: `sudo apt-get install build-essential pkg-config libx11-dev libxcursor-dev libxrandr-dev libxi-dev libxkbcommon-dev libwayland-dev libgl1-mesa-dev`
    * Fedora: `sudo dnf install gcc pkgconfig libX11-devel libXcursor-devel libXrandr-devel libXi-devel libxkbcommon-devel wayland-devel mesa-libGL-devel`
* **Windows 10/11 x64:**
  * Visual Studio Build Tools (C++ x64/x86 build tools, Windows SDK).

---

## 3. Сборка и запуск

Склонируйте репозиторий:

```bash
git clone https://github.com/MrQuster/mq.git
cd mq
```

### Сборка отладочной версии (Debug)
```bash
cargo build
```

### Сборка релизной версии (Release)
```bash
cargo build --release
```
Исполняемый файл будет доступен в `target/release/mq` (или `target/release/mq.exe` на Windows).

### Запуск GUI
```bash
cargo run --bin mq
```

### Запуск CLI команд
```bash
cargo run --bin mq -- doctor
cargo run --bin mq -- instances list
cargo run --bin mq -- accounts list
```

---

## 4. Контроль качества (Quality Gates)

Перед отправкой изменений в репозиторий убедитесь, что все автоматические проверки завершаются успешно.

### 1. Форматирование кода
```bash
cargo fmt --all -- --check
```
Для автоматического исправления стиля запустите:
```bash
cargo fmt --all
```

### 2. Статический анализ (Clippy)
Проект компилируется с запретом предупреждений (`-D warnings`):
```bash
cargo clippy --workspace --all-targets -- -D warnings
```

### 3. Автоматические тесты
```bash
cargo test --workspace
```

### 4. Комплексная проверка одним скриптом
В репозитории доступен скрипт, повторяющий шаги CI:
```bash
./scripts/check.sh
```

---

## 5. Безопасность диагностики и логирования

При составлении отчётов об ошибках или создании Pull Request:

1. **Санитизация логов:**
   - Перед публикацией фрагментов вывода `~/.local/share/mq/logs/` или `%LOCALAPPDATA%\MQ\logs\` убедитесь, что в них отсутствуют личные токены авторизации, пути к домашним директориям или конфиденциальные данные.
   - MQ автоматически маскирует чувствительные поля в логах, но пользователь несет ответственность за публикуемый контент.
2. **Нераскрытые уязвимости:**
   - Не создавайте публичные pull request или issue с демонстрацией критических уязвимостей безопасности. Следуйте [Политике безопасности](docs/SECURITY.md) и используйте приватные отчеты через [GitHub Security Advisories](https://github.com/MrQuster/mq/security/advisories/new).

---

## 6. Правила оформления Pull Request

1. Создавайте отдельную тематическую ветку для каждой функциональности или исправления:
   ```bash
   git checkout -b feature/issue-description
   ```
2. Пишите понятные сообщения коммитов в повелительном наклонении:
   - `fix: resolve race condition in download retry handler`
   - `feat: add support for CurseForge modpack manifest v2`
   - `docs: update troubleshooting guide for Wayland drivers`
3. Не коммитьте сгенерированные файлы (`target/`, `dist/`, `.part`, логи).
4. Убедитесь, что добавленная функциональность покрыта модульными или интеграционными тестами.
