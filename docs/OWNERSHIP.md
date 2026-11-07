# MQ — Разделение зон ответственности (OWNERSHIP.md)

| Агент | Кодовое имя | Зона ответственности и файлы |
|---|---|---|
| **01** | `ag-01-repro-doctor` | Команда `mq doctor --install <версия>`, пошаговая диагностика манифеста, version.json, Java, клиента, библиотек, нативов, ассетов. Структурированный лог установки, кнопка «Скопировать диагностику» без секретов. `crates/mq-cli/src/main.rs` (cmd_doctor), `crates/mq-cli/src/doctor.rs`. |
| **02** | `ag-02-version-json-core` | Разбор `version.json` всех эпох, схема `26.x`, правила `rules` (OS/arch/features), раскрытие всех `${...}` плейсхолдеров без утечек, наследование `inheritsFrom`. `crates/mq-meta/src/vanilla.rs`, `crates/mq-meta/src/models.rs`, `crates/mq-meta/src/rules.rs`, `crates/mq-launch/src/command.rs`. |
| **03** | `ag-03-downloader-integrity` | Транзакционный загрузчик, `.part`, SHA-1, повторы с экспоненциальной паузой и джиттером, `install_state.json`, `.installing`, CAS-store, лимиты и блокировки. `crates/mq-net/`, `crates/mq-install/src/cas.rs`, `crates/mq-install/src/pipeline.rs`. |
| **04** | `ag-04-java-runtime` | Официальные рантаймы Mojang (`https://launchermeta.mojang.com/v1/products/java-runtime/...`), распаковка, права chmod +x, приоритет источников (системная -> Mojang -> Adoptium/Zulu), проверка `<java> -version`. Автоматическая установка нужной Java при `install_game`. `crates/mq-launch/src/java.rs`, `crates/mq-launch/src/mojang_java.rs`. |
| **05** | `ag-05-libs-natives-assets-loaders` | Maven библиотеки, нативные библиотеки (.so/.dll), ассеты, лог-конфиги `logging.client`, модлоадеры (Fabric, Quilt, Forge, NeoForge), сохранение языкового фикса (`--assetsDir`, `--assetIndex`, `ru_ru`). `crates/mq-install/src/natives.rs`, `crates/mq-install/src/verify.rs`, `crates/mq-meta/src/loaders.rs`, `crates/mq-meta/src/assets.rs`. |
| **06** | `ag-06-install-ux` | Этапы и прогресс в UI, экран ошибки установки («Повторить», «Проверить файлы», «Скопировать подробности»), автозапуск установки при создании инстанса, дизайн по гайдлайнам проекта (матовый UI, токены темы, один акцент, i18n). `crates/mq-ui/`. |
| **07** | `ag-07-e2e-matrix-integrator` | Тестовая матрица `scripts/e2e-install.sh`, mock-сервер с внедрением сбоев в `testsupport/`, слияние веток в `integration`, контроль качества (`cargo fmt`, `clippy`, `cargo test`, `rg` заглушек), релиз и `FEATURE_MATRIX.md`. |

**Правило параллельной работы:** никто не редактирует файлы другого агента напрямую. При необходимости изменения чужого файла запрос фиксируется в `docs/BOARD.md`.
