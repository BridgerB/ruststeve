//! Issue 17 (17c and 17e, `docs/problems/endermen-leave.md`): after the rods, on to an enderman.
//!
//! With 7 rods held the spawner task walks away from the spawner as it did in issue 16; then
//! `endermen_task` takes over (it no longer ends the race):
//! - a ticking enderman within 32 is the stage: the bot seals itself into a 2-high pocket (the
//!   cook's hut, `cover::hut_block`), eyes down, and only then is the aim met;
//! - hurt under `travel_health` with a ticking blaze or ghast seeing it, it rests in cover first;
//!   at or under the danger line with nothing to heal with it holds in a pocket (never a death as
//!   a plan);
//! - one provisions note before the first leg (pickaxe uses, filler, rods, food);
//! - a ticking enderman within 64 is walked to within 32, a frozen one seen within 128 walked
//!   toward (it ticks once in range), each given up after 3 walks or 6,000 ticks;
//! - in a warped forest with none near, the bot waits `forest_wait_ticks` for one to spawn;
//! - inside the fortress's brick footprint a plain walk out first (no stairs down over the lava
//!   sea's thin floors); then the heading.
//!
//! Endermen stay neutral: the defend row never takes one as its foe (`defend::foe`). A provoked
//! one (its creepy flag, or one hit the bot) is answered by sealing the pocket where the bot
//! stands (`Plan::ender_threat`): an enderman is 2.9 tall, a mob's strike needs a line of sight,
//! and a teleport needs three free cells. The look is kept off every enderman by `look`.

use data::{BlockKind, EntityKind, ItemKind};
use physics::Controls;
use protocol::metadata::MetaValue;
use world::World;

use crate::path::{self, Cell, Goal};
use crate::walk::{Arrive, Walk};
use crate::{Plan, Task, cover, craft, hold};

/// The stage's distance: the feet within this of a ticking enderman (the issue's done-condition).
pub const ENDERMAN_NEAR: f64 = world::stage::ENDERMAN_NEAR;
/// A ticking enderman within this is walked to (it is already in the simulated chunks).
pub const APPROACH_FROM: f64 = 64.0;
/// A frozen enderman seen within this is walked toward (clojurecraft's sighting chase, 32 to 128).
pub const SIGHTING_RANGE: f64 = 128.0;
/// The approach walk's goal, by the route's measure (octile plus height runs longer than the
/// straight line): inside `ENDERMAN_NEAR` with room.
pub const APPROACH_GOAL: f64 = 26.0;
/// Walks begun toward one enderman, and ticks since the first, before it is given up
/// (clojurecraft's 3 failures or 5 minutes, counted in server ticks).
pub const CHASE_TRIES: u32 = 3;
pub const CHASE_TICKS: u64 = 6000;
/// Blocks one pocket may take: ten round a body in one column, more where it spans two (the hut's
/// `HUT_MAX`).
pub const POCKET_MAX: u32 = 24;
/// Chunk columns round the feet whose nether bricks give the fortress's footprint.
pub const FORTRESS_CHUNKS: i32 = 8;
/// Blocks round the bricks' bounds that still count as the footprint (a bridge's edge).
pub const FORTRESS_PAD: i32 = 4;
/// Ticks between two notes of the same standing condition (a hold, a rest).
const NOTE_EVERY: u64 = 600;
/// Ticks between two "look kept off" notes (the count is in each).
const LOOK_NOTE_EVERY: u64 = 200;
/// Ticks after a fruitless search for a way to a known place before the next is made.
const GO_EVERY: u64 = 100;
/// Ticks a valley target search is kept before it is made again.
const VALLEY_EVERY: u64 = 100;
/// Ticks between two progress notes of the valley wait.
const VALLEY_NOTE_EVERY: u64 = 1200;
/// Ticks the fortress footprint is kept before it is read again from memory's brick index.
const FORTRESS_EVERY: u64 = 600;
/// The pitch the bot holds in its pocket: straight down at its own floor.
pub const EYES_DOWN: f32 = 90.0;
/// The enderman's creepy flag (`EnderMan.DATA_CREEPY`, metadata index 17, a boolean): set while
/// it has a target.
pub const CREEPY_INDEX: u8 = 17;
/// Its stared-at flag (`DATA_STARED_AT`, index 18): set at once by a held stare, before any
/// anger (probe 45472254333). Seen true, it is a near miss of the look filter, counted.
pub const STARED_INDEX: u8 = 18;
/// What counts as filler for the provisions note: the blocks a bridge, a seal or a pocket uses.
pub const FILLER: &[ItemKind] = &[
    ItemKind::Netherrack,
    ItemKind::Cobblestone,
    ItemKind::CobbledDeepslate,
    ItemKind::Granite,
    ItemKind::Diorite,
    ItemKind::Andesite,
    ItemKind::Dirt,
    ItemKind::Tuff,
    ItemKind::Blackstone,
    ItemKind::Basalt,
    ItemKind::NetherBricks,
];
/// Filler kept unspent (the plan's keep: issue 17d raises the bridge reserve to it).
pub const FILLER_KEEP: i32 = 32;

/// Where the race stands after the rods (issue 18, docs/problems/pearls.md): on the way to a
/// ticking enderman (issue 17's stage), on the way to a warped forest, or hunting in one.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum EnderPhase {
    /// To a ticking enderman within 32 (issue 17).
    #[default]
    Reach,
    /// The endermen stage passed: to a warped forest (issue 18c, docs/problems/pearls-travel.md).
    Travel,
    /// The feet in a warped forest: the base, the burrow and the hunt (issue 18d to 18f).
    Hunt,
}

/// Blocks a forest target may lie above or below the feet before a lower or higher one is
/// taken only for want of one at the bot's level: the valley's `VALLEY_DY` (issue 17's valley
/// lesson: a place is chosen by the cell stood on, not by its column).
pub const FOREST_DY: i32 = VALLEY_DY;
/// Known forest columns looked through for a target, nearest first.
pub const FOREST_COLUMNS: usize = 32;
/// Cells along a side of the pocket past its wall read for the way out (`OPEN_RUN`).
pub const EXIT_RUN: i32 = 6;
/// Digs the way out of the pocket may take before it is given up (two cells a side; a third
/// for a block that came back, a fourth for a second side).
pub const EXIT_DIGS: u32 = 4;
/// The kinds whose side of the pocket is not opened while another side is free of them: they
/// shoot or strike on sight a bot with no gold on (`cover::FAR_SHOOTERS`, `NEAR_SHOOTERS`,
/// `path::SHUN_KINDS`), within `cover::COVER_RANGE`.
pub const EXIT_THREATS: &[EntityKind] = &[
    EntityKind::Ghast,
    EntityKind::Blaze,
    EntityKind::Skeleton,
    EntityKind::WitherSkeleton,
    EntityKind::Piglin,
    EntityKind::PiglinBrute,
    EntityKind::Hoglin,
    EntityKind::MagmaCube,
];

