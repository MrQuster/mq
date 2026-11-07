# Agent 05: Linux 1-Command Installer & Packaging

- **Zone:** `install.sh`, `packaging/PKGBUILD`, `packaging/mq.desktop`, `scripts/install-local.sh`
- **Ownership:** Linux 1-command installer script, desktop integration, icons, uninstaller, packaging scripts.
- **Goals:**
  1. Universal 1-command installer `install.sh`:
     - Run via `curl -fsSL https://github.com/MrQuster/mq/releases/latest/download/install.sh | sh`.
     - Entire logic enclosed in `main()` called on the final line to prevent execution on truncated download.
     - Strict `set -eu`, user-level install into `~/.local/bin/mq`, `~/.local/share/applications/mq.desktop`, `~/.local/share/icons/hicolor/scalable/apps/mq.svg`.
     - Determine architecture (`x86_64`), download binary and `SHA256SUMS`, verify SHA-256 hash before installing.
     - Optional minisign signature verification if `minisign` command is available.
     - Library compatibility check (`ldd` check) with package install hints for Arch, Debian, Ubuntu, Fedora.
     - Flags: `--version <tag>`, `--uninstall`, `--dry-run`, `MQ_INSTALL_DIR`.
     - Preserve existing user data upon reinstall or uninstall.
  2. Maintain `packaging/PKGBUILD` and `packaging/mq.desktop`.
  3. Validate with `shellcheck` and automated tests against mock release server in isolated `HOME`.
