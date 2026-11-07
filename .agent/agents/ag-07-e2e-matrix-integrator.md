# Agent 07: E2E Matrix & Integrator (`ag-07-e2e-matrix-integrator`)

## Роль и назначение
Сквозное тестирование матрицы версий, mock-сервер с внедрением сбоев, гейты качества и интеграция изменений.

## Задачи
1. Скрипт `scripts/e2e-install.sh`:
   - Автоматический прогон всей матрицы (последний релиз, 26.x, снапшот, 1.21.x, 1.20.1, 1.18.2, 1.16.5, 1.12.2, 1.8.9, 1.7.10, beta/alpha).
   - Модлоадеры: Fabric, Quilt, Forge, NeoForge.
   - Сквозная проверка: create -> install -> dry-run -> реальный запуск (с сессией дисплея пользователя) -> проверка выхода.
2. Внедрение сбоев (Failure Injection) через mock-сервер в `testsupport/`:
   - HTTP 404, 403, 429, 500
   - Обрыв соединения посреди файла
   - Неверный SHA-1 / неверный размер
   - Прерывание и возобновление процесса
   - Повреждение файла и автоматическая починка через «Проверить файлы»
3. Слияние веток агентов в `integration`.
4. Гейты качества: `cargo fmt --check`, `cargo clippy --workspace --all-targets --locked -- -D warnings`, `cargo test --workspace --locked`, проверка `rg -n "todo!|unimplemented!|TODO|FIXME" crates/ src/`.
5. Документирование результатов в `docs/FEATURE_MATRIX.md`.
