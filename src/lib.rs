// State/step model carries fields (sword, furnace, vitals, priority) used by later
// speedrun phases not ported yet; allow them until then. Also blankets the SDK.
#![allow(dead_code)]

//! ruststeve — Ender Dragon speedrun bot with a built-in Minecraft SDK. The SDK
//! modules below were formerly the separate `rustcraft` crate, now flattened in so
//! this is one self-contained crate. The bot core is the `app`/`state`/`steps`/`tasks`
//! modules; everything above `app` is the SDK (bot, world, protocol, …).

// ── Minecraft SDK (formerly the `rustcraft` crate) ──
pub mod anvil;
pub mod auth;
pub mod block;
pub mod bot;
pub mod chat;
pub mod chunk;
pub mod entity;
pub mod item;
pub mod nbt;
pub mod nibble;
pub mod path;
pub mod physics;
pub mod protocol;
pub mod rcon;
pub mod recipe;
pub mod registry;
pub mod varint;
pub mod vec3;
pub mod window;
pub mod world;
pub use vec3::Vec3;

// ── speedrun bot ──
pub mod app;
pub mod bot_utils;
pub mod gym;
pub mod memory;
pub mod sniff;
pub mod state;
pub mod steps;
pub mod survival;
pub mod tasks;
pub mod telemetry;
pub mod types;
pub mod viewer;
