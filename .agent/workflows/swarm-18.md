# Antigravity 18-Agent Swarm Orchestration Workflow (MQ v2.0.0)

## Overview
Coordination blueprint for developing, verifying, and releasing MQ v2.0.0. The swarm is partitioned into 4 squadrons working with designated file ownership and rigorous quality gates.

```
                    ┌──────────────────────────────┐
                    │     Swarm Coordinator (Lead) │
                    └──────────────┬───────────────┘
         ┌─────────────────────────┼─────────────────────────┐
         ▼                         ▼                         ▼
┌──────────────────┐      ┌──────────────────┐      ┌──────────────────┐
│   Squadron 1     │      │   Squadron 2     │      │   Squadron 3     │
│ Languages & CAS  │      │ Design & Premium │      │ Gameplay & Mods  │
│ (ag-01 .. ag-03) │      │ (ag-04 .. ag-09) │      │ (ag-10 .. ag-14) │
└────────┬─────────┘      └────────┬─────────┘      └────────┬─────────┘
         └─────────────────────────┼─────────────────────────┘
                                   ▼
                          ┌──────────────────┐
                          │   Squadron 4     │
                          │ Security, QA &   │
                          │ Release Gate     │
                          │ (ag-15 .. ag-18) │
                          └──────────────────┘
```

---

## Squadron 1: Languages, Asset Pipeline & Launch Arguments
* **ag-01-assets-core**: Exact Mojang `indexes/<id>.json` storage, priority lang & mcmeta download, CAS integrity.
* **ag-02-launch-args-inherits**: Absolute `--assetsDir`, exact `--assetIndex <id>`, profile merging with `inheritsFrom`.
* **ag-03-legacy-options-tests**: Legacy virtual assets (1.7.10–1.12.2), non-destructive `options.txt` patching with `lang_initialized`, pre-launch check, mock server tests.

## Squadron 2: Design & Premium UI (Zero AI-Slop)
* **ag-04-design-lead**: Removal of Liquid Glass (`with_transparent(false)`, alpha 255), 4/8 px grid enforcement, ban on AI slop/emoji.
* **ag-05-theme-typography-i18n**: Pure opaque `Tokens` system, 6 presets (Matte Dark default), Cyrillic font stack, unified i18n.
* **ag-06-icons-components**: Pure vector Painter icons, component library, focus navigation.
* **ag-07-shell-instances-detail**: 220px sidebar, 28px bottom status bar, instances grid/list/groups, detail view with 8 tabs, live console, drag & drop.
* **ag-08-catalog-settings-onboarding**: 4-step onboarding wizard, catalog browser with instance picker, downloads queue, settings form, modals.
* **ag-09-visual-qa**: `mq ui-snapshot`, headless screenshot generation of all screens/states/themes, 2-iteration visual audit.

## Squadron 3: Gameplay Systems & Content Management
* **ag-10-java-launch-tuning**: Java auto-detection, auto-download of Adoptium JRE 8/17/21, launch lifecycle, JVM tuning presets, Linux wrappers, quick-play.
* **ag-11-loaders-instance-creation**: Vanilla, Fabric, Quilt, Forge, NeoForge, creation dialog with validation, loader switching, manifest caching.
* **ag-12-mods-packs-catalog**: Mods list, conflict scanner, 1-click update via Modrinth, resourcepacks/shaders tabs, Modrinth/CurseForge catalog clients.
* **ag-13-worlds-servers-screens**: Worlds backups/restores, `servers.dat` editor and live TCP ping, screenshots gallery with viewer.
* **ag-14-modpacks-io**: Deep instance cloning, `.mrpack` & `.zip` export and import with zip-slip security guards.

## Squadron 4: Security, Robustness, Verification & Release
* **ag-15-accounts-security**: Offline UUID v3, Ely.by authlib-injector, Microsoft Device Code Flow, keyring/0600 token storage, skin preview.
* **ag-16-crash-network-state**: Crash log analyzer with Russian diagnostics and auto-fixes, network backoff/resume, atomic config state, panic hook, single-instance lock.
* **ag-17-completeness-auditor**: `docs/FEATURE_MATRIX.md` lifecycle, zero-stub check (`rg` for `todo!`), `mq doctor` & CLI suite, `cargo clippy`, `cargo test`, e2e script execution.
* **ag-18-packaging-release**: PKGBUILD, `.desktop`, SVG icon, portable archive, `install.sh`, documentation (`README.md`, `CHANGELOG.md`, `ARCHITECTURE.md`), GitHub Release v2.0.0.
