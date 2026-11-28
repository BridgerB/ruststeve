//! What the bot senses, as data (issue 16e, phase C). Pure: the bot keeps a [`Snapshot`] of the
//! world each tick, [`diff`] turns two of them into the [`Fact`]s of the ledger (health, food, a
//! hit and what dealt it, items and tools worn or broken, sounds, mobs appearing, turning angry,
//! fusing, charging or facing the bot), and [`senses`] gives one [`Senses`] row a second (with the
//! feet's cell, what it stands on and any lava or water against the body). Nothing
//! here reads anything the bot was not sent: no X-ray, and no server truth.

#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::BTreeMap;

use data::generated::mobs::{MobCategory, mob};
use data::{BlockKind, EntityKind};
use protocol::metadata::MetaValue;
use serde::{Deserialize, Serialize};
use world::{BlockPos, World};

/// How far a mob is watched for the ledger (blocks): clojurecraft's ledger notes each within 32.
pub const WATCH: f64 = 32.0;
/// A watched mob is gone only past `WATCH` and this margin, so one at the edge does not flicker
/// in and out (a bat at 32 blocks filled the first re-read ledger, issue 16e).
pub const WATCH_MARGIN: f64 = 4.0;
/// Hostiles in a senses row, nearest first (clojurecraft's six).
pub const HOSTILES: usize = 6;
/// A sound or level event within this is in the ledger; farther, only the loud kinds.
pub const HEAR: f64 = 16.0;
/// A tool's wear is noted as it falls under each share of its uses (half, a quarter, a tenth).
pub const WEAR_MARKS: [f64; 3] = [0.5, 0.25, 0.1];
/// Degrees off the line to the bot that count as facing it (clojurecraft's stare, 12).
pub const FACING: f64 = 12.0;
/// Client ticks a click is waited on before the holdings are read again: until the server
/// answers, the stack the click moved is in no slot and not on the cursor (the world forgets the
/// cursor as it clicks), so every count would dip and come back. A third of the planner's
/// patience with a click (`plan::craft::CLICK_PATIENCE_TICKS`, 60): an answer takes two or three.
pub const CLICK_SETTLE: u64 = 20;
/// Client ticks a damage event stands for a fall in health that follows it: the server sends the
/// event as the blow lands and the health at the end of that tick or the next, so the two can
/// come a tick or two apart. Half a second; past it a fall in health has no event of its own.
pub const HIT_WINDOW: u64 = 10;
/// Uses left at which a tool gone from its slot is said to have broken: its last use (issue 16,
/// observability gaps wall 2: under 4 was called a break, and a moved or dropped tool read so).
pub const BREAK_AT: i32 = 1;

// Entity-data indexes read here, from the game's class layout on 26.1.2 (Entity 0 to 7,
// LivingEntity 8 to 14, Mob 15, then each class's own): the creeper's swell direction and
// ignited flag, the blaze's charge, the enderman's creepy flag, the ghast's charging. Issue 16e
// plans to generate these from the jar; until then each is checked by a test on recorded bytes.
const CREEPER_SWELL: u8 = 16;
const CREEPER_IGNITED: u8 = 18;
const ENDERMAN_CREEPY: u8 = 17;
const GHAST_CHARGING: u8 = 16;

/// What a mob shows, as the ledger tracks it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Signs {
    /// A creeper swelling or ignited.
    pub fuse: bool,
    /// A blaze charging its volley, a ghast charging its shot.
    pub charge: bool,
    /// An enderman with a target (creepy).
    pub angry: bool,
    /// Its head points at the bot.
    pub facing: bool,
}

/// One mob in a snapshot.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Seen {
    pub kind: EntityKind,
    pub d: f64,
    pub signs: Signs,
    /// It has come within `WATCH` since it was first seen (its appearance is in the ledger); it
    /// stays so until it leaves `WATCH` and the margin.
    #[serde(default)]
    pub announced: bool,
}

/// A projectile near the bot: its kind and the closest it has come.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Flying {
    pub kind: EntityKind,
    pub closest: f64,
}

/// Projectiles watched (blocks): clojurecraft's senses list those within 16.
pub const PROJECTILE_WATCH: f64 = 16.0;

/// Endermen watched (blocks): the senses row's count was always within 128, the server's
/// despawn distance for monsters (`MobCategory.MONSTER`), so a farther one is gone soon.
pub const ENDERMAN_WATCH: f64 = 128.0;
/// Server ticks a ticking enderman may stand still before the ledger says so: a frozen mob
/// sends no movement, a ticking one strolls (`EnderMan` has a random stroll goal). A
/// disagreement is a note, never a decision (issue 17b); 30 s is long for an idle enderman.
pub const STILL_TICKS: u64 = 600;

/// An enderman within `ENDERMAN_WATCH`: whether it ticks (`world::ticks`), where it was, and
/// since when it has stood still while ticking (carried from tick to tick by `diff`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct EnderSeen {
    pub ticking: bool,
    pub pos: [f64; 3],
    pub still_since: u64,
    pub still_noted: bool,
    /// Its creepy flag (data index 17, issue 18b): it has a target.
    #[serde(default)]
    pub angry: bool,
    /// Its health (data index 9), when sent: a fall is a blow on it.
    #[serde(default)]
    pub health: Option<f32>,
    /// The bot has sent it an `attack` (`world::Entity::struck`): its death is the bot's kill.
    #[serde(default)]
    pub struck: bool,
    /// The server said it died (entity event 3).
    #[serde(default)]
    pub dead: bool,
}

/// Server ticks after an enderman's death in which a pearl item appearing within `PEARL_NEAR`
/// of it is its drop: the loot spawns in the tick of the death (`LivingEntity.die`), and its
/// item data may come a tick or two after its `add_entity`. Two seconds.
pub const KILL_LOOT_TICKS: u64 = 40;
/// Blocks from a death within which a pearl item is its drop (issue 18b): the drop spawns at
/// the body and scatters by at most a block or so; 8 leaves room for a knocked-back body.
pub const PEARL_NEAR: f64 = 8.0;

/// An enderman's death the ledger keeps for `KILL_LOOT_TICKS`: when, which and where.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Death {
    pub at: u64,
    pub id: i32,
    pub pos: [f64; 3],
}

/// Blocks from the bot within which a new eye of ender is its own throw (issue 19b): the eye
/// spawns at the thrower's eyes (`EnderEyeItem.use`), so 4 leaves room for a tick of flight.
pub const EYE_THROWN_NEAR: f64 = 4.0;
/// Eyes and eye items watched (blocks): an eye flies about 12 blocks before it ends (the jar,
/// probe 45685968956), so 64 holds every eye the bot threw and the item it drops.
pub const EYE_WATCH: f64 = 64.0;
/// Server ticks an ended eye waits for its dropped item (an `ender_eye` item within
/// `EYE_DROP_NEAR` of where it ended): the item is added in the tick the eye is removed, its
/// stack data a tick or two later. Half a second.
pub const EYE_DROP_TICKS: u64 = 10;
/// Blocks from an ended eye within which an `ender_eye` item is its drop (it spawns where the
/// eye was).
pub const EYE_DROP_NEAR: f64 = 2.0;
/// Blocks within which a silverfish appearing is noted as woken (issue 19b): a spawner wakes for
/// a player within 16, and an infested block broken releases one beside the bot.
pub const SILVERFISH_NEAR: f64 = 16.0;

/// An eye of ender in flight, watched from its first tick (issue 19b).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct EyeTrack {
    /// It appeared within `EYE_THROWN_NEAR` of the bot: the bot's own throw.
    pub own: bool,
    pub first: [f64; 3],
    pub last: [f64; 3],
    pub first_at: u64,
    /// Ticks it was seen in.
    pub samples: u32,
}

/// An eye ended (gone from the entities), waiting `EYE_DROP_TICKS` for its dropped item.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct EyeGone {
    pub id: i32,
    pub at: u64,
    pub pos: [f64; 3],
}

/// The horizontal heading from `a` to `b` in the game's yaw degrees (0 south, +z; 90 west,
/// -x), `None` for no horizontal move.
pub fn heading(a: [f64; 3], b: [f64; 3]) -> Option<f64> {
    let (dx, dz) = (b[0] - a[0], b[2] - a[2]);
    (dx.hypot(dz) > 1e-6).then(|| (-dx).atan2(dz).to_degrees())
}

/// The bridge blocks the planner spends (`plan::path::BRIDGE_BLOCKS`; a test in `results`
/// holds the two lists equal): the senses row's filler.
pub const FILLER: &[data::ItemKind] = &[
    data::ItemKind::Cobblestone,
    data::ItemKind::CobbledDeepslate,
    data::ItemKind::Netherrack,
    data::ItemKind::Granite,
    data::ItemKind::Diorite,
    data::ItemKind::Andesite,
    data::ItemKind::Tuff,
    data::ItemKind::Dirt,
    // The deltas' walls, bridge blocks since issue 19d (plan::path::BRIDGE_BLOCKS).
    data::ItemKind::Basalt,
    data::ItemKind::Blackstone,
];

/// The filler held, every kind together.
pub fn filler(w: &World) -> i64 {
    w.inventory
        .slots
        .values()
        .filter(|(k, _)| FILLER.contains(k))
        .map(|(_, n)| i64::from(*n))
        .sum()
}

/// Uses left over every pickaxe held (issue 17: the trip's scarce resource).
pub fn pickaxe_uses(w: &World) -> i32 {
    let picks = data::generated::tags::item_tag("pickaxes").unwrap_or(&[]);
    w.inventory
        .slots
        .iter()
        .filter(|(_, (k, _))| picks.contains(k))
        .filter_map(|(slot, _)| w.inventory.uses_left(*slot))
        .sum()
}

/// Endermen within `ENDERMAN_WATCH` of `here`, by id, the dead among them until the server
/// removes them (a death is a fact).
fn endermen_near(w: &World, here: [f64; 3], at: u64) -> BTreeMap<i32, EnderSeen> {
    w.entities
        .iter()
        .filter(|(_, e)| e.kind == EntityKind::Enderman)
        .filter(|(_, e)| dist(e.pos, here) <= ENDERMAN_WATCH)
        .map(|(id, e)| {
            (
                *id,
                EnderSeen {
                    ticking: world::ticks(w, e.pos),
                    pos: e.pos,
                    still_since: at,
                    still_noted: false,
                    angry: meta_bool(e, ENDERMAN_CREEPY),
                    health: e.health(),
                    struck: e.struck,
                    dead: e.dead,
                },
            )
        })
        .collect()
}

/// Ender pearl item entities within `ENDERMAN_WATCH` of `here`, by id, with where they lie.
fn pearls_near(w: &World, here: [f64; 3]) -> BTreeMap<i32, [f64; 3]> {
    w.entities
        .iter()
        .filter(|(_, e)| e.kind == EntityKind::Item && dist(e.pos, here) <= ENDERMAN_WATCH)
        .filter(|(_, e)| {
            e.item()
                .is_some_and(|(k, _)| k == data::ItemKind::EnderPearl)
        })
        .map(|(id, e)| (*id, e.pos))
        .collect()
}

/// Uses left over every sword held (issue 18: the sword breaks before the twelfth pearl).
pub fn sword_uses(w: &World) -> i32 {
    let swords = data::generated::tags::item_tag("swords").unwrap_or(&[]);
    w.inventory
        .slots
        .iter()
        .filter(|(_, (k, _))| swords.contains(k))
        .filter_map(|(slot, _)| w.inventory.uses_left(*slot))
        .sum()
}