/// The issue-17 planner state, kept in one field of `Plan` (`Plan::ender`).
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct EnderState {
    /// Issue 18: where the race stands after the rods.
    #[serde(default)]
    pub phase: EnderPhase,
    /// The pocket's way out at the hand-over: the side and the cells (head first) to open, and
    /// the digs begun.
    #[serde(default)]
    pub exit: Option<((i32, i32), Vec<Cell>)>,
    #[serde(default)]
    pub exit_digs: u32,
    /// When the travel's progress was last noted (health, pickaxe uses, filler).
    #[serde(default)]
    pub travel_leg_at: Option<u64>,
    /// The last forest target found and when, and when nylium refused for no room was noted.
    #[serde(default)]
    pub forest_cache: Option<(u64, Option<Cell>)>,
    #[serde(default)]
    pub forest_refused_at: Option<u64>,
    /// The leave walk from the spawner is over: `endermen_task` from here on.
    #[serde(default)]
    pub left: bool,
    #[serde(default)]
    pub provisions_noted: bool,
    /// When a ticking enderman was first within `ENDERMAN_NEAR`.
    #[serde(default)]
    pub reached_at: Option<u64>,
    /// Endermen walked toward: id, the first walk's tick, walks begun.
    #[serde(default)]
    pub chases: Vec<(i32, u64, u32)>,
    #[serde(default)]
    pub given_up: Vec<i32>,
    /// The forest wait: when it began, and whether it is over.
    #[serde(default)]
    pub forest_since: Option<u64>,
    #[serde(default)]
    pub forest_over: bool,
    /// The soul sand valley wait: when it began, whether it is over, the walks toward the valley,
    /// the nook's open side, and when its progress was last noted.
    #[serde(default)]
    pub valley_since: Option<u64>,
    #[serde(default)]
    pub valley_over: bool,
    #[serde(default)]
    pub place_tries: Vec<(Cell, u32)>,
    #[serde(default)]
    pub nook_open: Option<(Cell, (i32, i32))>,
    #[serde(default)]
    pub valley_noted_at: Option<u64>,
    /// Soul sand columns refused as a fortress's wart rooms (no longer written; kept for old
    /// state), and when soul cells refused for bricks were last noted.
    #[serde(default)]
    pub valleys_refused: Vec<(i32, i32)>,
    #[serde(default)]
    pub valley_refused_at: Option<u64>,
    /// No search for a way to a known place before this tick (`GO_EVERY` after a fruitless one).
    #[serde(default)]
    pub go_next: u64,
    /// The last valley target found and when.
    #[serde(default)]
    pub valley_cache: Option<(u64, Option<Cell>)>,
    /// The pocket being sealed: where the feet were, blocks placed, when the next may go.
    #[serde(default)]
    pub pocket_cell: Option<Cell>,
    #[serde(default)]
    pub pocket_blocks: u32,
    #[serde(default)]
    pub pocket_at: u64,
    /// The provoked enderman answered now.
    #[serde(default)]
    pub threat: Option<i32>,
    /// Looks changed so as not to stare, and when the last was noted.
    #[serde(default)]
    pub looks_kept: u64,
    #[serde(default)]
    pub look_noted_at: Option<u64>,
    /// Endermen seen with the stared-at flag set: near misses, each noted once.
    #[serde(default)]
    pub near_misses: Vec<i32>,
    /// When a standing condition was last noted.
    #[serde(default)]
    pub held_noted_at: Option<u64>,
    #[serde(default)]
    pub recover_noted_at: Option<u64>,
    #[serde(default)]
    pub travel_noted_at: Option<u64>,
    /// The fortress's footprint (min x, min z, max x, max z) and when it was read.
    #[serde(default)]
    pub fortress: Option<(i32, i32, i32, i32)>,
    #[serde(default)]
    pub fortress_at: Option<u64>,
}

/// One enderman as the planner sees it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sighting {
    pub id: i32,
    pub pos: [f64; 3],
    /// Feet to feet.
    pub dist: f64,
    pub ticking: bool,
}

fn distance(a: [f64; 3], b: [f64; 3]) -> f64 {
    (0..3).map(|i| (a[i] - b[i]).powi(2)).sum::<f64>().sqrt()
}

/// Whether an entity at `pos` ticks: its chunk within the server's simulation distance of the
/// player's chunk on both axes (`world::ticks`).
pub fn ticking(w: &World, pos: [f64; 3]) -> bool {
    world::ticks(w, pos)
}

/// The live endermen the world holds, the nearest first.
pub fn endermen(w: &World) -> Vec<Sighting> {
    let Some(body) = w.body else {
        return Vec::new();
    };
    let mut v: Vec<Sighting> = w
        .entities
        .iter()
        .filter(|(_, e)| e.kind == EntityKind::Enderman && !e.dead)
        .map(|(id, e)| Sighting {
            id: *id,
            pos: e.pos,
            dist: distance(body.pos, e.pos),
            ticking: ticking(w, e.pos),
        })
        .collect();
    v.sort_by(|a, b| a.dist.total_cmp(&b.dist).then(a.id.cmp(&b.id)));
    v
}

/// The stage's test: a ticking enderman within `ENDERMAN_NEAR` of the feet
/// (`world::stage::enderman_near`, which `Stage::Endermen` reads with the rods).
pub fn near_ticking(w: &World) -> Option<Sighting> {
    let (id, dist) = world::stage::enderman_near(w)?;
    let pos = w.entities.get(&id)?.pos;
    Some(Sighting {
        id,
        pos,
        dist,
        ticking: true,
    })
}

/// The held pickaxes' uses left, summed (a stack that does not report wear counts its item's
/// full durability).
pub fn pickaxe_uses_left(w: &World) -> i32 {
    let picks = data::generated::tags::item_tag("pickaxes").unwrap_or(&[]);
    w.inventory
        .slots
        .iter()
        .filter(|(slot, (k, _))| **slot >= 1 && picks.contains(k))
        .filter_map(|(slot, _)| w.inventory.uses_left(*slot))
        .sum()
}

/// Filler blocks held (`FILLER`).
pub fn filler_held(w: &World) -> i32 {
    FILLER.iter().map(|k| craft::count(w, *k)).sum()
}

/// A provoked enderman: one whose creepy flag is set (it has a target, and its only target can
/// be the player), or, when one hit the bot within `defend::PROVOKED_TICKS`, the nearest. Within
/// `APPROACH_FROM`, live.
pub fn angry(w: &World) -> Option<i32> {
    let near = endermen(w);
    let creepy = near.iter().find(|s| {
        s.dist <= APPROACH_FROM
            && w.entities.get(&s.id).is_some_and(|e| {
                matches!(e.metadata.get(&CREEPY_INDEX), Some(MetaValue::Bool(true)))
            })
    });
    if let Some(s) = creepy {
        return Some(s.id);
    }
    crate::defend::provoked(w, EntityKind::Enderman)
        .then(|| near.iter().find(|s| s.dist <= APPROACH_FROM).map(|s| s.id))
        .flatten()
}

/// The bounds (min x, min z, max x, max z) of the nether bricks on record within
/// `FORTRESS_CHUNKS` of the feet: the fortress's footprint. `None` with none.
pub fn fortress_bounds(w: &World) -> Option<(i32, i32, i32, i32)> {
    let bricks = world::stage::bricks_near(w, FORTRESS_CHUNKS);
    let first = bricks.first()?;
    Some(bricks.iter().fold(
        (first.0, first.2, first.0, first.2),
        |(ax, az, bx, bz), c| (ax.min(c.0), az.min(c.2), bx.max(c.0), bz.max(c.2)),
    ))
}

/// Whether a cell is inside a footprint, with `FORTRESS_PAD` round it.
pub fn in_footprint((ax, az, bx, bz): (i32, i32, i32, i32), c: Cell) -> bool {
    c.0 >= ax - FORTRESS_PAD
        && c.0 <= bx + FORTRESS_PAD
        && c.2 >= az - FORTRESS_PAD
        && c.2 <= bz + FORTRESS_PAD
}

/// Whether the bot stands in a warped forest: the biome at its feet, or warped nylium under them.
pub fn in_forest(w: &World) -> bool {
    w.feet_biome() == Some("warped_forest")
        || w.block_under_player() == Some(BlockKind::WarpedNylium)
}

/// The signs a forest and a valley are known by, walked to (memory's sign record: cells seen
/// exposed).
pub const FOREST_KINDS: &[BlockKind] = &[BlockKind::WarpedNylium];
pub const VALLEY_KINDS: &[BlockKind] = &[BlockKind::SoulSand, BlockKind::SoulSoil];

