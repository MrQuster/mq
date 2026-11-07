# Agent 01: Platform Paths & Filesystem

- **Zone:** `crates/mq-core/src/platform/`, `crates/mq-core/src/paths.rs`, `crates/mq-install/src/cas.rs`
- **Ownership:** Module `platform`, cross-platform paths, long path aware prefix, filename sanitization, atomic replace, hardlinks with copy fallback.
- **Goals:**
  1. Create `mq-core/src/platform.rs` (or `platform/` module) using `directories` or `dirs`:
     - Linux: Data in `~/.local/share/mq`, Config in `~/.config/mq`, Cache in `~/.cache/mq`.
     - Windows: Config in `%APPDATA%\MQ`, Data & Cache in `%LOCALAPPDATA%\MQ`.
     - Allow custom override of root directory (`MQ_HOME` or `with_root`).
  2. Long path prefix: automatic `\\?\` prefix for paths > 260 characters on Windows.
  3. Filename sanitization function `sanitize_filename(name: &str) -> String` handling `<>:"/\|?*`, reserved Windows names (`CON`, `PRN`, `AUX`, `NUL`, `COM1-9`, `LPT1-9`), and trailing dots/spaces.
  4. Non-symlink hardlink fallback: in CAS, check hardlink support; on Windows fallback to file copy.
  5. Atomic file writing: write to `.tmp` file, then atomic rename (`std::fs::rename`).
  6. Cyrillic and spaces path tests.