fn projectile(kind: EntityKind) -> bool {
    matches!(
        kind,
        EntityKind::SmallFireball
            | EntityKind::Fireball
            | EntityKind::Arrow
            | EntityKind::WitherSkull
            | EntityKind::DragonFireball
    )
}

// >>> E1 (issue 20b): the End.

/// The dragon's phase (EnderDragon.DATA_PHASE, an INT after Mob's 15): ids from EnderDragonPhase
/// in the 26.1.2 jar (probe 45863431767): 0 holding, 1 strafe, 2 landing approach, 3 landing,
/// 4 takeoff, 5 to 7 sitting, 8 charging, 9 dying, 10 hover.
pub const DRAGON_PHASE: u8 = 16;
/// LivingEntity's health (a FLOAT).
pub const HEALTH: u8 = 9;
/// AreaEffectCloud's radius (a FLOAT after Entity's 0 to 7).
pub const CLOUD_RADIUS: u8 = 8;
/// The dragon's max health (EnderDragon attributes, 200).
pub const DRAGON_MAX: f32 = 200.0;
/// The dragon's parts after its own id, in `getSubEntities` order (EnderDragon.setId gives
/// part i the id + i + 1): what a strike on a part's id names.
pub const DRAGON_PARTS: [&str; 8] = [
    "head", "neck", "body", "tail", "tail", "tail", "wing", "wing",
];
/// A breath or fireball cloud within this of the feet (beyond its radius) is told.
pub const BREATH_WATCH: f64 = 16.0;
/// Feet over the ground (the lowest top of the columns two out) from which a climb is a pillar.
pub const PILLAR_MIN: i32 = 3;
/// Ticks a pillar's top holds before `PillarUp` is told (the climb stopped).
pub const PILLAR_SETTLE: u64 = 40;
/// How far down the ground is looked for (deeper is the void).
pub const GROUND_SCAN: i32 = 64;
/// A strike is the reason a crystal went if the bot's attack on it was this recent (ticks).
pub const STRIKE_TICKS: u64 = 20;
/// A blast within this of a crystal's place, within `STRIKE_TICKS`, is what took it.
pub const BLAST_NEAR: f64 = 2.0;
/// A crystal removed farther than this went out of view, not out of the world (the tracking
/// distance, 112).
pub const CRYSTAL_VIEW: f64 = 100.0;
/// The stare's tolerance (EnderMan.isBeingStaredBy: 0.025, scaled by distance) and the near
/// miss's: a look within this many tolerances of an enderman's eyes is told.
pub const STARE_TOLERANCE: f64 = 0.025;
pub const NEAR_MISS: f64 = 4.0;
/// Endermen looked at within this (EnderMan's look-for-player range, 64).
pub const GAZE_RANGE: f64 = 64.0;
/// An enderman's eye over its feet (EntityType.ENDERMAN eye height 2.55).
pub const ENDERMAN_EYE_Y: f64 = 2.55;

/// A climb in the End: the most the feet have been over the ground, when, where, and whether
/// `PillarUp` was told.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Climb {
    pub top: i32,
    pub top_at: u64,
    pub cell: [i32; 3],
    pub told: bool,
}

/// A breath or fireball cloud watched, and the nearest the feet came to its edge.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Cloud {
    pub radius: f32,
    pub closest: f64,
}

fn meta_int(e: &world::Entity, i: u8) -> Option<i32> {
    match e.metadata.get(&i) {
        Some(MetaValue::Int(v)) => Some(*v),
        Some(_) | None => None,
    }
}

fn meta_float(e: &world::Entity, i: u8) -> Option<f32> {
    match e.metadata.get(&i) {
        Some(MetaValue::Float(v)) => Some(*v),
        Some(_) | None => None,
    }
}

/// The dragon the world holds alive, with its id.
pub fn dragon(w: &World) -> Option<(i32, &world::Entity)> {
    w.entities
        .iter()
        .filter(|(_, e)| e.kind == EntityKind::EnderDragon)
        .min_by_key(|(id, e)| (e.dead, **id))
        .map(|(id, e)| (*id, e))
}

/// The dragon's health: its own entity data, else its boss bar's share of 200.
pub fn dragon_health(w: &World) -> Option<f32> {
    dragon(w)
        .and_then(|(_, e)| meta_float(e, HEALTH))
        .or_else(|| {
            w.bosses
                .values()
                .find(|(n, _)| n.to_lowercase().replace(' ', "_").contains("ender_dragon"))
                .map(|(_, p)| p * DRAGON_MAX)
        })
}

/// The dragon's phase id, when its entity data has come.
pub fn dragon_phase(w: &World) -> Option<i32> {
    dragon(w).and_then(|(_, e)| meta_int(e, DRAGON_PHASE))
}

/// The feet over the ground: the lowest top among the eight columns two out from the feet's
/// (axes and diagonals), each looked down for `GROUND_SCAN`; `None` when every one is void or
/// unloaded (on the bridge, over the void).
pub fn over_ground(w: &World) -> Option<i32> {
    let b = feet(w)?;
    let (x, y, z) = (
        b[0].floor() as i32,
        b[1].floor() as i32,
        b[2].floor() as i32,
    );
    [
        (2, 0),
        (-2, 0),
        (0, 2),
        (0, -2),
        (2, 2),
        (2, -2),
        (-2, 2),
        (-2, -2),
    ]
    .iter()
    .filter_map(|(dx, dz)| {
        (1..=GROUND_SCAN).find_map(|k| match w.block_at(BlockPos::new(x + dx, y - k, z + dz)) {
            Some(BlockKind::Air | BlockKind::CaveAir | BlockKind::VoidAir) => None,
            Some(_) => Some(k - 1),
            None => None,
        })
    })
    .min()
}

/// End crystals in the world, by id, with their place.
fn crystals(w: &World) -> BTreeMap<i32, [f64; 3]> {
    w.entities
        .iter()
        .filter(|(_, e)| !e.dead && e.kind == EntityKind::EndCrystal)
        .map(|(id, e)| (*id, e.pos))
        .collect()
}

/// The frames on record within `world::stage::FRAME_NEAR` that hold an eye.
fn frames_with_eye(w: &World) -> Vec<[i32; 3]> {
    let Some(dim) = w.dimension.as_ref() else {
        return Vec::new();
    };
    if !w.memory.has_seen(BlockKind::EndPortalFrame) {
        return Vec::new();
    }
    world::stage::frames_near(w)
        .into_iter()
        .filter(|c| {
            w.memory
                .latest(&dim.name, *c)
                .is_some_and(|s| memory::frame_has_eye(s.state))
        })
        .map(|(x, y, z)| [x, y, z])
        .collect()
}

/// Endermen whose eyes the look sent last comes within `NEAR_MISS` tolerances of (a stare is
/// within one; line of sight is not tested here), by id, with the range and whether it is inside
/// the stare's own cone.
fn gazes(w: &World) -> BTreeMap<i32, (f64, bool)> {
    let (Some(f), Some((yaw, pitch))) = (feet(w), w.sent_look) else {
        return BTreeMap::new();
    };
    let eye = [f[0], f[1] + 1.62, f[2]];
    let (yr, pr) = (f64::from(yaw).to_radians(), f64::from(pitch).to_radians());
    let v = [-yr.sin() * pr.cos(), -pr.sin(), yr.cos() * pr.cos()];
    w.entities
        .iter()
        .filter(|(_, e)| !e.dead && e.kind == EntityKind::Enderman)
        .filter_map(|(id, e)| {
            let t = [e.pos[0], e.pos[1] + ENDERMAN_EYE_Y, e.pos[2]];
            let d = [t[0] - eye[0], t[1] - eye[1], t[2] - eye[2]];
            let len = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
            if len > GAZE_RANGE || len <= f64::EPSILON {
                return None;
            }
            let dot = (v[0] * d[0] + v[1] * d[1] + v[2] * d[2]) / len;
            (dot > 1.0 - NEAR_MISS * STARE_TOLERANCE / len)
                .then_some((*id, (len, dot > 1.0 - STARE_TOLERANCE / len)))
        })
        .collect()
}

/// The eye's distance to an entity's box: a crystal's 2 by 2 by 2, else its position.
fn reach_to(w: &World, e: &world::Entity) -> Option<f64> {
    reach_at(w, e.kind, e.pos)
}

/// `reach_to` for a kind at a place (a crystal the strike already removed).
fn reach_at(w: &World, kind: EntityKind, pos: [f64; 3]) -> Option<f64> {
    let f = feet(w)?;
    let eye = [f[0], f[1] + 1.62, f[2]];
    if kind == EntityKind::EndCrystal {
        let lo = [pos[0] - 1.0, pos[1], pos[2] - 1.0];
        let hi = [pos[0] + 1.0, pos[1] + 2.0, pos[2] + 1.0];
        return Some(
            (0..3)
                .map(|i| (eye[i] - eye[i].clamp(lo[i], hi[i])).powi(2))
                .sum::<f64>()
                .sqrt(),
        );
    }
    Some(dist(eye, pos))
}
// <<< E1