/// Where to walk for a known column: over the seen sign of `kinds` in it nearest the feet (the
/// cell above it, where a body stands); with none on record, the column's centre at the feet's
/// height.
pub fn column_target(w: &World, (cx, cz): (i32, i32), kinds: &[BlockKind]) -> Option<Cell> {
    let body = w.body?;
    let feet = path::feet_cell(body.pos);
    let dim = w.dimension.as_ref().map(|d| d.name.clone())?;
    let near = kinds
        .iter()
        .flat_map(|k| {
            w.memory
                .indexed_in_column(&dim, cx, cz, *k)
                .unwrap_or_default()
        })
        .map(|c| (c.0, c.1 + 1, c.2))
        .min_by_key(|c| {
            (
                (c.0 - feet.0).pow(2) + (c.1 - feet.1).pow(2) + (c.2 - feet.2).pow(2),
                *c,
            )
        });
    Some(near.unwrap_or((cx * 16 + 8, feet.1, cz * 16 + 8)))
}

/// Whether the bot stands in a soul sand valley: the biome at its feet, soul sand or soul soil
/// under them, or a known valley's column; never inside a fortress's footprint.
pub fn in_valley(w: &World) -> bool {
    let Some(body) = w.body else {
        return false;
    };
    let f = path::stand_cell(w, body.pos);
    let column = (f.0 >> 4, f.2 >> 4);
    let valley = w.feet_biome() == Some("soul_sand_valley")
        || matches!(
            w.block_under_player(),
            Some(BlockKind::SoulSand | BlockKind::SoulSoil)
        )
        || w.memory.soul_valleys().contains(&column);
    valley && !bricks_beside(w, (f.0, f.1 - 1, f.2))
}

/// Blocks round a soul cell within which a nether brick seen makes it a fortress's (a nether-wart
/// room's floor, a bridge over the valley), not the valley's.
pub const BRICK_NEAR: i32 = 4;
/// Blocks a valley target may lie above or below the feet: the valley's floor at the bot's level,
/// not soul sand seen down at the lava sea under the fortress (race 45481633896: three bots walked
/// at soul cells at y 23 from y 61, NoProgress and NoPath, into wither skeletons).
pub const VALLEY_DY: i32 = 12;
/// Known valley columns looked through for a target, nearest first.
pub const VALLEY_COLUMNS: usize = 32;

/// Whether a nether brick on record lies within `BRICK_NEAR` of `c` (memory's brick index for
/// the columns round it).
pub fn bricks_beside(w: &World, c: Cell) -> bool {
    let Some(dim) = w.dimension.as_ref() else {
        return false;
    };
    let (x0, x1) = ((c.0 - BRICK_NEAR) >> 4, (c.0 + BRICK_NEAR) >> 4);
    let (z0, z1) = ((c.2 - BRICK_NEAR) >> 4, (c.2 + BRICK_NEAR) >> 4);
    (x0..=x1).any(|cx| {
        (z0..=z1).any(|cz| {
            // An old store has no index (`None`): its sightings are scanned, as `kind_near` does.
            w.memory
                .indexed_in_column(&dim.name, cx, cz, BlockKind::NetherBricks)
                .unwrap_or_else(|| {
                    w.memory
                        .in_column(&dim.name, cx, cz)
                        .into_iter()
                        .filter(|s| data::block_of_state(s.state) == Some(BlockKind::NetherBricks))
                        .map(|s| s.pos)
                        .collect()
                })
                .iter()
                .any(|b| {
                    (b.0 - c.0).abs() <= BRICK_NEAR
                        && (b.1 - c.1).abs() <= BRICK_NEAR
                        && (b.2 - c.2).abs() <= BRICK_NEAR
                })
        })
    })
}

/// The valley cell to walk to: over the seen soul sand or soul soil, in a known valley column,
/// within `VALLEY_DY` of the feet's height, with no nether brick seen within `BRICK_NEAR`, not
/// in `skip`'s reach, nearest the feet. With how many seen soul cells were refused for bricks.
pub fn valley_target(w: &World, skip: &[Cell]) -> (Option<Cell>, usize) {
    let (Some(body), Some(dim)) = (w.body, w.dimension.as_ref()) else {
        return (None, 0);
    };
    let feet = path::stand_cell(w, body.pos);
    let d2 = |c: Cell| (c.0 - feet.0).pow(2) + (c.1 - feet.1).pow(2) + (c.2 - feet.2).pow(2);
    let mut columns = w.memory.soul_valleys().to_vec();
    columns.sort_by_key(|k| {
        (
            (k.0 * 16 + 8 - feet.0).pow(2) + (k.1 * 16 + 8 - feet.2).pow(2),
            *k,
        )
    });
    let mut walled = 0;
    let mut best: Option<Cell> = None;
    for k in columns.into_iter().take(VALLEY_COLUMNS) {
        for kind in VALLEY_KINDS {
            for c in w
                .memory
                .indexed_in_column(&dim.name, k.0, k.1, *kind)
                .unwrap_or_default()
            {
                let at = (c.0, c.1 + 1, c.2);
                if (at.1 - feet.1).abs() > VALLEY_DY
                    || skip
                        .iter()
                        .any(|s| path::flat_distance(*s, at) <= PLACE_SAME)
                    || best.is_some_and(|b| (d2(b), b) <= (d2(at), at))
                {
                    continue;
                }
                if bricks_beside(w, c) {
                    walled += 1;
                    continue;
                }
                best = Some(at);
            }
        }
    }
    (best, walled)
}

/// The warped forest cell to walk to (issue 18c): over a seen warped nylium cell in a known forest
/// column, its standing cell and the one over it not solid (a body fits; roots and fungi are
/// walked through), not in `skip`'s reach; one within `FOREST_DY` of the feet's height first
/// (chosen by the cell stood on, issue 17's valley lesson), then the nearest. With how many seen
/// nylium cells were refused for no room over them.
pub fn forest_target(w: &World, skip: &[Cell]) -> (Option<Cell>, usize) {
    let (Some(body), Some(dim)) = (w.body, w.dimension.as_ref()) else {
        return (None, 0);
    };
    let feet = path::stand_cell(w, body.pos);
    let rank = |c: Cell| {
        (
            (c.1 - feet.1).abs() > FOREST_DY,
            (c.0 - feet.0).pow(2) + (c.1 - feet.1).pow(2) + (c.2 - feet.2).pow(2),
            c,
        )
    };
    let mut columns = w.memory.forests().to_vec();
    columns.sort_by_key(|k| {
        (
            (k.0 * 16 + 8 - feet.0).pow(2) + (k.1 * 16 + 8 - feet.2).pow(2),
            *k,
        )
    });
    let room = |c: Cell| {
        path::classify(w.state_at(world::BlockPos::new(c.0, c.1, c.2))) != path::Class::Solid
    };
    let mut shut = 0;
    let mut best: Option<Cell> = None;
    for k in columns.into_iter().take(FOREST_COLUMNS) {
        for kind in FOREST_KINDS {
            for c in w
                .memory
                .indexed_in_column(&dim.name, k.0, k.1, *kind)
                .unwrap_or_default()
            {
                let at = (c.0, c.1 + 1, c.2);
                if skip
                    .iter()
                    .any(|s| path::flat_distance(*s, at) <= PLACE_SAME)
                    || best.is_some_and(|b| rank(b) <= rank(at))
                {
                    continue;
                }
                if !room(at) || !room((at.0, at.1 + 1, at.2)) {
                    shut += 1;
                    continue;
                }
                best = Some(at);
            }
        }
    }
    (best, shut)
}

