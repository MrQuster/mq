# Privacy Policy — MQ Launcher

## 1. Core Commitment: Zero Telemetry

MQ Launcher is built with a strict privacy-first philosophy:
- **No Analytics:** We do not collect, store, or transmit any analytics, crash reports, usage metrics, or diagnostic telemetry.
- **No Tracking:** There are no unique hardware identifiers, user profiling tokens, fingerprinting scripts, or tracking cookies.
- **No Third-Party Ad Networks:** MQ Launcher is completely ad-free, open-source software.
- **No Intermediate Relay Servers:** MQ Launcher communicates directly with upstream services (Mojang, Microsoft, Ely.by, Modrinth, Adoptium) without routing your traffic through intermediate developer proxy servers.

---

## 2. Local Data Storage

All data created or downloaded by MQ Launcher is stored exclusively on your local computer:
- **Linux:**
  - Game data & instances: `~/.local/share/mq/`
  - Configuration & credentials: `~/.config/mq/`
  - Download cache: `~/.cache/mq/`
- **Windows:**
  - Game data & instances: `%LOCALAPPDATA%\MQ\`
  - Configuration & credentials: `%LOCALAPPDATA%\MQ\config\`
  - Download cache: `%LOCALAPPDATA%\MQ\cache\`
- **Portable Mode (`portable.flag`):**
  - All data is stored in the `data/` subfolder adjacent to the launcher binary.

Credential files (`accounts.json`, `auth.json`) are stored with restricted file permissions (`0600` on Unix) accessible solely by your operating system user account.

---

## 3. Transparency: Outbound Network Connections

MQ Launcher initiates network connections strictly to fulfill requested actions (downloading games, authenticating accounts, updating launcher). Below is the comprehensive list of external domains contacted:

| Domain / Endpoint | Operator | Purpose |
| ----------------- | -------- | ------- |
| `launchermeta.mojang.com`<br>`piston-meta.mojang.com` | Mojang / Microsoft | Minecraft version manifests and release metadata. |
| `libraries.minecraft.net`<br>`resources.download.minecraft.net` | Mojang / Microsoft | Game libraries, native binaries, sound assets, and client jars. |
| `login.microsoftonline.com`<br>`user.auth.xboxlive.com`<br>`xsts.auth.xboxlive.com`<br>`api.minecraftservices.com` | Microsoft / Xbox | Official Microsoft OAuth2 authentication, Xbox Live device tokens, and Minecraft license verification. |
| `account.ely.by`<br>`authlib-injector.yggdrasil.net` | Ely.by / authlib-injector | Ely.by OAuth2 login (via your default browser), skin lookup, and authlib-injector JVM agent. |
| `meta.fabricmc.net`<br>`maven.fabricmc.net` | FabricMC | Fabric Loader and intermediary metadata and jar libraries. |
| `maven.neoforged.net`<br>`maven.minecraftforge.net` | NeoForged / Forge | NeoForge and Forge installer metadata and dependencies. |
| `meta.quiltmc.org`<br>`maven.quiltmc.org` | QuiltMC | Quilt Loader metadata and libraries. |
| `api.modrinth.com`<br>`cdn.modrinth.com` | Rinth, Inc. | Search and download of Minecraft mods, modpacks, resource packs, and shaders. |
| `api.curseforge.com` | Eternal / Overwolf | CurseForge mod search and download. |
| `api.adoptium.net`<br>`github.com/adoptium` | Eclipse Foundation | Temurin OpenJDK Java runtime discovery and archive downloads. |
| `api.github.com`<br>`github.com/MrQuster/mq` | GitHub / MQ Authors | Checking for launcher self-updates, downloading update archives, and verifying Minisign Ed25519 signatures. |

No other external network connections are established.

---

## 4. User Rights & Data Deletion

Because MQ Launcher does not operate remote servers or store personal data in the cloud, deleting your data is entirely within your control:
- Removing the application directories listed in Section 2 instantly and permanently deletes all accounts, instances, worlds, and cached files from your machine.
- During uninstallation (`install.sh --uninstall` or `install.ps1 -Uninstall`), MQ preserves your game instances and worlds by default to prevent accidental data loss, but you may remove the data folder at any time.