/// The parts of the world the ledger compares from tick to tick: cheap to take every tick.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Snapshot {
    pub at: u64,
    pub health: Option<f32>,
    pub food: Option<i32>,
    pub on_fire: bool,
    pub dimension: Option<String>,
    pub stage: Option<String>,
    pub died: bool,
    pub won: bool,
    /// Items held, by name, summed over slots and the cursor's stack.
    pub items: BTreeMap<String, i32>,
    /// The stack on the cursor (counted in `items`): a stack left there is in no slot the bot
    /// can use.
    #[serde(default)]
    pub cursor: Option<(String, i32)>,
    /// Client ticks since a click the server has not answered, when one waits.
    #[serde(default)]
    pub click_age: Option<u64>,
    /// The world's client tick.
    #[serde(default)]
    pub ticks: u64,
    /// The newest blow's server tick and how many blows carry it: what is new is told from this,
    /// not from the list's length (the list is pruned as it grows, so in a long fight its length
    /// stands still and every blow would go unseen).
    #[serde(default)]
    pub hit_mark: Option<(u64, usize)>,
    /// Damage events no fall in health has answered yet, and the client tick the newest came
    /// (carried from tick to tick by `diff`; they lapse after `HIT_WINDOW`).
    #[serde(default)]
    pub hits_unanswered: u32,
    #[serde(default)]
    pub hit_tick: u64,
    /// Each slot's wearing item: its name and uses left.
    pub tools: BTreeMap<i16, (String, i32)>,
    /// Monsters within `WATCH` and its margin, by entity id.
    pub mobs: BTreeMap<i32, Seen>,
    pub effects: Vec<String>,
    /// How many hits, sounds, level events and chat lines the world had taken in (their lists
    /// are capped, so the newest past these are new).
    /// Projectiles within `PROJECTILE_WATCH`, by id, with the closest each came (carried from
    /// tick to tick by `diff`).
    #[serde(default)]
    pub projectiles: BTreeMap<i32, Flying>,
    /// Blows the bot landed on each watched mob (damage events it dealt).
    #[serde(default)]
    pub landed: BTreeMap<i32, usize>,
    pub hits: usize,
    pub sounds: usize,
    pub level_events: usize,
    pub chat: usize,
    /// The time of day (`World::day_time`), when the server has told it.
    #[serde(default)]
    pub day_time: Option<i64>,
    /// The biome at the feet (`World::feet_biome`, issue 17b).
    #[serde(default)]
    pub biome: Option<String>,
    /// Endermen within `ENDERMAN_WATCH`, by id (carried by `diff`).
    #[serde(default)]
    pub endermen: BTreeMap<i32, EnderSeen>,
    /// Warped forests known from blocks seen (`Memory::forests`).
    #[serde(default)]
    pub forests: usize,
    /// Soul sand valleys known from blocks seen (`Memory::soul_valleys`).
    #[serde(default)]
    pub soul_valleys: usize,
    /// Ender pearl items on the ground within `ENDERMAN_WATCH`, by id (issue 18b).
    #[serde(default)]
    pub pearl_items: BTreeMap<i32, [f64; 3]>,
    /// Enderman deaths of the last `KILL_LOOT_TICKS` (carried by `diff`).
    #[serde(default)]
    pub deaths: Vec<Death>,
    /// Issue 19b: eyes of ender within `EYE_WATCH`, by id (carried by `diff`).
    #[serde(default)]
    pub eyes: BTreeMap<i32, EyeTrack>,
    /// Eyes ended in the last `EYE_DROP_TICKS` whose drop is not yet told (carried by `diff`).
    #[serde(default)]
    pub eyes_gone: Vec<EyeGone>,
    /// Ender eye items on the ground within `EYE_WATCH`, by id.
    #[serde(default)]
    pub eye_items: BTreeMap<i32, [f64; 3]>,
    /// End portal frames on record within `world::stage::FRAME_NEAR` (overworld).
    #[serde(default)]
    pub frames: Vec<[i32; 3]>,
    /// Silverfish within `SILVERFISH_NEAR`, by id.
    #[serde(default)]
    pub silverfish: Vec<i32>,
    // >>> E1 (issue 20b)
    /// Frames on record within `FRAME_NEAR` holding an eye.
    #[serde(default)]
    pub frames_eye: Vec<[i32; 3]>,
    /// End crystals alive, by id, and where.
    #[serde(default)]
    pub crystals: BTreeMap<i32, [f64; 3]>,
    /// `World::attacks_sent` and `World::last_attack`.
    #[serde(default)]
    pub attacks: u64,
    #[serde(default)]
    pub last_attack: Option<(u64, i32)>,
    /// The feet over the ground in the End (`over_ground`), and the climb (carried by `diff`).
    #[serde(default)]
    pub over: Option<i32>,
    #[serde(default)]
    pub climb: Option<Climb>,
    /// The dragon: its id, phase and health, and the blows the bot landed on it.
    #[serde(default)]
    pub dragon: Option<i32>,
    #[serde(default)]
    pub dragon_phase: Option<i32>,
    #[serde(default)]
    pub dragon_health: Option<f32>,
    #[serde(default)]
    pub dragon_landed: usize,
    /// `World::dragon_dead`.
    #[serde(default)]
    pub dragon_dead: Option<u64>,
    /// Breath and fireball clouds within `BREATH_WATCH`, by id (carried by `diff`).
    #[serde(default)]
    pub clouds: BTreeMap<i32, Cloud>,
    /// Endermen the look comes near (`gazes`), by id: range and inside the stare's cone.
    #[serde(default)]
    pub gazes: BTreeMap<i32, (f64, bool)>,
    // <<< E1
    /// Issue 56: the server's sprint flag for the bot (`world::Player::sprinting`), `None`
    /// before it said.
    #[serde(default)]
    pub sprint_server: Option<bool>,
    /// Issue 56: the sprint last reported to the server (`World::sent_sprinting`).
    #[serde(default)]
    pub sprinting: bool,
}

/// A fact for the ledger.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Fact {
    Health {
        from: Option<f32>,
        to: f32,
    },
    Food {
        from: Option<i32>,
        to: i32,
    },
    /// A blow on the bot: its damage type, what dealt it, the health after.
    Hit {
        damage_type: String,
        by: Option<String>,
        health: Option<f32>,
    },
    /// The bot caught fire, and the likeliest cause (the newest blow, else unknown).
    Alight {
        cause: String,
    },
    Item {
        item: String,
        from: i32,
        to: i32,
    },
    /// A tool fell under a share of its uses.
    ToolWorn {
        item: String,
        uses_left: i32,
    },
    /// A tool is gone from its slot with its uses run out (it broke).
    ToolBroke {
        item: String,
    },
    /// A tool is gone from the holdings with uses left: dropped, thrown or lost, not worn out.
    ToolGone {
        item: String,
        uses_left: i32,
    },
    /// Health fell with no damage event of its own (none in the `HIT_WINDOW` ticks before it that
    /// another fall had not answered): what was nearest and whether the bot burned, so no loss
    /// of health is silent.
    HealthDrop {
        by: f32,
        to: f32,
        hostile: Option<(String, f64)>,
        on_fire: bool,
    },
    /// The cursor's stack changed: what it holds now (`None` is an empty cursor).
    Cursor {
        item: Option<String>,
        count: i32,
    },
    Dimension {
        to: String,
        /// The dimension before (issue 19b; `None` at the first login and in older files).
        #[serde(default)]
        from: Option<String>,
    },
    Stage {
        reached: String,
    },
    Death,
    Won,
    Effect {
        name: String,
        on: bool,
    },
    Sound {
        name: String,
        d: Option<f64>,
    },
    LevelEvent {
        name: String,
        d: f64,
    },
    Chat {
        via: String,
        text: String,
    },
    Appear {
        id: i32,
        kind: EntityKind,
        d: f64,
    },
    Gone {
        id: i32,
        kind: EntityKind,
    },
    /// A projectile near the bot is gone: its kind and the closest it came (the 24 of 212
    /// blaze fireballs that hit clojurecraft's bots all passed within 1.4 blocks).
    Shot {
        kind: EntityKind,
        closest: f64,
    },
    /// The bot's blow landed on a mob (a damage event it dealt).
    Landed {
        id: i32,
        kind: EntityKind,
    },
    /// A mob began to show a sign (fuse, charge, anger, facing the bot).
    Sign {
        id: i32,
        kind: EntityKind,
        sign: String,
        d: f64,
    },
    /// Night fell (`world::time::DUSK`), or the bot joined at night: the time of day then.
    Night {
        day_time: i64,
    },
    /// Day broke (`world::time::DAWN`): the time of day then.
    Day {
        day_time: i64,
    },
    /// The biome at the feet changed (issue 17b).
    Biome {
        name: String,
    },
    /// An enderman within `ENDERMAN_WATCH` first seen, or begun or stopped ticking.
    Enderman {
        id: i32,
        d: f64,
        ticking: bool,
    },
    /// A ticking enderman (by distance) has not moved for `STILL_TICKS` or more.
    EndermanStill {
        id: i32,
        d: f64,
        for_ticks: u64,
    },
    /// A Nether chunk column became a known warped forest from blocks seen.
    ForestSeen {
        column: (i32, i32),
    },
    /// A Nether chunk column became a known soul sand valley from blocks seen.
    SoulValleySeen {
        column: (i32, i32),
    },
    /// An enderman within `ENDERMAN_WATCH` turned angry (its creepy flag, data index 17, went
    /// true; issue 18b): whether it ticks, so a provoke is counted and a frozen angry one kept.
    EndermanAngry {
        id: i32,
        d: f64,
        ticking: bool,
    },
    /// An enderman's health (data index 9) fell: a blow landed, by the bot or not.
    EndermanHit {
        id: i32,
        before: f32,
        after: f32,
    },
    /// The server said an enderman died (entity event 3); `by_bot` when the bot had struck it.
    EndermanKilled {
        id: i32,
        by_bot: bool,
    },
    /// An ender pearl item appeared within `PEARL_NEAR` of an enderman's death of the last
    /// `KILL_LOOT_TICKS`: its drop (`id` is the item entity's).
    PearlDropped {
        id: i32,
        pos: [f64; 3],
    },
    /// The bot picked up a pearl item (`take_item_entity` for one on the ground).
    PearlPicked {
        id: i32,
        amount: i32,
    },
    /// A cell the planner holds as its own (a burrow block) turned to air with no dig of the
    /// bot's (issue 18e; an enderman takes a holdable block). Emitted by its owner.
    BlockTaken {
        cell: [i32; 3],
    },
    /// Issue 19b: an eye of ender appeared within `EYE_THROWN_NEAR` of the bot (its throw):
    /// where, and the heading of its spawn velocity when the server sent one.
    EyeThrown {
        id: i32,
        from: [f64; 3],
        yaw: Option<f64>,
    },
    /// An eye's flight, told when it ends: the first and last positions seen, the heading
    /// between them (yaw degrees) and the ticks it was seen in.
    EyeFlight {
        id: i32,
        first: [f64; 3],
        last: [f64; 3],
        heading: Option<f64>,
        samples: u32,
        ticks: u64,
    },
    /// An eye ended: `dropped` when an `ender_eye` item appeared within `EYE_DROP_NEAR` of
    /// where it ended within `EYE_DROP_TICKS` (else it shattered), `at` its last position.
    EyeEnded {
        id: i32,
        dropped: bool,
        at: [f64; 3],
    },
    /// The bot picked up an ender eye item.
    EyePicked {
        id: i32,
        amount: i32,
    },
    /// An end portal frame came on record within `world::stage::FRAME_NEAR`.
    FrameSeen {
        cell: [i32; 3],
    },
    /// A silverfish appeared within `SILVERFISH_NEAR` (a spawner's, or out of an infested block).
    SilverfishWoken {
        id: i32,
        d: f64,
    },
    // >>> E1 (issue 20b)
    /// An end portal frame on record came to hold an eye.
    EyePlaced {
        cell: [i32; 3],
    },
    /// The End entered, from `from`, the feet at `at`.
    PortalEntered {
        from: Option<String>,
        at: [f64; 3],
    },
    /// An end crystal came into the world: where, and how far from the feet.
    CrystalSeen {
        id: i32,
        pos: [f64; 3],
        d: f64,
    },
    /// An end crystal left the world: `struck` (the bot's attack on it within `STRIKE_TICKS`),
    /// `blast` (an explosion beside it), `far` (out of view), else `gone`.
    CrystalGone {
        id: i32,
        pos: [f64; 3],
        how: String,
    },
    /// A climb's top held `PILLAR_SETTLE` ticks: the feet `height` over the ground at `cell`.
    PillarUp {
        height: i32,
        cell: [i32; 3],
    },
    /// Back near the ground after a pillar of `height`.
    PillarDown {
        height: i32,
    },
    /// An attack sent: its target (an entity kind, or `ender_dragon:<part>`), the id, and the
    /// eye's distance to it (a crystal's box; else its position).
    Strike {
        target: String,
        id: i32,
        reach: Option<f64>,
    },
    /// The dragon's phase changed (ids at `DRAGON_PHASE`).
    DragonPhase {
        from: Option<i32>,
        to: i32,
    },
    /// A blow of the bot's landed on the dragon: the part last struck and its health after.
    DragonHit {
        part: Option<String>,
        health: Option<f32>,
    },
    /// The dragon hurt the bot: the health lost.
    DragonBlow {
        lost: f32,
        damage_type: String,
    },
    /// A breath or fireball cloud came within `BREATH_WATCH` of the feet: its radius, and the
    /// feet's distance to its edge then (negative: inside).
    Breath {
        id: i32,
        radius: f32,
        dist: f64,
    },
    /// The look came within `NEAR_MISS` stare tolerances of an enderman's eyes; `stare` when
    /// inside the stare's own cone (line of sight not tested).
    GazeNearMiss {
        id: i32,
        d: f64,
        stare: bool,
    },
    /// The dragon was seen dead (`World::dragon_dead`).
    DragonDead,
    /// The credits (`win_game`), with the tick the dragon was seen dead, if it was.
    Credits {
        dragon_dead: Option<u64>,
    },
    // <<< E1
    /// Issue 56: the server's sprint flag for the bot changed (its data index 0, bit 0x08): its
    /// word on the `player_command` we sent, so a race counts sprints the server took.
    SprintServer {
        on: bool,
    },
    /// Issue 56: the bot reported a sprint begun or ended (`player_command`, from the body's
    /// sprint after the stop rules): `why` is the first rule that explains it (`sprint_why`),
    /// so a race counts sprints and what stopped them.
    Sprint {
        on: bool,
        why: String,
    },
}

