<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="docs/assets/logo.svg">
    <source media="(prefers-color-scheme: light)" srcset="docs/assets/logo-light.svg">
    <img alt="MQ Logo" src="docs/assets/logo.svg" width="96" height="96">
  </picture>
</p>

<h1 align="center">MQ</h1>

<p align="center">Minecraft launcher for Linux and Windows built in Rust</p>

<p align="center">
  <a href="https://github.com/MrQuster/mq/releases/latest"><img src="https://img.shields.io/github/v/release/MrQuster/mq?style=flat-square&color=3DDC97" alt="Latest Release"></a>
  <a href="https://github.com/MrQuster/mq/actions"><img src="https://img.shields.io/github/actions/workflow/status/MrQuster/mq/ci.yml?branch=main&style=flat-square" alt="CI Status"></a>
  <img src="https://img.shields.io/badge/platform-Linux%20%7C%20Windows-blue?style=flat-square" alt="Supported Platforms">
</p>

<p align="center">
  <a href="README.md">Русский</a> · <b>English</b>
</p>

<p align="center">
  <a href="#installation">Installation</a> ·
  <a href="#features">Features</a> ·
  <a href="#screenshots">Screenshots</a> ·
  <a href="#quick-start">Quick Start</a> ·
  <a href="#building-from-source">Building from Source</a> ·
  <a href="#security--privacy">Security & Privacy</a> ·
  <a href="docs/FAQ.md">FAQ</a>
</p>

---

MQ is a modular launcher for Minecraft: Java Edition written in Rust. It focuses on stability, instance isolation, multi-threaded file integrity verification via Rayon, and functional parity across Linux (Wayland / X11) and Windows 10/11 x64.

The launcher contains zero telemetry, analytics, or background user tracking.

---

## Features

| Category | Capabilities |
|---|---|
| **Platforms** | Linux (Wayland and X11, x86_64), Windows 10/11 x64, portable mode via `portable.flag`. |
| **Mod Loaders** | Vanilla, Fabric, Quilt, Forge, NeoForge. In-place mod loader switching on existing instances. |
| **Catalogs & Modpacks** | Unified Modrinth API v2 and CurseForge API v1 catalog: mods, resource packs, shaders, and modpacks. Dependency resolution, one-click updates with rollback, and `.mrpack` / CurseForge `.zip` / MultiMC/Prism import with Zip-Slip protection. Each user provides their own CurseForge API key (Modrinth works without a key). |
| **Authentication** | Offline mode (deterministic UUID v3 `OfflinePlayer:<nickname>` per RFC 4122), Ely.by (browser-only OAuth2 + PKCE S256 with automated `authlib-injector` injection), Microsoft (Device Code Flow in external browser). |
| **Java Management** | Automatic discovery of system OpenJDK installations, one-click automated download of Adoptium Temurin (Java 8, 17, 21), and manual Java binary path configuration. |
| **CAS Storage** | Content-addressable storage for shared libraries and assets with hard link deduplication to minimize disk usage. |
| **Graphics & Shaders** | Resource pack and shader pack management, GPU detection, automated configuration of `iris.properties` and `oculus.properties`. |
| **User Interface** | Built with `egui` / `eframe` in a solid matte theme without window transparency (Opaque Window), clean vector Painter icons, and zero emojis. |
| **CLI Tooling** | Command-line interface for automation and scripting: `mq launch`, `mq install`, `mq search`, `mq import`, `mq doctor`, `mq accounts`, `mq mods`, `mq self-update`. |

---

## Modrinth & CurseForge Catalog

* **Four content types:** mods, resource packs, shaders, and modpacks with Minecraft version, mod loader, category, and sort filters.
* **Dependencies & updates:** automatic resolution of required and optional dependencies, source-aware lockfile (`mods/.mq-lock.json`), and one-click updates with automatic `.bak` rollback.
* **Local import:** supports `.mrpack`, CurseForge `.zip`, and MultiMC/Prism archives (via **Import**, window drag-and-drop, or `mq import <file>`). Pre/post-launch commands in imported archives are ignored, and extraction enforces Zip-Slip, symlink, and zip-bomb protections.
* **CurseForge API key policy:** no shared key is embedded in release builds. Modrinth works out of the box without a key; for CurseForge, each user enters their own personal key in **Settings → Catalog** (or sets `MQ_CURSEFORGE_API_KEY`).
* **Author-restricted downloads:** if a CurseForge author disables third-party distribution (`downloadUrl == null`), MQ never bypasses the restriction — instead, it opens the official download page and watches your Downloads folder to pick up the file automatically.

---

## Screenshots

<p align="center">
  <img src="docs/assets/screenshots/01-instances.png" alt="Instances View" width="48%">
  <img src="docs/assets/screenshots/02-detail-mods.png" alt="Instance Details & Mods" width="48%">
</p>
<p align="center">
  <img src="docs/assets/screenshots/04-catalog.png" alt="Mod and Modpack Catalog" width="48%">
  <img src="docs/assets/screenshots/05-accounts.png" alt="Account Management" width="48%">
</p>
<p align="center">
  <img src="docs/assets/screenshots/06-settings-matte.png" alt="Settings Screen (Dark Theme)" width="48%">
  <img src="docs/assets/screenshots/07-settings-light.png" alt="Settings Screen (Light Theme)" width="48%">
</p>

---

## Installation

### Linux

Install the latest release with a single command:

```bash
curl -fsSL https://github.com/MrQuster/mq/releases/latest/download/install.sh | sh
```

Portable execution without system integration:

