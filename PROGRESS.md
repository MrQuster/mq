# MQ — Журнал прогресса разработки (PROGRESS.md)

## Статус роя субагентов (20 профильных инженеров)

| ID | Субагент | Зона ответственности | Волна | Статус |
|---|---|---|---|---|
| 01 | `subagent-01-env-scout` | Разведка Arch Linux, Java, Wayland/X11/Vulkan, `rust-toolchain.toml`, `.gitignore`, `BLOCKERS.md` | Wave 1 | Готово |
| 02 | `subagent-02-architect` | Проектирование `docs/ARCHITECTURE.md` (DAG, владение данными, эпохи, иерархия локов, E1–E9) | Wave 1 | Готово |
| 03 | `subagent-03-core` | `crates/mq-core` (ошибки, конфиг, XDG-пути, шина `Event`, модели метрик, SafePath) | Wave 2A | Готово (20/20 тестов) |
| 04 | `subagent-04-sched-dag` | `crates/mq-sched` (мост `tokio ↔ rayon`, DAG, эпохи отмены, `BufferPool`, машина состояний) | Wave 2A | Готово (19/19 тестов) |
| 05 | `subagent-05-cpu-telemetry` | `crates/mq-sched` (`/proc/stat` по ядрам, закон Амдала, `UiWatchdog >8ms`) | Wave 2A | Готово (10/10 тестов) |
| 06 | `subagent-06-net-engine` | `crates/mq-net` (HTTP/2 пул, retry+jitter, Range, потоковый SHA-1/SHA-512, DashMap дедуп) | Wave 2B | Готово |
| 07 | `subagent-07-cas-storage` | `crates/mq-install` (CAS хардлинки, дедупликация O(1), конвейер игры, параллельный `verify`) | Wave 3 | Готово |
| 08 | `subagent-08-meta-vanilla` | `crates/mq-meta` (Vanilla `version_manifest`, `version.json`, `rules`, `asset_index`) | Wave 2B | Готово |
| 09 | `subagent-09-meta-loaders` | `crates/mq-meta` (Fabric, Quilt, `inheritsFrom`, детерминированное слияние classpath) | Wave 2B | Готово |
| 10 | `subagent-10-auth-offline-ely` | `crates/mq-auth` (Offline UUID v3 `OfflinePlayer:`, Ely.by Yggdrasil/скины, `authlib-injector`) | Wave 2B | Готово |
| 11 | `subagent-11-auth-msa` | `crates/mq-auth` (Microsoft Device Code -> Xbox -> XSTS -> MC, хранилище `0600`) | Wave 2B | Готово |
| 12 | `subagent-12-modrinth-cf` | `crates/mq-mods` (Modrinth API v2, CurseForge API v1, рекурсивный резолвер зависимостей) | Wave 2B | Готово |
| 13 | `subagent-13-modpacks-scanner` | `crates/mq-mods` (установщик `.mrpack` и `.zip`, zip-slip защита, `rayon`-сканер `.jar` модов) | Wave 3 | Готово |
| 14 | `subagent-14-instances-launch` | `crates/mq-instances` + `crates/mq-launch` (`instance.toml`, поиск Java, `-javaagent` Ely.by) | Wave 3 | Готово |
| 15 | `subagent-15-ui-theme-layout` | `crates/mq-ui` (тема Dark Glass `#0D0F14`, 0 эмодзи, шрифты, 2D-голова скина, HUD ядер) | Wave 4 | Готово |
| 16 | `subagent-16-ui-screens` | `crates/mq-ui` (экраны Инстансы, Каталог Modrinth/CF, Аккаунты, Консоль, Настройки) | Wave 4 | Готово |
| 17 | `subagent-17-mock-server` | `testsupport` (`axum` mock-сервер: 3000+ ассетов, 100+ библиотек, 200+ модов, `.mrpack`, Ely.by) | Wave 2B | Готово |
| 18 | `subagent-18-cli-bench` | `crates/mq-cli` + `benches/` (все подкоманды `mq`, `mq plan`, `mq bench` 1..N потоков) | Wave 4 | Готово |
| 19 | `subagent-19-qa-loom-stress` | Независимый QA: эквивалентность E8, `loom`, 60с стресс-тест, RSS ≤ 256 МиБ, закрытие E1–E9 | Wave 5 | Готово |
| 20 | `subagent-20-packaging-release` | `packaging/PKGBUILD`, `mq.desktop`, `install-local.sh`, `package-friend.sh`, `README.md` | Wave 5 | Готово |

