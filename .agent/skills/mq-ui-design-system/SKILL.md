---
name: mq-ui-design-system
description: UI design rules, theme tokens, pixel-snapping, component ergonomics for MQ
---

# UI Design System & Component Guidelines

## Core Principles (Modrinth App / Linear / Steam Style)
- **One Accent**: Default emerald (`#10B981`) or cobalt (`#3B82F6`). Never mix multi-colored accents in the same view.
- **Matte Dark**: Solid opaque surfaces (`with_transparent(false)`). Absolutely no liquid glass or transparency blur.
- **Pixel Snapping**: Snap painter coordinates using `.floor() + 0.5` or `Painter::round_to_pixel` to avoid blurry lines on HiDPI.
- **Typography & Labels**: Sentence case everywhere ("Инстансы и сборки", "Папка модов", "Играть"). Zero emojis.
- **Controls**: Standard 32px height for action buttons, search bars, and dropdown selects.
- **Padding & Grid**: 4px / 8px / 12px / 16px grid. Card radius 8px, button radius 6px, chip radius 4px.
- **No Overlays**: No mascots or floating characters over interactive UI or consoles. Overlays reserved strictly for modals and toasts.
