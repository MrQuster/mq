//! Reusable solid UI component library for MQ Launcher.
//! Enforces 4/8 px grid, 8px card radius, 6px button radius, 4px badge radius,
//! unified interaction states (default, hover, pressed, focus), and zero emoji.

pub mod badge;
pub mod button;
pub mod card;
pub mod chip;
pub mod empty;
pub mod input;
pub mod list;
pub mod modal;
pub mod progress;
pub mod segmented;
pub mod select;
pub mod tabs;
pub mod toast;
pub mod toggle;

pub use badge::{render_badge, BadgeKind};
pub use button::{button_primary, button_secondary, icon_button};
pub use card::card_panel;
pub use chip::FilterChip;
pub use empty::{render_empty_state, render_error_state};
pub use input::{search_input, SearchField};
pub use list::list_row;
pub use modal::render_modal_container;
pub use progress::render_progress_bar;
pub use segmented::{segmented_switch, SegmentedOption};
pub use select::{Select, SelectOption};
pub use tabs::{render_underline_tabs, underline_tabs};
pub use toast::{Toast, ToastManager};
pub use toggle::toggle_switch;