/// A fact and the server tick it was seen.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Stamped {
    pub at: u64,
    pub fact: Fact,
}

fn feet(w: &World) -> Option<[f64; 3]> {
    w.body.map(|b| b.pos).or(w.player.pos)
}

fn dist(a: [f64; 3], b: [f64; 3]) -> f64 {
    (0..3).map(|i| (a[i] - b[i]).powi(2)).sum::<f64>().sqrt()
}

fn meta_bool(e: &world::Entity, i: u8) -> bool {
    match e.metadata.get(&i) {
        Some(MetaValue::Bool(b)) => *b,
        Some(MetaValue::Byte(b)) => *b != 0,
        Some(_) | None => false,
    }
}

/// The signs a mob shows, from its data and its head.
pub fn signs(e: &world::Entity, bot: [f64; 3]) -> Signs {
    let fuse = e.kind == EntityKind::Creeper
        && (matches!(e.metadata.get(&CREEPER_SWELL), Some(MetaValue::Int(v)) if *v > 0)
            || meta_bool(e, CREEPER_IGNITED));
    let charge = e.blaze_charged() || (e.kind == EntityKind::Ghast && meta_bool(e, GHAST_CHARGING));
    let angry = e.kind == EntityKind::Enderman && meta_bool(e, ENDERMAN_CREEPY);
    let facing = hostile(e.kind) && e.faces(bot, FACING).unwrap_or(false);
    Signs {
        fuse,
        charge,
        angry,
        facing,
    }
}

/// A monster by the game's own category.
pub fn hostile(kind: EntityKind) -> bool {
    mob(kind).is_some_and(|m| m.category == MobCategory::Monster)
}

fn item_name(i: data::ItemKind) -> String {
    i.name().unwrap_or("unknown").to_owned()
}

/// The parts of `w` the ledger compares.
pub fn snapshot(w: &World) -> Snapshot {
    let at = w.session.server_ticks;
    let bot = feet(w).unwrap_or([0.0; 3]);
    let mut items: BTreeMap<String, i32> = BTreeMap::new();
    for (item, n) in w.inventory.slots.values() {
        *items.entry(item_name(*item)).or_default() += n;
    }
    // The cursor's stack is held too: a pickaxe on the cursor is not a pickaxe lost.
    let cursor = w.inventory.carried.map(|(item, n)| (item_name(item), n));
    if let Some((item, n)) = &cursor {
        *items.entry(item.clone()).or_default() += n;
    }
    let tools = w
        .inventory
        .slots
        .iter()
        .filter_map(|(slot, (item, _))| {
            w.inventory
                .uses_left(*slot)
                .map(|u| (*slot, (item_name(*item), u)))
        })
        .collect();
    let mobs = w
        .entities
        .iter()
        .filter(|(_, e)| !e.dead && hostile(e.kind))
        .filter_map(|(id, e)| {
            let d = dist(e.pos, bot);
            (d <= WATCH + WATCH_MARGIN).then(|| {
                (
                    *id,
                    Seen {
                        kind: e.kind,
                        d,
                        signs: signs(e, bot),
                        announced: d <= WATCH,
                    },
                )
            })
        })
        .collect();
    let projectiles = w
        .entities
        .iter()
        .filter(|(_, e)| projectile(e.kind))
        .filter_map(|(id, e)| {
            let d = dist(e.pos, bot);
            (d <= PROJECTILE_WATCH).then_some((
                *id,
                Flying {
                    kind: e.kind,
                    closest: d,
                },
            ))
        })
        .collect();
    let landed = w
        .entities
        .iter()
        .filter(|(_, e)| hostile(e.kind))
        .map(|(id, e)| (*id, e.hurts.iter().filter(|h| h.by_bot).count()))
        .filter(|(_, n)| *n > 0)
        .collect();
    Snapshot {
        at,
        health: w.player.health,
        food: w.player.food,
        on_fire: w.player.on_fire,
        dimension: w.dimension.as_ref().map(|d| d.name.clone()),
        stage: w.stage_reached.map(|s| s.name().to_owned()),
        died: w.death.is_some(),
        won: w.won,
        items,
        cursor,
        click_age: w.click.map(|c| w.ticks.saturating_sub(c.at_tick)),
        ticks: w.ticks,
        hit_mark: w.hits.last().map(|h| {
            (
                h.at_tick,
                w.hits.iter().filter(|x| x.at_tick == h.at_tick).count(),
            )
        }),
        hits_unanswered: 0,
        hit_tick: 0,
        tools,
        mobs,
        effects: w.player.effects.keys().cloned().collect(),
        projectiles,
        landed,
        hits: w.hits.len(),
        sounds: w.sounds.len(),
        level_events: w.level_events.len(),
        chat: w.chat.len(),
        day_time: w.day_time(),
        biome: w.feet_biome().map(str::to_owned),
        endermen: endermen_near(w, bot, at),
        forests: w.memory.forests().len(),
        soul_valleys: w.memory.soul_valleys().len(),
        pearl_items: pearls_near(w, bot),
        deaths: Vec::new(),
        eyes: w
            .entities
            .iter()
            .filter(|(_, e)| e.kind == EntityKind::EyeOfEnder && dist(e.pos, bot) <= EYE_WATCH)
            .map(|(id, e)| {
                (
                    *id,
                    EyeTrack {
                        own: dist(e.pos, bot) <= EYE_THROWN_NEAR,
                        first: e.pos,
                        last: e.pos,
                        first_at: at,
                        samples: 1,
                    },
                )
            })
            .collect(),
        eyes_gone: Vec::new(),
        eye_items: w
            .entities
            .iter()
            .filter(|(_, e)| e.kind == EntityKind::Item && dist(e.pos, bot) <= EYE_WATCH)
            .filter(|(_, e)| e.item().is_some_and(|(k, _)| k == data::ItemKind::EnderEye))
            .map(|(id, e)| (*id, e.pos))
            .collect(),
        frames: if w.memory.has_seen(BlockKind::EndPortalFrame) {
            world::stage::frames_near(w)
                .into_iter()
                .map(|(x, y, z)| [x, y, z])
                .collect()
        } else {
            Vec::new()
        },
        silverfish: w
            .entities
            .iter()
            .filter(|(_, e)| {
                !e.dead && e.kind == EntityKind::Silverfish && dist(e.pos, bot) <= SILVERFISH_NEAR
            })
            .map(|(id, _)| *id)
            .collect(),
        // >>> E1 (issue 20b)
        frames_eye: frames_with_eye(w),
        crystals: crystals(w),
        attacks: w.attacks_sent,
        last_attack: w.last_attack,
        over: if world::stage::in_end(w) {
            over_ground(w)
        } else {
            None
        },
        climb: None,
        dragon: dragon(w).map(|(id, _)| id),
        dragon_phase: dragon_phase(w),
        dragon_health: dragon_health(w),
        dragon_landed: dragon(w).map_or(0, |(_, e)| e.hurts.iter().filter(|h| h.by_bot).count()),
        dragon_dead: w.dragon_dead,
        clouds: w
            .entities
            .iter()
            .filter(|(_, e)| e.kind == EntityKind::AreaEffectCloud)
            .filter_map(|(id, e)| {
                let r = meta_float(e, CLOUD_RADIUS).unwrap_or(0.0);
                let d = (e.pos[0] - bot[0]).hypot(e.pos[2] - bot[2]) - f64::from(r);
                (d <= BREATH_WATCH).then_some((
                    *id,
                    Cloud {
                        radius: r,
                        closest: d,
                    },
                ))
            })
            .collect(),
        gazes: gazes(w),
        // <<< E1
        sprint_server: w.player.sprinting,
        sprinting: w.sent_sprinting,
    }
}

/// Why the bot sprints or stopped, from the world it reported in: a sprint begun is the key
/// held (`asked`); one ended is the first of the client's stop rules (`LocalPlayer.aiStep`) that
/// holds: the key let go, no forward key, sneaking, food at or under `SPRINT_FOOD`, a wall,
/// water. None of them is `rule`: a rule this list does not name.
pub fn sprint_why(w: &World) -> String {
    if w.sent_sprinting {
        return "asked".to_owned();
    }
    let c = &w.controls;
    let body = w.body.unwrap_or_default();
    if !c.sprint {
        "released".to_owned()
    } else if !c.forward {
        "no forward".to_owned()
    } else if c.sneak {
        "sneak".to_owned()
    } else if let Some(food) = w.player.food.filter(|f| *f <= SPRINT_FOOD) {
        format!("food {food}")
    } else if body.horizontal_collision {
        "wall".to_owned()
    } else if body.in_water {
        "water".to_owned()
    } else {
        "rule".to_owned()
    }
}

/// Food at or under this stops a sprint (`LocalPlayer.hasEnoughFoodToSprint`: over 6.0, the
/// player's `foodData` level), for `sprint_why`'s words only: physics holds the rule.
const SPRINT_FOOD: i32 = 6;

/// Loud sounds heard from farther than `HEAR`: shots, warnings, stares, screams, teleports,
/// deaths (clojurecraft's far-sound list).
fn loud(name: &str) -> bool {
    [
        "shoot", "warn", "stare", "scream", "teleport", "death", "primed", "fireball",
    ]
    .iter()
    .any(|k| name.contains(k))
}

