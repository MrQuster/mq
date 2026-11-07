---
name: mq-windows-port
description: Guidelines and patterns for Windows 10/11 x64 porting in MQ Minecraft Launcher
---

# MQ Windows Porting Reference

## 1. Paths & Filesystem
- Module `platform`: use `directories` / `dirs` for cross-platform paths.
  - Windows: Config in `%APPDATA%\MQ`, Data & Cache in `%LOCALAPPDATA%\MQ`.
  - Linux: Maintain existing XDG paths (`~/.local/share/mq`, `~/.config/mq`, `~/.cache/mq`).
- Long path prefix `\\?\` for paths > 260 characters on Windows; add `longPathAware` in application manifest.
- Sanitize filenames on Windows: disallow `<>:"/\|?*`, reserved names (`CON`, `PRN`, `AUX`, `NUL`, `COM1-9`, `LPT1-9`), and trailing dots/spaces.
- Do NOT use symlinks on Windows (requires admin privileges). Use hardlinks with copy fallback.
- Atomic writes: write to `.tmp` file, then `std::fs::rename` (or `ReplaceFileW`).
- Cyrillic & space paths: Windows usernames often contain Cyrillic and spaces. Always pass `-Dfile.encoding=UTF-8` and use proper quoting / direct arg vectors without shell interpolation.

## 2. Process Launch & JVM
- Classpath separator: `;` on Windows, `:` on Unix (`platform::classpath_separator()`).
- Parse `rules` in `version.json` for `os.name = "windows"`, `os.arch`, `os.version`.
- Natives classifier: extract `natives-windows`.
- Spawn `java.exe` with `CREATE_NO_WINDOW` (`0x08000000`) and capture stdout/stderr for game console.
- Process tree termination: `taskkill /T /F /PID` or Job Object without `kill-on-close` so games survive launcher restart.
- Hide Linux-specific wrappers (`gamemoderun`, `mangohud`, `prime-run`) when running on Windows.

## 3. UI, Fonts & CLI
- Rendering: wgpu/DX12 with Glow/OpenGL fallback; handle graceful recovery.
- Windows Subsystem: `#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]` with `AttachConsole` to support CLI output (`mq launch --dry-run`).
- Open folder via `explorer` or `open::that()`.
- Windows fonts: Segoe UI + fallback Inter (no missing glyph boxes).
