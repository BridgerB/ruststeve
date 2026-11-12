//! What the bot shows the live stream (schema/snapshot-v1.md): bodies read from the world after
//! a step, never anything the world reads back.

use std::collections::BTreeMap;

use data::generated::mobs::{MOBS, MobCategory};
use protocol::S2c;
use results::{Doing, Ending};
use stream::{Body, Seen, Task};
use world::{Stage, World};

/// Ticks between `self` frames (4 ticks of 50 ms: 5 a second) and `entities` frames (10 a
/// second), the schema's caps.
pub const SELF_EVERY_TICKS: u64 = 4;
pub const ENTITIES_EVERY_TICKS: u64 = 2;

fn stage_name(s: Option<Stage>) -> String {
    s.map_or(String::new(), |s| s.name().to_owned())
}

pub fn hello(run_seed: u64, lane: u32, start: &str, server: &str) -> Body {
    Body::Hello {
        commit: record::COMMIT.to_owned(),
        mc_version: data::VERSION.id.to_owned(),
        seed: run_seed,
        lane,
        start: start.to_owned(),
        stage: String::new(),
        source: "real".to_owned(),
        server: server.to_owned(),
    }
}

/// The bot itself: where, how it is, what it holds, what it is doing.
pub fn self_state(w: &World, doing: &Doing) -> Option<Body> {
    let body = w.body?;
    let mut inv: BTreeMap<String, i64> = BTreeMap::new();
    for (item, n) in w.inventory.slots.values() {
        let name = item
            .name()
            .map_or_else(|| format!("unknown:{}", item.id()), str::to_owned);
        *inv.entry(name).or_default() += i64::from(*n);
    }
    Some(Body::SelfState {
        pos: body.pos,
        yaw: w.player.yaw,
        pitch: w.player.pitch,
        dim: w
            .dimension
            .as_ref()
            .map_or(String::new(), |d| d.name.clone()),
        on_ground: body.on_ground,
        health: w.player.health.unwrap_or(-1.0),
        food: w.player.food.unwrap_or(-1),
        // Air is entity metadata on the player, not folded yet.
        air: -1,
        // The selected hotbar slot is not folded yet.
        held: String::new(),
        inv,
        stage: stage_name(w.stage_reached),
        tasks: vec![Task {
            name: doing.task.clone(),
            why: doing.why.clone(),
        }],
        reason: doing.why.clone(),
    })
}

/// Every entity the world holds; hostile is the game's monster category.
pub fn entities(w: &World) -> Body {
    let list = w
        .entities
        .iter()
        .map(|(id, e)| Seen {
            id: *id,
            kind: e
                .kind
                .name()
                .map_or_else(|| format!("{:?}", e.kind), str::to_owned),
            pos: e.pos,
            hostile: MOBS
                .iter()
                .any(|m| m.entity == e.kind && m.category == MobCategory::Monster),
        })
        .collect();
    Body::Entities { list }
}

/// What a packet just folded shows: chunks as received, unloads, block changes, removals.
pub fn from_packet(p: &S2c, raw_body: Option<&[u8]>) -> Vec<Body> {
    match p {
        S2c::LevelChunkWithLight { x, z, .. } => raw_body
            .map(|data| Body::Chunk {
                x: *x,
                z: *z,
                data: data.to_vec(),
            })
            .into_iter()
            .collect(),
        S2c::ForgetLevelChunk { x, z } => vec![Body::Unload { x: *x, z: *z }],
        S2c::BlockUpdate { pos, state } => vec![Body::Block {
            x: pos.0,
            y: pos.1,
            z: pos.2,
            state: *state,
        }],
        S2c::SectionBlocksUpdate { changes } => changes
            .iter()
            .map(|(pos, state)| Body::Block {
                x: pos.0,
                y: pos.1,
                z: pos.2,
                state: *state,
            })
            .collect(),
        S2c::RemoveEntities { ids } => ids.iter().map(|id| Body::Gone { id: *id }).collect(),
        _ => Vec::new(),
    }
}

pub fn end(ending: &Ending, w: &World, case: &str) -> Body {
    let killer = match ending {
        Ending::Death { .. } => w.death.as_ref().map_or(String::new(), |d| d.text.clone()),
        _ => String::new(),
    };
    Body::End {
        ending: ending.name().to_owned(),
        killer,
        stage_reached: stage_name(w.stage_reached),
        case: case.to_owned(),
    }
}
