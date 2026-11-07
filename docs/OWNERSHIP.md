# MQ Workspace Ownership Matrix (v2.1.0)

This document establishes strict file ownership boundaries across all 17 subagents.
Shared files must only be modified by their designated owner or via coordinated integration requests.

---

## 1. Shared Files Ownership

| File | Designated Owner | Purpose / Rules |
|---|---|---|
| `Cargo.toml` (root workspace) | `ag-17-integrator-release` | Workspace members, dependencies, metadata |
| `Cargo.lock` | `ag-17-integrator-release` | Dependency locking |
| `crates/mq-cli/src/main.rs` | `ag-17-integrator-release` | Root CLI entrypoint |
| `crates/mq-ui/src/theme.rs` | `ag-04-design-system` | Design tokens, color palette, typography |
| `crates/mq-ui/src/components/*` | `ag-04-design-system` | Shared atomic UI components |
| `docs/FEATURE_MATRIX.md` | `ag-17-integrator-release` | Verification statuses across all test matrices |
| `docs/BOARD.md` | All Agents | Append-only status, blockers, and bug reports |

---

## 2. Agent Subsystem Ownership

| Agent ID | Role | Owned Source Files / Paths |
|---|---|---|
| `ag-01-instances-screen` | Instances View & Cards | `crates/mq-ui/src/instances_view.rs` (instance grid, cards, list mode, header controls) |
| `ag-02-instance-tabs-console` | Tabs & Live Console | `crates/mq-ui/src/instances_view.rs` (tab underline bar, console view, log filters), `components/tabs.rs` |
| `ag-03-shell-sidebar-statusbar` | Shell, Sidebar & HUD | `crates/mq-ui/src/sidebar.rs`, `crates/mq-ui/src/hud.rs`, update notification badge |
| `ag-04-design-system` | Design System Lead | `crates/mq-ui/src/theme.rs`, `crates/mq-ui/src/components/*`, `crates/mq-ui/src/widgets.rs` |
| `ag-05-catalog-downloads-settings-ui` | Catalog & Settings Views | `crates/mq-ui/src/catalog_view.rs`, `crates/mq-ui/src/settings_view.rs`, instance creation dialog |
| `ag-06-mod-deps-resolver` | Mod Dependencies & Lockfile | `crates/mq-mods/src/resolver.rs`, `crates/mq-mods/src/lockfile.rs`, Modrinth deps |
| `ag-07-mod-preflight-crash` | Preflight & Crash Analysis | `crates/mq-launch/src/prelaunch.rs`, `crates/mq-launch/src/crash_analyzer.rs`, mod metadata parsers |
| `ag-08-shaders` | Shaders & GPU Diagnostics | `crates/mq-mods/src/shaders.rs`, `crates/mq-launch/src/gpu.rs`, Shaders tab UI |
| `ag-09-resourcepacks-updates` | Resourcepacks & Mod Backups | `crates/mq-launch/src/options.rs`, `crates/mq-instances/src/options.rs`, mod updater & backup |
| `ag-10-versions-java-loaders` | Java & Version Schema (26.x) | `crates/mq-meta/src/vanilla.rs`, `crates/mq-meta/src/loaders.rs`, `crates/mq-launch/src/java.rs` |
| `ag-11-e2e-fabric-quilt` | Fabric & Quilt E2E Pipeline | `scripts/e2e-mods.sh` (Fabric/Quilt scenarios A, B, D, E, G), `crates/mq-launch/src/runner.rs` |
| `ag-12-e2e-forge-shaders-packs` | Forge/NeoForge & Shaders E2E | `scripts/e2e-forge.sh` (Scenarios C, F), Iris/Oculus real config tests |
| `ag-13-worlds-servers-screens-modpacks` | Worlds, Servers, Modpacks | `crates/mq-instances/src/worlds.rs`, `crates/mq-instances/src/servers.rs`, `crates/mq-mods/src/mrpack.rs`, `cfpack.rs` |
| `ag-14-accounts-elyby-browser` | Ely.by Browser OAuth2 Flow | `crates/mq-auth/src/elyby.rs`, `crates/mq-auth/src/oauth.rs`, `crates/mq-ui/src/accounts_view.rs` |
| `ag-15-self-update-stability` | Self-Update & Process Lifecycle | `crates/mq-update/*`, atomic file swaps, single-instance lock, crash hooks |
| `ag-16-visual-qa` | Visual QA & Snapshot Auditor | `crates/mq-ui/src/snapshot.rs`, `docs/UI_DEFECTS.md`, visual regression baseline & diffing |
| `ag-17-integrator-release` | Integrator, Clippy & Release | Integration branch merges, release builds, package manifests, GitHub release publishing |

---

## 3. Localization Strings

To avoid merge conflicts, localization strings are placed in subsystem-specific files under `i18n/ru/`:
- `i18n/ru/instances.toml`
- `i18n/ru/console.toml`
- `i18n/ru/shell.toml`
- `i18n/ru/settings.toml`
- `i18n/ru/mods.toml`
- `i18n/ru/accounts.toml`
- `i18n/ru/update.toml`
