//! `mq-mods`: Modrinth API v2 & CurseForge Core API v1 clients, recursive dependency resolver,
//! `.mrpack` and CurseForge `.zip` multi-threaded modpack installers, zip-slip protection,
//! local mod manager, and parallel `rayon` `.jar` metadata & conflict scanner.

#![deny(clippy::await_holding_lock)]

pub mod cfpack;
pub mod curseforge;
pub mod manager;
pub mod models;
pub mod modrinth;
pub mod mrpack;
pub mod resolver;
pub mod scanner;
pub mod zip_safe;

#[allow(unused_imports)]
pub use cfpack::*;
#[allow(unused_imports)]
pub use curseforge::*;
#[allow(unused_imports)]
pub use manager::*;
#[allow(unused_imports)]
pub use models::*;
#[allow(unused_imports)]
pub use modrinth::*;
#[allow(unused_imports)]
pub use mrpack::*;
#[allow(unused_imports)]
pub use resolver::*;
#[allow(unused_imports)]
pub use scanner::*;
#[allow(unused_imports)]
pub use zip_safe::*;
