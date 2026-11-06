# MQ — Журнал прогресса разработки (PROGRESS.md)

## Статус роя субагентов (20 профильных инженеров)

| ID | Субагент | Зона ответственности | Волна | Статус |
|---|---|---|---|---|
| 01 | `subagent-01-env-scout` | Разведка Arch Linux, Java, Wayland/X11/Vulkan, `rust-toolchain.toml`, `.gitignore`, `BLOCKERS.md` | Wave 1 | Готово |
| 02 | `subagent-02-architect` | Проектирование `docs/ARCHITECTURE.md` (DAG, владение данными, эпохи, иерархия локов, E1–E9) | Wave 1 | Готово |
| 03 | `subagent-03-core` | `crates/mq-core` (ошибки, конфиг, XDG-пути, шина `Event`, модели метрик, `[REDACTED]`) | Wave 2A | В работе |
| 04 | `subagent-04-sched-dag` | `crates/mq-sched` (мост `tokio ↔ rayon`, DAG, эпохи отмены, `BufferPool`, машина состояний) | Wave 2A | В работе |
| 05 | `subagent-05-cpu-telemetry` | `crates/mq-sched` (`/proc/stat` по ядрам, закон Амдала, `UiWatchdog >8ms`) | Wave 2A | В работе |
| 06 | `subagent-06-net-engine` | `crates/mq-net` (HTTP/2 пул, retry+jitter, Range, потоковый SHA-1/SHA-512, DashMap дедуп) | Wave 2B | Ожидание |
| 07 | `subagent-07-cas-storage` | `crates/mq-install` (CAS хардлинки, дедупликация O(1), конвейер игры, параллельный `verify`) | Wave 3 | Ожидание |
| 08 | `subagent-08-meta-vanilla` | `crates/mq-meta` (Vanilla `version_manifest`, `version.json`, `rules`, `asset_index`) | Wave 2B | Ожидание |
| 09 | `subagent-09-meta-loaders` | `crates/mq-meta` (Fabric, Quilt, `inheritsFrom`, детерминированное слияние classpath) | Wave 2B | Ожидание |
| 10 | `subagent-10-auth-offline-ely` | `crates/mq-auth` (Offline UUID v3 `OfflinePlayer:`, Ely.by Yggdrasil/скины, `authlib-injector`) | Wave 2B | Ожидание |
| 11 | `subagent-11-auth-msa` | `crates/mq-auth` (Microsoft Device Code -> Xbox -> XSTS -> MC, хранилище `0600`) | Wave 2B | Ожидание |
| 12 | `subagent-12-modrinth-cf` | `crates/mq-mods` (Modrinth API v2, CurseForge API v1, рекурсивный резолвер зависимостей) | Wave 3 | Ожидание |
| 13 | `subagent-13-modpacks-scanner` | `crates/mq-mods` (установщик `.mrpack` и `.zip`, zip-slip защита, `rayon`-сканер `.jar` модов) | Wave 3 | Ожидание |
| 14 | `subagent-14-instances-launch` | `crates/mq-instances` + `crates/mq-launch` (`instance.toml`, поиск Java, `-javaagent` Ely.by) | Wave 3 | Ожидание |
| 15 | `subagent-15-ui-theme-layout` | `crates/mq-ui` (тема Dark Glass `#0D0F14`, 0 эмодзи, шрифты, 2D-голова скина, HUD ядер) | Wave 4 | Ожидание |
| 16 | `subagent-16-ui-screens` | `crates/mq-ui` (экраны Инстансы, Каталог Modrinth/CF, Аккаунты, Консоль, Настройки) | Wave 4 | Ожидание |
| 17 | `subagent-17-mock-server` | `testsupport` (`axum` mock-сервер: 3000+ ассетов, 100+ библиотек, 200+ модов, `.mrpack`, Ely.by) | Wave 2B | Ожидание |
| 18 | `subagent-18-cli-bench` | `crates/mq-cli` + `benches/` (все подкоманды `mq`, `mq plan`, `mq bench` 1..N потоков) | Wave 4 | Ожидание |
| 19 | `subagent-19-qa-loom-stress` | Независимый QA: эквивалентность E8, `loom`, 60с стресс-тест, RSS ≤ 256 МиБ, закрытие E1–E9 | Wave 5 | Ожидание |
| 20 | `subagent-20-packaging-release` | `packaging/PKGBUILD`, `mq.desktop`, `install-local.sh`, `package-friend.sh`, `README.md` | Wave 5 | Ожидание |

---

## Хронология выполнения

### Шаг 0: Фундамент, рой субагентов и `docs/ARCHITECTURE.md`
* Старт: `2026-11-06 18:34 EET`
* Развёрнуто 20 специализированных субагентов в рантайме Antigravity и профилях `.agents/agents/`.
* Инициализирован Git-репозиторий и скелет Cargo Workspace из 13 пакетов до написания бизнес-логики (урок E5).
