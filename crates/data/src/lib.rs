//! The game's tables for vanilla 26.1.2 (protocol 775): packet ids, blocks and their states,
//! items, entity types, registries, tags, recipes, smelting, loot, dimension types and harvest
//! tools.
//!
//! Everything under [`generated`] is written by `cargo run -p datagen -- generate` on a runner
//! from the server's own reports and jar, and committed; CI regenerates it and fails on any diff.
//! Never edit it by hand (brain/ruststeve/design/datagen.md). [`types`] holds the hand-written
//! shapes the tables are made of.

#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

pub mod generated;
pub mod types;

pub use generated::blocks::{BlockKind, block_of_state};
pub use generated::entities::EntityKind;
pub use generated::items::ItemKind;
pub use generated::version::VERSION;