/// The side of the pocket round `feet` to open for the way on, and its cells to dig (the head's
/// first, then the feet's; only those not open already): never one with lava beside a cell it
/// opens; a side not facing (within 60 degrees) a threat (`EXIT_THREATS`, live, within
/// `cover::COVER_RANGE`) first, then the one with the longest run of standing cells past the
/// wall (up to `EXIT_RUN`), then the one nearest `toward`, then east, west, south, north. `None`
/// when every side has lava beside it.
pub fn exit_side(
    w: &World,
    feet: Cell,
    toward: Option<(i32, i32)>,
) -> Option<((i32, i32), Vec<Cell>)> {
    let kind = |c: Cell| w.block_at(world::BlockPos::new(c.0, c.1, c.2));
    let clear = |c: Cell| path::classify(w.state_at(world::BlockPos::new(c.0, c.1, c.2)));
    let centre = [
        f64::from(feet.0) + 0.5,
        f64::from(feet.1),
        f64::from(feet.2) + 0.5,
    ];
    let threats: Vec<[f64; 3]> = w
        .entities
        .values()
        .filter(|e| !e.dead && EXIT_THREATS.contains(&e.kind))
        .filter(|e| distance(centre, e.pos) <= cover::COVER_RANGE)
        .map(|e| e.pos)
        .collect();
    let faces = |(dx, dz): (i32, i32)| {
        threats
            .iter()
            .filter(|p| {
                let (vx, vz) = (p[0] - centre[0], p[2] - centre[2]);
                let len = (vx * vx + vz * vz).sqrt();
                len > 0.0 && (f64::from(dx) * vx + f64::from(dz) * vz) / len > 0.5
            })
            .count()
    };
    let run = |(dx, dz): (i32, i32)| {
        (2..=EXIT_RUN)
            .take_while(|k| {
                let c = (feet.0 + dx * k, feet.1, feet.2 + dz * k);
                clear(c) == path::Class::Clear
                    && clear((c.0, c.1 + 1, c.2)) == path::Class::Clear
                    && clear((c.0, c.1 - 1, c.2)) == path::Class::Solid
            })
            .count()
    };
    let lava_by = |c: Cell| {
        memory::neighbours(c)
            .iter()
            .any(|n| kind(*n) == Some(BlockKind::Lava))
    };
    let along = |(dx, dz): (i32, i32)| {
        toward.map_or(0, |(tx, tz)| {
            let (vx, vz) = (tx - feet.0, tz - feet.2);
            -(dx * vx + dz * vz)
        })
    };
    [(1, 0), (-1, 0), (0, 1), (0, -1)]
        .into_iter()
        .enumerate()
        .filter_map(|(i, d)| {
            let cells: Vec<Cell> = [1, 0]
                .iter()
                .map(|dy| (feet.0 + d.0, feet.1 + dy, feet.2 + d.1))
                .filter(|c| clear(*c) != path::Class::Clear)
                .collect();
            (!cells
                .iter()
                .any(|c| lava_by(*c) || kind(*c) == Some(BlockKind::Lava)))
            .then(|| {
                (
                    (faces(d), std::cmp::Reverse(run(d)), along(d), i),
                    (d, cells),
                )
            })
        })
        .min_by_key(|(k, _)| *k)
        .map(|(_, v)| v)
}

/// A known place's walk ends within this of its cell (issue 17's valley and forest walks).
pub const PLACE_GOAL: f64 = 4.0;
/// The travel's forest walk ends on the nylium cell itself, or beside it: the feet's biome is
/// the stage (`Stage::Forest`), and a walk stopped 4 short of a forest's edge stands outside it.
pub const FOREST_GOAL: f64 = 1.0;

/// Ticks between two notes of the trip's state (a minute of game time; a heading leg of 48
/// blocks is about one at a walk).
pub const TRAVEL_NOTE_EVERY: u64 = 1200;

/// What the trip has left, for its notes: health, food, pickaxe uses, filler, rods.
pub fn travel_state(w: &World, rules: Option<&crate::make::SurvivalRules>) -> String {
    format!(
        "health {:?} (danger {}), food {:?}, pickaxe uses {}, filler {}, rods {}",
        w.player.health,
        rules.map_or(0.0, |r| r.danger_health),
        w.player.food,
        pickaxe_uses_left(w),
        filler_held(w),
        craft::count(w, ItemKind::BlazeRod)
    )
}

/// Targets of `go_toward` within this (by the route's measure) are one place: its tries add up.
pub const PLACE_SAME: f64 = 8.0;

/// The kinds a nook's open side must not face at build time while another side is open.
pub const NOOK_SHOOTERS: &[EntityKind] = &[EntityKind::Ghast, EntityKind::Skeleton];

/// The side a nook at `feet` is left open on: the longest open run (`cover::open_runs`), but not
/// one facing (within 60 degrees) a ticking ghast or skeleton within `cover::COVER_RANGE` while a
/// side with any open run does not. With its run.
pub fn nook_side(w: &World, feet: Cell) -> ((i32, i32), usize) {
    let shooters: Vec<[f64; 3]> = w
        .entities
        .values()
        .filter(|e| !e.dead && NOOK_SHOOTERS.contains(&e.kind) && ticking(w, e.pos))
        .filter(|e| {
            let c = [
                f64::from(feet.0) + 0.5,
                f64::from(feet.1),
                f64::from(feet.2) + 0.5,
            ];
            distance(c, e.pos) <= cover::COVER_RANGE
        })
        .map(|e| e.pos)
        .collect();
    let faces = |(dx, dz): (i32, i32)| {
        shooters.iter().any(|p| {
            let (vx, vz) = (
                p[0] - f64::from(feet.0) - 0.5,
                p[2] - f64::from(feet.2) - 0.5,
            );
            let len = (vx * vx + vz * vz).sqrt();
            len > 0.0 && (f64::from(dx) * vx + f64::from(dz) * vz) / len > 0.5
        })
    };
    let runs = cover::open_runs(w, feet);
    let best = |safe: bool| {
        runs.iter()
            .rev()
            .filter(|r| !safe || (r.1 > 0 && !faces(r.0)))
            .max_by_key(|(_, n)| *n)
            .copied()
    };
    best(true).or_else(|| best(false)).unwrap_or(((1, 0), 0))
}

/// Whether a ticking blaze or ghast within `cover::COVER_RANGE` has a line to the body.
pub fn seen_by_far_shooter(w: &World) -> bool {
    let Some(body) = w.body else {
        return false;
    };
    let eyes: Vec<[f64; 3]> = w
        .entities
        .values()
        .filter(|e| !e.dead && cover::FAR_SHOOTERS.contains(&e.kind))
        .filter(|e| distance(e.pos, body.pos) <= cover::COVER_RANGE && ticking(w, e.pos))
        .map(|e| [e.pos[0], e.pos[1] + 1.2, e.pos[2]])
        .collect();
    !eyes.is_empty() && cover::seen_body(w, &eyes, body.pos)
}

/// What a step of the pocket did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pocket {
    /// A block placed or waited on.
    Building,
    /// No open cell round the body: sealed.
    Closed,
    /// It cannot be sealed: why.
    Cannot(&'static str),
}

impl Plan {
    /// Every tick's controls, their look kept off every enderman (`look::keep_off`); in the
    /// pocket at the stage, eyes down. The one place a look leaves the planner.
    pub(crate) fn keep_look(&mut self, w: &World, c: Controls) -> Controls {
        let c = if self.met && self.ender.reached_at.is_some() {
            Controls {
                pitch: Some(EYES_DOWN),
                ..c
            }
        } else {
            c
        };
        self.count_near_misses(w);
        let (kept, id) = crate::look::keep_off(w, c);
        if let Some(id) = id {
            self.ender.looks_kept += 1;
            let tick = w.ticks;
            if self
                .ender
                .look_noted_at
                .is_none_or(|t| tick >= t + LOOK_NOTE_EVERY)
            {
                self.ender.look_noted_at = Some(tick);
                let d = w
                    .body
                    .zip(w.entities.get(&i32::try_from(id).unwrap_or(i32::MAX)))
                    .map_or(-1.0, |(b, e)| distance(b.pos, e.pos));
                self.note(
                    tick,
                    format!("look kept off enderman {id}"),
                    format!(
                        "pitch {:.0} would stare at it {d:.1} off; sent {:.0}; {} looks kept in the run",
                        c.pitch.unwrap_or(w.player.pitch),
                        kept.pitch.unwrap_or(w.player.pitch),
                        self.ender.looks_kept
                    ),
                );
            }
        }
        kept
    }

