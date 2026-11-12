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

use data::generated::dimensions::DIMENSION_TYPES;
use data::{BlockKind, EntityKind, ItemKind};
use memory::{Memory, Seen};
use protocol::chunk::Column;
use protocol::metadata::MetaValue;
use protocol::slot::Slot;
use protocol::{C2s, Out, S2c, Session};

/// An entity as the server last described it.
#[derive(Clone, Debug, PartialEq)]
pub struct Entity {
    pub kind: EntityKind,
    /// Feet position, in blocks.
    pub pos: [f64; 3],
    /// Metadata by index, as skimmed (brain/wire/wire-entity-metadata.md).
    pub metadata: BTreeMap<u8, MetaValue>,
    /// Server ticks when it was added and last moved or described.
    pub first_seen: u64,
    pub last_seen: u64,
}

impl Entity {
    /// A living entity's health: metadata index 9, a float.
    pub fn health(&self) -> Option<f32> {
        match self.metadata.get(&9) {
            Some(MetaValue::Float(h)) => Some(*h),
            Some(_) | None => None,
        }
    }

    /// The shared flags bitfield, metadata index 0 (on fire, sneaking, ...).
    pub fn flags(&self) -> u8 {
        match self.metadata.get(&0) {
            Some(MetaValue::Byte(b)) => *b,
            Some(_) | None => 0,
        }
    }
}

/// Window 0: the player's inventory by menu slot (0 craft result, 1-4 grid, 5-8 armour, 9-35
/// main, 36-44 hotbar, 45 offhand).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Inventory {
    /// Every slot that holds something: the item and a count of at least 1.
    pub slots: BTreeMap<i16, (ItemKind, i32)>,
    /// The window's state id, echoed in every click.
    pub state_id: i32,
    /// The stack on the cursor.
    pub carried: Option<(ItemKind, i32)>,
    /// Slots whose components could not all be read (the rest of that frame was lost).
    pub truncated: u64,
}

impl Inventory {
    fn stack(s: Slot) -> Option<(ItemKind, i32)> {
        let (item, count) = s.stack()?;
        (count > 0).then(|| (ItemKind::from_id(item), count))
    }

    fn put(&mut self, slot: i16, s: Slot) {
        if s.truncated() {
            self.truncated += 1;
        }
        match Self::stack(s) {
            Some(stack) => {
                self.slots.insert(slot, stack);
            }
            None => {
                self.slots.remove(&slot);
            }
        }
    }

    /// The menu slot of a player-inventory index: 0-8 hotbar → 36-44, 9-35 main stay, 36-39
    /// armour (feet to head) → 8-5, 40 offhand → 45.
    pub fn menu_slot(index: i32) -> Option<i16> {
        let i = i16::try_from(index).ok()?;
        match i {
            0..=8 => Some(36 + i),
            9..=35 => Some(i),
            36..=39 => Some(8 - (i - 36)),
            40 => Some(45),
            _ => None,
        }
    }

