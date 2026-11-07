---
name: mq-install-and-release
description: 1-command installer scripts, Inno Setup, packaging, and release delivery
---

# MQ Install and Release Pipeline

## 1. Linux 1-Command Installer (`install.sh`)
- URL: `https://github.com/MrQuster/mq/releases/latest/download/install.sh`
- Invocation: `curl -fsSL https://github.com/MrQuster/mq/releases/latest/download/install.sh | sh`
- Self-contained inside `main()` called at EOF to prevent execution on truncated download.
- Flags: `set -eu`, user-level install into `~/.local/bin/mq`, `~/.local/share/applications/mq.desktop`, icon in `~/.local/share/icons`.
- SHA-256 validation against `SHA256SUMS` required; optional minisign check if installed.
- Flags: `--version vX.Y.Z`, `--uninstall`, `--dry-run`, `MQ_INSTALL_DIR`.
- `shellcheck` compliant.

## 2. Windows Installer & 1-Command Setup
- Primary installer: `MQ-Setup-<version>.exe` created via Inno Setup (`packaging/windows/mq.iss`).
  - Per-user installation to `%LOCALAPPDATA%\Programs\MQ` (no UAC required).
  - Clean uninstall via Windows Settings / Control Panel; preserves user data by default.
  - Silent flags: `/VERYSILENT /SUPPRESSMSGBOXES /NORESTART`.
- 1-command PowerShell: `irm https://github.com/MrQuster/mq/releases/latest/download/install.ps1 | iex`.
  - Downloads `MQ-Setup.exe` and `SHA256SUMS`, verifies SHA-256, runs silent install.
  - Parameters: `-Version`, `-Portable`, `-Uninstall`.
- Portable ZIP: `mq-windows-x86_64-portable.zip` (self-contained with `portable.flag`).
- Winget manifest in `packaging/winget/`, Scoop in `packaging/scoop/mq.json`.
- SmartScreen guidance in documentation (`docs/WINDOWS_SIGNING.md`).

## 3. GitHub Actions CI Matrix
- Matrix: `ubuntu-22.04` and `windows-latest`.
- Actions pinned by full commit SHA; minimal scoped permissions (`contents: write`, `id-token: write`).
- Generates `mq-linux-x86_64`, `mq-windows-x86_64.exe`, `MQ-Setup-<ver>.exe`, archives, `SHA256SUMS`, `SHA256SUMS.minisig`.