/// The facts between two snapshots of the same run, `w` being the world at `next`.
pub fn diff(prev: &Snapshot, next: &mut Snapshot, w: &World) -> Vec<Stamped> {
    let at = next.at;
    let mut out: Vec<Fact> = Vec::new();
    let bot = feet(w).unwrap_or([0.0; 3]);
    if let Some(h) = next.health
        && next.health != prev.health
    {
        out.push(Fact::Health {
            from: prev.health,
            to: h,
        });
    }
    if let Some(f) = next.food
        && next.food != prev.food
    {
        out.push(Fact::Food {
            from: prev.food,
            to: f,
        });
    }
    // New blows: those stamped after the newest seen before, and any more at that same stamp.
    // The world prunes its list as it grows, so the list's length says nothing in a long fight.
    let new_hits = match prev.hit_mark {
        Some((t, n)) => {
            let after = w.hits.iter().filter(|h| h.at_tick > t).count();
            let same = w.hits.iter().filter(|h| h.at_tick == t).count();
            after + same.saturating_sub(n)
        }
        None => w.hits.len(),
    };
    // A fall in health is answered by a damage event that came with it or just before it.
    let lapsed = next.ticks.saturating_sub(prev.hit_tick) > HIT_WINDOW;
    next.hits_unanswered = if lapsed { 0 } else { prev.hits_unanswered };
    next.hit_tick = prev.hit_tick;
    if new_hits > 0 {
        next.hits_unanswered += u32::try_from(new_hits).unwrap_or(u32::MAX);
        next.hit_tick = next.ticks;
    }
    if let (Some(was), Some(now)) = (prev.health, next.health)
        && now < was
    {
        if next.hits_unanswered > 0 {
            next.hits_unanswered -= 1;
        } else {
            let hostile = next
                .mobs
                .values()
                .min_by(|a, b| a.d.total_cmp(&b.d))
                .map(|m| (m.kind.name().unwrap_or("unknown").to_owned(), m.d));
            out.push(Fact::HealthDrop {
                by: was - now,
                to: now,
                hostile,
                on_fire: next.on_fire,
            });
        }
    }
    for h in w.hits.iter().rev().take(new_hits).rev() {
        out.push(Fact::Hit {
            damage_type: h.damage_type.clone(),
            by: h
                .cause
                .or(h.direct)
                .map(|k| k.name().unwrap_or("unknown").to_owned()),
            health: next.health,
        });
    }
    if next.on_fire && !prev.on_fire {
        let cause = w
            .hits
            .last()
            .filter(|h| h.at_tick + 40 >= at)
            .map_or_else(|| "unknown".to_owned(), |h| h.damage_type.clone());
        out.push(Fact::Alight { cause });
    }
    // A click not yet answered: the holdings are as they last settled (see `CLICK_SETTLE`).
    if next.click_age.is_some_and(|a| a <= CLICK_SETTLE) {
        next.items.clone_from(&prev.items);
        next.tools.clone_from(&prev.tools);
        next.cursor.clone_from(&prev.cursor);
    }
    if next.cursor != prev.cursor {
        out.push(Fact::Cursor {
            item: next.cursor.as_ref().map(|(i, _)| i.clone()),
            count: next.cursor.as_ref().map_or(0, |(_, n)| *n),
        });
    }
    for (item, n) in &next.items {
        let was = prev.items.get(item).copied().unwrap_or(0);
        if *n != was {
            out.push(Fact::Item {
                item: item.clone(),
                from: was,
                to: *n,
            });
        }
    }
    for (item, was) in &prev.items {
        if !next.items.contains_key(item) {
            out.push(Fact::Item {
                item: item.clone(),
                from: *was,
                to: 0,
            });
        }
    }
    for (slot, (item, left)) in &next.tools {
        if let Some((pitem, pleft)) = prev.tools.get(slot)
            && pitem == item
        {
            let max = data::generated::item_props::max_damage(
                data::ItemKind::from_name(item).unwrap_or(data::ItemKind::Air),
            )
            .max(1);
            for m in WEAR_MARKS {
                let line = (f64::from(max) * m).floor() as i32;
                if *pleft > line && *left <= line {
                    out.push(Fact::ToolWorn {
                        item: item.clone(),
                        uses_left: *left,
                    });
                }
            }
        }
    }
    for (slot, (item, left)) in &prev.tools {
        match next.tools.get(slot).filter(|(i, _)| i == item) {
            // Still in its slot: broken only if its uses ran out there.
            Some((_, now)) => {
                if *now <= 0 && *left > 0 {
                    out.push(Fact::ToolBroke { item: item.clone() });
                }
            }
            // Gone from its slot, and one fewer held in every slot and on the cursor (a tool
            // moved to another slot or to the cursor keeps the count): broken if it was on its
            // last use, else gone with uses left, which is said as such and never as a break.
            None => {
                let held = |s: &Snapshot| s.items.get(item).copied().unwrap_or(0);
                if held(next) < held(prev) {
                    out.push(if *left <= BREAK_AT {
                        Fact::ToolBroke { item: item.clone() }
                    } else {
                        Fact::ToolGone {
                            item: item.clone(),
                            uses_left: *left,
                        }
                    });
                }
            }
        }
    }
    if next.dimension != prev.dimension
        && let Some(d) = &next.dimension
    {
        out.push(Fact::Dimension {
            to: d.clone(),
            from: prev.dimension.clone(),
        });
    }
    if next.stage != prev.stage
        && let Some(s) = &next.stage
    {
        out.push(Fact::Stage { reached: s.clone() });
    }
    if next.died && !prev.died {
        out.push(Fact::Death);
    }
    if next.won && !prev.won {
        out.push(Fact::Won);
    }
    if next.biome != prev.biome
        && let Some(name) = &next.biome
    {
        out.push(Fact::Biome { name: name.clone() });
    }
    // The server's first word that the bot is not sprinting is no change: only a sprint it
    // took, and its end, are told.
    if let Some(on) = next.sprint_server
        && next.sprint_server != prev.sprint_server
        && (on || prev.sprint_server.is_some())
    {
        out.push(Fact::SprintServer { on });
    }
    if next.sprinting != prev.sprinting {
        out.push(Fact::Sprint {
            on: next.sprinting,
            why: sprint_why(w),
        });
    }
    for (id, n) in &mut next.endermen {
        let d = dist(n.pos, bot);
        match prev.endermen.get(id) {
            None => out.push(Fact::Enderman {
                id: *id,
                d,
                ticking: n.ticking,
            }),
            Some(p) => {
                if p.ticking != n.ticking {
                    out.push(Fact::Enderman {
                        id: *id,
                        d,
                        ticking: n.ticking,
                    });
                }
                // Still while ticking: the span carries on; a move or a change starts it again.
                if p.ticking && n.ticking && p.pos == n.pos {
                    n.still_since = p.still_since;
                    n.still_noted = p.still_noted;
                }
            }
        }
        let for_ticks = at.saturating_sub(n.still_since);
        if n.ticking && !n.still_noted && for_ticks >= STILL_TICKS {
            n.still_noted = true;
            out.push(Fact::EndermanStill {
                id: *id,
                d,
                for_ticks,
            });
        }
    }
    // Issue 18b: anger, blows, deaths and pearls (`EnderSeen` carries the flags).
    next.deaths = prev
        .deaths
        .iter()
        .filter(|k| k.at + KILL_LOOT_TICKS >= at)
        .cloned()
        .collect();
    for (id, n) in &next.endermen {
        let d = dist(n.pos, bot);
        let p = prev.endermen.get(id);
        if n.angry && !n.dead && !p.is_some_and(|p| p.angry) {
            out.push(Fact::EndermanAngry {
                id: *id,
                d,
                ticking: n.ticking,
            });
        }
        if let (Some(before), Some(after)) = (p.and_then(|p| p.health), n.health)
            && after < before
        {
            out.push(Fact::EndermanHit {
                id: *id,
                before,
                after,
            });
        }
        if n.dead && !p.is_some_and(|p| p.dead) {
            out.push(Fact::EndermanKilled {
                id: *id,
                by_bot: n.struck,
            });
            next.deaths.push(Death {
                at,
                id: *id,
                pos: n.pos,
            });
        }
    }
    for (id, pos) in &next.pearl_items {
        if !prev.pearl_items.contains_key(id)
            && next.deaths.iter().any(|k| dist(k.pos, *pos) <= PEARL_NEAR)
        {
            out.push(Fact::PearlDropped { id: *id, pos: *pos });
        }
    }
    for id in prev.pearl_items.keys() {
        if !next.pearl_items.contains_key(id)
            && let Some(p) = w.pickups.iter().find(|p| p.item_entity == *id)
        {
            out.push(Fact::PearlPicked {
                id: *id,
                amount: p.amount,
            });
        }
    }
    // Issue 19b: eyes thrown, flown, ended and picked; frames seen; silverfish woken.
    for (id, e) in &mut next.eyes {
        match prev.eyes.get(id) {
            Some(p) => {
                e.own = p.own;
                e.first = p.first;
                e.first_at = p.first_at;
                e.samples = p.samples + 1;
            }
            None if e.own => out.push(Fact::EyeThrown {
                id: *id,
                from: e.first,
                yaw: w
                    .entities
                    .get(id)
                    .and_then(|x| x.spawn_velocity)
                    .and_then(|v| heading([0.0; 3], v)),
            }),
            None => {}
        }
    }
    // Every ended eye waits until its drop is seen or its wait is over; each is told once.
    next.eyes_gone.clone_from(&prev.eyes_gone);
    for (id, e) in &prev.eyes {
        if !next.eyes.contains_key(id) && e.own {
            out.push(Fact::EyeFlight {
                id: *id,
                first: e.first,
                last: e.last,
                heading: heading(e.first, e.last),
                samples: e.samples,
                ticks: at.saturating_sub(e.first_at),
            });
            next.eyes_gone.push(EyeGone {
                id: *id,
                at,
                pos: e.last,
            });
        }
    }
    let mut told = Vec::new();
    for g in &next.eyes_gone {
        let dropped = next
            .eye_items
            .values()
            .any(|p| dist(*p, g.pos) <= EYE_DROP_NEAR);
        if dropped || g.at + EYE_DROP_TICKS <= at {
            out.push(Fact::EyeEnded {
                id: g.id,
                dropped,
                at: g.pos,
            });
            told.push(g.id);
        }
    }
    next.eyes_gone.retain(|g| !told.contains(&g.id));
    for id in prev.eye_items.keys() {
        if !next.eye_items.contains_key(id)
            && let Some(p) = w.pickups.iter().find(|p| p.item_entity == *id)
        {
            out.push(Fact::EyePicked {
                id: *id,
                amount: p.amount,
            });
        }
    }
    for cell in &next.frames {
        if !prev.frames.contains(cell) {
            out.push(Fact::FrameSeen { cell: *cell });
        }
    }
    for id in &next.silverfish {
        if !prev.silverfish.contains(id)
            && let Some(e) = w.entities.get(id)
        {
            out.push(Fact::SilverfishWoken {
                id: *id,
                d: dist(e.pos, bot),
            });
        }
    }
    for column in w.memory.forests().iter().skip(prev.forests) {
        out.push(Fact::ForestSeen { column: *column });
    }
    for column in w.memory.soul_valleys().iter().skip(prev.soul_valleys) {
        out.push(Fact::SoulValleySeen { column: *column });
    }
    let night = |s: &Snapshot| s.day_time.map(world::time::is_night);
    match (night(prev), night(next), next.day_time) {
        (Some(true), Some(false), Some(day_time)) => out.push(Fact::Day { day_time }),
        (Some(false) | None, Some(true), Some(day_time)) => out.push(Fact::Night { day_time }),
        _ => {}
    }
    for e in &next.effects {
        if !prev.effects.contains(e) {
            out.push(Fact::Effect {
                name: e.clone(),
                on: true,
            });
        }
    }
    for e in &prev.effects {
        if !next.effects.contains(e) {
            out.push(Fact::Effect {
                name: e.clone(),
                on: false,
            });
        }
    }
    let new_sounds = next.sounds.saturating_sub(prev.sounds);
    for s in w.sounds.iter().rev().take(new_sounds).rev() {
        let d = s.pos.map(|p| dist(p, bot));
        if d.is_none_or(|d| d <= HEAR) || loud(&s.name) {
            out.push(Fact::Sound {
                name: s.name.clone(),
                d,
            });
        }
    }
    let new_events = next.level_events.saturating_sub(prev.level_events);
    for e in w.level_events.iter().rev().take(new_events).rev() {
        let p = [f64::from(e.pos.0), f64::from(e.pos.1), f64::from(e.pos.2)];
        let d = dist(p, bot);
        let name = e
            .name
            .clone()
            .unwrap_or_else(|| format!("level_event:{}", e.id));
        if d <= HEAR || loud(&name) {
            out.push(Fact::LevelEvent { name, d });
        }
    }
    let new_chat = next.chat.saturating_sub(prev.chat);
    for c in w.chat.iter().rev().take(new_chat).rev() {
        out.push(Fact::Chat {
            via: c.via.clone(),
            text: c.text.clone(),
        });
    }
    for (id, m) in &mut next.mobs {
        let before = prev.mobs.get(id).filter(|p| p.announced);
        m.announced = before.is_some() || m.d <= WATCH;
        match before {
            None if m.announced => out.push(Fact::Appear {
                id: *id,
                kind: m.kind,
                d: m.d,
            }),
            None => {}
            Some(p) => {
                for (sign, now, was) in [
                    ("fuse", m.signs.fuse, p.signs.fuse),
                    ("charge", m.signs.charge, p.signs.charge),
                    ("angry", m.signs.angry, p.signs.angry),
                    ("facing", m.signs.facing, p.signs.facing),
                ] {
                    if now && !was {
                        out.push(Fact::Sign {
                            id: *id,
                            kind: m.kind,
                            sign: sign.to_owned(),
                            d: m.d,
                        });
                    }
                }
            }
        }
    }
    for (id, m) in &prev.mobs {
        if m.announced && !next.mobs.contains_key(id) {
            out.push(Fact::Gone {
                id: *id,
                kind: m.kind,
            });
        }
    }
    for (id, f) in &mut next.projectiles {
        if let Some(p) = prev.projectiles.get(id) {
            f.closest = f.closest.min(p.closest);
        }
    }
    for (id, p) in &prev.projectiles {
        if !next.projectiles.contains_key(id) {
            out.push(Fact::Shot {
                kind: p.kind,
                closest: p.closest,
            });
        }
    }
    for (id, n) in &next.landed {
        let was = prev.landed.get(id).copied().unwrap_or(0);
        if *n > was
            && let Some(m) = w.entities.get(id)
        {
            out.push(Fact::Landed {
                id: *id,
                kind: m.kind,
            });
        }
    }
    end_facts(prev, next, w, &mut out);
    out.into_iter().map(|fact| Stamped { at, fact }).collect()
}

