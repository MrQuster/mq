---
name: mq-release
description: Packaging, release quality gates, GitHub release automation
---

# Release Quality Gates & Packaging Specification

## Pre-Release Gates
1. Zero forbidden keywords:
   `rg -n "todo!|unimplemented!|TODO|FIXME|Coming soon|Not implemented|lorem" crates/ src/` (must be completely empty).
2. Clippy gate:
   `cargo clippy --workspace --all-targets -- -D warnings` (0 warnings).
3. Test gate:
   `cargo test --workspace -- --test-threads=1` (100% pass).
4. Code format:
   `cargo fmt --check`.

## Artifact Creation
- Binary: `target/release/mq` -> `dist/mq-linux-x86_64`
- Portable archive: `dist/mq-portable-linux-x86_64.tar.gz` containing `mq`, `mq.desktop`, `mq.svg`, `install.sh`
- Checksums: `SHA256SUMS` with SHA256 of both files.
- GitHub Release tag: `v2.1.0` via `gh release create`.
