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
use data::generated::shapes::collision;
use data::types::Collision;
use data::{BlockKind, EntityKind, ItemKind};
use memory::{Memory, Seen};
use physics::{Body, Controls, Fluid, FluidKind};
use protocol::chunk::Column;
use protocol::metadata::MetaValue;
use protocol::nbt::Death;
use protocol::slot::Slot;
use protocol::{C2s, Out, S2c, Session};

pub mod stage;
pub use stage::Stage;

/// How long hits are kept: a minute of server ticks (a case file's "first hit in the last
/// minute", clojurecraft's observability rule).
pub const HITS_KEPT_TICKS: u64 = 1200;

/// A hit on the bot, from a `damage_event` naming its own entity.
#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Hit {
    /// Server ticks when it landed.
    pub at_tick: u64,
    /// The damage type's name (`mob_attack`, `lava`, `fall`), from the generated registry in
    /// the order the server syncs it; `unknown:<id>` for an id the table lacks.
    pub damage_type: String,
    /// The entity behind it and the one that dealt it (an arrow's shooter, the arrow), by kind
    /// when the world holds them; `None` for no entity or one the world does not hold.
    pub cause: Option<EntityKind>,
    pub direct: Option<EntityKind>,
}

/// An entity as the server last described it.
#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, PartialEq)]
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

    /// What an item entity holds: metadata index 8, an item stack (ItemEntity.DATA_ITEM).
    pub fn item(&self) -> Option<(ItemKind, i32)> {
        match self.metadata.get(&8) {
            Some(MetaValue::Item(s)) => s.stack().map(|(i, n)| (ItemKind::from_id(i), n)),
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
#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, Default, PartialEq, Eq)]
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
#[derive(
    serde::Serialize, serde::Deserialize, Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash,
)]
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
#[derive(
    serde::Serialize, serde::Deserialize, Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash,
)]
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
    /// What to press from now on: a script today, the planner from issue 09.
    Controls(Controls),
    /// Something to do that the server must answer (issue 10): dig, hold, click.
    Act(Act),
}

/// A dig's step (`player_action`'s status).
#[derive(serde::Serialize, serde::Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum DigStep {
    Start,
    Abort,
    Finish,
}

impl DigStep {
    fn status(self) -> i32 {
        match self {
            DigStep::Start => 0,
            DigStep::Abort => 1,
            DigStep::Finish => 2,
        }
    }
}

/// An act: a decision the world turns into packets and remembers the sending of, so what the
/// server answers can be matched to it (brain/ruststeve/design/digging.md, crafting-window.md).
#[derive(serde::Serialize, serde::Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum Act {
    Dig {
        step: DigStep,
        pos: BlockPos,
        face: i8,
    },
    /// Select hotbar slot 0-8.
    Hold {
        slot: u8,
    },
    /// A click in window 0, predicting nothing, stamped with the last state id the server sent.
    Click {
        slot: i16,
        button: i8,
        mode: i32,
    },
    CloseWindow,
    Swing,
}

/// A click sent and not yet answered: the window's state id when it went, and when.
#[derive(serde::Serialize, serde::Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub struct PendingClick {
    pub slot: i16,
    pub state_id: i32,
    pub at_tick: u64,
}

/// An item picked up by the bot (`take_item_entity` naming it).
#[derive(serde::Serialize, serde::Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Pickup {
    pub at_tick: u64,
    pub item_entity: i32,
    pub amount: i32,
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
#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, PartialEq, Eq)]
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
#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, Default, PartialEq)]
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
#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, Default, PartialEq)]
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
    /// The body physics moves: placed by the server's teleport (velocity zeroed), stepped every
    /// tick once its column is loaded, cleared by a change of dimension.
    pub body: Option<Body>,
    /// What is pressed: set by `Event::Controls` (a script today, the planner from issue 09).
    pub controls: Controls,
    /// Ticks the world has stepped, and what was last reported: the movement packets go out
    /// when position or look changed, else status-only once every 20 ticks (vanilla's rule).
    pub ticks: u64,
    pub sent_pos: Option<[f64; 3]>,
    pub sent_look: Option<(f32, f32)>,
    pub sent_tick: u64,
    /// Hits on the bot in the last `HITS_KEPT_TICKS` server ticks, oldest first.
    #[serde(default)]
    pub hits: Vec<Hit>,
    /// The server's account of the bot's death (`player_combat_kill`); the run ends there.
    #[serde(default)]
    pub death: Option<Death>,
    /// The highest stage the world has held during the run (items get spent, so it only
    /// rises): raised on every tick.
    #[serde(default)]
    pub stage_reached: Option<Stage>,
    /// The selected hotbar slot (0-8): set by the server's `set_held_slot` and by the bot's own
    /// `Act::Hold`.
    #[serde(default)]
    pub held: u8,
    /// The last block-action sequence sent, and the highest the server has acknowledged.
    #[serde(default)]
    pub sequence_sent: i32,
    #[serde(default)]
    pub sequence_acked: i32,
    /// Pickups by the bot in the last minute of server ticks, oldest first.
    #[serde(default)]
    pub pickups: Vec<Pickup>,
    /// The click waiting for the server's answer: answered when window 0's state id moves.
    #[serde(default)]
    pub click: Option<PendingClick>,
    /// Clicks sent and answered.
    #[serde(default)]
    pub clicks_sent: u64,
    #[serde(default)]
    pub clicks_answered: u64,
}

