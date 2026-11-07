---
name: mq-selfupdate
description: GitHub Releases self-update, atomic binary swap, rollback and smoke testing
---

# Self-Update Architecture & Protocol

## Release Asset Specification
1. `mq-linux-x86_64` (standalone executable binary)
2. `mq-portable-linux-x86_64.tar.gz` (portable tarball archive)
3. `SHA256SUMS` (checksum manifest signed or plaintext sha256)

## Security & Atomic Swap Protocol
1. Fetch `https://api.github.com/repos/MrQuster/mq/releases/latest`.
2. Compare semver against `env!("CARGO_PKG_VERSION")`.
3. Check write permissions of `current_exe()`. If read-only or in `/usr/`, show package manager advisory.
4. Download binary into `<exe>.new` with progress.
5. Verify SHA-256 against `SHA256SUMS`.
6. `chmod 755 <exe>.new`.
7. Execute `<exe>.new --version` in child process. Verify output matches expected version.
8. Atomic rename: `rename(<exe>, <exe>.old)` then `rename(<exe>.new, <exe>)`.
9. Restart process using `setsid` with flag `--after-update`.
10. Rollback button: restores `<exe>.old` if needed.
