# MQ v2.2.0 Swarm Task Board

Общая координационная доска для 10 субагентов.

## 📋 Статусы задач

| ID | Агент | Задача | Статус | Блокеры / Заметки |
|:---|:---|:---|:---:|:---|
| **TASK-01** | `ag-01` | Модуль `platform`: пути `%LOCALAPPDATA%`/`%APPDATA%`, санитизация имен, long paths, хардлинки/копии | `In Progress` | - |
| **TASK-02** | `ag-02` | Запуск игры на Windows: classpath `;`, `rules`, natives-windows, `CREATE_NO_WINDOW`, Adoptium Java | `In Progress` | Зависит от TASK-01 (`platform`) |
| **TASK-03** | `ag-03` | Windows UI: fallback рендерера, `AttachConsole`, шрифты Segoe UI, замена `xdg-open` | `In Progress` | - |
| **TASK-04** | `ag-04` | Ely.by OAuth2 PKCE browser-only, удаление старых паролей, хранилище Windows Credential / DPAPI | `In Progress` | - |
| **TASK-05** | `ag-05` | Универсальный `install.sh`: функция `main()`, sha256sum check, `--uninstall`, `shellcheck` | `In Progress` | - |
| **TASK-06** | `ag-06` | Inno Setup `mq.iss`, PowerShell `install.ps1`, портативный ZIP, winget/scoop, `WINDOWS_SIGNING.md` | `In Progress` | - |
| **TASK-07** | `ag-07` | CI матрица Ubuntu/Windows, закрепление SHA Actions, smoke тесты установки, аттестации | `In Progress` | - |
| **TASK-08** | `ag-08` | Minisign подпись `SHA256SUMS.minisig`, секрет `MQ_SIGNING_KEY`, самообновление на Windows (`.old`) | `In Progress` | - |
| **TASK-09** | `ag-09` | Харденинг: `SECURITY.md`, `THREAT_MODEL.md`, `PRIVACY.md`, `deny.toml`, защита от zip-бомб и команд | `In Progress` | - |
| **TASK-10** | `ag-10` | Слияние веток, гейты fmt/clippy/test, zero-stubs, обновление README, CHANGELOG, релиз | `In Progress` | Координация |

---

## 💬 Сообщения и запросы между агентами
- `ag-01`: Готовит `mq_core::platform` с `classpath_separator()` и `sanitize_filename()`.
- `ag-08`: Готовит пару ключей minisign. Публичный ключ будет экспортирован в `mq-update`.
