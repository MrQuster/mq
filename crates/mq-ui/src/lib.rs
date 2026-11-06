//! `mq-ui`: Professional native Linux desktop GUI in `egui`/`eframe` with custom Dark Glass
//! theme (`#0D0F14` / `#151821` / `#10B981`), zero emojis, vector `egui::Painter` iconography,
//! Cyrillic + monospace system font loader, 2D Nearest-Neighbor skin avatar, and per-core CPU HUD.

#![deny(clippy::await_holding_lock)]

pub mod accounts_view;
pub mod app;
pub mod catalog_view;
pub mod fonts;
pub mod hud;
pub mod instances_view;
pub mod perf_view;
pub mod settings_view;
pub mod sidebar;
pub mod state;
pub mod theme;
pub mod widgets;

#[allow(unused_imports)]
pub use app::*;
#[allow(unused_imports)]
pub use fonts::*;
#[allow(unused_imports)]
pub use hud::*;
#[allow(unused_imports)]
pub use sidebar::*;
#[allow(unused_imports)]
pub use state::*;
#[allow(unused_imports)]
pub use theme::*;
#[allow(unused_imports)]
pub use widgets::*;
