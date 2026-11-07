# Agent 08: Self-Update & Minisign Code Signing

- **Zone:** `crates/mq-update/`
- **Ownership:** Ed25519 minisign key generation & verification, repository secret management, embedded public key, self-update on Windows (`mq.exe` -> `mq.exe.old`, Inno Setup silent setup) and Linux, downgrade protection, signature forgery tests.
- **Goals:**
  1. Key generation: generate an Ed25519 keypair for minisign.
     - Store private key in GitHub secret `MQ_SIGNING_KEY` (via `gh secret set`).
     - Save local backup key in `~/.config/mq-release/minisign.key` with `0600` permissions.
     - Embed public key directly into `crates/mq-update/src/lib.rs` and publish in `docs/SECURITY.md`.
  2. Signature verification:
     - Verify signature of `SHA256SUMS.minisig` using the embedded public key before trusting any binary hash.
     - Verify binary SHA-256 matches `SHA256SUMS`.
  3. Downgrade attack protection:
     - Disallow auto-updating to any version older than or equal to current running version (except explicit manual rollback).
  4. Windows self-update:
     - Portable mode: download -> verify signature & SHA-256 -> rename running `mq.exe` to `mq.exe.old` -> write new `mq.exe` -> restart. `mq.exe.old` cleaned up on next startup.
     - Installed mode: check for `unins000.exe` in exe directory; download `MQ-Setup.exe` -> verify signature & SHA-256 -> execute `/VERYSILENT /SUPPRESSMSGBOXES /NORESTART` -> restart.
  5. Linux self-update:
     - Preserve existing atomic `<exe>.new` swap, smoke test, rollback, and detached `setsid` restart.
  6. Security tests:
     - Tampered binary rejected.
     - Forged signature / wrong key rejected.
     - Modified `SHA256SUMS` rejected.
     - Version downgrade rejected.
     - Failed smoke test triggers rollback.