    /// An enderman whose stared-at flag is set: noted once each (the filter should leave none).
    fn count_near_misses(&mut self, w: &World) {
        let stared: Vec<i32> = w
            .entities
            .iter()
            .filter(|(id, e)| {
                e.kind == EntityKind::Enderman
                    && matches!(e.metadata.get(&STARED_INDEX), Some(MetaValue::Bool(true)))
                    && !self.ender.near_misses.contains(id)
            })
            .map(|(id, _)| *id)
            .collect();
        for id in stared {
            self.ender.near_misses.push(id);
            let n = self.ender.near_misses.len();
            self.note(
                w.ticks,
                format!("near miss: enderman {id} stared at"),
                format!(
                    "its stared-at flag is set; {n} in the run; pitch {:.0}",
                    w.player.pitch
                ),
            );
        }
    }

    /// A provoked enderman: the bot seals a pocket where it stands and holds in it, eyes down,
    /// until the enderman is calm, gone or farther than `APPROACH_FROM`. Never a swing. Ahead of
    /// every task but the always-rows (an escape from lava still comes first).
    pub(crate) fn ender_threat(&mut self, w: &World, tick: u64) -> Option<Controls> {
        let Some(id) = angry(w) else {
            if let Some(old) = self.ender.threat.take() {
                self.note(
                    tick,
                    format!("enderman {old} calm or gone"),
                    "out of the pocket, on with the task".to_owned(),
                );
            }
            return None;
        };
        if self.ender.threat != Some(id) {
            self.ender.threat = Some(id);
            let creepy = w.entities.get(&id).is_some_and(|e| {
                matches!(e.metadata.get(&CREEPY_INDEX), Some(MetaValue::Bool(true)))
            });
            self.note(
                tick,
                format!("provoked enderman {id}: into a sealed pocket"),
                format!(
                    "{}; health {:?}; never a swing at an enderman (issue 17)",
                    if creepy {
                        "its creepy flag is set"
                    } else {
                        "an enderman hit the bot"
                    },
                    w.player.health
                ),
            );
        }
        let state = self.pocket_step(w, tick);
        if matches!(state, Pocket::Closed | Pocket::Cannot(_))
            && self
                .ender
                .held_noted_at
                .is_none_or(|t| tick >= t + NOTE_EVERY)
        {
            self.ender.held_noted_at = Some(tick);
            self.note(
                tick,
                format!("held from enderman {id}"),
                match state {
                    Pocket::Cannot(why) => format!("pocket not sealed: {why}; eyes down"),
                    Pocket::Closed | Pocket::Building => {
                        format!(
                            "in a sealed pocket of {} blocks, eyes down",
                            self.ender.pocket_blocks
                        )
                    }
                },
            );
        }
        Some(Controls {
            pitch: Some(EYES_DOWN),
            ..Controls::default()
        })
    }

    /// The next block of a pocket round the body (the cook's hut: the cells beside the feet and
    /// the head, then the roof), one a `SHELTER_EVERY_TICKS`.
    pub(crate) fn pocket_step(&mut self, w: &World, tick: u64) -> Pocket {
        self.shell_step(w, tick, None)
    }

    /// The pocket, or with `open` a nook (`cover::nook_block`): the same blocks, one side left.
    fn shell_step(&mut self, w: &World, tick: u64, open: Option<(i32, i32)>) -> Pocket {
        let Some(body) = w.body else {
            return Pocket::Building;
        };
        // On soul sand the feet sink into its cell: the body is counted from the cell over it.
        let feet = path::stand_cell(w, body.pos);
        let pos = [body.pos[0], f64::from(feet.1), body.pos[2]];
        if self.ender.pocket_cell != Some(feet) {
            self.ender.pocket_cell = Some(feet);
            self.ender.pocket_blocks = 0;
        }
        let threat = self
            .ender
            .threat
            .and_then(|id| w.entities.get(&id))
            .map(|e| e.pos);
        let next = match open {
            Some(side) => cover::nook_block(w, pos, side),
            None => cover::pocket_block(w, pos, threat),
        };
        let Some((cell, click, face)) = next else {
            return Pocket::Closed;
        };
        if !body.on_ground {
            return Pocket::Building;
        }
        if self.ender.pocket_blocks >= POCKET_MAX {
            return Pocket::Cannot("pocket given up");
        }
        let Some(item) = crate::SHELTER_BLOCKS
            .iter()
            .copied()
            .find(|k| craft::count(w, *k) > 0)
        else {
            return Pocket::Cannot("no block held for a pocket");
        };
        if w.window.is_some() && w.click.is_none() && self.acts.is_empty() {
            self.acts.push(world::Act::CloseWindow);
            return Pocket::Building;
        }
        if w.window.is_some()
            || w.click.is_some()
            || !self.acts.is_empty()
            || tick < self.ender.pocket_at
        {
            return Pocket::Building;
        }
        match hold::toward(w, item) {
            hold::Hold::Held => {
                self.ender.pocket_at = tick + crate::SHELTER_EVERY_TICKS;
                self.ender.pocket_blocks += 1;
                self.acts.push(world::Act::UseOn {
                    pos: world::BlockPos::new(click.0, click.1, click.2),
                    face,
                });
                self.note(
                    tick,
                    format!("pocket block {} at {cell:?}", self.ender.pocket_blocks),
                    format!("a 2-high pocket round the feet {feet:?}; on {click:?} face {face}"),
                );
                Pocket::Building
            }
            hold::Hold::Act(a) => {
                self.acts.push(a);
                Pocket::Building
            }
            hold::Hold::Wait => Pocket::Building,
            hold::Hold::Missing => Pocket::Cannot("no block held for a pocket"),
        }
    }