    /// How many of an item the inventory holds.
    pub fn count(&self, item: ItemKind) -> i64 {
        self.slots
            .values()
            .filter(|(k, _)| *k == item)
            .map(|(_, n)| i64::from(*n))
            .sum()
    }
}

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
    /// Entities of the current dimension by id: added by `add_entity` (never the player's own
    /// id), moved by moves, syncs and teleports, given metadata by `set_entity_data`, dropped by
    /// `remove_entities` or a change of dimension. One entry per id by construction.
    pub entities: BTreeMap<i32, Entity>,
    /// Moves and data for ids the world does not hold: counted, never invented.
    pub unknown_entity_updates: u64,
    /// Window 0, the player's own inventory.
    pub inventory: Inventory,
    /// Sightings of watched blocks seen with an exposed face, all dimensions, never retracted.
    pub memory: Memory,
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
            // Chunks and entities belong to a dimension: none of the old ones are here. Memory
            // keeps everything, by dimension.
            self.columns.clear();
            self.overlay.clear();
            self.entities.clear();
        }
        self.dimension = Some(Dimension::new(type_id, name));
    }

    /// Set an entity's absolute position (relative flags as `player_position`'s: bit 0 x, 1 y,
    /// 2 z); an unknown id is counted, not invented.
    fn place_entity(&mut self, id: i32, [x, y, z]: [f64; 3], flags: u32) {
        let tick = self.session.server_ticks;
        let Some(e) = self.entities.get_mut(&id) else {
            self.unknown_entity_updates += 1;
            return;
        };
        let rel = |bit: u32| flags & (1 << bit) != 0;
        let pos = [
            if rel(0) { e.pos[0] + x } else { x },
            if rel(1) { e.pos[1] + y } else { y },
            if rel(2) { e.pos[2] + z } else { z },
        ];
        if pos.iter().all(|v| v.is_finite()) {
            e.pos = pos;
        }
        e.last_seen = tick;
    }

    /// Record the block at `pos` in memory if it is watched (or already on record) and has an
    /// exposed face now: the only way a sighting is made (no X-ray, memory's `Seen`).
    fn observe(&mut self, pos: BlockPos) {
        let Some(dim) = self.dimension.as_ref().map(|d| d.name.clone()) else {
            return;
        };
        let Some(state) = self.state_at(pos) else {
            return;
        };
        let cell = (pos.x, pos.y, pos.z);
        if !memory::watched(state) && !self.memory.on_record(&dim, cell) {
            return;
        }
        let neighbours =
            memory::neighbours(cell).map(|(x, y, z)| self.state_at(BlockPos::new(x, y, z)));
        if let Some(seen) = Seen::check(&dim, cell, state, neighbours) {
            self.memory.record(seen, self.session.server_ticks);
        }
    }

    /// A changed cell and its six neighbours: a change can expose a neighbour.
    fn observe_around(&mut self, pos: BlockPos) {
        self.observe(pos);
        for (x, y, z) in memory::neighbours((pos.x, pos.y, pos.z)) {
            self.observe(BlockPos::new(x, y, z));
        }
    }

    /// A column just arrived: its watched cells (sections tested by palette before any scan),
    /// the positions already on record in it, and the edge cells of loaded neighbouring columns,
    /// whose faces towards this column could not be judged before it loaded.
    fn observe_column(&mut self, at: ChunkPos) {
        let Some(column) = self.columns.get(&at).map(Arc::clone) else {
            return;
        };
        let (bx, bz) = (at.x * 16, at.z * 16);
        let mut cells = Vec::new();
        for (si, section) in column.sections.iter().enumerate() {
            if !section.blocks.may_contain(memory::watched) {
                continue;
            }
            let by = column.min_y + 16 * si as i32;
            for i in 0..4096usize {
                if section.blocks.get(i).is_some_and(memory::watched) {
                    let (lx, lz, ly) = ((i & 15) as i32, ((i >> 4) & 15) as i32, (i >> 8) as i32);
                    cells.push(BlockPos::new(bx + lx, by + ly, bz + lz));
                }
            }
        }
        if let Some(dim) = self.dimension.as_ref().map(|d| d.name.clone()) {
            cells.extend(
                self.memory
                    .in_column(&dim, at.x, at.z)
                    .iter()
                    .map(|s| BlockPos::new(s.pos.0, s.pos.1, s.pos.2)),
            );
            // Neighbouring columns' edge cells that face this one.
            for (dx, dz) in [(-1, 0), (1, 0), (0, -1), (0, 1)] {
                let n = ChunkPos {
                    x: at.x + dx,
                    z: at.z + dz,
                };
                let Some(nc) = self.columns.get(&n).map(Arc::clone) else {
                    continue;
                };
                // The neighbour's face towards us: its local x (or z) 15 when it lies below us
                // on that axis, 0 when above.
                let edge = |d: i32| if d < 0 { 15 } else { 0 };
                for (si, section) in nc.sections.iter().enumerate() {
                    if !section.blocks.may_contain(memory::watched) {
                        continue;
                    }
                    let by = nc.min_y + 16 * si as i32;
                    for a in 0..16 {
                        for ly in 0..16 {
                            let (lx, lz) = if dx != 0 {
                                (edge(dx), a)
                            } else {
                                (a, edge(dz))
                            };
                            let i = protocol::chunk::block_index(lx, ly, lz);
                            if section.blocks.get(i).is_some_and(memory::watched) {
                                cells.push(BlockPos::new(n.x * 16 + lx, by + ly, n.z * 16 + lz));
                            }
                        }
                    }
                }
            }
        }
        for c in cells {
            self.observe(c);
        }
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
                self.observe_column(at);
            }
            S2c::ForgetLevelChunk { x, z } => {
                let at = ChunkPos { x: *x, z: *z };
                self.overlay.remove(&at);
                self.columns.remove(&at);
            }
            S2c::BlockUpdate { pos, state } => {
                self.set_block(*pos, *state);
                self.observe_around(BlockPos::new(pos.0, pos.1, pos.2));
            }
            S2c::SectionBlocksUpdate { changes } => {
                for (pos, state) in changes {
                    self.set_block(*pos, *state);
                }
                for (pos, _) in changes {
                    self.observe_around(BlockPos::new(pos.0, pos.1, pos.2));
                }
            }
            S2c::AddEntity {
                id, kind, x, y, z, ..
            } => {
                if Some(*id) != self.player.entity_id && [x, y, z].iter().all(|v| v.is_finite()) {
                    let tick = self.session.server_ticks;
                    self.entities.insert(
                        *id,
                        Entity {
                            kind: EntityKind::from_id(*kind),
                            pos: [*x, *y, *z],
                            metadata: BTreeMap::new(),
                            first_seen: tick,
                            last_seen: tick,
                        },
                    );
                }
            }
            S2c::RemoveEntities { ids } => {
                for id in ids {
                    self.entities.remove(id);
                }
            }
            S2c::MoveEntity { id, dx, dy, dz } => {
                let tick = self.session.server_ticks;
                match self.entities.get_mut(id) {
                    Some(e) => {
                        let moved = [e.pos[0] + dx, e.pos[1] + dy, e.pos[2] + dz];
                        if moved.iter().all(|v| v.is_finite()) {
                            e.pos = moved;
                        }
                        e.last_seen = tick;
                    }
                    None => self.unknown_entity_updates += 1,
                }
            }
            S2c::MoveEntityRot { id } => {
                let tick = self.session.server_ticks;
                match self.entities.get_mut(id) {
                    Some(e) => e.last_seen = tick,
                    None => self.unknown_entity_updates += 1,
                }
            }
            S2c::EntityPositionSync { id, x, y, z } => self.place_entity(*id, [*x, *y, *z], 0),
            S2c::TeleportEntity { id, x, y, z, flags } => {
                self.place_entity(*id, [*x, *y, *z], *flags)
            }
            S2c::SetEntityData { id, entries } => {
                let tick = self.session.server_ticks;
                match self.entities.get_mut(id) {
                    Some(e) => {
                        for (index, value) in entries {
                            e.metadata.insert(*index, value.clone());
                        }
                        e.last_seen = tick;
                    }
                    None => self.unknown_entity_updates += 1,
                }
            }
            S2c::ContainerSetContent {
                window,
                state_id,
                slots,
                carried,
            } => {
                if *window == 0 {
                    self.inventory.state_id = *state_id;
                    self.inventory.slots.clear();
                    for (i, s) in slots.iter().enumerate() {
                        if let Ok(i) = i16::try_from(i) {
                            self.inventory.put(i, *s);
                        }
                    }
                    self.inventory.carried = carried.and_then(Inventory::stack);
                }
            }
            S2c::ContainerSetSlot {
                window,
                state_id,
                slot,
                item,
            } => {
                if *window == 0 {
                    self.inventory.state_id = *state_id;
                    self.inventory.put(*slot, *item);
                }
            }
            S2c::SetPlayerInventory { slot, item } => {
                if let Some(menu) = Inventory::menu_slot(*slot) {
                    self.inventory.put(menu, *item);
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
                let pos = [
                    if rel(0) { px + x } else { *x },
                    if rel(1) { py + y } else { *y },
                    if rel(2) { pz + z } else { *z },
                ];
                // Non-finite is a boundary error; a built packet that slips one in changes nothing.
                if pos.iter().all(|v| v.is_finite()) {
                    self.player.pos = Some(pos);
                }
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
