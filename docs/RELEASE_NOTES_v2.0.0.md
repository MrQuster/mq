# MQ v2.0.0 — Solid Release

MQ is a high-performance, modular, modern Minecraft launcher written in Rust. Version 2.0.0 represents a complete evolutionary leap: zero stubs, a solid professional UI without AI-generated visual clutter, 100% resolution of the in-game language bug, automated Java runtime management, mod updates, and exhaustive automated test coverage.

---

## 🌟 Highlights

### 1. Fix for Missing In-Game Languages (Critical Bug #1)
In previous versions, launching instances resulted in an empty language selection menu in Minecraft (only English US was shown or the game fell back to defaults). In MQ v2.0.0, this is completely solved:
- **Canonical Asset Flags:** Strict passing of absolute `--assetsDir` and exact asset index identifier `--assetIndex <id>` (e.g. `5`, `17`, `1.20`, `legacy`) rather than the raw game version string.
- **Byte-for-Byte Preservation:** Original asset index JSON downloaded directly from official metadata and preserved byte-for-byte in `~/.local/share/mq/assets/indexes/<id>.json`.
- **Priority DAG Language Download:** Priority asset downloader fetches all `minecraft/lang/*` files and `pack.mcmeta` ahead of standard textures/sounds.
- **Content-Addressable Storage (CAS):** Hash-verified atomic asset links with fallbacks and pre-JVM object existence verification.
- **Legacy Minecraft Reconstruction:** Automatic reconstruction of legacy assets in `assets/virtual/<id>/` and `<instance_dir>/resources/` for versions 1.7.10 through 1.12.2 (`"virtual": true` / `"map_to_resources": true`).
- **Options.txt Russian Localization:** Smart non-destructive initialization of `lang:ru_ru` (or `lang:ru_RU` on legacy versions) in `options.txt`. Honors player preferences once changed via the `lang_initialized` metadata flag.
- **Modloader Profile Cascading:** Full `inheritsFrom` profile merging for Fabric, Quilt, Forge, and NeoForge, cascading `assetIndex`, `assets`, and download metadata from vanilla parents.

### 2. Solid Premium UI (Zero AI-Slop)
- **Elimination of Liquid Glass:** Completely removed semi-transparent windows, fake glass gradients, blur artifacts, and neon glows. The window is rendered with `with_transparent(false)` and all backgrounds use solid `alpha = 255`.
- **Product-Grade Spatial Design:** Disciplined 4/8px spacing grid, uniform 8px card radiuses, 6px button radiuses, and 4px badge radiuses.
- **Single Cohesive Accent:** Clean `#3DDC97` emerald accent for primary CTAs, active indicators, and focus states.
- **Vector Painter Icons:** Purged all emojis from the UI in favor of custom-rendered vector canvas icons (Play, Settings, World, Trash, Folder, Refresh, etc.).
- **6 Solid Presets:**
  - *Matte Dark (Default)* — Deep charcoal background with subtle slate borders.
  - *Prism Classic Dark* — Familiar utilitarian dark theme for power users.
  - *Modrinth Emerald* — High-contrast dark with emerald branding.
  - *Catppuccin Mocha* — Soft pastel dark palette.
  - *Nord* — Arctic cool blue-gray tones.
  - *Light Clean* — High-contrast solid light theme with crisp borders.
- **Headless Snapshot Engine:** Built-in offscreen Vulkan/WGPU renderer (`mq ui-snapshot`) to test and capture pixel-perfect interface states in CI/CD without an active display server.

### 3. Full Production Features (Zero Stubs)
- **Automated Adoptium JRE Downloads:** CLI and UI integration with Adoptium API to automatically fetch, verify (SHA-256), and extract Eclipse Temurin OpenJDK 8, 17, and 21 for Linux x86_64.
- **Process Lifecycle & Logs:** Real-time PID tracking, graceful `SIGTERM` and forced `SIGKILL` termination, and a 10,000-line memory-bounded ring buffer for real-time game logs.
- **Multi-Instance Management:** Create instances with custom vanilla or modloader configurations, clone existing setups, safe delete with confirmation, and export/import portable `.zip` packages.
- **1-Click Mod Updates:** Direct integration with Modrinth API (`POST /v2/version_files/update`) calculating file SHA-1 hashes and updating mods to compatible versions in one click.
- **Worlds Manager:** Inspect local world saves, create instant `.zip` backups with timestamps, and restore worlds with safety overwrite checks.
- **Server Ping & Management:** Read and write NBT `servers.dat` files with live TCP Minecraft Server List Ping (SLP) returning server MOTD, player counts, version, and roundtrip ping latency.
- **Crash Analyzer:** Heuristic log scanner detecting out-of-memory errors, missing dependencies, and corrupted mods, with 1-click anonymous crash log uploading to `https://mclogs.com`.
- **System Doctor:** `mq doctor` command testing Vulkan/WGPU compatibility, Java installations, available disk space, and network latency to Mojang and Modrinth CDNs.

---

## 📦 Verification & Quality Gates

Every feature in MQ v2.0.0 is backed by automated tests, verification scripts, and real game dry-runs:
- **Feature Matrix:** All 71 features documented and marked `verified` in [`docs/FEATURE_MATRIX.md`](docs/FEATURE_MATRIX.md).
- **Automated E2E Suite:** Passing test script [`scripts/e2e.sh`](scripts/e2e.sh) executing doctor, accounts, java, instances, worlds, launch dry-run, UI snapshot rendering, and integration tests.
- **Zero Warnings:** `cargo clippy --workspace --all-targets -- -D warnings` and `cargo fmt --all --check` passing with zero warnings or errors.
- **Zero Stubs:** Zero instances of `todo!()`, `unimplemented!()`, `// FIXME`, or dead buttons across the codebase.

---

## 🚀 Installation & Usage

### Standalone Tarball (Linux x86_64)
```bash
# Download and unpack release tarball
tar -xzf mq-portable-linux-x86_64.tar.gz
cd mq-portable-linux-x86_64

# Run system doctor
./mq doctor

# Launch GUI
./mq
```

### Arch Linux (PKGBUILD)
```bash
makepkg -si
```

### Quick CLI Commands
```bash
# Check system status
mq doctor

# Download Java 21 (Eclipse Temurin)
mq java install 21

# Create an instance
mq instances create "MySurvival" --game-version 1.20.1 --loader fabric

# List saves and backup a world
mq worlds list "MySurvival"
mq worlds backup "MySurvival" "New World"

# Launch instance with offline account
mq launch "MySurvival" --offline "PlayerName"
```
