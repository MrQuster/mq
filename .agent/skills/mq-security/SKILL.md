---
name: mq-security
description: Security hardening, supply chain integrity, minisign verification, and threat model
---

# MQ Security Hardening & Release Verification

## 1. Supply Chain & Code Signing
- Release signing: Ed25519 signature in minisign format (`SHA256SUMS.minisig`).
- Private key generated once and stored in GitHub secret `MQ_SIGNING_KEY` and local backup `~/.config/mq-release/` (chmod 0600).
- Public key embedded into launcher binary and published in `docs/SECURITY.md`.
- Self-update validation: verify signature of `SHA256SUMS.minisig` with embedded public key before trusting binary hashes.
- Downgrade attack prevention: reject versions older than running version (except explicit manual rollback).

## 2. Process Execution & Safe Spawning
- Strict execution using `std::process::Command::new(binary).args(args)` directly. Zero shell wrapping (`sh -c`, `cmd.exe /c`).
- Pre/post-launch commands: execute ONLY if explicitly configured by the local user on that instance.
- Modpack import guard: ignore any pre/post launch commands embedded inside imported MultiMC/Prism/CurseForge/Modrinth packs and alert user.

## 3. Secret Protection & Storage
- Credential storage: Linux Secret Service (`keyring`), Windows Credential Manager / DPAPI fallback, 0600 file fallback.
- Secret redaction: tokens, client secrets, `code_verifier`, and `state` must NEVER leak to stdout, logs, crash dumps, or `--dry-run`.
- Redaction filter scrubs sensitive values with `***`.

## 4. Archive & Network Security
- Zip-slip & path traversal protection on all archive extraction routines (`canonicalize()` and root boundary checks).
- Zip-bomb protection: track decompressed size and enforce strict ratio/size limits.
- HTTPS only for all network calls; strict origin host checking for downloads.
- Zero telemetry: no analytics, tracking, or unsolicited telemetry pings.
