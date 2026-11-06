---
name: github-one-command-release
description: Standard operating procedure for building portable release artifacts, automating GitHub releases via gh CLI, and creating an instant 1-command installer script.
---

# GitHub One-Command Release & Installer Protocol

## 1. Portable Artifact Packaging
- Produce stripped production release binary `target/release/mq`.
- Package into `dist/mq-portable-linux-x86_64.tar.gz` with:
  - `mq` executable
  - Desktop integration (`mq.desktop`)
  - Scalable vector icon (`assets/icons/mq.svg`) and raster icon (`assets/icons/mq-128.png`)
  - Local launcher installer script

## 2. 1-Command Curl Installer (`install.sh`)
- URL: `curl -fsSL https://raw.githubusercontent.com/<GITHUB_USER>/<REPO>/main/install.sh | bash`
- Steps:
  1. Detect distribution (Arch Linux, Fedora, Ubuntu/Debian).
  2. Verify Java 21+ (`jre21-openjdk`).
  3. Attempt direct download of precompiled release `mq-portable-linux-x86_64.tar.gz` from GitHub Releases for 3-second installation.
  4. Fallback to git clone + cargo build if GitHub release binary is unavailable.
  5. Install binary to `~/.local/bin/mq`.
  6. Register desktop entry and icons in `~/.local/share/applications/` and `~/.local/share/icons/hicolor/`.
  7. Ensure `~/.local/bin` is in shell PATH (`.bashrc`, `.zshrc`).
  8. Print friendly colored completion message.

## 3. GitHub Automation via `gh`
- Inspect `gh auth status`.
- Tag version and push to remote:
  ```bash
  gh release create <tag> <artifacts...> --title "<Title>" --notes "<Notes>"
  ```