// >>> E1 (issue 20b)
/// The End's facts between two snapshots (`diff`'s tail).
fn end_facts(prev: &Snapshot, next: &mut Snapshot, w: &World, out: &mut Vec<Fact>) {
    let at = next.at;
    let bot = feet(w).unwrap_or([0.0; 3]);
    for cell in &next.frames_eye {
        if !prev.frames_eye.contains(cell) {
            out.push(Fact::EyePlaced { cell: *cell });
        }
    }
    let end = Some("minecraft:the_end");
    if next.dimension.as_deref() == end
        && prev.dimension.is_some()
        && prev.dimension.as_deref() != end
    {
        out.push(Fact::PortalEntered {
            from: prev.dimension.clone(),
            at: bot,
        });
    }
    for (id, pos) in &next.crystals {
        if !prev.crystals.contains_key(id) {
            out.push(Fact::CrystalSeen {
                id: *id,
                pos: *pos,
                d: dist(*pos, bot),
            });
        }
    }
    for (id, pos) in &prev.crystals {
        if next.crystals.contains_key(id) {
            continue;
        }
        let struck = next
            .last_attack
            .is_some_and(|(t, x)| x == *id && t + STRIKE_TICKS >= at);
        let blast = w
            .explosions
            .iter()
            .any(|b| b.at_tick + STRIKE_TICKS >= at && dist(b.center, *pos) <= BLAST_NEAR);
        let how = if struck {
            "struck"
        } else if blast {
            "blast"
        } else if dist(*pos, bot) > CRYSTAL_VIEW {
            "far"
        } else {
            "gone"
        };
        out.push(Fact::CrystalGone {
            id: *id,
            pos: *pos,
            how: how.to_owned(),
        });
    }
    // Strikes: every attack sent since the last snapshot, told by its last target.
    if next.attacks > prev.attacks
        && let Some((_, id)) = next.last_attack
    {
        let (target, reach) = match (w.entities.get(&id), prev.crystals.get(&id)) {
            (Some(e), _) => (
                e.kind.name().unwrap_or("unknown").to_owned(),
                reach_to(w, e),
            ),
            // A crystal the strike removed in the same tick: where it was.
            (None, Some(pos)) => (
                "end_crystal".to_owned(),
                reach_at(w, EntityKind::EndCrystal, *pos),
            ),
            (None, None) => match next.dragon {
                Some(d) if id > d && id <= d + DRAGON_PARTS.len() as i32 => {
                    let part = DRAGON_PARTS
                        .get(usize::try_from(id - d - 1).unwrap_or(0))
                        .copied()
                        .unwrap_or("part");
                    (
                        format!("ender_dragon:{part}"),
                        w.entities.get(&d).and_then(|e| reach_to(w, e)),
                    )
                }
                Some(_) | None => ("unknown".to_owned(), None),
            },
        };
        out.push(Fact::Strike { target, id, reach });
    }
    // Pillars: a climb over `PILLAR_MIN` told once its top holds, and its end near the ground.
    next.climb = prev.climb;
    match next.over {
        Some(h) if h >= PILLAR_MIN => {
            let c = next.climb.get_or_insert(Climb {
                top: h,
                top_at: at,
                cell: world_cell(bot),
                told: false,
            });
            if h > c.top {
                *c = Climb {
                    top: h,
                    top_at: at,
                    cell: world_cell(bot),
                    told: false,
                };
            }
            if !c.told && at >= c.top_at + PILLAR_SETTLE {
                c.told = true;
                out.push(Fact::PillarUp {
                    height: c.top,
                    cell: c.cell,
                });
            }
        }
        Some(h) if h < PILLAR_MIN - 1 => {
            if let Some(c) = next.climb.take()
                && c.told
            {
                out.push(Fact::PillarDown { height: c.top });
            }
        }
        // Between, or over the void: the climb stands as it was.
        Some(_) | None => {}
    }
    if let Some(to) = next.dragon_phase
        && next.dragon_phase != prev.dragon_phase
    {
        out.push(Fact::DragonPhase {
            from: prev.dragon_phase,
            to,
        });
    }
    if next.dragon == prev.dragon && next.dragon_landed > prev.dragon_landed {
        let part = next.last_attack.and_then(|(_, id)| {
            let d = next.dragon?;
            if id == d {
                return Some("body".to_owned());
            }
            (id > d && id <= d + DRAGON_PARTS.len() as i32)
                .then(|| DRAGON_PARTS.get(usize::try_from(id - d - 1).unwrap_or(0)))
                .flatten()
                .map(|p| (*p).to_owned())
        });
        out.push(Fact::DragonHit {
            part,
            health: next.dragon_health,
        });
    }
    // The new blows as `diff` counts them: after the newest seen before, and more at its stamp.
    let fresh = match prev.hit_mark {
        Some((t, n)) => {
            let after = w.hits.iter().filter(|h| h.at_tick > t).count();
            let same = w.hits.iter().filter(|h| h.at_tick == t).count();
            after + same.saturating_sub(n)
        }
        None => w.hits.len(),
    };
    for h in w.hits.iter().rev().take(fresh) {
        if h.cause == Some(EntityKind::EnderDragon) || h.direct == Some(EntityKind::EnderDragon) {
            let lost = match (prev.health, next.health) {
                (Some(a), Some(b)) => (a - b).max(0.0),
                _ => 0.0,
            };
            out.push(Fact::DragonBlow {
                lost,
                damage_type: h.damage_type.clone(),
            });
        }
    }
    for (id, c) in &mut next.clouds {
        match prev.clouds.get(id) {
            None => out.push(Fact::Breath {
                id: *id,
                radius: c.radius,
                dist: c.closest,
            }),
            Some(p) => c.closest = c.closest.min(p.closest),
        }
    }
    for (id, n) in &next.gazes {
        if !prev.gazes.get(id).is_some_and(|p| p.1 || !n.1) {
            out.push(Fact::GazeNearMiss {
                id: *id,
                d: n.0,
                stare: n.1,
            });
        }
    }
    if next.dragon_dead.is_some() && prev.dragon_dead.is_none() {
        out.push(Fact::DragonDead);
    }
    if next.won && !prev.won {
        out.push(Fact::Credits {
            dragon_dead: next.dragon_dead,
        });
    }
}

fn world_cell(p: [f64; 3]) -> [i32; 3] {
    [
        p[0].floor() as i32,
        p[1].floor() as i32,
        p[2].floor() as i32,
    ]
}
// <<< E1

/// A mob in a senses row.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Near {
    pub kind: EntityKind,
    pub d: f64,
    pub health: Option<f32>,
    pub signs: Signs,
    /// What it holds in its main hand.
    pub held: Option<String>,
}

/// One row a second: where the bot is, how it is, what it holds, what it is doing and why, and
/// what is near (issue 16e; clojurecraft's senses line).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Senses {
    pub at: u64,
    pub pos: Option<[f64; 3]>,
    pub dimension: Option<String>,
    pub health: Option<f32>,
    pub food: Option<i32>,
    pub saturation: Option<f32>,
    pub air: Option<i32>,
    pub on_fire: bool,
    /// The held item and its uses left.
    pub held: Option<String>,
    pub held_uses: Option<i32>,
    /// Every wearing item held, with its uses left (pickaxes, swords, armour).
    pub tools: Vec<(String, i32)>,
    /// Empty slots in the main inventory and hotbar.
    pub free_slots: u32,
    pub task: String,
    pub why: String,
    /// The nearest hostiles within `WATCH`, nearest first (`HOSTILES`).
    pub hostiles: Vec<Near>,
    /// Endermen within 128.
    pub endermen: u32,
    /// The newest sound heard.
    pub sound: Option<String>,
    /// The planner's time for its last tick, microseconds.
    pub tick_us: Option<u64>,
    /// The cell the feet are in: the position floored, as the planner's `plan::path::feet_cell`
    /// has it (a rounded position reads one cell off: -18.3 is cell -19, not -18).
    #[serde(default)]
    pub cell: Option<[i32; 3]>,
    /// The block in the feet's cell (air on a full block; soul sand, a slab or a fluid when the
    /// feet are inside its cell) and the block under that cell. `None` where no column is loaded.
    #[serde(default)]
    pub feet_block: Option<String>,
    #[serde(default)]
    pub under: Option<String>,
    /// Lava and water in the body's two cells or against them.
    #[serde(default)]
    pub lava: Touch,
    #[serde(default)]
    pub water: Touch,
    /// The stack on the cursor, when there is one.
    #[serde(default)]
    pub cursor: Option<(String, i32)>,
    /// The time of day, 0 to 23,999 (`World::day_time`), when the server has told it.
    #[serde(default)]
    pub day_time: Option<i64>,
    /// The biome at the feet (issue 17b).
    #[serde(default)]
    pub biome: Option<String>,
    /// The nearest enderman within `ENDERMAN_WATCH` that ticks, and the nearest frozen one.
    #[serde(default)]
    pub enderman_ticking: Option<f64>,
    #[serde(default)]
    pub enderman_frozen: Option<f64>,
    /// Uses left over every pickaxe held, and the filler (bridge blocks) held.
    #[serde(default)]
    pub pickaxe_uses: Option<i32>,
    #[serde(default)]
    pub filler: Option<i64>,
    /// Issue 18b: ender pearls held, the bot's kills so far (`World::kills`), angry endermen
    /// within `ENDERMAN_WATCH` that tick and that are frozen, and uses left over every sword.
    #[serde(default)]
    pub pearls: Option<i32>,
    #[serde(default)]
    pub kills: Option<u64>,
    #[serde(default)]
    pub angry_ticking: Option<u32>,
    #[serde(default)]
    pub angry_frozen: Option<u32>,
    #[serde(default)]
    pub sword_uses: Option<i32>,
    /// Issue 19b: eyes of ender held, and the end portal frames on record within
    /// `world::stage::FRAME_NEAR`.
    #[serde(default)]
    pub eyes: Option<i32>,
    #[serde(default)]
    pub frames: Option<u32>,
    // >>> E1 (issue 20b): in the End only.
    /// The dragon's health and phase, the crystals alive, the feet's distance to the fountain,
    /// the feet over the ground (`over_ground`; `None` over the void) and endermen within 16.
    #[serde(default)]
    pub dragon_health: Option<f32>,
    #[serde(default)]
    pub dragon_phase: Option<i32>,
    #[serde(default)]
    pub crystals: Option<u32>,
    #[serde(default)]
    pub fountain: Option<f64>,
    #[serde(default)]
    pub over: Option<i32>,
    #[serde(default)]
    pub endermen_16: Option<u32>,
    // <<< E1
    /// The crystals left as the stages count them (`world::stage::crystals_left`: the ten spikes
    /// less those destroyed, memory's record, so a resume carries it), in the End only (E9,
    /// issue 20 part 2). `crystals` is the ones tracked now.
    #[serde(default)]
    pub crystals_left: Option<u32>,
}

