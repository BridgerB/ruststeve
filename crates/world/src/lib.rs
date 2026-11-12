//! The world as one value, changed only by folding what the server sent
//! (brain/ruststeve/design/world-value.md). `World::step` is the only way it changes: an event
//! in, effects out. No I/O, no clock: time and randomness arrive on `Event::Tick`.
//!
//! Every field says what it means and when it changes, as clojurecraft's `model.clj` did for its
//! world map.

#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::BTreeMap;
use std::sync::Arc;

use data::BlockKind;
use data::generated::dimensions::DIMENSION_TYPES;
use protocol::chunk::Column;
use protocol::{C2s, Out, S2c, Session};

/// A block position, in whole blocks.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BlockPos {
    pub x: i32,
    pub y: i32,
    pub z: i32,
}

impl BlockPos {
    pub fn new(x: i32, y: i32, z: i32) -> Self {
        Self { x, y, z }
    }

    /// The column this block is in.
    pub fn chunk(self) -> ChunkPos {
        ChunkPos {
            x: self.x >> 4,
            z: self.z >> 4,
        }
    }

    /// The block containing a point (floor of each coordinate).
    pub fn containing(x: f64, y: f64, z: f64) -> Self {
        Self::new(x.floor() as i32, y.floor() as i32, z.floor() as i32)
    }
}

/// A chunk column's position, in columns (blocks / 16).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ChunkPos {
    pub x: i32,
    pub z: i32,
}

/// What the world folds: a packet from the server, or a tick of the loop's clock.
#[derive(Clone, Debug, PartialEq)]
pub enum Event {
    Packet(S2c),
    /// The loop's clock and a random seed: the only time and randomness the core sees
    /// (brain/ruststeve/decisions/sync-core-and-time.md).
    Tick {
        now_ms: u64,
        seed: u64,
    },
}

/// What the world asks the shell to do.
#[derive(Clone, Debug, PartialEq)]
pub enum Effect {
    Send(C2s),
    /// Compress every frame written from now on.
    SetCompression(i32),
}

impl From<Out> for Effect {
    fn from(o: Out) -> Self {
        match o {
            Out::Send(p) => Effect::Send(p),
            Out::SetCompression(t) => Effect::SetCompression(t),
        }
    }
}

/// The dimension the player is in.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Dimension {
    /// The dimension type's index in the synced registry (sorted by name: 3 is the Nether).
    pub type_id: i32,
    /// The level's name, `minecraft:the_nether`.
    pub name: String,
    /// The lowest block y and the height, from the generated dimension types; `None` for a
    /// type id the table lacks.
    pub min_y: Option<i32>,
    pub height: Option<u32>,
}

impl Dimension {
    fn new(type_id: i32, name: &str) -> Self {
        let row = usize::try_from(type_id)
            .ok()
            .and_then(|i| DIMENSION_TYPES.get(i));
        Self {
            type_id,
            name: name.to_owned(),
            min_y: row.map(|d| d.min_y),
            height: row.map(|d| d.height),
        }
    }
}

/// The player's own body, as the server last said.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Player {
    /// The entity id the play login gave the player.
    pub entity_id: Option<i32>,
    /// Feet position, from the server's teleports (`player_position`, relative flags applied).
    pub pos: Option<[f64; 3]>,
    pub yaw: f32,
    pub pitch: f32,
    /// Health 0-20, food 0-20 and saturation, from `set_health`; `None` before the first.
    pub health: Option<f32>,
    pub food: Option<i32>,
    pub saturation: Option<f32>,
}

/// The world value.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct World {
    /// The protocol upkeep: state, keep-alives, server ticks. Its replies become effects.
    pub session: Session,
    /// The current dimension: set by the play login, changed by a respawn.
    pub dimension: Option<Dimension>,
    /// Chunk columns of the current dimension, as received: added by `level_chunk_with_light`,
    /// dropped by `forget_level_chunk` or a change of dimension. Shared with readers by `Arc`.
    pub columns: BTreeMap<ChunkPos, Arc<Column>>,
    /// Block changes over the columns, by column: set by `block_update` and
    /// `section_blocks_update`, dropped when the column is resent or forgotten.
    pub overlay: BTreeMap<ChunkPos, BTreeMap<BlockPos, u32>>,
    pub player: Player,
    /// The loop clock and seed of the last tick.
    pub now_ms: u64,
    pub seed: u64,
}

impl World {
    pub fn new() -> Self {
        Self::default()
    }

