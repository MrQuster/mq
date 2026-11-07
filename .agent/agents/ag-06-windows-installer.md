# Agent 06: Windows Installer & Package Delivery

- **Zone:** `packaging/windows/`, `packaging/winget/`, `packaging/scoop/`, `docs/WINDOWS_SIGNING.md`
- **Ownership:** Inno Setup script (`packaging/windows/mq.iss`), PowerShell 1-command installer (`install.ps1`), portable ZIP creation, winget & Scoop manifests, SmartScreen documentation.
- **Goals:**
  1. Inno Setup script `packaging/windows/mq.iss`:
     - Creates `MQ-Setup-<version>.exe`.
     - Per-user installation to `%LOCALAPPDATA%\Programs\MQ` (no UAC / admin privileges needed).
     - Start Menu shortcuts, optional desktop icon.
     - Registers uninstaller in "Settings -> Installed apps".
     - Bilingual installer (Russian & English).
     - Closes running MQ instances before installation.
     - Preserves user game data in `%LOCALAPPDATA%\MQ` upon uninstall unless explicitly chosen to wipe.
  2. PowerShell 1-command installer `install.ps1`:
     - Run via `irm https://github.com/MrQuster/mq/releases/latest/download/install.ps1 | iex`.
     - `$ErrorActionPreference = 'Stop'`, TLS 1.2+, downloads `MQ-Setup.exe` and `SHA256SUMS`, verifies SHA-256, runs silent install (`/VERYSILENT /SUPPRESSMSGBOXES /NORESTART`).
     - Parameters: `-Version`, `-Portable`, `-Uninstall`.
  3. Portable ZIP structure: `mq-windows-x86_64-portable.zip` with `portable.flag`.
  4. Package manager manifests: Winget in `packaging/winget/`, Scoop in `packaging/scoop/mq.json`.
  5. Documentation `docs/WINDOWS_SIGNING.md`: explaining SmartScreen behavior without paid certificate, how to run ("More info -> Run anyway"), SHA-256 and `gh attestation verify` validation steps, SignPath Foundation & Azure Trusted Signing integration guide.
