# Agent 07: CI/CD & GitHub Actions Release Pipeline

- **Zone:** `.github/workflows/`, `.github/dependabot.yml`
- **Ownership:** `ci.yml`, `release.yml`, Dependabot config, GitHub Actions pinned SHAs, minimal permissions, installation smoke tests in CI.
- **Goals:**
  1. `ci.yml`:
     - Matrix: `ubuntu-22.04` and `windows-latest`.
     - Triggers on `push` and `pull_request`.
     - Pin all GitHub Actions by full 40-character commit SHA with version comments.
     - Minimal permissions: `contents: read`.
     - Steps:
       - `cargo fmt --check`
       - `cargo clippy --workspace --all-targets --locked -- -D warnings`
       - `cargo test --workspace --locked`
       - `cargo audit`
       - `cargo deny check`
       - End-to-end dry run: `mq launch <test_instance> --dry-run` against local mock server verifying classpath separator (`;` on Windows, `:` on Linux), natives, `--assetsDir`, `--assetIndex`, Ely.by `-javaagent` args.
  2. `release.yml`:
     - Triggers on tag push `v*`.
     - Minimal permissions: `contents: write`, `id-token: write` (for attestations).
     - Matrix building:
       - Linux: `mq-linux-x86_64` (built on ubuntu-22.04 for older glibc compatibility), `mq-portable-linux-x86_64.tar.gz`, `install.sh`.
       - Windows: `mq-windows-x86_64.exe`, Inno Setup build `MQ-Setup-<ver>.exe`, `mq-windows-x86_64-portable.zip`, `install.ps1`.
       - Hashes: `SHA256SUMS`.
       - Minisign signature: `SHA256SUMS.minisig` using secret `MQ_SIGNING_KEY`.
       - Build provenance attestations: `actions/attest-build-provenance`.
       - GitHub release publication with body extracted from `CHANGELOG.md`.
  3. Installation smoke tests inside CI:
     - Linux: install via `install.sh` in clean HOME, verify `mq --version`, verify `--uninstall`.
     - Windows: silent install `MQ-Setup.exe`, verify `mq --version`, simulate update, uninstall, check data preservation.
  4. Watch CI runs via `gh run watch` and fix any failures until green.