```bash
curl -LO https://github.com/MrQuster/mq/releases/download/v2.4.0/mq-linux-x86_64.tar.gz
tar -xzf mq-linux-x86_64.tar.gz
./mq
```

### Windows 10 / 11 x64

Install via PowerShell with a single command (no administrator privileges required):

```powershell
irm https://github.com/MrQuster/mq/releases/latest/download/install.ps1 | iex
```

| Install Method | Command / File | Notes |
|---|---|---|
| **Inno Setup 6 Installer** | [`MQ-Setup-2.4.0-x64.exe`](https://github.com/MrQuster/mq/releases/download/v2.4.0/MQ-Setup-2.4.0-x64.exe) | Installs to `%LOCALAPPDATA%\Programs\MQ` without UAC, RU/UK/EN, `.mrpack` association |
| **PowerShell Script** | `irm https://github.com/MrQuster/mq/releases/latest/download/install.ps1 \| iex` | Verifies `SHA256SUMS` & Minisign, supports `-Portable`, `-Silent`, `-Uninstall` |
| **Portable Archive** | [`mq-windows-x86_64-portable.zip`](https://github.com/MrQuster/mq/releases/download/v2.4.0/mq-windows-x86_64-portable.zip) | Includes `portable.flag`, stores all data in `data/` next to `mq.exe` |
| **winget** | `winget install MrQuster.MQ` | Manifest in `packaging/winget/` |
| **Scoop** | `scoop install packaging/scoop/mq.json` | Portable build with `persist: ["data"]` |

For SHA-256 verification, Minisign signatures, and Windows SmartScreen guidance, see [docs/WINDOWS_SIGNING.md](docs/WINDOWS_SIGNING.md).

For distribution-specific guides (Arch Linux, Ubuntu/Debian, Fedora), refer to [docs/INSTALL.md](docs/INSTALL.md).

---

## Quick Start

1. **Launch the application:**
   ```bash
   mq
   ```
2. **Add an account:**
   Open the **Accounts** tab and choose your preferred sign-in method: Offline mode, Ely.by, or Microsoft.
3. **Create an instance:**
   Click **Create**, specify an instance name, Minecraft version, and mod loader (e.g., 1.21.1 + Fabric).
4. **Install mods (optional):**
   Navigate to the **Catalog** tab or drag and drop `.mrpack` / `.jar` files into the window.
5. **Click Play:**
   The launcher verifies game assets, libraries, and Java runtime, then starts the game client.

---

## Building from Source

Prerequisites: a stable Rust toolchain (version 1.80+):

```bash
# 1. Clone the repository
git clone https://github.com/MrQuster/mq.git
cd mq

# 2. Build release binary
cargo build --release

# 3. Run test suite
cargo test --workspace
```

The resulting binary will be placed at `target/release/mq` (or `target/release/mq.exe` on Windows).

See [CONTRIBUTING.md](CONTRIBUTING.md) for build dependencies and developer guidelines.

---

## Security & Privacy

| Feature | Implementation |
|---|---|
| **Telemetry** | Zero telemetry (0 analytics endpoints, 0 user tracking, 0 crash reporting to remote hosts). |
| **Credential Storage** | Linux Secret Service / Keyring API, Windows DPAPI / Credential Manager. Local credential file permissions restricted to `0600`. |
| **Microsoft Auth** | Standard OAuth2 Device Code Flow in external default browser without collecting credentials inside the app. |
| **Ely.by Auth** | OAuth2 + PKCE S256 in external browser via local loopback listener `127.0.0.1:<port>/callback`. |
| **Archive Guardrails** | Zip-Slip path sanitization for modpack and mod extraction. Disables untrusted pre-launch commands. |
| **Supply Chain Integrity** | Minisign Ed25519 digital signatures, GitHub OIDC Sigstore build provenance attestations, and SHA-256 checksums. |

For detailed policies:
* [Security Policy (SECURITY.md)](docs/SECURITY.md)
* [Privacy Policy (PRIVACY.md)](docs/PRIVACY.md)
* [Threat Model (THREAT_MODEL.md)](docs/THREAT_MODEL.md)

---

## FAQ

Detailed answers regarding data paths, custom Java runtimes, shader installation, SmartScreen prompts, and self-updates can be found in [docs/FAQ.md](docs/FAQ.md).

---

## Acknowledgements

MQ relies on and thanks the following upstream projects and communities:
* **[Modrinth](https://modrinth.com/)** — open mod platform and `.mrpack` format specification.
* **[Ely.by](https://ely.by/)** — community skin system and authentication service.
* **[authlib-injector](https://github.com/yushijinhun/authlib-injector)** — JVM agent for custom authentication servers.
* **[Mojang Studios](https://www.minecraft.net/)** — creators of Minecraft.
* **[Fabric](https://fabricmc.net/)** and **[Quilt](https://quiltmc.org/)** — modular mod loaders.

Complete third-party crate and library licenses are listed in [THIRD_PARTY_LICENSES.md](THIRD_PARTY_LICENSES.md).

---

## Author

* **MrQuster** — [MrQuster](https://github.com/MrQuster)

---

## Legal Disclaimer

MQ is an unofficial, independent open-source project. It is not affiliated with, endorsed by, or associated with Mojang Studios, Microsoft Corporation, or any of their subsidiaries. Minecraft is a registered trademark of Mojang Studios.

Playing on servers with online verification enabled (`online-mode=true`) requires an official Minecraft license. Ely.by is an independent community service. MQ does not distribute or use proprietary Minecraft textures, assets, or trademarks.