    /// Issue 17's task, after the leave walk: the stage and its safe hold, recovery, the
    /// provisions note, the approach, the forest wait, out of the fortress, then the heading.
    pub(crate) fn endermen_task(&mut self, w: &World, tick: u64) -> Task {
        let Some(body) = w.body else {
            return Task::Idle;
        };
        let feet = path::feet_cell(body.pos);
        // >>> travel (P2, issue 18c, docs/problems/pearls-travel.md): the forest at the feet ends
        // the travel; the hunt is the next phase's.
        if self.ender.phase == EnderPhase::Travel && w.feet_biome() == Some("warped_forest") {
            self.ender.phase = EnderPhase::Hunt;
            self.note(
                tick,
                "warped forest reached: on to the hunt".to_owned(),
                format!(
                    "the feet's biome at {feet:?}; {}",
                    travel_state(w, self.table.as_ref().map(|t| &t.survival))
                ),
            );
        }
        if self.ender.phase == EnderPhase::Hunt {
            return self.pearls_task(w, tick).unwrap_or(Task::Idle);
        }
        if self.ender.phase == EnderPhase::Travel
            && let Some(t) = self.exit_step(w, tick)
        {
            return t;
        }
        // The stage seen by the world's ladder (a resumed bot's first look may come after it).
        if self.ender.reached_at.is_none()
            && self.ender.phase == EnderPhase::Reach
            && w.stage_reached
                .is_some_and(|s| s >= world::stage::Stage::Endermen)
        {
            self.ender.reached_at = Some(tick);
            self.note(
                tick,
                "endermen reached".to_owned(),
                format!(
                    "the stage is held ({:?}); a sealed pocket and eyes down before the hand-over",
                    w.stage_reached
                ),
            );
        }
        // <<< travel
        // The stage: a ticking enderman within 32. Sealed in, eyes down, then the aim is met.
        if self.ender.reached_at.is_none()
            && let Some(s) = near_ticking(w)
        {
            self.ender.reached_at = Some(tick);
            self.note(
                tick,
                "endermen reached".to_owned(),
                format!(
                    "enderman {} ticking {:.1} off; a sealed pocket and eyes down before the aim is met",
                    s.id, s.dist
                ),
            );
        }
        if self.ender.reached_at.is_some() && self.ender.phase == EnderPhase::Reach {
            return match self.pocket_step(w, tick) {
                Pocket::Building => {
                    self.retry_at = tick + 1;
                    Task::Idle
                }
                // >>> travel (P2, issue 18c): the race goes on from the sealed pocket to a warped
                // forest; `met` waits for the pearls.
                Pocket::Closed if self.aim == crate::Aim::Race => {
                    self.hand_over(w, tick, feet, "sealed");
                    self.retry_at = tick + 1;
                    Task::Idle
                }
                Pocket::Cannot(why) if self.aim == crate::Aim::Race => {
                    self.hand_over(w, tick, feet, why);
                    self.retry_at = tick + 1;
                    Task::Idle
                }
                // <<< travel
                Pocket::Closed => {
                    self.met = true;
                    self.note(
                        tick,
                        "endermen met".to_owned(),
                        format!(
                            "held in a sealed pocket of {} blocks at {feet:?}, eyes down",
                            self.ender.pocket_blocks
                        ),
                    );
                    Task::Idle
                }
                Pocket::Cannot(why) => {
                    self.met = true;
                    self.note(
                        tick,
                        "endermen met unsealed".to_owned(),
                        format!("{why}; eyes down at {feet:?}"),
                    );
                    Task::Idle
                }
            };
        }
        let rules = self.table.as_ref().map(|t| t.survival.clone());
        if !self.ender.provisions_noted {
            self.ender.provisions_noted = true;
            let reserve = rules.as_ref().map_or(0, |r| r.pickaxe_reserve);
            let food = rules.as_ref().map_or(0, |r| crate::eat::food_points(w, r));
            self.note(
                tick,
                "provisions".to_owned(),
                format!(
                    "pickaxe uses left {} (reserve {reserve}); filler {} (keep {FILLER_KEEP}); blaze rods {} (never spent); food {food} points held, bar {:?}; health {:?}",
                    pickaxe_uses_left(w),
                    filler_held(w),
                    craft::count(w, ItemKind::BlazeRod),
                    w.player.food,
                    w.player.health
                ),
            );
        }
        // Recover before travel.
        if let Some(r) = &rules {
            let health = w.player.health;
            let heal = crate::can_heal(w.player.food, crate::eat::best_food(w, r).is_some());
            if health.is_some_and(|h| h <= r.danger_health) && !heal {
                if self
                    .ender
                    .held_noted_at
                    .is_none_or(|t| tick >= t + NOTE_EVERY)
                {
                    self.ender.held_noted_at = Some(tick);
                    self.note(
                        tick,
                        "hold in cover: nothing to heal with".to_owned(),
                        format!(
                            "health {health:?} at or under the danger line {}, food {:?} and no safe food held; a pocket, never a death as a plan",
                            r.danger_health, w.player.food
                        ),
                    );
                }
                self.pocket_step(w, tick);
                self.retry_at = tick + 1;
                return Task::Idle;
            }
            if health.is_some_and(|h| h < r.travel_health) && seen_by_far_shooter(w) {
                if let Some(t) = self.take_cover(w, tick) {
                    if self
                        .ender
                        .recover_noted_at
                        .is_none_or(|t| tick >= t + NOTE_EVERY)
                    {
                        self.ender.recover_noted_at = Some(tick);
                        self.note(
                            tick,
                            "recover before travel".to_owned(),
                            format!(
                                "health {health:?} under {} with a ticking blaze or ghast in sight; food {:?}",
                                r.travel_health, w.player.food
                            ),
                        );
                    }
                    return t;
                }
                if self
                    .ender
                    .travel_noted_at
                    .is_none_or(|t| tick >= t + NOTE_EVERY)
                {
                    self.ender.travel_noted_at = Some(tick);
                    self.note(
                        tick,
                        "travel hurt".to_owned(),
                        format!(
                            "health {health:?} under {} and no rest to take (food {:?}, safe food held {heal}); on with the danger rules",
                            r.travel_health, w.player.food
                        ),
                    );
                }
            }
        }
        let travel = self.ender.phase == EnderPhase::Travel;
        // In travel the stage is passed: no enderman is walked to (the hunt waits for the forest).
        if !travel && let Some(t) = self.approach(w, tick) {
            return t;
        }
        // >>> travel (P2, issue 18c): a warped forest known from blocks seen is walked to by the
        // cell stood on; a note of the trip's state every `TRAVEL_NOTE_EVERY`.
        if travel {
            self.travel_note(w, tick);
            if let Some(c) = self.forest_cell(w, tick)
                && let Some(t) = self.go_toward(w, tick, c, FOREST_GOAL, "the warped forest")
            {
                return t;
            }
        }
        // <<< travel
        if self
            .ender
            .fortress_at
            .is_none_or(|t| tick >= t + FORTRESS_EVERY)
        {
            self.ender.fortress_at = Some(tick);
            self.ender.fortress = fortress_bounds(w);
        }
        let bounds = self.ender.fortress;
        // A warped forest known from blocks seen comes first (the enderman is its only monster).
        if !travel
            && !in_forest(w)
            && let Some(c) = w
                .memory
                .forest_known()
                .and_then(|k| column_target(w, k, FOREST_KINDS))
            && let Some(t) = self.go_toward(w, tick, c, PLACE_GOAL, "the warped forest")
        {
            return t;
        }
        // In a warped forest with none near: wait for one to spawn.
        if !travel && in_forest(w) && !self.ender.forest_over {
            let wait = rules.as_ref().map_or(0, |r| r.forest_wait_ticks);
            let since = *self.ender.forest_since.get_or_insert(tick);
            if since == tick {
                self.note(
                    tick,
                    "wait in the warped forest".to_owned(),
                    format!("on warped nylium at {feet:?}, no ticking enderman within {ENDERMAN_NEAR}; up to {wait} ticks"),
                );
            }
            if tick.saturating_sub(since) < wait {
                self.retry_at = tick + crate::RETRY_TICKS;
                return Task::Idle;
            }
            self.ender.forest_over = true;
            self.note(
                tick,
                "forest wait over".to_owned(),
                format!("no enderman came within {ENDERMAN_NEAR} in {wait} ticks; on"),
            );
        }
        // A soul sand valley known (seen soul sand and soul soil), or stood in: there, in a nook,
        // wait for an enderman (docs/problems/endermen.md, the decision of 2026-11-24).
        if !travel && !self.ender.valley_over {
            if in_valley(w) {
                let wait = rules.as_ref().map_or(0, |r| r.valley_wait_ticks);
                if let Some(t) = self.valley_wait(w, tick, wait) {
                    return t;
                }
            } else if let Some(c) = self.valley_cell(w, tick)
                && let Some(t) = self.go_toward(w, tick, c, PLACE_GOAL, "the soul sand valley")
            {
                return t;
            }
        }
        // Out of the fortress's footprint on a plain walk first: no stairs down inside it, where
        // the floors are thin over the lava sea (clojurecraft #715).
        // Not in travel: the heading turns from a fortress seen ahead, and its tunnel takes no stair
        // down in a footprint (race 45531575517 lane 3: this walk led it along a second fortress).
        if let Some(b) = bounds.filter(|b| !travel && in_footprint(*b, feet)) {
            let centre = ((b.0 + b.2) / 2, feet.1, (b.1 + b.3) / 2);
            let half = (b.2 - b.0).max(b.3 - b.1) / 2;
            let range = f64::from(half + FORTRESS_PAD + 2);
            let goal = Goal::Afar { pos: centre, range };
            if !self.refused_now(centre, tick) {
                let mut walk = Walk::new(centre, goal, Arrive::Goal, tick);
                if walk.route_now(w, tick) {
                    self.walks += 1;
                    self.refuse(centre, tick);
                    self.note(
                        tick,
                        "out of the fortress footprint".to_owned(),
                        format!(
                            "bricks from ({}, {}) to ({}, {}); a plain walk to {range:.0} from {centre:?}, no stairs down inside",
                            b.0, b.1, b.2, b.3
                        ),
                    );
                    return Task::Explore(walk);
                }
                self.refuse(centre, tick);
            }
        }
        self.travel_on(w, tick, rules.as_ref().map_or(0, |r| r.pickaxe_reserve))
    }

