# Threat Model — MQ Launcher

## 1. Overview & System Scope

MQ Launcher is a multi-threaded, high-performance Minecraft launcher built in Rust. It manages:
- Game installation, assets, libraries, and native binaries.
- Player identities (Microsoft OAuth2, Ely.by OAuth2 with PKCE, Offline).
- Mod loaders (Fabric, NeoForge, Forge, Quilt) and modpacks (Modrinth, CurseForge, ZIP archives).
- Game execution and lifecycle monitoring.
- Self-updates and releases.

This document describes the threat landscape, trust boundaries, evaluated attack vectors, and implemented defensive mitigations.

---

## 2. Assets & Trust Boundaries

### 2.1 Critical Assets
1. **Authentication Secrets:**
   - Microsoft OAuth2 refresh tokens and Xbox Live user hashes.
   - Ely.by OAuth2 refresh tokens and access tokens.
2. **System Integrity & Execution Boundaries:**
   - Operating system permissions and local user environment.
   - Child process lifecycle (JVM, helper tools).
3. **User Data & Game Worlds:**
   - Saved worlds, screenshots, server lists, custom configs.
4. **Supply Chain & Update Integrity:**
   - Official launcher binaries, release checksums, and update delivery.

### 2.2 Trust Boundaries
- **Untrusted External:** Public Internet, third-party mod download mirrors (Modrinth, CurseForge, Adoptium API), third-party Minecraft servers, imported modpack archives (`.zip`, `.mrpack`).
- **Semi-Trusted:** Identity providers (Mojang/Microsoft, Ely.by OAuth2 endpoints).
- **Trusted Local:** MQ Launcher binary, local user data directory (`~/.local/share/mq` or `%LOCALAPPDATA%\MQ`), user-controlled OS session.

---

## 3. Threat Actors & Vectors

### 3.1 Untrusted Modpacks & Third-Party Mods
- **Risk:** Malicious modpacks attempting arbitrary code execution before or after game launch via wrapper scripts or hook directives (e.g. MultiMC `PreLaunchCommand`, `PostExitCommand`).
- **Mitigation:**
  - MQ rejects or strips arbitrary pre/post-launch command hooks defined in imported manifests with explicit warnings.
  - Command invocation always uses typed argument slices (`std::process::Command::new(binary).args(...)`), bypassing shell interpolation (`/bin/sh -c` or `cmd.exe /c`).

### 3.2 Path Traversal & Archive Extraction (Zip-Slip)
- **Risk:** Malicious `.zip` or `.mrpack` files containing filenames like `../../../../.bashrc` or `..\..\AppData\Roaming\Microsoft\Windows\Start Menu\Programs\Startup\bad.exe`.
- **Mitigation:**
  - The archive extraction engine (`mq-instances` and `mq-install`) verifies every path component.
  - Any entry starting with `/`, `\`, containing `..`, or resolving outside target instance root is aborted and rejected.
  - Strict size and decompression ratio limits protect against Zip-Bomb / denial-of-service exhaustion.

### 3.3 Network Interception (Man-in-the-Middle)
- **Risk:** Intercepting game jars, Java distributions, or update binaries in transit over insecure or spoofed networks.
- **Mitigation:**
  - All outbound network calls require HTTPS / TLS 1.2+ using `rustls` with `webpki-roots` (Mozilla trusted root store).
  - Adoptium Java distributions, Mojang libraries, and Fabric/Forge assets are verified against expected SHA-1 / SHA-256 hashes provided by upstream signed manifests.
  - Self-update manifests and binaries require Minisign Ed25519 digital signature verification (`SHA256SUMS.minisig`) against the embedded official public key.

### 3.4 Credential Exposure & Token Leakage
- **Risk:** Leaking access tokens or credentials via logs, console outputs, process environment variables, or insecure disk storage.
- **Mitigation:**
  - Token Redaction: All logging passes through redaction filters (`[REDACTED]`).
  - Storage Permissions: On Unix, credential files (`accounts.json`, `auth.json`) are stored with strict POSIX permissions `0600` (read/write only by the owning user). On Windows, files reside within `%LOCALAPPDATA%\MQ` isolated to the user profile.
  - CLI Dry-Run: `mq launch --dry-run` sanitizes authentication arguments, replacing tokens with placeholder hashes.
  - Browser-Only OAuth2: Ely.by and Microsoft sign-in never accept passwords or emails in the launcher UI. All authentication flows occur exclusively in the system default browser using OAuth2 + PKCE S256 with an ephemeral loopback listener bound strictly to `127.0.0.1`.

### 3.5 Malicious Minecraft Servers
- **Risk:** Malicious servers attempting to exploit client-side vulnerabilities, send dangerous server resource packs, or trigger exploits via chat packets (e.g. Log4Shell / CVE-2021-44228).
- **Mitigation:**
  - MQ automatically configures JVM log4j patch arguments (`-Dlog4j2.formatMsgNoLookups=true`) for legacy Minecraft versions.
  - Server addresses in `servers.dat` are treated as opaque strings without shell evaluation.

### 3.6 Supply Chain Compromise
- **Risk:** Compromised build environments, tampered release artifacts, or unauthorized binaries uploaded to GitHub Releases.
- **Mitigation:**
  - GitHub Actions workflows use pinned full commit SHAs (`uses: actions/checkout@11bd71901bbe5b1630ceea73d27597364c9af683`).
  - GitHub Sigstore build provenance attestations (`actions/attest-build-provenance`).
  - Cryptographic Ed25519 signature generated with Minisign and verified both in installer scripts and in the launcher's self-updater.
  - Downgrade Protection: Self-updater refuses to apply older versions to prevent downgrade attacks.