    /// Fold one event; return what the shell must do, in order.
    pub fn step(&mut self, e: &Event) -> Vec<Effect> {
        match e {
            Event::Tick { now_ms, seed } => {
                self.now_ms = *now_ms;
                self.seed = *seed;
                Vec::new()
            }
            Event::Packet(p) => {
                self.fold(p);
                self.session
                    .on_packet(p)
                    .into_iter()
                    .map(Effect::from)
                    .collect()
            }
        }
    }

    fn enter(&mut self, type_id: i32, name: &str) {
        let same = self.dimension.as_ref().is_some_and(|d| d.name == name);
        if !same {
            // Chunks belong to a dimension: none of the old ones are here.
            self.columns.clear();
            self.overlay.clear();
        }
        self.dimension = Some(Dimension::new(type_id, name));
    }

    fn set_block(&mut self, (x, y, z): (i32, i32, i32), state: u32) {
        let pos = BlockPos::new(x, y, z);
        self.overlay
            .entry(pos.chunk())
            .or_default()
            .insert(pos, state);
    }

    fn fold(&mut self, p: &S2c) {
        match p {
            S2c::Login {
                entity_id,
                dimension_type,
                dimension,
                ..
            } => {
                self.player.entity_id = Some(*entity_id);
                self.enter(*dimension_type, dimension);
            }
            S2c::Respawn {
                dimension_type,
                dimension,
            } => self.enter(*dimension_type, dimension),
            S2c::LevelChunkWithLight { x, z, column } => {
                let at = ChunkPos { x: *x, z: *z };
                self.overlay.remove(&at);
                self.columns.insert(at, Arc::clone(column));
            }
            S2c::ForgetLevelChunk { x, z } => {
                let at = ChunkPos { x: *x, z: *z };
                self.overlay.remove(&at);
                self.columns.remove(&at);
            }
            S2c::BlockUpdate { pos, state } => self.set_block(*pos, *state),
            S2c::SectionBlocksUpdate { changes } => {
                for (pos, state) in changes {
                    self.set_block(*pos, *state);
                }
            }
            S2c::PlayerPosition {
                x,
                y,
                z,
                yaw,
                pitch,
                flags,
                ..
            } => {
                // Relative flags (1.21.2+): bit 0 x, 1 y, 2 z, 3 yaw, 4 pitch.
                let rel = |bit: u32| flags & (1 << bit) != 0;
                let [px, py, pz] = self.player.pos.unwrap_or([0.0; 3]);
                self.player.pos = Some([
                    if rel(0) { px + x } else { *x },
                    if rel(1) { py + y } else { *y },
                    if rel(2) { pz + z } else { *z },
                ]);
                self.player.yaw = if rel(3) { self.player.yaw + yaw } else { *yaw };
                self.player.pitch = if rel(4) {
                    self.player.pitch + pitch
                } else {
                    *pitch
                };
            }
            S2c::SetHealth {
                health,
                food,
                saturation,
            } => {
                self.player.health = Some(*health);
                self.player.food = Some(*food);
                self.player.saturation = Some(*saturation);
            }
            S2c::LoginCompression { .. }
            | S2c::LoginFinished { .. }
            | S2c::LoginDisconnect { .. }
            | S2c::SelectKnownPacks
            | S2c::ConfigKeepAlive { .. }
            | S2c::ConfigPing { .. }
            | S2c::FinishConfiguration
            | S2c::ConfigDisconnect { .. }
            | S2c::KeepAlive { .. }
            | S2c::Ping { .. }
            | S2c::SetTime
            | S2c::ChunkBatchFinished { .. }
            | S2c::StartConfiguration
            | S2c::Disconnect { .. }
            | S2c::Unknown { .. } => {}
        }
    }

    /// The block state id at `pos`: a change the server sent, else the column as received;
    /// `None` where no column is loaded.
    pub fn state_at(&self, pos: BlockPos) -> Option<u32> {
        let at = pos.chunk();
        if let Some(s) = self.overlay.get(&at).and_then(|o| o.get(&pos)) {
            return Some(*s);
        }
        self.columns.get(&at)?.state_at(pos.x, pos.y, pos.z)
    }

    /// The block at `pos`; `None` where no column is loaded.
    pub fn block_at(&self, pos: BlockPos) -> Option<BlockKind> {
        data::block_of_state(self.state_at(pos)?)
    }

    /// The block the player stands on (just below the feet).
    pub fn block_under_player(&self) -> Option<BlockKind> {
        let [x, y, z] = self.player.pos?;
        self.block_at(BlockPos::containing(x, y - 0.01, z))
    }
}
