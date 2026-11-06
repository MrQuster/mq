# MQ Rules & Code Standards

## 1. Zero-Slop Design Guidelines
- Zero Unicode emojis (`🚀`, `🔥`, `⚙️`, `📦`, `🎮`) in production UI.
- All iconography must be rendered via `egui::Painter` vector geometry.
- All cards, panels, and sidebars must follow the Liquid Glass paradigm with configurable opacity, subtle borders, and smooth hover feedback.
- System fonts with complete Cyrillic support (JetBrains Mono, Noto Sans, Cantarell, DejaVu Sans).

## 2. Rust Code Quality & Concurrency
- `cargo fmt --check` must pass cleanly.
- `cargo clippy --workspace --all-targets -- -D warnings` must produce 0 warnings.
- `cargo test --workspace` must pass with 0 failures.
- No blind `.unwrap()` in network, parsing, or file I/O operations; always handle errors gracefully with meaningful contextual messages.
- Asynchronous tasks in `tokio`, CPU tasks in `rayon`, lock-free message passing via `flume`.

## 3. Localization & Minecraft Sync
- Launcher supports 10+ core UI languages dynamically without restart.
- Full catalog of 80+ Minecraft locales supported.
- `options.txt` auto-synchronization preserves existing settings and updates `lang:<selected>`.
