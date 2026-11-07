# MQ v2.2.0 File Ownership Matrix

В целях предотвращения конфликтов каждый файл в репозитории закреплен за конкретным субагентом:

| Субагент | Закрепленные файлы и области |
|:---|:---|
| `ag-01-platform-paths-fs` | `crates/mq-core/src/platform.rs`, `crates/mq-core/src/paths.rs`, `crates/mq-install/src/cas.rs` |
| `ag-02-launch-java-windows` | `crates/mq-launch/src/command.rs`, `crates/mq-launch/src/java.rs`, `crates/mq-launch/src/runner.rs`, `crates/mq-meta/src/rules.rs` |
| `ag-03-windows-ui-integration` | `crates/mq-ui/src/fonts.rs`, `crates/mq-ui/src/widgets.rs`, `crates/mq-cli/src/main.rs` (GUI/CLI subsys), иконки окна |
| `ag-04-elyby-browser-auth` | `crates/mq-auth/src/elyby.rs`, `crates/mq-auth/src/store.rs`, `crates/mq-ui/src/accounts_view.rs` |
| `ag-05-linux-installer` | `install.sh`, `packaging/PKGBUILD`, `packaging/mq.desktop`, `scripts/install-local.sh` |
| `ag-06-windows-installer` | `packaging/windows/`, `packaging/winget/`, `packaging/scoop/`, `install.ps1`, `docs/WINDOWS_SIGNING.md` |
| `ag-07-ci-release-pipeline` | `.github/workflows/ci.yml`, `.github/workflows/release.yml`, `.github/dependabot.yml` |
| `ag-08-update-signing` | `crates/mq-update/src/lib.rs`, подпись Minisign, генерация ключей, логика обновления на Windows |
| `ag-09-security-hardening` | `docs/SECURITY.md`, `docs/THREAT_MODEL.md`, `docs/PRIVACY.md`, `deny.toml` |
| `ag-10-integrator-qa-docs` | `Cargo.toml`, `Cargo.lock`, `CHANGELOG.md`, `README.md`, `docs/FEATURE_MATRIX.md`, общая координация |

---

### Правила изменения общих файлов:
- `Cargo.toml` и `Cargo.lock` модифицируются исключительно через согласование с интегратором `ag-10`.
- Запрещено напрямую модифицировать файлы других агентов без записи в `docs/BOARD.md`.
