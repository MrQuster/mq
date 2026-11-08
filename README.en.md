# MQ — High-Performance Multithreaded Minecraft Launcher

> **Disclaimer:** MQ is an unofficial, independent open-source launcher for Minecraft: Java Edition. It is not affiliated with, endorsed by, or associated with Mojang Studios, Microsoft Corporation, or any of their subsidiaries. All trademarks and registered trademarks belong to their respective owners.

MQ is an autonomous, high-performance modular launcher written from scratch in Rust with full parity across **Linux** (Wayland/X11) and **Windows 10/11 x64**. It delivers ultra-fast game installations and modpack resolution by saturating all available CPU cores on hash verification, archive extractions, and metadata scanning without freezing the UI or exceeding strict memory budgets.

---

## Quick Installation

### Linux (Arch / Ubuntu / Fedora / Debian / NixOS)
Install the latest release with desktop integration:
```bash
curl -fsSL https://github.com/MrQuster/mq/releases/latest/download/install.sh | sh
```

### Windows 10 / 11 (PowerShell)
Install into the user profile (`%LOCALAPPDATA%\Programs\MQ`) without administrator privileges:
```powershell
irm https://github.com/MrQuster/mq/releases/latest/download/install.ps1 | iex
```

#### Windows Package Managers:
* **winget:** `winget install MrQuster.MQ`
* **Scoop:** `scoop install https://raw.githubusercontent.com/MrQuster/mq/main/packaging/scoop/mq.json`
* **Portable ZIP:** Download `mq-windows-x86_64-portable.zip` from [Releases](https://github.com/MrQuster/mq/releases/latest).

---

## Verification & Minisign Public Key

Every official MQ release is signed using **Minisign (Ed25519)**:

```
untrusted comment: minisign public key: AB8B07260EB5C665
RWRlxrUOJgeLq7AIndJRG9ZhsdT5QiHy9usykXvxt1C49a1h/km3h9lt
```

Verify checksums:
```bash
minisign -Vm SHA256SUMS -P RWRlxrUOJgeLq7AIndJRG9ZhsdT5QiHy9usykXvxt1C49a1h/km3h9lt
sha256sum -c SHA256SUMS
```

For security, threat model, and privacy policies:
* [Security Policy (SECURITY.md)](SECURITY.md)
* [Threat Model (docs/THREAT_MODEL.md)](docs/THREAT_MODEL.md)
* [Privacy & Zero-Telemetry Policy (docs/PRIVACY.md)](docs/PRIVACY.md)

---

## Key Features

* **Linux & Windows Parity:** Native classpath separators, Windows console suppression (`CREATE_NO_WINDOW`), process tree termination (`taskkill`), and automatic Java runtime management.
* **Authentication:** Microsoft MSA, Ely.by (authlib-injector with skin support), and offline mode.
* **Mod Loaders & Modpacks:** Fabric, Quilt, Forge, and NeoForge support with cascading profile resolution. Native support for Modrinth (`.mrpack`) and CurseForge (`.zip`) packages with Zip-Slip protection.
* **Solid Immediate-Mode UI:** Matte Dark interface powered by `egui`/`eframe` with vector painter icons, sub-millisecond render frames, and zero AI-slop.
* **Content-Addressable Storage (CAS):** Deduplicated asset and library storage with atomic verification.
* **Built-in Diagnostics:** `mq doctor` command for step-by-step integrity verification.

---

## Quick Start (CLI)

```bash
# Launch GUI
mq

# Run diagnostic verification
mq doctor

# Add an offline account
mq accounts add Player1 --offline

# Add an Ely.by account with skin support
mq accounts add --ely

# Create and install instance
mq install 1.21.1 --loader fabric --instance MyFabric

# Install Modrinth modpack
mq modpack install /path/to/modpack.mrpack --instance MyModpack

# Launch dry-run
mq launch MyFabric --dry-run
```

---

## Repository Structure

* `crates/mq-core`: Base types, configuration, error handling, XDG paths, event bus.
* `crates/mq-sched`: Asynchronous DAG scheduler, Tokio-Rayon bridge, telemetry.
* `crates/mq-net`: Resilient HTTP/2 download engine with semaphore backpressure.
* `crates/mq-meta`: Mojang/Fabric/Quilt manifests, OS rules, version inheritance.
* `crates/mq-auth`: Offline UUID v3, Ely.by, and Microsoft MSA authentication.
* `crates/mq-install`: CAS storage, O(1) deduplication, SHA verification pipeline.
* `crates/mq-mods`: Modpack parsers, scanner, and dependency resolution.
* `crates/mq-instances`: Isolated instance management.
* `crates/mq-launch`: Deterministic JVM command line generator and Java runner.
* `crates/mq-update`: Cryptographic self-update subsystem.
* `crates/mq-ui`: Desktop user interface in `egui`/`eframe`.
* `crates/mq-cli`: Main CLI entry point.
* `testsupport`: Local mock server and test harness.
