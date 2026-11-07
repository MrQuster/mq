# MQ v2.2.0 Porting Audit: Linux -> Windows Parity

Этот документ фиксирует полный аудит кодовой базы MQ Launcher перед портированием на Windows 10/11 x64.

---

## 1. Карта крейтов и зависимости от платформы

| Крейт | Назначение | Платформозависимые участки | Требуемые изменения для Windows |
|:---|:---|:---|:---|
| `crates/mq-core` | Базовые типы, пути, конфиг | Пути `MqPaths` завязаны на XDG (`~/.local/share/mq`, `~/.config/mq`); права `0600` через `PermissionsExt` | Вынести в модуль `platform`: Windows конфиг `%APPDATA%\MQ`, данные `%LOCALAPPDATA%\MQ`; длинные пути `\\?\`; санитизация имен файлов |
| `crates/mq-sched` | DAG, Rayon, телеметрия | Чтение `/proc/stat` и `/proc/self/status` | Обернуть чтение `/proc` в `#[cfg(unix)]`; на Windows использовать API `GetProcessMemoryInfo` и `GetSystemTimes` (или заглушку) |
| `crates/mq-net` | HTTP клиент, CAS | Нет | Полностью кроссплатформенный (reqwest + tokio) |
| `crates/mq-meta` | Манифесты версий | Правила `rules` в `version.json` для `os.name = "windows"` | Парсинг и применение правил для `windows`; проверка нативных библиотек `natives-windows` |
| `crates/mq-auth` | Аккаунты, хранение токенов | Хранение `keyring` / `0600` файл; Java agent injection | Linux Secret Service -> Windows Credential Manager; fallback на DPAPI; пути к Java agent |
| `crates/mq-install` | CAS, установка, распаковка | Хардлинки в CAS; Zip-Slip защита | На Windows fallback с хардлинка на копирование; запрет запрещенных символов в путях распаковки |
| `crates/mq-mods` | Каталог, парсинг jar, lockfile | `glxinfo -B` для GPU диагностики | На Windows использовать безопасную проверку адаптера через `wgpu` или заглушку |
| `crates/mq-instances` | Менеджер инстансов | Поле `wrapper_cmd` (`gamemoderun`, `mangohud`) | Скрытие Linux-врапперов на Windows |
| `crates/mq-launch` | Генерация команд запуска | Разделитель classpath `:`, вызов `java`, права `0755`, `setsid` | Разделитель classpath `;`; запуск `java.exe` с `CREATE_NO_WINDOW`; остановка через дерево процессов (`taskkill /T /F /PID`); скрытие Linux-врапперов |
| `crates/mq-ui` | GUI на egui/eframe | Вызовы `xdg-open`; рендерер WGPU/Glow; шрифты | Замена `xdg-open` на кроссплатформенный `open::that()` / `explorer`; WGPU/DX12 + Glow fallback; Segoe UI + Inter; DPI awareness |
| `crates/mq-update` | Самообновление | Атомарная замена `<exe>.new`, `setsid` | На Windows: переименование `mq.exe` в `mq.exe.old`, запись нового `mq.exe`, тихий запуск Inno Setup; запуск detached процесса через Windows API |
| `crates/mq-cli` | CLI точка входа | CLI запуск | `#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]` + `AttachConsole(ATTACH_PARENT_PROCESS)` для CLI вывода |

---

## 2. Аудит сценариев запуска и путей

1. **Разделитель Classpath:** На Linux используется `:`. На Windows JVM требует строго `;`.
2. **Нативные библиотеки:** В манифестах версий Mojang нативные библиотеки для Windows помечаются как `natives-windows` или классификатором `windows`.
3. **Длинные пути:** В Windows путь может превышать лимит MAX_PATH (260 символов), особенно для вложенных модов и ресурспаков. Необходима поддержка префикса `\\?\` и манифеста `longPathAware`.
4. **Символы в именах файлов:** Символы `< > : " / \ | ? *` запрещены на Windows, а также зарезервированные имена `CON`, `PRN`, `AUX`, `NUL`, `COM1-9`, `LPT1-9`.
5. **Пути с кириллицей и пробелами:** На Windows учетная запись часто называется кириллицей (напр., `C:\Users\Иван Петров\AppData\Local\MQ`). JVM требует аргумент `-Dfile.encoding=UTF-8` и строгое разделение аргументов без шелл-склейки.

---

## 3. Аудит установщиков и скриптов

1. `install.sh`: Требуется переработка в функцию `main()`, вызов на последней строке, определение x86_64, валидация SHA-256 против `SHA256SUMS`, интеграция minisign, флаги `--version`, `--uninstall`, `--dry-run`.
2. Windows: Создание `packaging/windows/mq.iss` для сборки `MQ-Setup.exe` без требования прав администратора (User-level) и PowerShell 1-command скрипта `install.ps1`.
3. Скрипты упаковки: Портативный zip для Windows `mq-windows-x86_64-portable.zip` с флагом `portable.flag`.