/// How a fluid touches the body (the feet's cell and the one over it).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Touch {
    /// In neither cell and in no cell against them.
    #[default]
    No,
    /// In a cell against the body: one of the four beside either cell, the one under the feet or
    /// the one over the head.
    Beside,
    /// In the feet's cell or the head's.
    In,
}

/// The cell a position's feet are in: each coordinate floored. The same as the planner's
/// `plan::path::feet_cell` (a test in `results` holds the two together).
pub fn feet_cell(pos: [f64; 3]) -> [i32; 3] {
    [
        pos[0].floor() as i32,
        pos[1].floor() as i32,
        pos[2].floor() as i32,
    ]
}

/// How `fluid` touches a body whose feet are in `cell`.
fn touch(w: &World, cell: [i32; 3], fluid: BlockKind) -> Touch {
    let is = |dx: i32, dy: i32, dz: i32| {
        w.block_at(BlockPos::new(cell[0] + dx, cell[1] + dy, cell[2] + dz)) == Some(fluid)
    };
    if is(0, 0, 0) || is(0, 1, 0) {
        return Touch::In;
    }
    let beside = [(1, 0), (-1, 0), (0, 1), (0, -1)]
        .iter()
        .any(|(dx, dz)| is(*dx, 0, *dz) || is(*dx, 1, *dz));
    if beside || is(0, -1, 0) || is(0, 2, 0) {
        Touch::Beside
    } else {
        Touch::No
    }
}

fn block_name(w: &World, x: i32, y: i32, z: i32) -> Option<String> {
    w.block_at(BlockPos::new(x, y, z))
        .map(|b| b.name().unwrap_or("unknown").to_owned())
}

/// Blocks a body's cell holds when nothing is in it.
const AIRS: [&str; 3] = ["air", "cave_air", "void_air"];

impl Senses {
    /// Issue 17's part of the row, each piece only when known (older rows print nothing):
    /// ` biome soul_sand_valley ender 31 ticking, 70 frozen picks 144 filler 620`.
    pub fn travel(&self) -> String {
        let mut s = String::new();
        if let Some(b) = &self.biome {
            s.push_str(&format!(" biome {b}"));
        }
        match (self.enderman_ticking, self.enderman_frozen) {
            (None, None) => {}
            (t, f) => {
                let part = |d: Option<f64>, what: &str| d.map(|d| format!("{d:.0} {what}"));
                let parts: Vec<String> = [part(t, "ticking"), part(f, "frozen")]
                    .into_iter()
                    .flatten()
                    .collect();
                s.push_str(&format!(" ender {}", parts.join(", ")));
            }
        }
        if let Some(p) = self.pickaxe_uses {
            s.push_str(&format!(" picks {p}"));
        }
        if let Some(f) = self.filler {
            s.push_str(&format!(" filler {f}"));
        }
        s
    }

    /// Issue 18's part of the row, each piece only when it says something (older rows print
    /// nothing): ` pearls 3 kills 5 angry 1 ticking, 0 frozen swords 120`.
    pub fn hunt(&self) -> String {
        let mut s = String::new();
        if let Some(p) = self.pearls.filter(|p| *p > 0) {
            s.push_str(&format!(" pearls {p}"));
        }
        if let Some(k) = self.kills.filter(|k| *k > 0) {
            s.push_str(&format!(" kills {k}"));
        }
        let (t, f) = (
            self.angry_ticking.unwrap_or(0),
            self.angry_frozen.unwrap_or(0),
        );
        if t + f > 0 {
            s.push_str(&format!(" angry {t} ticking, {f} frozen"));
        }
        if let Some(u) = self.sword_uses {
            s.push_str(&format!(" swords {u}"));
        }
        s
    }

    /// Issue 19's part of the row, each piece only when it says something (older rows print
    /// nothing): ` overworld eyes 12 frames 9`. The dimension is said outside the Nether.
    pub fn eyes(&self) -> String {
        let mut s = String::new();
        if let Some(d) = self
            .dimension
            .as_deref()
            .filter(|d| *d != "minecraft:the_nether")
        {
            s.push_str(&format!(" {}", d.trim_start_matches("minecraft:")));
        }
        if let Some(e) = self.eyes.filter(|e| *e > 0) {
            s.push_str(&format!(" eyes {e}"));
        }
        if let Some(f) = self.frames.filter(|f| *f > 0) {
            s.push_str(&format!(" frames {f}"));
        }
        s
    }

    // >>> E1 (issue 20b)
    /// Issue 20's part of the row, in the End only (older rows print nothing):
    /// ` dragon 180.0 p0 crystals 7 fountain 34.2 over 12 endermen16 1` (`over void` with no
    /// ground below).
    pub fn end(&self) -> String {
        let Some(f) = self.fountain else {
            return String::new();
        };
        let mut s = String::new();
        if let Some(h) = self.dragon_health {
            s.push_str(&format!(" dragon {h:.1}"));
            if let Some(p) = self.dragon_phase {
                s.push_str(&format!(" p{p}"));
            }
        }
        if let Some(c) = self.crystals {
            s.push_str(&format!(" crystals {c}"));
        }
        if let Some(c) = self.crystals_left {
            s.push_str(&format!(" left {c}"));
        }
        s.push_str(&format!(" fountain {f:.1}"));
        match self.over {
            Some(o) => s.push_str(&format!(" over {o}")),
            None => s.push_str(" over void"),
        }
        if let Some(n) = self.endermen_16.filter(|n| *n > 0) {
            s.push_str(&format!(" endermen16 {n}"));
        }
        s
    }
    // <<< E1

    /// The feet's cell: as recorded, else the position floored (rows from before the cell was
    /// kept).
    pub fn feet(&self) -> Option<[i32; 3]> {
        self.cell.or_else(|| self.pos.map(feet_cell))
    }

    /// Where the bot stands, in plain terms: `cell 30,31,-19 on netherrack, lava beside`,
    /// `cell 56,59,-115 over air (in the air or on an edge)`, or
    /// `cell 30,31,-19 in soul_sand over netherrack` when the feet's own cell is not empty.
    /// Rows from before the blocks were kept give the cell alone.
    pub fn standing(&self) -> String {
        let Some([x, y, z]) = self.feet() else {
            return "cell -".to_owned();
        };
        let mut s = format!("cell {x},{y},{z}");
        let under = self.under.as_deref().unwrap_or("?");
        match self.feet_block.as_deref() {
            // Nothing under the cell: in the air, or the body's box rests on a block beside.
            Some(b) if AIRS.contains(&b) && AIRS.contains(&under) => {
                s.push_str(" over air (in the air or on an edge)");
            }
            Some(b) if AIRS.contains(&b) => s.push_str(&format!(" on {under}")),
            Some(b) => s.push_str(&format!(" in {b} over {under}")),
            None if self.under.is_some() => s.push_str(&format!(" in ? over {under}")),
            None => {}
        }
        for (name, t) in [("lava", self.lava), ("water", self.water)] {
            match t {
                // Said already when the feet's own cell is the fluid.
                Touch::In if self.feet_block.as_deref() == Some(name) => {}
                Touch::In => s.push_str(&format!(", in {name}")),
                Touch::Beside => s.push_str(&format!(", {name} beside")),
                Touch::No => {}
            }
        }
        s
    }
}

/// The senses row for `w`, with the task and its reason as the planner holds them.
pub fn senses(w: &World, task: &str, why: &str, tick_us: Option<u64>) -> Senses {
    let in_end = world::stage::in_end(w);
    let bot = feet(w);
    let here = bot.unwrap_or([0.0; 3]);
    let cell = bot.map(feet_cell);
    let held_slot = 36 + i16::from(w.held);
    let held = w
        .inventory
        .slots
        .get(&held_slot)
        .map(|(i, _)| item_name(*i));
    let mut tools: Vec<(String, i32)> = w
        .inventory
        .slots
        .iter()
        .filter_map(|(slot, (item, _))| w.inventory.uses_left(*slot).map(|u| (item_name(*item), u)))
        .collect();
    tools.sort();
    let free_slots = (9..=44)
        .filter(|s| !w.inventory.slots.contains_key(s))
        .count() as u32;
    let mut hostiles: Vec<Near> = w
        .entities
        .values()
        .filter(|e| !e.dead && hostile(e.kind))
        .filter_map(|e| {
            let d = dist(e.pos, here);
            (d <= WATCH).then(|| Near {
                kind: e.kind,
                d,
                health: e.health(),
                signs: signs(e, here),
                held: e.equipment.get(&0).map(|i| item_name(*i)),
            })
        })
        .collect();
    hostiles.sort_by(|a, b| a.d.total_cmp(&b.d));
    hostiles.truncate(HOSTILES);
    let endermen = w
        .entities
        .values()
        .filter(|e| !e.dead && e.kind == EntityKind::Enderman && dist(e.pos, here) <= 128.0)
        .count() as u32;
    let nearest = |ticking: bool| {
        w.entities
            .values()
            .filter(|e| !e.dead && e.kind == EntityKind::Enderman)
            .filter(|e| world::ticks(w, e.pos) == ticking)
            .map(|e| dist(e.pos, here))
            .filter(|d| *d <= ENDERMAN_WATCH)
            .min_by(f64::total_cmp)
    };
    let angry = |ticking: bool| {
        w.entities
            .values()
            .filter(|e| !e.dead && e.kind == EntityKind::Enderman)
            .filter(|e| meta_bool(e, ENDERMAN_CREEPY) && dist(e.pos, here) <= ENDERMAN_WATCH)
            .filter(|e| world::ticks(w, e.pos) == ticking)
            .count() as u32
    };
    Senses {
        at: w.session.server_ticks,
        pos: bot,
        dimension: w.dimension.as_ref().map(|d| d.name.clone()),
        health: w.player.health,
        food: w.player.food,
        saturation: w.player.saturation,
        air: w.player.air,
        on_fire: w.player.on_fire,
        held_uses: w.inventory.uses_left(held_slot),
        held,
        tools,
        free_slots,
        task: task.to_owned(),
        why: why.to_owned(),
        hostiles,
        endermen,
        sound: w.sounds.last().map(|s| s.name.clone()),
        tick_us,
        cell,
        feet_block: cell.and_then(|[x, y, z]| block_name(w, x, y, z)),
        under: cell.and_then(|[x, y, z]| block_name(w, x, y - 1, z)),
        lava: cell.map_or(Touch::No, |c| touch(w, c, BlockKind::Lava)),
        water: cell.map_or(Touch::No, |c| touch(w, c, BlockKind::Water)),
        cursor: w.inventory.carried.map(|(item, n)| (item_name(item), n)),
        day_time: w.day_time(),
        biome: w.feet_biome().map(str::to_owned),
        enderman_ticking: nearest(true),
        enderman_frozen: nearest(false),
        pickaxe_uses: Some(pickaxe_uses(w)),
        filler: Some(filler(w)),
        pearls: Some(
            i32::try_from(w.inventory.count(data::ItemKind::EnderPearl)).unwrap_or(i32::MAX),
        ),
        kills: Some(w.kills),
        angry_ticking: Some(angry(true)),
        angry_frozen: Some(angry(false)),
        sword_uses: Some(sword_uses(w)),
        eyes: Some(i32::try_from(w.inventory.count(data::ItemKind::EnderEye)).unwrap_or(i32::MAX)),
        frames: Some(if w.memory.has_seen(BlockKind::EndPortalFrame) {
            u32::try_from(world::stage::frames_near(w).len()).unwrap_or(u32::MAX)
        } else {
            0
        }),
        // >>> E1 (issue 20b)
        dragon_health: in_end.then(|| dragon_health(w)).flatten(),
        dragon_phase: in_end.then(|| dragon_phase(w)).flatten(),
        crystals: in_end
            .then(|| u32::try_from(world::stage::crystals_alive(w)).unwrap_or(u32::MAX)),
        fountain: in_end.then(|| world::stage::from_fountain(w)).flatten(),
        over: in_end.then(|| over_ground(w)).flatten(),
        crystals_left: world::stage::crystals_left(w).map(|n| u32::try_from(n).unwrap_or(u32::MAX)),
        endermen_16: in_end.then(|| {
            w.entities
                .values()
                .filter(|e| !e.dead && e.kind == EntityKind::Enderman && dist(e.pos, here) <= 16.0)
                .count() as u32
        }),
        // <<< E1
    }
}

