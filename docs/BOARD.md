# MQ Swarm Engineering Board (v2.1.0)

Append-only tracking board for blockers, cross-agent coordination, and verification gates.

---

## `ag-01-instances-screen`
- **Status:** Initialized.
- **Goals:** Implement B1, B4, B6, B8, B10, B13, B14 on `instances_view.rs`.

## `ag-02-instance-tabs-console`
- **Status:** Initialized.
- **Goals:** Implement B3 (full-height console with ScrollArea), B5 (underline tabs), B7 (filter chips with counts).

## `ag-03-shell-sidebar-statusbar`
- **Status:** Initialized.
- **Goals:** Implement B2 (mascot removal from main UI), B9 (sentence case, no RU/EN mix), B11 (unified 16px icons), B12 (status-bar labels & monospaced stats), update notification badge in sidebar.

## `ag-04-design-system`
- **Status:** Initialized.
- **Goals:** Deliver clean components (`SearchField`, `Select`, `Segmented`, `FilterChip`, `UnderlineTabs`, `EmptyState`) in `crates/mq-ui/src/components/`.

## `ag-05-catalog-downloads-settings-ui`
- **Status:** Initialized.
- **Goals:** Catalog search layout, Adoptium downloads UI, Settings screens ("Обновления" and "О программе" with author `MrQuster`), responsive layout checks.

## `ag-06-mod-deps-resolver`
- **Status:** Initialized.
- **Goals:** Recursive Modrinth dependency resolver, lockfile `mods/.mq-lock.json`, client/server side filters, unused dependency pruning.

## `ag-07-mod-preflight-crash`
- **Status:** Initialized.
- **Goals:** Fast mod preflight (<1s), metadata parsing (`fabric.mod.json`, `mods.toml`), auto-fix dialog, localized crash analyzer for mods and shaders.

## `ag-08-shaders`
- **Status:** Initialized.
- **Goals:** Iris + Sodium / NeoForge Iris / Oculus + Embeddium engine resolver, auto-prompt to install shader engine, `config/iris.properties` manager, GPU diagnostics.

## `ag-09-resourcepacks-updates`
- **Status:** Initialized.
- **Goals:** `options.txt` `resourcePacks` atomic ordering, mod update check with `.mq-backup/` and 1-click rollback.

## `ag-10-versions-java-loaders`
- **Status:** Initialized.
- **Goals:** Minecraft `26.x` version schema support (chronological order from manifest, not semver), dynamic Java major version resolution and Adoptium Temurin / Zulu download, dynamic loader availability.

## `ag-11-e2e-fabric-quilt`
- **Status:** Initialized.
- **Goals:** Scenarios A, B, D, E, G test suite in `scripts/e2e-mods.sh`.

## `ag-12-e2e-forge-shaders-packs`
- **Status:** Initialized.
- **Goals:** Scenarios C, F in `scripts/e2e-forge.sh`, verified shader configurations.

## `ag-13-worlds-servers-screens-modpacks`
- **Status:** Initialized.
- **Goals:** Modpack import/export (.mrpack, CurseForge zip) with zip-slip prevention, worlds/servers tabs polish.

## `ag-14-accounts-elyby-browser`
- **Status:** Initialized.
- **Goals:** Remove legacy password forms; implement loopback OAuth2 browser flow for Ely.by (`minecraft_server_session` scope), token storage in keyring / 0600 file, mock OAuth2 integration tests.

## `ag-15-self-update-stability`
- **Status:** Initialized.
- **Goals:** Create `crates/mq-update`, GitHub Releases API checking, SHA-256 verification, atomic `<exe>.new` replacement and rollback, package manager detection, mock tests.

## `ag-16-visual-qa`
- **Status:** Initialized.
- **Goals:** Capture multi-resolution baseline snapshots (960x600, 1180x740, 1920x1080), audit B1–B15, document >=20 UI defects in `docs/UI_DEFECTS.md`.

## `ag-17-integrator-release`
- **Status:** Initialized.
- **Goals:** Coordinated branch merges, quality gates (`fmt`, `clippy`, `test`), metadata updates, release packaging for v2.1.0.