---

## Хронология выполнения и закрытия этапов

### Шаг 0: Фундамент, рой субагентов и `docs/ARCHITECTURE.md`
* Старт: `2026-11-06 18:34 EET`
* Развёрнуто 20 специализированных субагентов в рантайме Antigravity.
* Инициализирован Git-репозиторий и Cargo Workspace из 13 пакетов до написания бизнес-логики (урок E5).
* Утверждена архитектурная спецификация в `docs/ARCHITECTURE.md`.

### Шаг 1: Wave 2A & 2B (Инфраструктурные и сетевые крейты)
* `mq-core`: Реализован XDG layout, SafePath (защита от zip-slip), EventBus, Redacted Secrets.
* `mq-sched`: Мост Tokio ↔ Rayon, `BufferPool`, монотонные эпохи отмены `CancelEpoch`, телеметрия ядер CPU из `/proc/stat`.
* `mq-net`: HTTP/2 движок с бэкпрешером `Semaphore(32)`, экспоненциальным jitter-retry, DashMap дедупликацией.
* `mq-meta`: Клиенты манифестов Mojang, Fabric, Quilt, детерминированное слияние версий.
* `mq-auth`: Offline UUID v3, Ely.by с автоматической доставкой Java-агента `authlib-injector`, Microsoft MSA Flow.

### Шаг 2: Wave 3 (Хранилище, конвейер установки и моды)
* `mq-install`: Глобальный CAS store со сквозной дедупликацией через жесткие ссылки (`hard link`), конвейер установки игры с параллельной верификацией.
* `mq-mods`: Безопасная распаковка `.mrpack` и CurseForge `.zip`, Rayon-сканер JAR-файлов модов, детекция конфликтов и отсутствующих зависимостей.
* `mq-instances`: CRUD изолированных инстансов.
* `mq-launch`: Детерминированное построение командной строки запуска JVM, обнаружение системной Java.

### Шаг 3: Wave 4 & 5 (UI, CLI, QA и бенчмаркинг)
* `mq-ui`: Интерфейс на `egui`/`eframe` в теме Dark Glass (`#0D0F14`), без эмодзи, с живым HUD загрузки CPU и отслеживанием кадра `UiWatchdog`.
* `mq-cli`: Консольные команды управления, `mq plan`, `mq bench`.
* Независимый QA: Доказательство детерминизма E8 (`test_multithreading_equivalence_single_vs_multi_threaded`), хаос-тест отмены `CancelEpoch`.
* Релизные пакеты: `dist/mq-portable-linux-x86_64.tar.gz`, `packaging/PKGBUILD`, `scripts/install-local.sh`, `scripts/package-friend.sh`.

---

## Итоговая таблица масштабируемости (Benchmark Output)

```text
================================================================================
                   ИТОГОВАЯ ТАБЛИЦА МАСШТАБИРУЕМОСТИ MQ
================================================================================
| Потоки (p) | Время (с) | Ускорение S(p) | Амдал s  | CPU Загрузка | Peak RSS (MiB) |
|------------|-----------|----------------|----------|--------------|----------------|
| 1          | 1.348     | 1.00           | 0.000    | 90.5%        | 49.1           |
| 2          | 1.169     | 1.15           | 0.735    | 94.4%        | 58.8           |
| 4          | 1.134     | 1.19           | 0.788    | 94.2%        | 62.3           |
================================================================================

Поядерная утилизация CPU (/proc/stat):
  threads=1 : [cpu0: 90.4%, cpu1: 89.7%, cpu2: 88.1%, cpu3: 94.0%]
  threads=2 : [cpu0: 91.3%, cpu1: 93.2%, cpu2: 96.6%, cpu3: 96.6%]
  threads=4 : [cpu0: 90.4%, cpu1: 95.6%, cpu2: 94.7%, cpu3: 95.6%]
================================================================================
```
