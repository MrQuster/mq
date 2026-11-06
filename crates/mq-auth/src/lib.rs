//! `mq-auth`: Multi-provider authentication (Offline UUID v3 `OfflinePlayer:`,
//! Ely.by Yggdrasil/OAuth2 + `authlib-injector.jar` auto-downloader + 2D skin face renderer,
//! and Microsoft Device Code -> Xbox Live -> XSTS -> Minecraft Services) with `0600` storage.

#![deny(clippy::await_holding_lock)]

pub mod elyby;
pub mod models;
pub mod msa;
pub mod offline;
pub mod skin;
pub mod store;

#[allow(unused_imports)]
pub use elyby::*;
pub use models::*;
#[allow(unused_imports)]
pub use msa::*;
#[allow(unused_imports)]
pub use offline::*;
#[allow(unused_imports)]
pub use skin::*;
#[allow(unused_imports)]
pub use store::*;
