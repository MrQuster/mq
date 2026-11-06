//! `mq-meta`: Minecraft `version_manifest_v2.json`, `version.json`, OS/arch/feature rules,
//! asset index parser, and Fabric / Quilt / `inheritsFrom` modloader profile resolution.

#![deny(clippy::await_holding_lock)]

pub mod assets;
pub mod loaders;
pub mod merge;
pub mod models;
pub mod rules;
pub mod vanilla;

#[allow(unused_imports)]
pub use assets::*;
#[allow(unused_imports)]
pub use loaders::*;
#[allow(unused_imports)]
pub use merge::*;
pub use models::*;
#[allow(unused_imports)]
pub use rules::*;
#[allow(unused_imports)]
pub use vanilla::*;
