---
name: egui-liquid-glass
description: Rules and architectural patterns for creating native Liquid Glass UI in egui/eframe without default grey styling or AI slop.
---

# Egui Liquid Glass UI Architecture

## Core Principles
1. **No Default Gray**: Never use default raw eframe widgets without theme integration.
2. **Layered Glass**:
   - Translucent backgrounds using `Color32::from_rgba_unmultiplied`.
   - Thin 1px borders with subtle top highlights (`rgba(255, 255, 255, 20..35)`).
   - Dynamic corner rounding (`6.0..14.0px`).
   - Smooth hover animations driven by `ctx.animate_bool_with_time`.
3. **Pure Vector Iconography**:
   - Zero Unicode emojis in buttons, headers, tabs, or telemetry.
   - All icons are rendered geometrically via `egui::Painter`: `line_segment`, `circle_stroke`, `rect_stroke`, `convex_polygon`.
   - Standard icons: Play (triangle), Stop/Cancel (square / X), Folder (tabbed folder outline), Settings (gear outline), Download (down arrow + tray), Check (checkmark), Search (magnifying glass), Globe (grid sphere), Trash (bin outline), Copy (overlapping rectangles).
4. **Window Transparency & Native Options**:
   - `eframe::NativeOptions` configured with:
     ```rust
     viewport: egui::ViewportBuilder::default()
         .with_transparent(true)
         .with_decorations(true)
         .with_min_inner_size([960.0, 620.0])
     ```
   - Wayland / Hyprland blur integration with configurable opacity.