/// Ticks between status-only movement packets when nothing moved: once a second.
const STATUS_ONLY_EVERY: u64 = 20;

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
                if let Some(s) = Stage::highest(self)
                    && self.stage_reached.is_none_or(|r| s > r)
                {
                    self.stage_reached = Some(s);
                }
                self.tick()
            }
            Event::Controls(c) => {
                self.controls = *c;
                Vec::new()
            }
            Event::Act(a) => self.act(*a),
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

    /// What collides at a block, for physics: a full cube, a flat box's top, or nothing. An
    /// unloaded block counts as a full cube, so the body never falls out of the world.
    pub fn solid(&self, x: i32, y: i32, z: i32) -> Option<f64> {
        let Some(state) = self.state_at(BlockPos::new(x, y, z)) else {
            return Some(1.0);
        };
        match data::block_of_state(state).and_then(collision) {
            Some(Collision::Empty) => None,
            Some(Collision::Partial { top }) => Some(top),
            Some(Collision::Full) | None => Some(1.0),
        }
    }

    /// The fluid at a block, for physics: water or lava and its level (the block state's
    /// `level` property; unloaded is no fluid).
    pub fn fluid(&self, x: i32, y: i32, z: i32) -> Option<Fluid> {
        let state = self.state_at(BlockPos::new(x, y, z))?;
        let kind = match data::block_of_state(state)? {
            BlockKind::Water => FluidKind::Water,
            BlockKind::Lava => FluidKind::Lava,
            _ => return None,
        };
        let first = data::block_of_state(state)?.info()?.first_state;
        let level = u8::try_from(state.checked_sub(first)?).ok()?;
        Some(Fluid { kind, level })
    }

    /// One tick of the body: physics, then the movement packet vanilla would send.
    fn tick(&mut self) -> Vec<Effect> {
        self.ticks += 1;
        let Some(body) = self.body else {
            return Vec::new();
        };
        let at = BlockPos::containing(body.pos[0], body.pos[1], body.pos[2]).chunk();
        if !self.columns.contains_key(&at) {
            // No physics without the ground, but still the once-a-second status: the server
            // re-centres a player's chunks only when the client reports a move (handleMovePlayer
            // calls ServerChunkCache.move), so a teleport far from the loaded columns would
            // otherwise wait forever on chunks that wait on a move (found by the parity table).
            if self.ticks - self.sent_tick >= STATUS_ONLY_EVERY {
                self.sent_tick = self.ticks;
                return vec![Effect::Send(C2s::MovePlayerStatusOnly { flags: 0 })];
            }
            return Vec::new();
        }
        let solid = |x, y, z| self.solid(x, y, z);
        let fluid = |x, y, z| self.fluid(x, y, z);
        let moved = physics::step(&solid, &fluid, &body, &self.controls);
        self.body = Some(moved);
        self.player.pos = Some(moved.pos);
        // Physics and the movement packets use the controls' yaw; the server's look is kept.
        #[allow(clippy::cast_possible_truncation)]
        let look = (self.controls.yaw as f32, self.player.pitch);
        self.player.yaw = look.0;
        let flags = u8::from(moved.on_ground) | (u8::from(moved.horizontal_collision) << 1);
        if self.sent_pos != Some(moved.pos) || self.sent_look != Some(look) {
            self.sent_pos = Some(moved.pos);
            self.sent_look = Some(look);
            self.sent_tick = self.ticks;
            let [x, y, z] = moved.pos;
            return vec![Effect::Send(C2s::MovePlayerPosRot {
                x,
                y,
                z,
                yaw: look.0,
                pitch: look.1,
                flags,
            })];
        }
        if self.ticks - self.sent_tick >= STATUS_ONLY_EVERY {
            self.sent_tick = self.ticks;
            return vec![Effect::Send(C2s::MovePlayerStatusOnly { flags })];
        }
        Vec::new()
    }

    fn enter(&mut self, type_id: i32, name: &str) {
        let same = self.dimension.as_ref().is_some_and(|d| d.name == name);
        if !same {
            // Chunks and entities belong to a dimension: none of the old ones are here. Memory
            // keeps everything, by dimension.
            self.columns.clear();
            self.overlay.clear();
            self.entities.clear();
            self.body = None;
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

    /// An act's packets, remembering what was sent.
    fn act(&mut self, a: Act) -> Vec<Effect> {
        match a {
            Act::Dig { step, pos, face } => {
                self.sequence_sent += 1;
                vec![Effect::Send(C2s::PlayerAction {
                    status: step.status(),
                    pos: (pos.x, pos.y, pos.z),
                    face,
                    sequence: self.sequence_sent,
                })]
            }
            Act::Hold { slot } => {
                self.held = slot.min(8);
                vec![Effect::Send(C2s::SetCarriedItem {
                    slot: i16::from(self.held),
                })]
            }
            Act::Click { slot, button, mode } => {
                self.click = Some(PendingClick {
                    slot,
                    state_id: self.inventory.state_id,
                    at_tick: self.ticks,
                });
                self.clicks_sent += 1;
                // The click claims an empty cursor, and the server sends the cursor only when it
                // differs from the claim (`setRemoteCarried`, then `broadcastChanges`): so the
                // cursor is forgotten here and a `set_cursor_item` restores it when it holds
                // something (clojurecraft's `click` does the same).
                self.inventory.carried = None;
                vec![Effect::Send(C2s::ContainerClick {
                    window: 0,
                    state_id: self.inventory.state_id,
                    slot,
                    button,
                    mode,
                })]
            }
            Act::CloseWindow => vec![Effect::Send(C2s::ContainerClose { window: 0 })],
            Act::Swing => vec![Effect::Send(C2s::Swing { hand: 0 })],
        }
    }

    /// The server answered the pending click: window 0's state id moved.
    fn window_moved(&mut self, state_id: i32) {
        if let Some(c) = self.click
            && c.state_id != state_id
        {
            self.click = None;
            self.clicks_answered += 1;
        }
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
                    self.window_moved(*state_id);
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
                    self.window_moved(*state_id);
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
                    // The server placed the body: vanilla zeroes its velocity on a teleport.
                    let mut body = self.body.unwrap_or_default();
                    body.pos = pos;
                    body.vel = [0.0; 3];
                    self.body = Some(body);
                    self.sent_pos = Some(pos);
                }
                self.player.yaw = if rel(3) { self.player.yaw + yaw } else { *yaw };
                self.player.pitch = if rel(4) {
                    self.player.pitch + pitch
                } else {
                    *pitch
                };
            }
            S2c::BlockChangedAck { sequence } => {
                self.sequence_acked = self.sequence_acked.max(*sequence);
            }
            S2c::SetHeldSlot { slot } => {
                self.held = u8::try_from(*slot).unwrap_or(0).min(8);
            }
            S2c::TakeItemEntity {
                item,
                collector,
                amount,
            } => {
                if Some(*collector) == self.player.entity_id {
                    let tick = self.session.server_ticks;
                    self.pickups.retain(|p| p.at_tick + HITS_KEPT_TICKS >= tick);
                    self.pickups.push(Pickup {
                        at_tick: tick,
                        item_entity: *item,
                        amount: *amount,
                    });
                }
            }
            S2c::ContainerClose { .. } => {}
            // The cursor carries no state id: it never answers a click on its own (the slot the
            // click emptied or filled does).
            S2c::SetCursorItem { item } => self.inventory.carried = Inventory::stack(*item),
            S2c::PlayerCombatKill { player_id, death } => {
                if Some(*player_id) == self.player.entity_id {
                    self.death = Some(death.clone());
                }
            }
            S2c::DamageEvent {
                entity_id,
                source_type,
                cause,
                direct,
            } => {
                if Some(*entity_id) == self.player.entity_id {
                    let tick = self.session.server_ticks;
                    let kind =
                        |id: &Option<i32>| id.and_then(|i| self.entities.get(&i)).map(|e| e.kind);
                    let damage_type = usize::try_from(*source_type)
                        .ok()
                        .and_then(|i| data::generated::registries::DAMAGE_TYPE.get(i))
                        .map_or_else(|| format!("unknown:{source_type}"), |n| (*n).to_owned());
                    let hit = Hit {
                        at_tick: tick,
                        damage_type,
                        cause: kind(cause),
                        direct: kind(direct),
                    };
                    self.hits.retain(|h| h.at_tick + HITS_KEPT_TICKS >= tick);
                    self.hits.push(hit);
                }
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

    /// Whether the column under the body is loaded (physics runs only then).
    pub fn columns_loaded_under_body(&self) -> bool {
        self.body.is_some_and(|b| {
            self.columns
                .contains_key(&BlockPos::containing(b.pos[0], b.pos[1], b.pos[2]).chunk())
        })
    }

    /// The block the player stands on (just below the feet).
    pub fn block_under_player(&self) -> Option<BlockKind> {
        let [x, y, z] = self.player.pos?;
        self.block_at(BlockPos::containing(x, y - 0.01, z))
    }
}
