---
name: wayland-hyprland-stability
description: Hyprland, Wayland and X11 stability rules for tiling window managers, rapid resizing, and jitter-free telemetry rendering.
---

# Wayland & Hyprland Stability Protocol

## 1. Zero-Panic Resizing Guards
- Tiling window managers (Hyprland, Sway) can resize windows to zero or microscopic dimensions during split or layout animations.
- Every drawing calculation with division or bounding box operations must guard:
  `rect.width() > 2.0 && rect.height() > 2.0`.
- Clamp all dimensions to non-zero positive floats.

## 2. Jitter-Free Monospace Telemetry
- Telemetry values (CPU per core, RAM, download speed `MB/s`, files/s, ping) frequently shift string widths if proportional fonts are used.
- All numbers and metric units must use `FontId::monospace(size)` inside fixed-width containers (`ui.allocate_ui_with_layout` or fixed-width columns).
- Prevents UI layout shaking, flickering, and micro-stuttering across frame updates.
