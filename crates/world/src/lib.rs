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
use data::generated::movement::movement;
use data::generated::shapes::collision;
use data::types::Collision;
use data::{BlockKind, EntityKind, ItemKind};
use memory::{Memory, Seen};
use physics::{Body, Controls, Fluid, FluidKind, Surface};
use protocol::chunk::Column;
use protocol::metadata::MetaValue;
use protocol::nbt::Death;
use protocol::packets::{INPUT_FORWARD, INPUT_JUMP, INPUT_SHIFT};
use protocol::slot::{Slot, Wear};
use protocol::{C2s, Out, S2c, Session};

pub mod biome;
pub mod stage;
pub mod time;
pub use biome::{BIOME_REGISTRY, ticks};
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
    /// The server said it died (entity event 3); it is removed a moment later.
    #[serde(default)]
    pub dead: bool,
    /// The bot has sent it an `attack`: its death counts as a kill.
    #[serde(default)]
    pub struck: bool,
    /// Its UUID, from `add_entity` (truth files name mobs by it).
    #[serde(default)]
    pub uuid: Option<[u8; 16]>,
    /// Its velocity as the server last set it, blocks per tick (a fireball's heading).
    #[serde(default)]
    pub velocity: Option<[f64; 3]>,
    /// The newest entity-event statuses with their server tick (`ENTITY_STATUSES_KEPT`).
    #[serde(default)]
    pub statuses: Vec<(u64, i8)>,
    /// The newest damage events on it: tick, damage type, and whether the bot dealt it
    /// (`ENTITY_HURTS_KEPT`). A real server sends no hurt animation for a mob, so this is how
    /// a swing is known to have landed (clojurecraft hear.clj).
    #[serde(default)]
    pub hurts: Vec<Hurt>,
    /// Where its head points, degrees (`rotate_head`): an enderman facing the bot.
    #[serde(default)]
    pub head_yaw: Option<f32>,
    /// Its body's yaw, degrees, from the moves that carry one (issue 20g: the perched dragon's
    /// facing, `plan::dragon`). `None` until one comes.
    #[serde(default)]
    pub yaw: Option<f32>,
    /// What it holds and wears, by equipment slot (0 main hand, 1 off hand, 2 to 5 feet to
    /// head, 6 body, 7 saddle): a wither skeleton's sword, a piglin's crossbow.
    #[serde(default)]
    pub equipment: BTreeMap<u8, ItemKind>,
    /// Its last main-hand swing (`animate` 0), server tick: a strike coming.
    #[serde(default)]
    pub swung_at: Option<u64>,
    /// A fireball's acceleration power (`projectile_power`).
    #[serde(default)]
    pub power: Option<f64>,
    /// Its velocity at spawn, from `add_entity` (issue 19b: a thrown eye's first motion). Kept
    /// apart from `velocity` (the server's later `set_entity_motion`), which the fireball
    /// readers take as "sent since".
    #[serde(default)]
    pub spawn_velocity: Option<[f64; 3]>,
    /// What rides it (`set_passengers`).
    #[serde(default)]
    pub passengers: Vec<i32>,
    /// Effects on it by name, with their amplifier.
    #[serde(default)]
    pub effects: BTreeMap<String, i32>,
    /// Its attributes as the server sent them (base plus add-value modifiers).
    #[serde(default)]
    pub attributes: BTreeMap<String, f64>,
}

/// Sounds and level events kept (the newest): a minute of a fight is well under this.
pub const SOUNDS_KEPT: usize = 64;

/// A sound the bot heard: by name, where (or on which entity), and when.
#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, PartialEq)]
pub struct Heard {
    pub at_tick: u64,
    /// `entity.blaze.shoot`, from the generated sound registry.
    pub name: String,
    pub pos: Option<[f64; 3]>,
    pub entity: Option<i32>,
    pub volume: f32,
}

/// A level event: its id, its constant's name from the game, and where.
#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, PartialEq)]
pub struct LevelEventSeen {
    pub at_tick: u64,
    pub id: i32,
    pub name: Option<String>,
    pub pos: (i32, i32, i32),
    pub data: i32,
}

/// A registry entry's name by id, or `id:<n>` when the table lacks it.
fn registry_name(table: &[&str], id: i32) -> String {
    usize::try_from(id)
        .ok()
        .and_then(|i| table.get(i))
        .map_or_else(|| format!("id:{id}"), |n| (*n).to_owned())
}

/// A line of chat or screen text and where it came from.
#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, PartialEq)]
pub struct ChatLine {
    pub at_tick: u64,
    /// `system`, `action_bar`, `title` or `subtitle`.
    pub via: String,
    pub text: String,
}

/// A blast: where, when, and the knockback it gave the bot.
#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, PartialEq)]
pub struct Blast {
    pub at_tick: u64,
    pub center: [f64; 3],
    pub knockback: Option<[f64; 3]>,
}

/// A crystal's removal and its blast within this many server ticks of each other are its
/// destruction (the same tick on a real server; a second of slack, as `observe`'s strikes).
pub const CRYSTAL_BLAST_TICKS: u64 = 20;
/// A blast this near a removed crystal's position is its own (the crystal explodes at its feet
/// position, `EndCrystal.hurtServer`).
pub const CRYSTAL_BLAST_NEAR: f64 = 1.0;
/// A destroyed crystal belongs to the spike whose centre column is this near, horizontally (the
/// crystal stands on the centre + 0.5, config/end.toml).
pub const CRYSTAL_SPIKE_NEAR: f64 = 2.0;