impl Fact {
    /// The fact as one line of a case file or a timeline.
    pub fn line(&self) -> String {
        let kind = |k: &EntityKind| k.name().unwrap_or("unknown").to_owned();
        match self {
            Fact::Health { from, to } => match from {
                Some(f) => format!("health {f:.1} -> {to:.1}"),
                None => format!("health {to:.1}"),
            },
            Fact::Food { from, to } => match from {
                Some(f) => format!("food {f} -> {to}"),
                None => format!("food {to}"),
            },
            Fact::Hit {
                damage_type,
                by,
                health,
            } => format!(
                "hit {damage_type}{}{}",
                by.as_ref().map_or_else(String::new, |b| format!(" by {b}")),
                health.map_or_else(String::new, |h| format!(", health {h:.1}"))
            ),
            Fact::Alight { cause } => format!("alight ({cause})"),
            Fact::Item { item, from, to } => format!("item {item} {from} -> {to}"),
            Fact::ToolWorn { item, uses_left } => {
                format!("tool worn: {item}, {uses_left} uses left")
            }
            Fact::ToolBroke { item } => format!("tool broke: {item}"),
            Fact::ToolGone { item, uses_left } => format!(
                "tool gone: {item} with {uses_left} uses left (dropped or lost, not broken)"
            ),
            Fact::HealthDrop {
                by,
                to,
                hostile,
                on_fire,
            } => format!(
                "health dropped {by:.1} to {to:.1} with no damage event; nearest hostile {}; on fire {}",
                hostile
                    .as_ref()
                    .map_or_else(|| "none".to_owned(), |(k, d)| format!("{k} at {d:.1}")),
                if *on_fire { "yes" } else { "no" }
            ),
            Fact::Cursor { item, count } => match item {
                Some(i) => format!("on cursor: {i} {count}"),
                None => "cursor empty".to_owned(),
            },
            Fact::Dimension { to, from } => match from {
                Some(f) => format!("dimension {f} -> {to}"),
                None => format!("dimension {to}"),
            },
            Fact::Stage { reached } => format!("stage {reached}"),
            Fact::Death => "died".to_owned(),
            Fact::Won => "won".to_owned(),
            Fact::Effect { name, on } => {
                format!("effect {name} {}", if *on { "on" } else { "off" })
            }
            Fact::Sound { name, d } => match d {
                Some(d) => format!("heard {name} at {d:.0}"),
                None => format!("heard {name}"),
            },
            Fact::LevelEvent { name, d } => format!("level event {name} at {d:.0}"),
            Fact::Chat { via, text } => format!("{via}: {text}"),
            Fact::Appear { id, kind: k, d } => format!("{}#{id} appeared at {d:.0}", kind(k)),
            Fact::Gone { id, kind: k } => format!("{}#{id} gone", kind(k)),
            Fact::Sign {
                id,
                kind: k,
                sign,
                d,
            } => format!("{}#{id} {sign} at {d:.0}", kind(k)),
            Fact::Shot { kind: k, closest } => format!("{} passed within {closest:.1}", kind(k)),
            Fact::Landed { id, kind: k } => format!("blow landed on {}#{id}", kind(k)),
            Fact::Night { day_time } => format!("night fell (day time {day_time})"),
            Fact::Day { day_time } => format!("day broke (day time {day_time})"),
            Fact::Biome { name } => format!("feet biome {name}"),
            Fact::Enderman { id, d, ticking } => format!(
                "enderman#{id} at {d:.0}, {}",
                if *ticking { "ticking" } else { "frozen" }
            ),
            Fact::EndermanStill { id, d, for_ticks } => format!(
                "enderman#{id} at {d:.0} ticks by distance but has not moved for {for_ticks} ticks"
            ),
            Fact::ForestSeen { column: (cx, cz) } => {
                format!("warped forest known from blocks seen: chunk column {cx},{cz}")
            }
            Fact::SoulValleySeen { column: (cx, cz) } => {
                format!("soul sand valley known from blocks seen: chunk column {cx},{cz}")
            }
            Fact::EndermanAngry { id, d, ticking } => format!(
                "enderman#{id} angry at {d:.1}, {}",
                if *ticking { "ticking" } else { "frozen" }
            ),
            Fact::EndermanHit { id, before, after } => {
                format!("enderman#{id} hit: health {before:.1} -> {after:.1}")
            }
            Fact::EndermanKilled { id, by_bot } => format!(
                "enderman#{id} killed{}",
                if *by_bot {
                    " by the bot"
                } else {
                    ", not by the bot"
                }
            ),
            Fact::PearlDropped { id, pos } => format!(
                "pearl dropped (item#{id}) at {:.1},{:.1},{:.1}",
                pos[0], pos[1], pos[2]
            ),
            Fact::PearlPicked { id, amount } => format!("pearl picked up (item#{id}, {amount})"),
            Fact::BlockTaken { cell: [x, y, z] } => {
                format!("block taken at {x},{y},{z} (no dig of ours)")
            }
            Fact::EyeThrown { id, from, yaw } => format!(
                "eye thrown (eye#{id}) from {:.1},{:.1},{:.1}{}",
                from[0],
                from[1],
                from[2],
                yaw.map_or_else(String::new, |y| format!(", spawn heading {y:.1}"))
            ),
            Fact::EyeFlight {
                id,
                first,
                last,
                heading,
                samples,
                ticks,
            } => format!(
                "eye#{id} flew {:.1},{:.1},{:.1} -> {:.1},{:.1},{:.1}, heading {}, {samples} samples in {ticks} ticks",
                first[0],
                first[1],
                first[2],
                last[0],
                last[1],
                last[2],
                heading.map_or_else(|| "-".to_owned(), |h| format!("{h:.1}"))
            ),
            Fact::EyeEnded { id, dropped, at } => format!(
                "eye#{id} {} at {:.1},{:.1},{:.1}",
                if *dropped { "dropped" } else { "shattered" },
                at[0],
                at[1],
                at[2]
            ),
            Fact::EyePicked { id, amount } => format!("eye picked up (item#{id}, {amount})"),
            Fact::FrameSeen { cell: [x, y, z] } => format!("end portal frame seen at {x},{y},{z}"),
            Fact::SilverfishWoken { id, d } => format!("silverfish#{id} woken at {d:.1}"),
            // >>> E1 (issue 20b)
            Fact::EyePlaced { cell } => {
                format!("eye placed in frame {},{},{}", cell[0], cell[1], cell[2])
            }
            Fact::PortalEntered { from, at } => format!(
                "end entered from {} at {:.1},{:.1},{:.1}",
                from.as_deref()
                    .unwrap_or("?")
                    .trim_start_matches("minecraft:"),
                at[0],
                at[1],
                at[2]
            ),
            Fact::CrystalSeen { id, pos, d } => format!(
                "crystal#{id} seen at {:.1},{:.1},{:.1} ({d:.1} off)",
                pos[0], pos[1], pos[2]
            ),
            Fact::CrystalGone { id, pos, how } => format!(
                "crystal#{id} gone ({how}) at {:.1},{:.1},{:.1}",
                pos[0], pos[1], pos[2]
            ),
            Fact::PillarUp { height, cell } => format!(
                "pillar up {height} over the ground at {},{},{}",
                cell[0], cell[1], cell[2]
            ),
            Fact::PillarDown { height } => format!("pillar down from {height}"),
            Fact::Strike { target, id, reach } => match reach {
                Some(r) => format!("strike {target}#{id} at reach {r:.2}"),
                None => format!("strike {target}#{id}"),
            },
            Fact::DragonPhase { from, to } => match from {
                Some(f) => format!("dragon phase {f} -> {to}"),
                None => format!("dragon phase {to}"),
            },
            Fact::DragonHit { part, health } => format!(
                "dragon hit on the {} (health {})",
                part.as_deref().unwrap_or("?"),
                health.map_or_else(|| "?".to_owned(), |h| format!("{h:.1}"))
            ),
            Fact::DragonBlow { lost, damage_type } => {
                format!("dragon blow: {lost:.1} health lost ({damage_type})")
            }
            Fact::Breath { id, radius, dist } => {
                format!("breath cloud#{id} radius {radius:.1}, {dist:.1} from its edge")
            }
            Fact::GazeNearMiss { id, d, stare } => format!(
                "gaze {} enderman#{id} at {d:.1}",
                if *stare {
                    "in the stare cone of"
                } else {
                    "near"
                }
            ),
            Fact::DragonDead => "dragon dead".to_owned(),
            Fact::Credits { dragon_dead } => match dragon_dead {
                Some(t) => format!("credits (the dragon seen dead at {t})"),
                None => "credits (the dragon not seen dead)".to_owned(),
            },
            // <<< E1
            Fact::SprintServer { on } => {
                format!("server: sprint {}", if *on { "on" } else { "off" })
            }
            Fact::Sprint { on: true, why } => format!("sprint on ({why})"),
            Fact::Sprint { on: false, why } => format!("sprint off ({why})"),
        }
    }
}