    /// The approach (17e.1): an enderman seen within `SIGHTING_RANGE`, ticking ones first,
    /// walked to within `APPROACH_GOAL` by a plain route, else the far
    /// walk; never tunnelled. Each given up after `CHASE_TRIES` walks or `CHASE_TICKS`.
    fn approach(&mut self, w: &World, tick: u64) -> Option<Task> {
        let candidates: Vec<Sighting> = endermen(w)
            .into_iter()
            .filter(|s| !self.ender.given_up.contains(&s.id))
            .filter(|s| {
                // Ticking or frozen, any seen within `SIGHTING_RANGE` (ticking ones first): a
                // ticking enderman 64.9 off was left alone for 2,000 ticks by a bot waiting in
                // its nook, the rule then being 64 for a ticking one (race 45487204794 lane 1).
                s.dist <= SIGHTING_RANGE
            })
            .collect();
        // Ticking ones first, each nearest first.
        let mut order = candidates.clone();
        order.sort_by(|a, b| {
            b.ticking
                .cmp(&a.ticking)
                .then(a.dist.total_cmp(&b.dist))
                .then(a.id.cmp(&b.id))
        });
        for s in order {
            let chase = match self.ender.chases.iter().position(|c| c.0 == s.id) {
                Some(i) => i,
                None => {
                    self.ender.chases.push((s.id, tick, 0));
                    self.ender.chases.len() - 1
                }
            };
            let (_, first, tries) = self.ender.chases[chase];
            if tries >= CHASE_TRIES || tick.saturating_sub(first) >= CHASE_TICKS {
                self.ender.given_up.push(s.id);
                self.note(
                    tick,
                    format!("enderman {} given up", s.id),
                    format!(
                        "{tries} walks toward it in {} ticks; {:.1} off, ticking {}",
                        tick.saturating_sub(first),
                        s.dist,
                        s.ticking
                    ),
                );
                continue;
            }
            self.ender.chases[chase].2 += 1;
            let c = path::feet_cell(s.pos);
            let goal = Goal::Near {
                pos: c,
                range: APPROACH_GOAL,
            };
            let mut walk = Walk::new(c, goal, Arrive::Goal, tick);
            let mut how = "a plain route";
            if !walk.route_now(w, tick) {
                walk = Walk::far_to(c, goal, tick);
                how = "the far walk";
                if !walk.route_now(w, tick) {
                    self.note(
                        tick,
                        format!("no way to enderman {}", s.id),
                        format!(
                            "{:.1} off, ticking {}; walk {} of {CHASE_TRIES}",
                            s.dist,
                            s.ticking,
                            tries + 1
                        ),
                    );
                    continue;
                }
            }
            self.walks += 1;
            self.note(
                tick,
                format!("approach enderman {} at {c:?}", s.id),
                format!(
                    "{} {:.1} off; to {APPROACH_GOAL} by {how}, walk {} of {CHASE_TRIES}",
                    if s.ticking { "ticking" } else { "frozen" },
                    s.dist,
                    tries + 1
                ),
            );
            return Some(Task::Explore(walk));
        }
        None
    }

    /// The soul sand valley cell to go to (`valley_target`), skipping places given up; the soul
    /// cells refused for bricks beside them noted at most once a `NOTE_EVERY`.
    fn valley_cell(&mut self, w: &World, tick: u64) -> Option<Cell> {
        let skip: Vec<Cell> = self
            .ender
            .place_tries
            .iter()
            .filter(|p| p.1 >= CHASE_TRIES)
            .map(|p| p.0)
            .collect();
        // The search reads memory's sign and brick records for up to 32 columns: once a
        // `VALLEY_EVERY`, the answer kept between.
        if let Some((at, cell)) = self.ender.valley_cache
            && tick < at + VALLEY_EVERY
        {
            return cell;
        }
        let (cell, walled) = valley_target(w, &skip);
        self.ender.valley_cache = Some((tick, cell));
        if walled > 0
            && self
                .ender
                .valley_refused_at
                .is_none_or(|t| tick >= t + NOTE_EVERY)
        {
            self.ender.valley_refused_at = Some(tick);
            self.note(
                tick,
                format!("{walled} soul cells refused"),
                format!(
                    "nether bricks seen within {BRICK_NEAR}: a fortress's wart rooms or bridges, no valley; the target {cell:?}"
                ),
            );
        }
        cell
    }

    /// A walk toward a known place: a plain route, else the far walk, else (above the pickaxe
    /// reserve) the travel tunnel; at most `CHASE_TRIES` begun for one place, then it is noted
    /// given up and `None` from then on.
    fn go_toward(&mut self, w: &World, tick: u64, c: Cell, range: f64, what: &str) -> Option<Task> {
        let i = match self
            .ender
            .place_tries
            .iter()
            .position(|p| path::flat_distance(p.0, c) <= PLACE_SAME)
        {
            Some(i) => i,
            None => {
                self.ender.place_tries.push((c, 0));
                self.ender.place_tries.len() - 1
            }
        };
        let tries = self.ender.place_tries[i].1;
        if tries >= CHASE_TRIES {
            return None;
        }
        // One search a look, and none sooner than `GO_EVERY` after a fruitless one: a far
        // search on the frontier recording took about 516 ms (E2's replan probe 45483359193), and
        // a plain search, a far one and a tunnel's in one step stalls the bot's loop. The bot
        // holds where it is between tries.
        if tick < self.ender.go_next {
            self.retry_at = self.ender.go_next;
            return Some(Task::Idle);
        }
        self.ender.place_tries[i].1 += 1;
        let goal = Goal::Near { pos: c, range };
        let reserve = self
            .table
            .as_ref()
            .map_or(0, |t| t.survival.pickaxe_reserve);
        // The first try a plain route, the second the far walk (bridging), the third the travel
        // tunnel above the pickaxe reserve.
        let found = match tries {
            0 => {
                let mut walk = Walk::new(c, goal, Arrive::Goal, tick);
                walk.route_now(w, tick).then_some((walk, "a plain route"))
            }
            1 => {
                let mut walk = Walk::far_to(c, goal, tick);
                walk.route_now(w, tick).then_some((walk, "the far walk"))
            }
            _ => None,
        };
        if let Some((walk, how)) = found {
            self.walks += 1;
            self.note(
                tick,
                format!("to {what} at {c:?}"),
                format!("by {how}, try {} of {CHASE_TRIES}", tries + 1),
            );
            return Some(Task::Explore(walk));
        }
        if tries >= 2
            && pickaxe_uses_left(w) > reserve
            && let Some(t) = self.tunnel_toward(w, c, (what, "no walk to it"), tick)
        {
            return Some(t);
        }
        self.note(
            tick,
            format!("no way to {what} at {c:?}"),
            format!("try {} of {CHASE_TRIES}", tries + 1),
        );
        if tries + 1 >= CHASE_TRIES {
            self.note(
                tick,
                format!("{what} given up"),
                format!("{CHASE_TRIES} tries with no way"),
            );
            return None;
        }
        self.ender.go_next = tick + GO_EVERY;
        self.retry_at = self.ender.go_next;
        Some(Task::Idle)
    }

