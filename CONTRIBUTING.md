# Contributing to MQ

Thank you for your interest in contributing to MQ!

## Code of Conduct & Principles

- **Zero AI-slop / Clean code:** We maintain clean, idiomatic Rust with zero placeholder comments, strict memory and concurrency budgets, and no dead code.
- **Safety & Privacy:** MQ respects user privacy. No telemetry, tracking, or unexpected external network connections.
- **Cross-platform:** All changes must build cleanly and pass tests on both Linux and Windows.

## Development Setup

1. **Prerequisites:**
   - Rust stable (1.80+)
   - Standard build tools (`pkg-config`, `libxkbcommon`, `wayland` on Linux)

2. **Clone and Build:**
   ```bash
   git clone https://github.com/MrQuster/mq.git
   cd mq
   cargo check --workspace
   cargo test --workspace
   ```

3. **Code Style & Verification:**
   Before submitting changes, ensure all checks pass:
   ```bash
   cargo fmt --check
   cargo clippy --workspace --all-targets -- -D warnings
   cargo test --workspace
   ```

## Pull Request Guidelines

1. Create a focused branch from `main`.
2. Keep commits atomic and messages descriptive.
3. Include tests covering new functionality or bug fixes.
4. Open a Pull Request detailing the changes and verification steps.
