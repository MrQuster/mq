---
name: mq-visual-qa
description: Visual regression testing, headless snapshot capture, resolution matrix
---

# Visual QA & Snapshot Guidelines

## Capture Methods
- Headless / Egui direct snapshotting: `mq ui-snapshot --view <name> --width <w> --height <h> --scale <s> --theme <theme> --output <path.png>`.
- Wayland / Hyprland direct capture: `grim -g "<geom>" <path.png>`.

## Standard Resolution Matrix
- **Compact Window**: 960 × 600
- **Standard Laptop**: 1180 × 740
- **Full HD**: 1920 × 1080
- **Scales**: 1.0, 1.5, 2.0
- **Themes**: All 6 opaque matte presets.

## Verification Checklist
- [ ] No clipped card borders or cut-off text.
- [ ] No horizontal/vertical text overlaps.
- [ ] Search input aligned with adjacent actions (32px height).
- [ ] No mascots or floating overlays over interactive content.
- [ ] Underline tabs render cleanly with no wrap artifacts.