    /// The wait in a soul sand valley: a nook built round the feet (its open side toward the most
    /// open floor, never toward a ticking ghast or skeleton while another side is open), then
    /// stood in until an enderman comes (the approach and the stage take it from there) or
    /// `wait` ticks pass. `None` once the wait is over.
    fn valley_wait(&mut self, w: &World, tick: u64, wait: u64) -> Option<Task> {
        let body = w.body?;
        let feet = path::stand_cell(w, body.pos);
        let since = *self.ender.valley_since.get_or_insert(tick);
        let spent = tick.saturating_sub(since);
        if spent >= wait {
            self.ender.valley_over = true;
            self.note(
                tick,
                "valley wait over".to_owned(),
                format!("no ticking enderman came within {ENDERMAN_NEAR} in {spent} ticks; on"),
            );
            return None;
        }
        // Falling (a ghast's fireball knocked a bot off its ledge, race 45487204794 lane 2: a
        // nook chosen at every cell of the fall): the nook waits for the ground.
        if !body.on_ground {
            self.retry_at = tick + 1;
            return Some(Task::Idle);
        }
        let side = match self.ender.nook_open {
            Some((at, side)) if at == feet => side,
            _ => {
                let (side, run) = nook_side(w, feet);
                self.ender.nook_open = Some((feet, side));
                self.note(
                    tick,
                    format!("nook at {feet:?}, open to {side:?}"),
                    format!(
                        "in a soul sand valley; open floor {run} cells that way; up to {wait} ticks for an enderman"
                    ),
                );
                side
            }
        };
        if self.shell_step(w, tick, Some(side)) == Pocket::Building {
            self.retry_at = tick + 1;
            return Some(Task::Idle);
        }
        if self
            .ender
            .valley_noted_at
            .is_none_or(|t| tick >= t + VALLEY_NOTE_EVERY)
        {
            self.ender.valley_noted_at = Some(tick);
            let near = endermen(w).into_iter().next();
            self.note(
                tick,
                "waiting in the soul sand valley".to_owned(),
                format!(
                    "{spent} of {wait} ticks in the nook at {feet:?} ({} blocks); nearest enderman {}; health {:?}",
                    self.ender.pocket_blocks,
                    near.map_or("none".to_owned(), |s| format!(
                        "{} at {:.1}, ticking {}",
                        s.id, s.dist, s.ticking
                    )),
                    w.player.health
                ),
            );
        }
        self.retry_at = tick + crate::RETRY_TICKS;
        Some(Task::Idle)
    }

    // >>> travel (P2, issue 18c, docs/problems/pearls-travel.md): the hand-over, the pocket's way
    // out, the forest by cell, the trip's notes.
    /// The endermen stage held in a pocket (sealed, or `why` not): on to a warped forest. The
    /// pocket's way out is chosen now (`exit_side`) and dug by `exit_step`.
    fn hand_over(&mut self, w: &World, tick: u64, feet: Cell, why: &str) {
        self.ender.phase = EnderPhase::Travel;
        let stand = w.body.map_or(feet, |b| path::stand_cell(w, b.pos));
        let toward = w
            .memory
            .forest_known()
            .map(|(cx, cz)| (cx * 16 + 8, cz * 16 + 8));
        self.ender.exit = exit_side(w, stand, toward);
        self.ender.exit_digs = 0;
        self.note(
            tick,
            "endermen reached: on to the forest".to_owned(),
            format!(
                "the pocket at {stand:?} {why} ({} blocks); the way out {}; {}",
                self.ender.pocket_blocks,
                match &self.ender.exit {
                    Some((d, cells)) => format!("to {d:?}, {} cells to open", cells.len()),
                    None => "none: lava beside every side".to_owned(),
                },
                travel_state(w, self.table.as_ref().map(|t| &t.survival))
            ),
        );
    }

    /// The pocket's way out: its cells dug one at a time (the dig picks the pickaxe), at most
    /// `EXIT_DIGS`; `None` once open or given up.
    fn exit_step(&mut self, w: &World, tick: u64) -> Option<Task> {
        let (side, cells) = self.ender.exit.clone()?;
        let shut: Vec<Cell> = cells
            .iter()
            .filter(|c| {
                path::classify(w.state_at(world::BlockPos::new(c.0, c.1, c.2)))
                    != path::Class::Clear
            })
            .copied()
            .collect();
        let Some(next) = shut.first().copied() else {
            self.ender.exit = None;
            self.note(
                tick,
                format!("out of the pocket to {side:?}"),
                format!(
                    "{} cells opened in {} digs; {}",
                    cells.len(),
                    self.ender.exit_digs,
                    travel_state(w, self.table.as_ref().map(|t| &t.survival))
                ),
            );
            return None;
        };
        if self.ender.exit_digs >= EXIT_DIGS {
            self.ender.exit = None;
            self.note(
                tick,
                "pocket exit given up".to_owned(),
                format!(
                    "{EXIT_DIGS} digs and {next:?} still shut; the travel's own ways from here"
                ),
            );
            return None;
        }
        self.ender.exit_digs += 1;
        self.note(
            tick,
            format!("pocket opened at {next:?}"),
            format!(
                "dig {} of the way out to {side:?}; pickaxe uses left {}",
                self.ender.exit_digs,
                pickaxe_uses_left(w)
            ),
        );
        Some(Task::Dig(crate::dig::Dig::new(w, next, tick)))
    }

    /// The warped forest cell to go to (`forest_target`), skipping places given up; nylium
    /// refused for no room noted at most once a `NOTE_EVERY`; the search once a `VALLEY_EVERY`.
    fn forest_cell(&mut self, w: &World, tick: u64) -> Option<Cell> {
        if w.memory.forests().is_empty() {
            return None;
        }
        if let Some((at, cell)) = self.ender.forest_cache
            && tick < at + VALLEY_EVERY
        {
            return cell;
        }
        let skip: Vec<Cell> = self
            .ender
            .place_tries
            .iter()
            .filter(|p| p.1 >= CHASE_TRIES)
            .map(|p| p.0)
            .collect();
        let (cell, shut) = forest_target(w, &skip);
        self.ender.forest_cache = Some((tick, cell));
        if shut > 0
            && self
                .ender
                .forest_refused_at
                .is_none_or(|t| tick >= t + NOTE_EVERY)
        {
            self.ender.forest_refused_at = Some(tick);
            self.note(
                tick,
                format!("{shut} nylium cells refused"),
                format!("no room over them for a body; the target {cell:?}"),
            );
        }
        cell
    }

    /// The trip's state, once a `TRAVEL_NOTE_EVERY`.
    fn travel_note(&mut self, w: &World, tick: u64) {
        if self
            .ender
            .travel_leg_at
            .is_some_and(|t| tick < t + TRAVEL_NOTE_EVERY)
        {
            return;
        }
        self.ender.travel_leg_at = Some(tick);
        let biome = w.feet_biome().unwrap_or("unknown");
        self.note(
            tick,
            "to the forest".to_owned(),
            format!(
                "at {:?} in {biome}; {} forest columns known; {}",
                w.body.map(|b| path::feet_cell(b.pos)),
                w.memory.forests().len(),
                travel_state(w, self.table.as_ref().map(|t| &t.survival))
            ),
        );
    }

    // >>> pearls stub (P3 replaces it with crates/plan/src/pearls.rs's `Plan::pearls_task`; drop
    // this block at that merge): the hunt (issue 18d to 18f). `None` until it lands.
    #[allow(clippy::unused_self)]
    pub(crate) fn pearls_task(&mut self, _w: &World, _tick: u64) -> Option<Task> {
        None
    }
    // <<< pearls stub
    // <<< travel

    /// The way on with no enderman to go to: the heading (issue 17d).
    fn travel_on(&mut self, w: &World, tick: u64, reserve: i32) -> Task {
        // >>> heading (E2): the held heading (issue 17d, docs/problems/heading.md): a forest known,
        // else a soul sand valley, else the axis, leg by leg. `None` when it is over (a warped
        // forest at the feet, or turned out): the explore walk below.
        if let Some(t) = self.heading_task(w, tick) {
            return t;
        }
        let uses = pickaxe_uses_left(w);
        if uses <= reserve
            && self
                .ender
                .travel_noted_at
                .is_none_or(|t| tick >= t + NOTE_EVERY)
        {
            self.ender.travel_noted_at = Some(tick);
            self.note(
                tick,
                "no tunnel: the pickaxe reserve".to_owned(),
                format!("{uses} pickaxe uses left, reserve {reserve}; walks only"),
            );
        }
        // <<< heading
        if let Some(t) = self.explore_walk(w, tick, "no enderman in view") {
            return t;
        }
        self.retry_at = tick + crate::RETRY_TICKS;
        self.note(
            tick,
            "no way on toward endermen".to_owned(),
            "no tunnel leg and no explore walk".to_owned(),
        );
        Task::Idle
    }
}
