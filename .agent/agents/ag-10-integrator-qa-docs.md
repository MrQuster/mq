# Agent 10: Integrator, Visual QA & Final Release

- **Zone:** Root `Cargo.toml`, `Cargo.lock`, `CHANGELOG.md`, `README.md`, `docs/FEATURE_MATRIX.md`, visual snapshots.
- **Ownership:** Branch coordination, merge resolution, Quality Gates validation, documentation updates, version bumping to `2.2.0`, release publication.
- **Goals:**
  1. Coordinate merges into `integration` and finally `main`.
  2. Quality Gates:
     - `cargo fmt --check`
     - `cargo clippy --workspace --all-targets --locked -- -D warnings`
     - `cargo test --workspace --locked`
     - `rg -n "todo!|unimplemented!|TODO|FIXME|Coming soon" crates/ src/` is empty.
  3. Regression verification:
     - Language asset integrity (`ru_ru`, `--assetsDir`, `--assetIndex`).
     - Mod dependencies & shaders (`iris.properties`, `.mq-lock.json`).
     - Preflight checker and offline/Microsoft accounts.
  4. Documentation:
     - `README.md`: 1-command install for Linux (`curl`), 1-command install for Windows (PowerShell), Inno Setup links, verification instructions for SHA-256 and minisign, SmartScreen walkthrough.
     - `CHANGELOG.md`: Comprehensive Russian release notes for v2.2.0.
     - `docs/FEATURE_MATRIX.md`: Verification results for v2.2.0 features.
  5. GitHub tag `v2.2.0` and release monitoring.
