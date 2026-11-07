# Agent 03: Windows UI & OS Integration

- **Zone:** `crates/mq-ui/`, `crates/mq-cli/`
- **Ownership:** Renderer fallback on Windows, DPI-awareness (Per-Monitor V2 manifest), Windows subsystem `windows_subsystem = "windows"` + `AttachConsole` for CLI, Windows fonts (Segoe UI + Inter fallback), explorer folder opening, native file dialogs.
- **Goals:**
  1. Renderer: WGPU/DX12 default with Glow/OpenGL fallback if DX12 adapter fails to initialize.
  2. Windows GUI subsystem: add `#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]` in `crates/mq-cli/src/main.rs`.
  3. CLI Console attachment: call Windows `AttachConsole(ATTACH_PARENT_PROCESS)` at CLI startup so commands like `mq launch --dry-run` or `mq --help` print directly to PowerShell / CMD.
  4. System integration: replace `xdg-open` with cross-platform URL opening (`open::that(...)` or platform helper) and folder opening (`explorer` on Windows, `xdg-open` on Linux).
  5. Fonts: ensure Segoe UI is loaded on Windows, with bundled JetBrains Mono & Inter fallback so Russian Cyrillic and symbols never render as `□`.
  6. File dialogs: `rfd::FileDialog` or native file picker for selecting modpacks, folders, and icons.