/// Whether a blast's centre is a removed crystal's own (`CRYSTAL_BLAST_NEAR`).
fn blast_near(center: [f64; 3], pos: [f64; 3]) -> bool {
    let d = [0, 1, 2].map(|i| center[i] - pos[i]);
    (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt() <= CRYSTAL_BLAST_NEAR
}

/// A spike seen with no crystal this many world ticks running is one gone (20 s: a resume's
/// re-send of the crystals is done within about 80, race 46044026128 lane 0: untracked on tick
/// 40, every one sent again by tick 100).
pub const CRYSTAL_ABSENT_TICKS: u64 = 400;

/// Entity-event statuses kept per entity.
pub const ENTITY_STATUSES_KEPT: usize = 4;
/// Damage events kept per entity.
pub const ENTITY_HURTS_KEPT: usize = 4;

/// A damage event on another entity.
#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, PartialEq)]
pub struct Hurt {
    pub at_tick: u64,
    pub damage_type: String,
    /// The bot was the cause (its swing or its arrow).
    pub by_bot: bool,
    // >>> pumpkin (issue 80)
    /// The entity that caused it (the packet's source cause, else its direct source), `None`
    /// for a hurt with no entity (water, a fall): only one with an entity sets an enderman's
    /// hurt-by target (`LivingEntity.hurtServer`, probe 46160185304).
    #[serde(default)]
    pub cause: Option<i32>,
    // <<< pumpkin
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

    /// A blaze's charge flag: its own metadata index 16, bit 0, set 1.4 to 3.5 s before its
    /// fireballs spawn, with no direction in it (clojurecraft dodge.clj, measured on 26.1.2;
    /// docs/problems/blaze.md). False for anything else or when not yet sent.
    pub fn blaze_charged(&self) -> bool {
        self.kind == EntityKind::Blaze
            && matches!(self.metadata.get(&16), Some(MetaValue::Byte(b)) if b & 1 == 1)
    }

    /// Whether its head points at `target` within `degrees` on the plane (`None` before any
    /// `rotate_head`): an enderman staring, a skeleton turned to aim. Yaw 0 faces +z, 90 faces
    /// -x, as the game counts it; clojurecraft takes 12 degrees as a stare.
    pub fn faces(&self, target: [f64; 3], degrees: f64) -> Option<bool> {
        let yaw = f64::from(self.head_yaw?);
        let (dx, dz) = (target[0] - self.pos[0], target[2] - self.pos[2]);
        let toward = (-dx).atan2(dz).to_degrees();
        let diff = (yaw - toward).rem_euclid(360.0);
        Some(diff.min(360.0 - diff) <= degrees)
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
    /// The same, by the component that stopped the read (what to read next, issue 16e).
    #[serde(default)]
    pub truncated_by: BTreeMap<String, u64>,
    /// The wear of each slot whose stack carries damage or a max damage (issue 16e: a pickaxe
    /// wearing out was invisible, blaze wall 7). A slot not here is unworn.
    #[serde(default)]
    pub wear: BTreeMap<i16, Wear>,
}

impl Inventory {
    fn stack(s: Slot) -> Option<(ItemKind, i32)> {
        let (item, count) = s.stack()?;
        (count > 0).then(|| (ItemKind::from_id(item), count))
    }

    fn put(&mut self, slot: i16, s: Slot) {
        if s.truncated() {
            self.truncated += 1;
            *self
                .truncated_by
                .entry(s.stopped_at().unwrap_or("unknown").to_owned())
                .or_default() += 1;
        }
        match Self::stack(s) {
            Some(stack) => {
                self.slots.insert(slot, stack);
            }
            None => {
                self.slots.remove(&slot);
            }
        }
        match s
            .wear()
            .filter(|w| w.damage.is_some() || w.max_damage.is_some())
        {
            Some(w) if self.slots.contains_key(&slot) => {
                self.wear.insert(slot, w);
            }
            Some(_) | None => {
                self.wear.remove(&slot);
            }
        }
    }

    /// Every slot emptied (a whole-window refresh).
    fn clear(&mut self) {
        self.slots.clear();
        self.wear.clear();
    }

    /// Uses left on the stack in `slot`: its max damage (the component, else the item's
    /// default) less its damage. `None` for an empty slot or an item that does not wear.
    pub fn uses_left(&self, slot: i16) -> Option<i32> {
        let (item, _) = self.slots.get(&slot)?;
        let wear = self.wear.get(&slot).copied().unwrap_or_default();
        let max = wear
            .max_damage
            .unwrap_or_else(|| data::generated::item_props::max_damage(*item));
        (max > 0).then(|| max - wear.damage.unwrap_or(0))
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
    /// A click in the open window (window 0 when none is), predicting nothing, stamped with that
    /// window's last state id.
    Click {
        slot: i16,
        button: i8,
        mode: i32,
    },
    CloseWindow,
    Swing,
    /// Ask for window 0 again: a click with a state id the server never has, on slot -1 (which
    /// does nothing), so the server answers with the whole window (clojurecraft's
    /// `resync-click`). After a missed click.
    Resync,
    /// Right-click a block's face with the main hand (`use_item_on`, issue 11a): place the held
    /// block against it, or open the block (a crafting table).
    UseOn {
        pos: BlockPos,
        face: i8,
    },
    /// Melee an entity (775's `attack` packet, issue 12a).
    Attack {
        entity: i32,
    },
    /// Start using the held item, aimed by the body's look (eating starts here).
    UseItem,
    /// Let go of the use key (`player_action` status 5).
    ReleaseUse,
    /// Respawn after a death (`client_command` 0): the death is cleared and the body waits for
    /// the server's next position (the marathon keeps playing through deaths).
    Respawn,
    /// Start using the off-hand item (`use_item` with hand 1), aimed by the body's look: a
    /// shield is raised and stays up until `ReleaseUse` (docs/problems/shield.md).
    UseOffHand,
}

/// A window the server opened (`open_screen`, issue 11a): its id, menu type (an index into
/// `data::generated::registries::MENU`), slots and state id. The player's inventory is its last 36
/// slots, mirrored into window 0's view as they change.
#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, Default, PartialEq, Eq)]
pub struct Window {
    pub id: i32,
    pub menu: i32,
    pub slots: BTreeMap<i16, (ItemKind, i32)>,
    pub size: i16,
    pub state_id: i32,
    /// Its properties as the server last sent them (`container_set_data`): a furnace's 0 lit
    /// time left, 1 lit total, 2 cook progress, 3 cook total (issue 13a).
    #[serde(default)]
    pub properties: BTreeMap<i16, i16>,
}

impl Window {
    /// Window 0's slot for one of this window's inventory slots (its last 36: main 27, hotbar 9).
    pub fn inventory_slot(&self, slot: i16) -> Option<i16> {
        let first = self.size - 36;
        (self.size > 36 && slot >= first && slot < self.size).then_some(9 + slot - first)
    }
}

/// A click sent and not yet answered: the window's state id when it went, and when.
#[derive(serde::Serialize, serde::Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub struct PendingClick {
    /// The window it went to (0, or the open window).
    #[serde(default)]
    pub window: i32,
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
    /// The air supply, 300 full (the player's own metadata index 1), and whether it is on fire
    /// (index 0, bit 0), as the server last said (issue 12a).
    #[serde(default)]
    pub air: Option<i32>,
    #[serde(default)]
    pub on_fire: bool,
    /// Items finished using (an eat: entity event 9 for the bot), counted.
    #[serde(default)]
    pub uses_finished: u64,
    /// Effects on the bot by name: amplifier and the server tick it ends (issue 16e).
    #[serde(default)]
    pub effects: BTreeMap<String, (i32, u64)>,
    /// Its attributes as the server holds them (`armor`, `max_health`): base plus the
    /// add-value modifiers.
    #[serde(default)]
    pub attributes: BTreeMap<String, f64>,
    /// The server thinks it is in a fight (`player_combat_enter` until `_end`).
    #[serde(default)]
    pub in_combat: bool,
    /// Experience: level and total points.
    #[serde(default)]
    pub experience: Option<(i32, i32)>,
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
    /// Entity-data reads cut short, by the serializer they stopped at (issue 16e).
    #[serde(default)]
    pub metadata_unread: BTreeMap<String, u64>,
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
    /// The keys last reported to the server (`C2s::PlayerInput`'s bits): a new report goes out
    /// only when they change, as the client's `lastSentInput` does.
    #[serde(default)]
    pub sent_input: u8,
    /// Hits on the bot in the last `HITS_KEPT_TICKS` server ticks, oldest first.
    #[serde(default)]
    pub hits: Vec<Hit>,
    /// Sounds heard, newest last (`SOUNDS_KEPT`): what the bot hears out of sight (issue 16e).
    #[serde(default)]
    pub sounds: Vec<Heard>,
    /// Level events, newest last (`SOUNDS_KEPT`): 1018 a blaze shooting, 1015 a ghast's warning.
    #[serde(default)]
    pub level_events: Vec<LevelEventSeen>,
    /// Blasts heard and felt, newest last (`SOUNDS_KEPT`).
    #[serde(default)]
    pub explosions: Vec<Blast>,
    /// End crystals removed with no explosion at their spot yet, (tick, position), newest last:
    /// the server removes a crystal before its blast (`EndCrystal.hurtServer`: `remove`, then
    /// `explode`), so a removal waits `CRYSTAL_BLAST_TICKS` for one (issue 20 part 2).
    #[serde(default)]
    pub crystals_removed: Vec<(u64, [f64; 3])>,
    /// Spikes (centre columns) seen with no crystal on them, since this world tick: the bot
    /// within `stage::CRYSTALS_NEAR` of the fountain (every crystal then inside the tracking
    /// distance) and the spike's column loaded. Held `CRYSTAL_ABSENT_TICKS`, it is a crystal gone
    /// that the bot did not see go (a development start's RCON kill, a synthetic island). Counted
    /// from `ticks` before its increment: recorded on the `CRYSTAL_ABSENT_TICKS` + 1th tick.
    #[serde(default)]
    pub crystals_absent: BTreeMap<(i32, i32), u64>,
    /// Each spawner's mob from its block-entity data (`blaze`): which spawner is the blaze one,
    /// with no blaze in sight (blaze wall 4 took a dungeon spawner for it).
    #[serde(default)]
    pub spawner_mobs: BTreeMap<(i32, i32, i32), String>,
    /// Blocks cracking under another's digging, by position: (breaker, stage, server tick).
    #[serde(default)]
    pub cracks: BTreeMap<(i32, i32, i32), (i32, i8, u64)>,
    /// The bot's last hurt flash: server tick and the yaw the blow came from.
    #[serde(default)]
    pub hurt_from: Option<(u64, f32)>,
    /// Item cooldowns by group (`ender_pearl`): the server tick each ends.
    #[serde(default)]
    pub cooldowns: BTreeMap<String, u64>,
    /// Game events, newest last (`SOUNDS_KEPT`): server tick, id, its name, parameter.
    #[serde(default)]
    pub game_events: Vec<(u64, u8, Option<String>, f32)>,
    /// The game said the run is won (`win_game`, after the dragon).
    #[serde(default)]
    pub won: bool,
    // >>> E1 (issue 20b)
    /// The server tick the dragon was first seen dead: entity event 3 on an `ender_dragon`
    /// (LivingEntity.die at the killing blow, the 26.1.2 jar) or, in the End, the dragon's boss
    /// bar removed (EnderDragonFight.setDragonKilled hides it after the 200-tick death). The
    /// `win` stage asks for it before `won`.
    #[serde(default)]
    pub dragon_dead: Option<u64>,
    /// Attacks the bot has sent (`Act::Attack`), and the last one's server tick and target id
    /// (a dragon part's id is not an entity of its own): the ledger's strikes.
    #[serde(default)]
    pub attacks_sent: u64,
    #[serde(default)]
    pub last_attack: Option<(u64, i32)>,
    // <<< E1
    /// Chat and the screen's text, newest last (`SOUNDS_KEPT`).
    #[serde(default)]
    pub chat: Vec<ChatLine>,
    /// Boss bars by uuid: name and health 0 to 1 (the dragon's, issue 20).
    #[serde(default)]
    pub bosses: BTreeMap<[u8; 16], (String, f32)>,
    /// The chunk the server centres the view on, the view radius, and the simulation distance
    /// (mobs beyond it do not move: issue 17's frozen endermen).
    #[serde(default)]
    pub chunk_center: Option<(i32, i32)>,
    #[serde(default)]
    pub view_radius: Option<i32>,
    #[serde(default)]
    pub simulation_distance: Option<i32>,
    /// Chunks whose biomes the server changed since the run began.
    #[serde(default)]
    pub biome_updates: u64,
    /// The biome registry's names in the server's order (`registry_data`, issue 17b): a
    /// chunk's biome ids index it. Empty until it comes; `biome` then falls back to the
    /// generated table.
    #[serde(default)]
    pub biome_registry: Vec<String>,
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
    /// The window open besides the inventory (issue 11a).
    #[serde(default)]
    pub window: Option<Window>,
    /// The window last closed by the client: the server may still answer a click made in it
    /// after the close went out (a furnace's take answered over two ticks, issue 13a), and what
    /// such an answer puts in the player's slots is still the player's.
    #[serde(default)]
    pub closed_window: Option<Window>,
    /// Entities the bot struck that the server then said died (entity event 3) (issue 12a):
    /// other deaths in view (another player's kill, a player in lava) are not the bot's.
    #[serde(default)]
    pub kills: u64,
    /// The level's game time from the newest `set_time`.
    #[serde(default)]
    pub game_time: Option<i64>,
    /// The overworld clock from the newest `set_time` that carried it: its total ticks, the
    /// game time it came with and its rate. `day_time` runs it on from there.
    #[serde(default)]
    pub day_clock: Option<DayClock>,
}

/// The overworld clock as last told (`protocol::OVERWORLD_CLOCK`).
#[derive(serde::Serialize, serde::Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct DayClock {
    pub total_ticks: i64,
    pub game_time: i64,
    /// Clock ticks per game tick: 1, or 0 with the day cycle stopped.
    pub rate: f32,
}

/// Ticks between status-only movement packets when nothing moved: once a second.
const STATUS_ONLY_EVERY: u64 = 20;

impl World {
    pub fn new() -> Self {
        Self::default()
    }

    // >>> E9 (issue 20 part 2): a crystal is gone only when destroyed, never when untracked.
    /// A crystal removed: destroyed if a blast at its spot came just before, else held for one.
    fn crystal_removed(&mut self, pos: [f64; 3]) {
        let now = self.session.server_ticks;
        let blast = self
            .explosions
            .iter()
            .any(|b| b.at_tick + CRYSTAL_BLAST_TICKS >= now && blast_near(b.center, pos));
        if blast {
            self.crystal_destroyed(pos);
            return;
        }
        self.crystals_removed
            .retain(|(t, _)| t + CRYSTAL_BLAST_TICKS >= now);
        self.crystals_removed.push((now, pos));
    }

    /// A blast: every crystal removed just before at its spot was destroyed by it.
    fn crystal_blast(&mut self, center: [f64; 3]) {
        let now = self.session.server_ticks;
        let (hit, kept): (Vec<_>, Vec<_>) = self
            .crystals_removed
            .iter()
            .filter(|(t, _)| t + CRYSTAL_BLAST_TICKS >= now)
            .partition(|(_, p)| blast_near(center, *p));
        self.crystals_removed = kept;
        for (_, p) in hit {
            self.crystal_destroyed(p);
        }
    }

    /// Each spike with no crystal on it, seen as `crystals_absent` says: held
    /// `CRYSTAL_ABSENT_TICKS`, recorded gone. Any break (a crystal there, the bot far off, the
    /// column unloaded, out of the End) starts it again.
    fn crystals_absent_tick(&mut self) {
        let near = stage::in_end(self)
            && stage::from_fountain(self).is_some_and(|d| d <= stage::CRYSTALS_NEAR);
        let now = self.ticks;
        let done = self.memory.crystals_destroyed().clone();
        for [x, z] in stage::SPIKES {
            let loaded = self.columns.contains_key(&ChunkPos {
                x: x.div_euclid(16),
                z: z.div_euclid(16),
            });
            let crystal = self.entities.values().any(|e| {
                e.kind == EntityKind::EndCrystal
                    && !e.dead
                    && (e.pos[0] - (f64::from(x) + 0.5)).hypot(e.pos[2] - (f64::from(z) + 0.5))
                        <= CRYSTAL_SPIKE_NEAR
            });
            if !near || !loaded || crystal || done.contains(&(x, z)) {
                self.crystals_absent.remove(&(x, z));
                continue;
            }
            let since = *self.crystals_absent.entry((x, z)).or_insert(now);
            if now >= since + CRYSTAL_ABSENT_TICKS {
                self.memory.record_crystal_destroyed((x, z));
                self.crystals_absent.remove(&(x, z));
            }
        }
    }

    /// Record the spike under a destroyed crystal; one off every spike is not counted.
    fn crystal_destroyed(&mut self, pos: [f64; 3]) {
        let spike = stage::SPIKES.iter().find(|[x, z]| {
            (pos[0] - (f64::from(*x) + 0.5)).hypot(pos[2] - (f64::from(*z) + 0.5))
                <= CRYSTAL_SPIKE_NEAR
        });
        if let Some([x, z]) = spike {
            self.memory.record_crystal_destroyed((*x, *z));
        }
    }
    // <<< E9

    /// The time of day, 0 to 23,999 (`time`'s helpers read it): the overworld clock as last
    /// told, run on by the game ticks since at its rate, so a `set_time` that leaves the clock
    /// out still moves it. `None` before any `set_time` with the overworld clock.
    pub fn day_time(&self) -> Option<i64> {
        let c = self.day_clock?;
        let since = self
            .game_time
            .unwrap_or(c.game_time)
            .saturating_sub(c.game_time);
        let ran = (since as f64 * f64::from(c.rate)).floor() as i64;
        Some(time::of_day(c.total_ticks.saturating_add(ran)))
    }

    /// Night now (`time::is_night`); `None` while the time is unknown.
    pub fn is_night(&self) -> Option<bool> {
        self.day_time().map(time::is_night)
    }

    /// Fold one event; return what the shell must do, in order.
    pub fn step(&mut self, e: &Event) -> Vec<Effect> {
        match e {
            Event::Tick { now_ms, seed } => {
                self.now_ms = *now_ms;
                self.seed = *seed;
                // Only the stages above the one reached, and the two that scan the sightings of 25
                // chunk columns (fortress, spawner) once a second: in a fortress the scan is
                // thousands of sightings a tick (callgrind of a replay of race 45350933958 lane 1:
                // `Stage::highest` -> `kind_near` -> `Memory::in_column`).
                let scans = self.ticks.is_multiple_of(stage::STAGE_EVERY_TICKS);
                self.crystals_absent_tick();
                if let Some(s) = Stage::highest_above(self, self.stage_reached, scans) {
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

    /// What the block at a cell does to a body moving on it, for physics: its speed and jump
    /// factors from the generated table (soul sand 0.4, honey 0.4 and 0.5), and whether it is
    /// water or a bubble column. An unloaded block is plain.
    pub fn surface(&self, x: i32, y: i32, z: i32) -> Surface {
        let Some(block) = self
            .state_at(BlockPos::new(x, y, z))
            .and_then(data::block_of_state)
        else {
            return Surface::PLAIN;
        };
        let m = movement(block);
        Surface {
            speed_factor: f64::from(m.speed_factor),
            jump_factor: f64::from(m.jump_factor),
            wet: matches!(block, BlockKind::Water | BlockKind::BubbleColumn),
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

    /// The keys held, as the server is told them: forward, jump and shift from the controls.
    /// The bot has no backward, strafe or sprint key (it walks; sprinting is never reported).
    pub fn input_flags(controls: &Controls) -> u8 {
        (if controls.forward { INPUT_FORWARD } else { 0 })
            | (if controls.jump { INPUT_JUMP } else { 0 })
            | (if controls.sneak { INPUT_SHIFT } else { 0 })
    }

    /// One tick of the body: the keys when they changed, physics, then the movement packet
    /// vanilla would send.
    fn tick(&mut self) -> Vec<Effect> {
        self.ticks += 1;
        let Some(body) = self.body else {
            return Vec::new();
        };
        let mut out = self.report_input();
        out.extend(self.move_body(body));
        out
    }

    /// The keys, when they differ from the last report: vanilla's `LocalPlayer.tick` sends
    /// `player_input` after the body's tick and before its position whenever the pressed keys
    /// are not the last ones sent. This is the only way a server learns the bot sneaks: its
    /// shift key spares it on magma (`MagmaBlock.stepOn`) and gives it the crouching pose.
    fn report_input(&mut self) -> Vec<Effect> {
        let flags = Self::input_flags(&self.controls);
        if flags == self.sent_input {
            return Vec::new();
        }
        self.sent_input = flags;
        vec![Effect::Send(C2s::PlayerInput { flags })]
    }

    fn move_body(&mut self, body: Body) -> Vec<Effect> {
        let at = BlockPos::containing(body.pos[0], body.pos[1], body.pos[2]).chunk();
        if !self.columns.contains_key(&at) {
            // >>> E3 (issue 20e): a look asked for goes out though the ground is not loaded yet.
            // The server judges an enderman's stare by the look it holds, and after a teleport
            // that is the one it set (pitch 0 on arriving in the End) until a look packet comes;
            // with no column under the feet none was sent (E5's race 45913296602 lane 1: an
            // enderman angered at tick 60 of the arrival, the bot killed fighting it).
            if let Some(pitch) = self.controls.pitch {
                #[allow(clippy::cast_possible_truncation)]
                let look = (self.controls.yaw as f32, pitch);
                if self.sent_look != Some(look) {
                    self.sent_look = Some(look);
                    self.sent_pos = Some(body.pos);
                    self.sent_tick = self.ticks;
                    self.player.yaw = look.0;
                    self.player.pitch = look.1;
                    let [x, y, z] = body.pos;
                    return vec![Effect::Send(C2s::MovePlayerPosRot {
                        x,
                        y,
                        z,
                        yaw: look.0,
                        pitch: look.1,
                        flags: u8::from(body.on_ground),
                    })];
                }
            }
            // <<< E3
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
        let surface = |x, y, z| self.surface(x, y, z);
        let moved = physics::step_on(&solid, &fluid, &surface, &body, &self.controls);
        self.body = Some(moved);
        self.player.pos = Some(moved.pos);
        // Physics and the movement packets use the controls' yaw; the pitch is the controls' when
        // set (a bucket aimed, issue 14a), else the server's look is kept.
        #[allow(clippy::cast_possible_truncation)]
        let look = (
            self.controls.yaw as f32,
            self.controls.pitch.unwrap_or(self.player.pitch),
        );
        self.player.yaw = look.0;
        self.player.pitch = look.1;
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
    /// exposed face now: the only way a sighting is made (no X-ray, memory's `Seen`). A biome
    /// sign with an exposed face is kept in memory's sign record too (issue 17b).
    fn observe(&mut self, pos: BlockPos) {
        let Some(dim) = self.dimension.as_ref().map(|d| d.name.clone()) else {
            return;
        };
        let Some(state) = self.state_at(pos) else {
            return;
        };
        let cell = (pos.x, pos.y, pos.z);
        self.memory
            .forget_sign(&dim, cell, data::block_of_state(state));
        let sign = memory::is_sign(state);
        let watched = memory::watched_in(&dim, state) || self.memory.on_record(&dim, cell);
        if !sign && !watched {
            return;
        }
        let neighbours =
            memory::neighbours(cell).map(|(x, y, z)| self.state_at(BlockPos::new(x, y, z)));
        if let Some(seen) = Seen::check(&dim, cell, state, neighbours) {
            if sign {
                self.memory.record_sign(&seen);
            }
            if watched {
                self.memory.record(seen, self.session.server_ticks);
            }
        }
    }

    /// A column's biome signs with an exposed face (issue 17b), sections tested by palette
    /// first; a sign whose record in the column is full (`memory::SIGNS_KEPT`) is skipped, so a
    /// soul sand valley column costs a palette test and a few hundred face checks. Neighbours
    /// inside the section are read from it directly.
    fn observe_column_signs(&mut self, at: ChunkPos, column: &Arc<protocol::chunk::Column>) {
        let Some(dim) = self.dimension.as_ref().map(|d| d.name.clone()) else {
            return;
        };
        let kinds = memory::SIGN_KINDS;
        let ix = |k: data::BlockKind| kinds.iter().position(|x| *x == k).unwrap_or(0);
        let mut full = kinds.map(|k| self.memory.signs_full(&dim, at.x, at.z, k));
        let (bx, bz) = (at.x * 16, at.z * 16);
        for (si, section) in column.sections.iter().enumerate() {
            if full.iter().all(|f| *f) {
                return;
            }
            if !section.blocks.may_contain(memory::is_sign) {
                continue;
            }
            let by = column.min_y + 16 * si as i32;
            for i in 0..4096usize {
                let Some(state) = section.blocks.get(i) else {
                    continue;
                };
                let Some(kind) =
                    data::block_of_state(state).filter(|k| memory::sign_of(*k).is_some())
                else {
                    continue;
                };
                if full[ix(kind)] {
                    continue;
                }
                let (lx, lz, ly) = ((i & 15) as i32, ((i >> 4) & 15) as i32, (i >> 8) as i32);
                let cell = (bx + lx, by + ly, bz + lz);
                let neighbours = memory::neighbours(cell).map(|(x, y, z)| {
                    let (nx, ny, nz) = (x - bx, y - by, z - bz);
                    if (0..16).contains(&nx) && (0..16).contains(&ny) && (0..16).contains(&nz) {
                        section.blocks.get(protocol::chunk::block_index(nx, ny, nz))
                    } else {
                        self.state_at(BlockPos::new(x, y, z))
                    }
                });
                if let Some(seen) = Seen::check(&dim, cell, state, neighbours)
                    && self.memory.record_sign(&seen)
                {
                    full[ix(kind)] = self.memory.signs_full(&dim, at.x, at.z, kind);
                }
            }
        }
    }

    /// A changed cell and its six neighbours: a change can expose a neighbour.
    fn observe_around(&mut self, pos: BlockPos) {
        self.observe(pos);
        self.observe_source(pos);
        for (x, y, z) in memory::neighbours((pos.x, pos.y, pos.z)) {
            self.observe(BlockPos::new(x, y, z));
            self.observe_source(BlockPos::new(x, y, z));
        }
    }

    /// Keep or forget a fluid source at `pos` in memory's pools (issue 14b): kept only when
    /// `SeenSource::check` finds it exposed; a cell that holds no source now is forgotten.
    fn observe_source(&mut self, pos: BlockPos) {
        let Some(dim) = self.dimension.as_ref().map(|d| d.name.clone()) else {
            return;
        };
        let Some(state) = self.state_at(pos) else {
            return;
        };
        let cell = (pos.x, pos.y, pos.z);
        if !memory::fluid_source(state) {
            self.memory.forget_source(&dim, cell);
            return;
        }
        let neighbours =
            memory::neighbours(cell).map(|(x, y, z)| self.state_at(BlockPos::new(x, y, z)));
        if let Some(seen) = memory::SeenSource::check(&dim, cell, state, neighbours) {
            self.memory.record_source(seen);
        }
    }

    /// A column's fluid sources, top section first so a lake's surface fills its pool before
    /// the water under it is read; a pool full stops the scan (issue 14b).
    fn observe_column_sources(&mut self, at: ChunkPos, column: &Arc<protocol::chunk::Column>) {
        let (bx, bz) = (at.x * 16, at.z * 16);
        let Some(dim) = self.dimension.as_ref().map(|d| d.name.clone()) else {
            return;
        };
        for (si, section) in column.sections.iter().enumerate().rev() {
            if !section.blocks.may_contain(memory::fluid_source) {
                continue;
            }
            let by = column.min_y + 16 * si as i32;
            for i in (0..4096usize).rev() {
                let Some(fluid) = section.blocks.get(i).and_then(memory::Memory::source_fluid)
                else {
                    continue;
                };
                if self.memory.pool_full(&dim, at.x, at.z, fluid) {
                    continue;
                }
                {
                    let (lx, lz, ly) = ((i & 15) as i32, ((i >> 4) & 15) as i32, (i >> 8) as i32);
                    self.observe_source(BlockPos::new(bx + lx, by + ly, bz + lz));
                }
            }
        }
    }

    /// A column just arrived: its watched cells (sections tested by palette before any scan),
    /// the positions already on record in it, and the edge cells of loaded neighbouring columns,
    /// whose faces towards this column could not be judged before it loaded.
    fn observe_column(&mut self, at: ChunkPos) {
        let Some(column) = self.columns.get(&at).map(Arc::clone) else {
            return;
        };
        self.observe_column_sources(at, &column);
        self.observe_column_signs(at, &column);
        let (bx, bz) = (at.x * 16, at.z * 16);
        let mut cells = Vec::new();
        // >>> E1 (issue 20b): the End's kinds are watched in the End only.
        let here = self
            .dimension
            .as_ref()
            .map(|d| d.name.clone())
            .unwrap_or_default();
        let watched = |s: u32| memory::watched_in(&here, s);
        // <<< E1
        for (si, section) in column.sections.iter().enumerate() {
            if !section.blocks.may_contain(watched) {
                continue;
            }
            let by = column.min_y + 16 * si as i32;
            for i in 0..4096usize {
                if section.blocks.get(i).is_some_and(watched) {
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
                    if !section.blocks.may_contain(watched) {
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
                            if section.blocks.get(i).is_some_and(watched) {
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
                let (window, state_id) = self.current_window();
                self.click = Some(PendingClick {
                    window,
                    slot,
                    state_id,
                    at_tick: self.ticks,
                });
                self.clicks_sent += 1;
                // The click claims an empty cursor, and the server sends the cursor only when it
                // differs from the claim (`setRemoteCarried`, then `broadcastChanges`): so the
                // cursor is forgotten here and a `set_cursor_item` restores it when it holds
                // something (clojurecraft's `click` does the same).
                self.inventory.carried = None;
                vec![Effect::Send(C2s::ContainerClick {
                    window,
                    state_id,
                    slot,
                    button,
                    mode,
                })]
            }
            Act::CloseWindow => {
                // The client closes at once; the server sends nothing back.
                let (window, _) = self.current_window();
                self.closed_window = self.window.take();
                self.click = None;
                vec![Effect::Send(C2s::ContainerClose { window })]
            }
            Act::Attack { entity } => {
                if let Some(e) = self.entities.get_mut(&entity) {
                    e.struck = true;
                }
                // >>> E1 (issue 20b)
                self.attacks_sent += 1;
                self.last_attack = Some((self.session.server_ticks, entity));
                // <<< E1
                vec![Effect::Send(C2s::Attack { entity })]
            }
            Act::Respawn => {
                self.death = None;
                self.body = None;
                vec![Effect::Send(C2s::ClientCommand { action: 0 })]
            }
            Act::UseItem => {
                self.sequence_sent += 1;
                // The server snaps the player to this rotation before the use
                // (`handleUseItem`): a bucket goes along it, so it carries the look's pitch
                // (issue 14a parity found every real bucket use level at pitch 0).
                vec![Effect::Send(C2s::UseItem {
                    hand: 0,
                    sequence: self.sequence_sent,
                    yaw: self.controls.yaw as f32,
                    pitch: self.controls.pitch.unwrap_or(self.player.pitch),
                })]
            }
            Act::UseOffHand => {
                self.sequence_sent += 1;
                vec![Effect::Send(C2s::UseItem {
                    hand: 1,
                    sequence: self.sequence_sent,
                    yaw: self.controls.yaw as f32,
                    pitch: self.controls.pitch.unwrap_or(self.player.pitch),
                })]
            }
            Act::ReleaseUse => vec![Effect::Send(C2s::PlayerAction {
                status: 5,
                pos: (0, 0, 0),
                face: 0,
                sequence: 0,
            })],
            Act::UseOn { pos, face } => {
                self.sequence_sent += 1;
                // The cursor at the middle of the face clicked.
                let cursor = match face {
                    0 => [0.5, 0.0, 0.5],
                    1 => [0.5, 1.0, 0.5],
                    2 => [0.5, 0.5, 0.0],
                    3 => [0.5, 0.5, 1.0],
                    4 => [0.0, 0.5, 0.5],
                    _ => [1.0, 0.5, 0.5],
                };
                vec![Effect::Send(C2s::UseItemOn {
                    hand: 0,
                    pos: (pos.x, pos.y, pos.z),
                    face: i32::from(face),
                    cursor,
                    inside: false,
                    sequence: self.sequence_sent,
                })]
            }
            Act::Swing => vec![Effect::Send(C2s::Swing { hand: 0 })],
            Act::Resync => {
                let (window, state_id) = self.current_window();
                self.click = Some(PendingClick {
                    window,
                    slot: -1,
                    state_id,
                    at_tick: self.ticks,
                });
                vec![Effect::Send(C2s::ContainerClick {
                    window,
                    state_id: -1,
                    slot: -1,
                    button: 0,
                    mode: 0,
                })]
            }
        }
    }

    /// The open window's id and last state id, or window 0's.
    fn current_window(&self) -> (i32, i32) {
        self.window
            .as_ref()
            .map_or((0, self.inventory.state_id), |w| (w.id, w.state_id))
    }

    /// The server answered the pending click: its window's state id moved.
    fn window_moved(&mut self, window: i32, state_id: i32) {
        if let Some(c) = self.click
            && c.window == window
            && c.state_id != state_id
        {
            self.click = None;
            self.clicks_answered += 1;
        }
    }

    /// A line of chat or screen text, kept with the newest `SOUNDS_KEPT`.
    fn say(&mut self, via: &str, text: &str) {
        self.chat.push(ChatLine {
            at_tick: self.session.server_ticks,
            via: via.to_owned(),
            text: text.to_owned(),
        });
        let over = self.chat.len().saturating_sub(SOUNDS_KEPT);
        self.chat.drain(..over);
    }

    /// A sound heard, kept with the newest `SOUNDS_KEPT`.
    fn hear(&mut self, h: Heard) {
        self.sounds.push(h);
        let over = self.sounds.len().saturating_sub(SOUNDS_KEPT);
        self.sounds.drain(..over);
    }

    /// Counts an entity-data read that stopped early, by its serializer.
    fn count_unread(&mut self, entries: &[(u8, MetaValue)]) {
        if let Some(s) = protocol::metadata::unread(entries) {
            *self.metadata_unread.entry(s.to_owned()).or_default() += 1;
        }
    }

    fn fold(&mut self, p: &S2c) {
        match p {
            S2c::Login {
                entity_id,
                dimension_type,
                dimension,
                view_distance,
                simulation_distance,
                ..
            } => {
                self.player.entity_id = Some(*entity_id);
                self.enter(*dimension_type, dimension);
                self.simulation_distance = Some(*simulation_distance);
                self.view_radius = Some(*view_distance);
            }
            S2c::Respawn {
                dimension_type,
                dimension,
            } => {
                self.enter(*dimension_type, dimension);
                // A respawn makes the client a new player whose last sent keys are none
                // (`LocalPlayer`'s `lastSentInput` starts empty), and after a death the server's
                // new player holds no shift key: keys still held are reported again.
                self.sent_input = 0;
            }
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
                id,
                uuid,
                kind,
                x,
                y,
                z,
                velocity: spawn,
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
                            dead: false,
                            struck: false,
                            uuid: Some(*uuid),
                            velocity: None,
                            statuses: Vec::new(),
                            hurts: Vec::new(),
                            head_yaw: None,
                            yaw: None,
                            equipment: BTreeMap::new(),
                            swung_at: None,
                            power: None,
                            spawn_velocity: spawn.iter().all(|v| v.is_finite()).then_some(*spawn),
                            passengers: Vec::new(),
                            effects: BTreeMap::new(),
                            attributes: BTreeMap::new(),
                        },
                    );
                }
            }
            S2c::RemoveEntities { ids } => {
                for id in ids {
                    if let Some(e) = self.entities.remove(id)
                        && e.kind == EntityKind::EndCrystal
                    {
                        self.crystal_removed(e.pos);
                    }
                }
            }
            S2c::MoveEntity {
                id,
                dx,
                dy,
                dz,
                yaw,
            } => {
                let tick = self.session.server_ticks;
                match self.entities.get_mut(id) {
                    Some(e) => {
                        let moved = [e.pos[0] + dx, e.pos[1] + dy, e.pos[2] + dz];
                        if moved.iter().all(|v| v.is_finite()) {
                            e.pos = moved;
                        }
                        if let Some(y) = yaw {
                            e.yaw = Some(*y);
                        }
                        e.last_seen = tick;
                    }
                    None => self.unknown_entity_updates += 1,
                }
            }
            S2c::MoveEntityRot { id, yaw } => {
                let tick = self.session.server_ticks;
                match self.entities.get_mut(id) {
                    Some(e) => {
                        e.yaw = Some(*yaw);
                        e.last_seen = tick;
                    }
                    None => self.unknown_entity_updates += 1,
                }
            }
            S2c::EntityPositionSync { id, x, y, z } => self.place_entity(*id, [*x, *y, *z], 0),
            S2c::TeleportEntity {
                id,
                x,
                y,
                z,
                flags,
                yaw,
            } => {
                self.place_entity(*id, [*x, *y, *z], *flags);
                if let Some(e) = self.entities.get_mut(id)
                    && yaw.is_finite()
                {
                    // Flag bit 3: the yaw is relative (`PositionMoveRotation.calculateAbsolute`).
                    e.yaw = Some(if flags & (1 << 3) != 0 {
                        e.yaw.unwrap_or(0.0) + yaw
                    } else {
                        *yaw
                    });
                }
            }
            S2c::SetEntityData { id, entries } if Some(*id) == self.player.entity_id => {
                self.count_unread(entries);
                for (index, value) in entries {
                    match (index, value) {
                        (1, MetaValue::Int(air)) => self.player.air = Some(*air),
                        (0, MetaValue::Byte(flags)) => self.player.on_fire = flags & 1 == 1,
                        _ => {}
                    }
                }
            }
            S2c::SetEntityData { id, entries } => {
                self.count_unread(entries);
                let tick = self.session.server_ticks;
                match self.entities.get_mut(id) {
                    Some(e) => {
                        // An `Unread` entry marks where the read stopped; the value it would
                        // replace is kept rather than lost.
                        for (index, value) in entries {
                            if !matches!(value, MetaValue::Unread(_)) {
                                e.metadata.insert(*index, value.clone());
                            }
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
                if let Some(w) = self.window.as_mut().filter(|w| w.id == *window) {
                    w.state_id = *state_id;
                    w.size = i16::try_from(slots.len()).unwrap_or(0);
                    w.slots.clear();
                    let mut mirror = Vec::new();
                    for (i, s) in slots.iter().enumerate() {
                        let Ok(i) = i16::try_from(i) else { continue };
                        if let Some(stack) = Inventory::stack(*s) {
                            w.slots.insert(i, stack);
                        }
                        if let Some(inv) = w.inventory_slot(i) {
                            mirror.push((inv, *s));
                        }
                    }
                    for (inv, s) in mirror {
                        self.inventory.put(inv, s);
                    }
                    self.inventory.carried = carried.and_then(Inventory::stack);
                    self.window_moved(*window, *state_id);
                } else if *window == 0 {
                    self.window_moved(0, *state_id);
                    self.inventory.state_id = *state_id;
                    self.inventory.clear();
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
                if let Some(w) = self.window.as_mut().filter(|w| w.id == *window) {
                    w.state_id = *state_id;
                    match Inventory::stack(*item) {
                        Some(stack) => w.slots.insert(*slot, stack),
                        None => w.slots.remove(slot),
                    };
                    let inv = w.inventory_slot(*slot);
                    if let Some(inv) = inv {
                        self.inventory.put(inv, *item);
                    }
                    self.window_moved(*window, *state_id);
                } else if *window == 0 {
                    self.window_moved(0, *state_id);
                    self.inventory.state_id = *state_id;
                    self.inventory.put(*slot, *item);
                } else if let Some(inv) = self
                    .closed_window
                    .as_ref()
                    .filter(|w| w.id == *window)
                    .and_then(|w| w.inventory_slot(*slot))
                {
                    self.inventory.put(inv, *item);
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
            S2c::ContainerClose { window } => {
                if self.window.as_ref().is_some_and(|w| w.id == *window) {
                    self.window = None;
                    self.click = None;
                }
            }
            S2c::SetEntityMotion { id, velocity } => {
                // Knockback on the bot: the server sets its velocity (the client's
                // `handleSetEntityMotion` sets `deltaMovement`).
                if Some(*id) == self.player.entity_id
                    && let Some(b) = self.body.as_mut()
                    && velocity.iter().all(|v| v.is_finite())
                {
                    b.vel = *velocity;
                } else if let Some(e) = self.entities.get_mut(id)
                    && velocity.iter().all(|v| v.is_finite())
                {
                    // Any other entity's velocity is kept: a fireball's heading (issue 16e).
                    e.velocity = Some(*velocity);
                }
            }
            S2c::EntityEvent { id, status } => {
                let tick = self.session.server_ticks;
                if let Some(e) = self.entities.get_mut(id) {
                    e.statuses.push((tick, *status));
                    let over = e.statuses.len().saturating_sub(ENTITY_STATUSES_KEPT);
                    e.statuses.drain(..over);
                }
                if Some(*id) == self.player.entity_id {
                    if *status == 9 {
                        self.player.uses_finished += 1;
                    }
                } else if *status == 3
                    && let Some(e) = self.entities.get_mut(id)
                {
                    if !e.dead && e.struck {
                        self.kills += 1;
                    }
                    e.dead = true;
                    // >>> E1 (issue 20b)
                    if e.kind == EntityKind::EnderDragon && self.dragon_dead.is_none() {
                        self.dragon_dead = Some(self.session.server_ticks);
                    }
                    // <<< E1
                }
            }
            S2c::OpenScreen { window, menu } => {
                self.window = Some(Window {
                    id: *window,
                    menu: *menu,
                    ..Window::default()
                });
                self.click = None;
            }
            S2c::ContainerSetData {
                window,
                property,
                value,
            } => {
                if let Some(wd) = self.window.as_mut().filter(|wd| wd.id == *window) {
                    wd.properties.insert(*property, *value);
                }
            }
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
                } else {
                    // Damage on another entity was dropped before issue 16e: it is how a swing
                    // is known to land, and what wears a blaze down.
                    let tick = self.session.server_ticks;
                    let by_bot = self.player.entity_id.is_some()
                        && (*cause == self.player.entity_id || *direct == self.player.entity_id);
                    let damage_type = usize::try_from(*source_type)
                        .ok()
                        .and_then(|i| data::generated::registries::DAMAGE_TYPE.get(i))
                        .map_or_else(|| format!("unknown:{source_type}"), |n| (*n).to_owned());
                    if let Some(e) = self.entities.get_mut(entity_id) {
                        e.hurts.push(Hurt {
                            at_tick: tick,
                            damage_type,
                            by_bot,
                            cause: (*cause).or(*direct), // pumpkin (issue 80)
                        });
                        let over = e.hurts.len().saturating_sub(ENTITY_HURTS_KEPT);
                        e.hurts.drain(..over);
                    }
                }
            }
            S2c::Sound {
                sound, pos, volume, ..
            } => {
                let tick = self.session.server_ticks;
                self.hear(Heard {
                    at_tick: tick,
                    name: sound.name(),
                    pos: Some(*pos),
                    entity: None,
                    volume: *volume,
                });
            }
            S2c::SoundEntity {
                sound,
                entity,
                volume,
                ..
            } => {
                let tick = self.session.server_ticks;
                let pos = self.entities.get(entity).map(|e| e.pos);
                self.hear(Heard {
                    at_tick: tick,
                    name: sound.name(),
                    pos,
                    entity: Some(*entity),
                    volume: *volume,
                });
            }
            S2c::LevelEvent {
                event, pos, data, ..
            } => {
                let tick = self.session.server_ticks;
                self.level_events.push(LevelEventSeen {
                    at_tick: tick,
                    id: *event,
                    name: data::generated::events::level_event_name(*event).map(str::to_owned),
                    pos: *pos,
                    data: *data,
                });
                let over = self.level_events.len().saturating_sub(SOUNDS_KEPT);
                self.level_events.drain(..over);
            }
            // A sound stopped: nothing the bot keeps is playing (sounds are instants here).
            S2c::StopSound { .. } => {}
            S2c::RotateHead { id, yaw } => {
                if let Some(e) = self.entities.get_mut(id) {
                    e.head_yaw = Some(*yaw);
                }
            }
            S2c::SetEquipment { id, items } => {
                if let Some(e) = self.entities.get_mut(id) {
                    for (slot, item) in items {
                        match item.stack() {
                            Some((i, n)) if n > 0 => {
                                e.equipment.insert(*slot, ItemKind::from_id(i));
                            }
                            Some(_) | None => {
                                e.equipment.remove(slot);
                            }
                        }
                    }
                }
            }
            S2c::Animate { id, action } => {
                let tick = self.session.server_ticks;
                if *action == 0
                    && let Some(e) = self.entities.get_mut(id)
                {
                    e.swung_at = Some(tick);
                }
            }
            S2c::HurtAnimation { id, yaw } => {
                if Some(*id) == self.player.entity_id {
                    self.hurt_from = Some((self.session.server_ticks, *yaw));
                }
            }
            S2c::ProjectilePower { id, power } => {
                if let Some(e) = self.entities.get_mut(id) {
                    e.power = Some(*power);
                }
            }
            S2c::SetPassengers {
                vehicle,
                passengers,
            } => {
                if let Some(e) = self.entities.get_mut(vehicle) {
                    e.passengers.clone_from(passengers);
                }
            }
            S2c::Explode { center, knockback } => {
                self.explosions.push(Blast {
                    at_tick: self.session.server_ticks,
                    center: *center,
                    knockback: *knockback,
                });
                let over = self.explosions.len().saturating_sub(SOUNDS_KEPT);
                self.explosions.drain(..over);
                self.crystal_blast(*center);
            }
            S2c::BlockEntityData { pos, spawns, .. } => {
                if let Some(m) = spawns {
                    self.spawner_mobs.insert(*pos, m.clone());
                }
            }
            S2c::UpdateMobEffect {
                id,
                effect,
                amplifier,
                duration,
            } => {
                let name =
                    registry_name(data::generated::registries::MOB_EFFECT.as_slice(), *effect);
                if Some(*id) == self.player.entity_id {
                    // A duration of -1 is infinite.
                    let until = u64::try_from(*duration)
                        .map_or(u64::MAX, |d| self.session.server_ticks.saturating_add(d));
                    self.player.effects.insert(name, (*amplifier, until));
                } else if let Some(e) = self.entities.get_mut(id) {
                    e.effects.insert(name, *amplifier);
                }
            }
            S2c::RemoveMobEffect { id, effect } => {
                let name =
                    registry_name(data::generated::registries::MOB_EFFECT.as_slice(), *effect);
                if Some(*id) == self.player.entity_id {
                    self.player.effects.remove(&name);
                } else if let Some(e) = self.entities.get_mut(id) {
                    e.effects.remove(&name);
                }
            }
            S2c::UpdateAttributes { id, attributes } => {
                let values = attributes.iter().map(|(a, base, add)| {
                    (
                        registry_name(data::generated::registries::ATTRIBUTE.as_slice(), *a),
                        base + add,
                    )
                });
                if Some(*id) == self.player.entity_id {
                    self.player.attributes.extend(values);
                } else if let Some(e) = self.entities.get_mut(id) {
                    e.attributes.extend(values);
                }
            }
            S2c::PlayerCombatEnter => self.player.in_combat = true,
            S2c::PlayerCombatEnd { .. } => self.player.in_combat = false,
            S2c::SetExperience { level, total, .. } => {
                self.player.experience = Some((*level, *total));
            }
            S2c::Cooldown { group, ticks } => {
                let group = group.trim_start_matches("minecraft:").to_owned();
                if *ticks > 0 {
                    let until = self.session.server_ticks + u64::try_from(*ticks).unwrap_or(0);
                    self.cooldowns.insert(group, until);
                } else {
                    self.cooldowns.remove(&group);
                }
            }
            S2c::GameEvent { event, param } => {
                let name = data::generated::events::game_event_name(i32::from(*event));
                if name == Some("win_game") {
                    self.won = true;
                }
                self.game_events.push((
                    self.session.server_ticks,
                    *event,
                    name.map(str::to_owned),
                    *param,
                ));
                let over = self.game_events.len().saturating_sub(SOUNDS_KEPT);
                self.game_events.drain(..over);
            }
            S2c::SystemChat { text, overlay } => {
                let via = if *overlay { "action_bar" } else { "system" };
                self.say(via, text);
            }
            S2c::ActionBar { text } => self.say("action_bar", text),
            S2c::Title { text } => self.say("title", text),
            S2c::Subtitle { text } => self.say("subtitle", text),
            S2c::BossEvent { uuid, op } => match op {
                protocol::BossOp::Add { name, progress } => {
                    self.bosses.insert(*uuid, (name.clone(), *progress));
                }
                protocol::BossOp::Remove => {
                    // >>> E1 (issue 20b): the dragon's bar goes when it is killed.
                    let in_end = self
                        .dimension
                        .as_ref()
                        .is_some_and(|d| d.name == "minecraft:the_end");
                    if in_end
                        && self.dragon_dead.is_none()
                        && self.bosses.get(uuid).is_some_and(|b| {
                            b.0.to_lowercase()
                                .replace(' ', "_")
                                .contains("ender_dragon")
                        })
                    {
                        self.dragon_dead = Some(self.session.server_ticks);
                    }
                    // <<< E1
                    self.bosses.remove(uuid);
                }
                protocol::BossOp::Progress(p) => {
                    if let Some(b) = self.bosses.get_mut(uuid) {
                        b.1 = *p;
                    }
                }
                protocol::BossOp::Name(n) => {
                    if let Some(b) = self.bosses.get_mut(uuid) {
                        b.0.clone_from(n);
                    }
                }
                protocol::BossOp::Style => {}
            },
            S2c::ChunksBiomes { chunks } => self.biome_updates += chunks.len() as u64,
            S2c::SetChunkCacheCenter { x, z } => self.chunk_center = Some((*x, *z)),
            S2c::SetChunkCacheRadius { radius } => self.view_radius = Some(*radius),
            S2c::SetSimulationDistance { distance } => self.simulation_distance = Some(*distance),
            S2c::BlockDestruction {
                breaker,
                pos,
                stage,
            } => {
                if (0..=9).contains(stage) {
                    self.cracks
                        .insert(*pos, (*breaker, *stage, self.session.server_ticks));
                } else {
                    self.cracks.remove(pos);
                }
            }
            S2c::SetTime { game_time, clocks } => {
                self.game_time = Some(*game_time);
                if let Some(c) = clocks.iter().find(|c| c.clock == protocol::OVERWORLD_CLOCK) {
                    self.day_clock = Some(DayClock {
                        total_ticks: c.total_ticks,
                        game_time: *game_time,
                        rate: c.rate,
                    });
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
            S2c::RegistryData { registry, entries } => {
                if registry == biome::BIOME_REGISTRY {
                    self.biome_registry.clone_from(entries);
                }
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

#[cfg(test)]
mod blaze_charge {
    use super::*;

    fn entity(kind: EntityKind, flag: Option<u8>) -> Entity {
        Entity {
            kind,
            pos: [0.0; 3],
            metadata: flag
                .map(|b| (16u8, MetaValue::Byte(b)))
                .into_iter()
                .collect(),
            first_seen: 0,
            last_seen: 0,
            dead: false,
            struck: false,
            uuid: None,
            velocity: None,
            statuses: Vec::new(),
            hurts: Vec::new(),
            head_yaw: None,
            yaw: None,
            equipment: BTreeMap::new(),
            swung_at: None,
            power: None,
            spawn_velocity: None,
            passengers: Vec::new(),
            effects: BTreeMap::new(),
            attributes: BTreeMap::new(),
        }
    }

    /// Issue 16: a blaze's charge flag is bit 0 of its metadata index 16; nothing else charges.
    #[test]
    fn the_charge_flag_is_bit_0_of_index_16_on_a_blaze() {
        assert!(entity(EntityKind::Blaze, Some(1)).blaze_charged());
        assert!(!entity(EntityKind::Blaze, Some(0)).blaze_charged());
        assert!(!entity(EntityKind::Blaze, None).blaze_charged());
        assert!(!entity(EntityKind::Zombie, Some(1)).blaze_charged());
    }
}

#[cfg(test)]
mod wear_tests {
    use super::*;
    use protocol::slot::Wear;

    fn set(w: &mut World, slot: i16, s: Slot) {
        w.inventory.put(slot, s);
    }

    /// Blaze wall 7: the uses left on a worn stone pickaxe come from its damage component, and
    /// an unworn one from the item's default; an item that does not wear has none.
    #[test]
    fn uses_left_reads_the_wear_and_the_default() {
        let mut w = World::new();
        let pick = ItemKind::StonePickaxe;
        set(
            &mut w,
            36,
            Slot::Item {
                item: pick.id(),
                count: 1,
                wear: Wear {
                    damage: Some(120),
                    max_damage: None,
                },
            },
        );
        assert_eq!(w.inventory.uses_left(36), Some(11));
        set(&mut w, 37, Slot::item(pick.id(), 1));
        assert_eq!(w.inventory.uses_left(37), Some(131));
        set(&mut w, 38, Slot::item(ItemKind::Cobblestone.id(), 64));
        assert_eq!(w.inventory.uses_left(38), None);
        // The slot emptied, its wear goes with it.
        set(&mut w, 36, Slot::Empty);
        assert!(!w.inventory.wear.contains_key(&36));
        assert_eq!(w.inventory.uses_left(36), None);
    }
}

#[cfg(test)]
mod entity_tests {
    use super::*;

    /// Issue 16e: a mob keeps its uuid and velocity, and damage on it is kept with whether the
    /// bot dealt it (before, damage on anything but the bot was dropped).
    #[test]
    fn a_mob_keeps_its_uuid_velocity_and_the_damage_on_it() {
        let mut w = World::new();
        w.player.entity_id = Some(1);
        w.fold(&S2c::AddEntity {
            id: 7,
            uuid: [9; 16],
            kind: EntityKind::Blaze.id(),
            x: 0.0,
            y: 64.0,
            z: 0.0,
            velocity: [0.0; 3],
        });
        w.fold(&S2c::SetEntityMotion {
            id: 7,
            velocity: [0.1, 0.0, -0.2],
        });
        w.fold(&S2c::DamageEvent {
            entity_id: 7,
            source_type: 0,
            cause: Some(1),
            direct: Some(1),
        });
        w.fold(&S2c::EntityEvent { id: 7, status: 2 });
        let e = &w.entities[&7];
        assert_eq!(e.uuid, Some([9; 16]));
        assert_eq!(e.velocity, Some([0.1, 0.0, -0.2]));
        assert_eq!(e.hurts.len(), 1);
        assert!(e.hurts[0].by_bot);
        assert_eq!(e.statuses, vec![(0, 2)]);
    }

    /// An entity-data read that stopped early is counted by serializer, and the value it would
    /// have replaced is kept.
    #[test]
    fn a_cut_short_entity_data_read_is_counted_and_keeps_the_old_value() {
        let mut w = World::new();
        w.fold(&S2c::AddEntity {
            id: 7,
            uuid: [0; 16],
            kind: EntityKind::Blaze.id(),
            x: 0.0,
            y: 64.0,
            z: 0.0,
            velocity: [0.0; 3],
        });
        w.fold(&S2c::SetEntityData {
            id: 7,
            entries: vec![(9, MetaValue::Float(20.0))],
        });
        w.fold(&S2c::SetEntityData {
            id: 7,
            entries: vec![(9, MetaValue::Unread("PARTICLE".to_owned()))],
        });
        assert_eq!(w.entities[&7].health(), Some(20.0));
        assert_eq!(w.metadata_unread.get("PARTICLE"), Some(&1));
    }
}

#[cfg(test)]
mod hearing_tests {
    use super::*;
    use protocol::SoundRef;

    /// Issue 16e: a blaze's shot is heard as a level event named from the game's table, and a
    /// sound on an entity is placed where the entity stands.
    #[test]
    fn the_world_keeps_what_it_hears_by_name() {
        let mut w = World::new();
        w.fold(&S2c::LevelEvent {
            event: 1018,
            pos: (1, 70, 2),
            data: 0,
            global: false,
        });
        assert_eq!(
            w.level_events.last().and_then(|e| e.name.as_deref()),
            Some("sound_blaze_fireball")
        );
        w.fold(&S2c::AddEntity {
            id: 5,
            uuid: [0; 16],
            kind: EntityKind::Enderman.id(),
            x: 3.0,
            y: 64.0,
            z: 4.0,
            velocity: [0.0; 3],
        });
        w.fold(&S2c::SoundEntity {
            sound: SoundRef::Inline("entity.enderman.stare".to_owned()),
            source: 5,
            entity: 5,
            volume: 2.5,
            pitch: 1.0,
        });
        let h = w.sounds.last().unwrap();
        assert_eq!(h.name, "entity.enderman.stare");
        assert_eq!(h.pos, Some([3.0, 64.0, 4.0]));
        for i in 0..(SOUNDS_KEPT + 5) {
            w.fold(&S2c::Sound {
                sound: SoundRef::Registry(u32::try_from(i).unwrap_or(0)),
                source: 0,
                pos: [0.0; 3],
                volume: 1.0,
                pitch: 1.0,
            });
        }
        assert_eq!(w.sounds.len(), SOUNDS_KEPT);
    }
}

#[cfg(test)]
mod sight_tests {
    use super::*;

    fn add(w: &mut World, id: i32, kind: EntityKind, pos: [f64; 3]) {
        w.fold(&S2c::AddEntity {
            id,
            uuid: [0; 16],
            kind: kind.id(),
            x: pos[0],
            y: pos[1],
            z: pos[2],
            velocity: [0.0; 3],
        });
    }

    /// Issue 16e: a spawner's mob comes from its block-entity data (blaze wall 4), a mob's gear
    /// and gaze are kept, and `faces` reads the game's yaw (0 toward +z, 90 toward -x).
    #[test]
    fn spawner_mob_gear_and_gaze_are_kept() {
        let mut w = World::new();
        w.fold(&S2c::BlockEntityData {
            pos: (10, 60, 10),
            kind: 9,
            spawns: Some("blaze".to_owned()),
        });
        assert_eq!(
            w.spawner_mobs.get(&(10, 60, 10)).map(String::as_str),
            Some("blaze")
        );
        add(&mut w, 4, EntityKind::WitherSkeleton, [0.0, 64.0, 0.0]);
        w.fold(&S2c::SetEquipment {
            id: 4,
            items: vec![(0, protocol::slot::Slot::item(ItemKind::StoneSword.id(), 1))],
        });
        assert_eq!(
            w.entities[&4].equipment.get(&0),
            Some(&ItemKind::StoneSword)
        );
        w.fold(&S2c::RotateHead { id: 4, yaw: 90.0 });
        let e = &w.entities[&4];
        assert_eq!(e.faces([-5.0, 64.0, 0.0], 12.0), Some(true));
        assert_eq!(e.faces([0.0, 64.0, 5.0], 12.0), Some(false));
        w.fold(&S2c::RotateHead { id: 4, yaw: 0.0 });
        assert_eq!(w.entities[&4].faces([0.0, 64.0, 5.0], 12.0), Some(true));
    }
}

#[cfg(test)]
mod body_tests {
    use super::*;
    use protocol::BossOp;

    /// Issue 16e: effects, attributes, chat, the boss bar and the win reach the world by name.
    #[test]
    fn effects_chat_bosses_and_the_win_are_kept() {
        let mut w = World::new();
        w.player.entity_id = Some(1);
        let wither = data::generated::registries::MOB_EFFECT
            .iter()
            .position(|n| *n == "wither")
            .and_then(|i| i32::try_from(i).ok())
            .unwrap();
        w.fold(&S2c::UpdateMobEffect {
            id: 1,
            effect: wither,
            amplifier: 0,
            duration: 200,
        });
        assert_eq!(w.player.effects.get("wither").map(|e| e.0), Some(0));
        let armor = data::generated::registries::ATTRIBUTE
            .iter()
            .position(|n| *n == "armor")
            .and_then(|i| i32::try_from(i).ok())
            .unwrap();
        w.fold(&S2c::UpdateAttributes {
            id: 1,
            attributes: vec![(armor, 0.0, 15.0)],
        });
        assert_eq!(w.player.attributes.get("armor"), Some(&15.0));
        w.fold(&S2c::SystemChat {
            text: "rs-l01-b00 was slain by Blaze".to_owned(),
            overlay: false,
        });
        assert_eq!(w.chat.last().map(|c| c.via.as_str()), Some("system"));
        w.fold(&S2c::BossEvent {
            uuid: [3; 16],
            op: BossOp::Add {
                name: "Ender Dragon".to_owned(),
                progress: 1.0,
            },
        });
        w.fold(&S2c::BossEvent {
            uuid: [3; 16],
            op: BossOp::Progress(0.5),
        });
        assert_eq!(w.bosses.get(&[3; 16]).map(|b| b.1), Some(0.5));
        w.fold(&S2c::GameEvent {
            event: 4,
            param: 1.0,
        });
        assert!(w.won);
        w.fold(&S2c::SetSimulationDistance { distance: 3 });
        assert_eq!(w.simulation_distance, Some(3));
    }
}
