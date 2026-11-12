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

/// Kinds travel as their protocol id (in recordings and checkpoints): an id the tables do not
/// know comes back as `Unknown(id)`, so a newer server's ids survive a round trip.
macro_rules! serde_by_id {
    ($($ty:ty),*) => {$(
        impl serde::Serialize for $ty {
            fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
                s.serialize_u32(self.id())
            }
        }
        impl<'de> serde::Deserialize<'de> for $ty {
            fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
                Ok(Self::from_id(<u32 as serde::Deserialize>::deserialize(d)?))
            }
        }
    )*};
}

serde_by_id!(BlockKind, ItemKind, EntityKind);
