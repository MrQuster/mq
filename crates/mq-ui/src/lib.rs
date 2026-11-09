//! `mq-ui`: Professional native Linux desktop GUI in `egui`/`eframe` with solid opaque
//! design, zero emojis, vector `egui::Painter` iconography, Cyrillic + monospace system
//! font loader, 2D Nearest-Neighbor skin avatar, and per-core CPU HUD.

#![deny(clippy::await_holding_lock)]

pub mod accounts_view;
pub mod app;
pub mod catalog_state;
pub mod catalog_view;
pub mod catalog_worker;
pub mod components;
pub mod fonts;
pub mod hud;
pub mod i18n;
pub mod install_types;
pub mod instances_view;
pub mod opener;
pub mod perf_view;
pub mod settings_view;
pub mod sidebar;
pub mod snapshot;
pub mod state;
pub mod theme;
pub mod widgets;

#[allow(unused_imports)]
pub use accounts_view::*;
#[allow(unused_imports)]
pub use app::*;
#[allow(unused_imports)]
pub use catalog_state::*;
pub use catalog_view::*;
#[allow(unused_imports)]
pub use components::*;
#[allow(unused_imports)]
pub use fonts::*;
#[allow(unused_imports)]
pub use hud::*;
#[allow(unused_imports)]
pub use i18n::*;
#[allow(unused_imports)]
pub use install_types::*;
#[allow(unused_imports)]
pub use instances_view::*;
#[allow(unused_imports)]
pub use opener::*;
#[allow(unused_imports)]
pub use perf_view::*;
#[allow(unused_imports)]
pub use settings_view::*;
#[allow(unused_imports)]
pub use sidebar::*;
#[allow(unused_imports)]
pub use snapshot::*;
#[allow(unused_imports)]
pub use state::*;
#[allow(unused_imports)]
pub use theme::*;
#[allow(unused_imports)]
pub use widgets::*;
