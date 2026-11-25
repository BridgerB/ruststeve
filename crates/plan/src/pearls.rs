//! The hunt (issue 18f, `docs/problems/pearls-hunt.md`): provoke, fight and collect from the
//! burrow (`burrow`), as a small rule table read whole. The rows are tried in order each tick;
//! the first that holds acts, and its id is the reason written to the ledger.
//!
//! | # | Row id | When | Act |
//! |---|---|---|---|
//! | 1 | `pearls-done` | `pearls_wanted` pearls held | the mouth sealed, eyes down; met |
//! | 2 | `breach` | hurt by an enderman, or the bot not in B with an angry enderman within 4, or the mouth sealed | seal M from B, eyes down, until none angry ticks within 16 |
//! | 3 | `strike` | the target angry, its box in reach, a full charge, the bot in B | the torso (not a stare), attack then swing |
//! | 4 | `hold` | an angry enderman present, ticking or frozen | stay in B, look level at the mouth at torso height |
//! | 5 | `collect` | a pearl within `collect_range` of O, none angry, a free slot | step to M, then on to it within `collect_reach` of B; back |
//! | 6 | `base` | the forest base not done (`base_task`, agent P5) | its steps |
//! | 7 | `build` | no burrow, the bot out of it, a block missing or the mouth closed | find, walk to, build, mend, reopen |
//! | 8 | `provoke` | in M, a calm ticking enderman 6 to 36 off in sight, none angry | its eye until the creepy flag, at most `provoke_ticks` server ticks; then B |
//! | 9 | `wait` | none of the above | at O, the yaw sweeping, eyes level; the burrow moved after `burrow_idle_ticks` with none within `seen_near` |
//!
//! Progress invariants (each writes a note when it fires): provoke tries per enderman; a fight's
//! health-fall window, its parked target and its cap with the mouth sealed; a kill per
//! `kill_window` or the burrow moves, two moves without a kill end the hunt; resources per pearl
//! in every kill's note; the weapon switched under `weapon_min_uses`; the bounce counter (one step
//! out of B a `step_out_every` without a kill or a pickup between).
//!
//! Never a stare but the provoke row's, at a calm enderman, until its creepy flag: an angry one
//! stared at within 16 freezes (clojurecraft's real bots parked endermen for minutes so, 5d894db).

use std::collections::BTreeMap;

use data::{EntityKind, ItemKind};
use physics::Controls;
use protocol::metadata::MetaValue;
use world::World;

use crate::burrow::{self, Burrow};
use crate::endermen::{self, CREEPY_INDEX, EYES_DOWN, Sighting};
use crate::path::{self, Goal};
use crate::walk::{Arrive, Walk, look_at, yaw_to};
use crate::{Plan, Task, cover, craft, defend, hold, look};

/// `[pearls]` in `config/goals.toml`: the hunt's numbers, each with its source there.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct PearlRules {
    pub pearls_wanted: i32,
    pub provoke_ticks: u64,
    pub provoke_min: f64,
    pub provoke_max: f64,
    pub provoke_out_min: f64,
    pub provoke_out_max: f64,
    pub provoke_tries: u32,
    pub provoke_skip_ticks: u64,
    pub fall_window: u64,
    pub parked_ticks: u64,
    pub parked_seal_ticks: u64,
    pub parked_look_ticks: u64,
    pub fight_cap: u64,
    pub kill_window: u64,
    pub burrow_idle_ticks: u64,
    pub burrow_idle_after_kill: u64,
    pub moves_without_kill: u32,
    pub burrow_move: i32,
    pub site_every: u64,
    pub weapon_min_uses: i32,
    pub step_out_every: u64,
    pub build_max: u32,
    pub walk_tries: u32,
    pub collect_range: f64,
    pub collect_reach: f64,
    pub collect_ticks: u64,
    pub breach_near: f64,
    pub near_o: f64,
    pub base_wait_ticks: u64,
    pub seen_near: f64,
    pub hostile_near: f64,
    pub cube_near: f64,
    pub look_out_reach: i32,
    /// Where the hunt goes first (x, z), by known geography (Bridger's ruling); `None` hunts
    /// where the bot stands.
    pub hunt_point: Option<[i32; 2]>,
    pub look_out_far_min: f64,
    pub look_out_far_max: f64,
    pub cube_clear: f64,
    pub piglin_near: f64,
    pub piglin_clear: f64,
    pub restock_below: i32,
    pub restock_to: i32,
    pub threat_hold_ticks: u64,
    pub base_retry_ticks: u64,
    pub calm_range: f64,
}

impl Default for PearlRules {
    fn default() -> Self {
        Self {
            pearls_wanted: 12,
            provoke_ticks: 60,
            provoke_min: 6.0,
            provoke_max: 36.0,
            provoke_out_min: 12.0,
            provoke_out_max: 64.0,
            provoke_tries: 3,
            provoke_skip_ticks: 2400,
            fall_window: 200,
            parked_ticks: 600,
            parked_seal_ticks: 1200,
            parked_look_ticks: 40,
            fight_cap: 2400,
            kill_window: 12000,
            burrow_idle_ticks: 9600,
            burrow_idle_after_kill: 4800,
            moves_without_kill: 1000,
            burrow_move: 48,
            site_every: 100,
            weapon_min_uses: 10,
            step_out_every: 1200,
            build_max: 40,
            walk_tries: 3,
            collect_range: 6.0,
            collect_reach: 4.0,
            collect_ticks: 200,
            breach_near: 4.0,
            near_o: 8.0,
            base_wait_ticks: 600,
            seen_near: 40.0,
            hostile_near: 16.0,
            cube_near: 3.0,
            look_out_reach: 4,
            hunt_point: None,
            look_out_far_min: 24.0,
            look_out_far_max: 36.0,
            cube_clear: 8.0,
            piglin_near: 12.0,
            piglin_clear: 16.0,
            restock_below: 60,
            restock_to: 120,
            threat_hold_ticks: 1200,
            base_retry_ticks: 6000,
            calm_range: 16.0,
        }
    }
}

/// Blocks the burrow is built of: full, and none `#enderman_holdable` (the 26.1.2 tag: dirt,
/// gravel and nylium are; these are not). The stone nothing else wants first (granite, tuff:
/// the frontier bot holds 64 and 23), cobblestone last: the base makes stone swords of it.
pub const BURROW_BLOCKS: &[ItemKind] = &[
    ItemKind::Granite,
    ItemKind::Tuff,
    ItemKind::Andesite,
    ItemKind::Diorite,
    ItemKind::Blackstone,
    ItemKind::Netherrack,
    ItemKind::CobbledDeepslate,
    ItemKind::Cobblestone,
];
/// Ticks between two burrow blocks (the shelter's `SHELTER_EVERY_TICKS`: the update arrives in
/// a tick or two).
const BUILD_EVERY: u64 = 10;
/// A step to a cell's middle is done this near it, on the plane.
const STEP_DONE: f64 = 0.2;
/// Burrow blocks held, every kind together.
pub fn burrow_filler(w: &World) -> i32 {
    BURROW_BLOCKS.iter().map(|k| craft::count(w, *k)).sum()
}

/// Whether a standing cell `f` has within `near` (on the plane, at its level) an open cell with
/// no floor in the 3 below (a drop of 3 or more), or lava in the 4 below one.
pub fn ledge(w: &World, f: path::Cell, near: i32) -> bool {
    let at = |c: path::Cell| w.block_at(world::BlockPos::new(c.0, c.1, c.2));
    let open = |c: path::Cell| {
        at(c).and_then(data::generated::shapes::collision) == Some(data::types::Collision::Empty)
    };
    (-near..=near).any(|dx| {
        (-near..=near).any(|dz| {
            let c = (f.0 + dx, f.1, f.2 + dz);
            if !open(c) {
                return false;
            }
            let lava = (1..=4).any(|d| at((c.0, c.1 - d, c.2)) == Some(data::BlockKind::Lava));
            let drop = (1..=3).all(|d| open((c.0, c.1 - d, c.2)));
            lava || drop
        })
    })
}

/// Whether every cell the eye-to-eye line crosses is loaded and has no collision shape: the
/// server's stare clips against collision shapes, and `look::clear` (full blocks seen exposed only)
/// passed lines a fungus, a slab or a vine cut (race 45563567457: six stares at one 58.8 off
/// never set its stared-at flag). The first solid cell a line meets is exposed to it (the cell
/// before it is open), so this reads no hidden block.
pub fn line_open(w: &World, from: [f64; 3], to: [f64; 3]) -> bool {
    crate::scoop::ray_cells(from, to).into_iter().all(|c| {
        w.block_at(world::BlockPos::new(c.0, c.1, c.2))
            .and_then(data::generated::shapes::collision)
            == Some(data::types::Collision::Empty)
    })
}

/// Whether delta ground (basalt, blackstone, magma) lies under or at the level of a cell within
/// `r` on the plane: the ground magma cubes come from.
pub fn near_delta(w: &World, c: [i32; 3], r: i32) -> bool {
    (-r..=r).any(|dx| {
        (-r..=r).any(|dz| {
            (-2..=0).any(|dy| {
                w.block_at(world::BlockPos::new(c[0] + dx, c[1] + dy, c[2] + dz))
                    .is_some_and(|k| burrow::DELTA_GROUND.contains(&k))
            })
        })
    })
}

/// The nearest delta ground to `c` on the plane (Chebyshev rings, the floor two down to the
/// level), within `r`: the forest's edge, where magma cubes and ghasts come from.
pub fn delta_distance(w: &World, c: [i32; 3], r: i32) -> Option<i32> {
    (0..=r).find(|k| {
        (-k..=*k).any(|dx| {
            (-k..=*k).any(|dz| {
                (dx.abs().max(dz.abs()) == *k)
                    && (-2..=0).any(|dy| {
                        w.block_at(world::BlockPos::new(c[0] + dx, c[1] + dy, c[2] + dz))
                            .is_some_and(|b| burrow::DELTA_GROUND.contains(&b))
                    })
            })
        })
    })
}

/// Places to search for a site from besides the feet: warped nylium seen within `SUPPLY_REACH`
/// (one a chunk column) with the most endermen seen within 40, at least 16 from the feet, the
/// best `ANCHORS`.
fn supply_anchors(
    w: &World,
    seen: &[[f64; 3]],
    feet: [i32; 3],
    away_from: Option<([i32; 3], i32)>,
    near_point: Option<[i32; 2]>,
) -> Vec<[i32; 3]> {
    let Some(dim) = w.dimension.as_ref() else {
        return Vec::new();
    };
    let r = SUPPLY_REACH >> 4;
    let mut v: Vec<(usize, [i32; 3])> = Vec::new();
    for dx in -r..=r {
        for dz in -r..=r {
            let cells = w
                .memory
                .indexed_in_column(
                    &dim.name,
                    (feet[0] >> 4) + dx,
                    (feet[2] >> 4) + dz,
                    data::BlockKind::WarpedNylium,
                )
                .unwrap_or_default();
            let Some(c) = cells.iter().min_by_key(|c| (c.1 - feet[1]).abs()) else {
                continue;
            };
            let at = [c.0, c.1 + 1, c.2];
            let d2 = (at[0] - feet[0]).pow(2) + (at[2] - feet[2]).pow(2);
            if !(16 * 16..=SUPPLY_REACH * SUPPLY_REACH).contains(&d2) {
                continue;
            }
            // A move's next place: `burrow_move` or more from the old B (new chunks tick, new
            // spawns: the locals were used up), never by delta ground, near the hunt point.
            if away_from.is_some_and(|(o, d)| (o[0] - at[0]).pow(2) + (o[2] - at[2]).pow(2) < d * d)
                || near_delta(w, at, 16)
                || w.entities.values().any(|e| {
                    !e.dead
                        && PIGLINS.contains(&e.kind)
                        && flat(e.pos, Burrow::centre(at)) <= PIGLIN_AWAY
                })
                || near_point.is_some_and(|[x, z]| {
                    (x - at[0]).pow(2) + (z - at[2]).pow(2) > HUNT_AREA * HUNT_AREA
                })
            {
                continue;
            }
            let p = Burrow::centre(at);
            let n = seen.iter().filter(|s| flat(**s, p) <= 40.0).count();
            v.push((n, at));
        }
    }
    v.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    v.into_iter().take(ANCHORS).map(|(_, c)| c).collect()
}

/// A site's expected supply in view: endermen seen within 40 of O, a quarter of the open floor
/// cells round O within 8 (the full circle), less 10 with delta ground within 16.
pub fn supply_score(w: &World, seen: &[[f64; 3]], b: Burrow) -> i64 {
    let o = Burrow::centre(b.o);
    let n = i64::try_from(seen.iter().filter(|s| flat(**s, o) <= 40.0).count()).unwrap_or(0);
    let mut cells = path::Cells::new(w);
    let mut open = 0i64;
    for dx in -8..=8 {
        for dz in -8..=8 {
            if dx * dx + dz * dz <= 64 && cells.standable((b.o[0] + dx, b.o[1], b.o[2] + dz)) {
                open += 1;
            }
        }
    }
    let delta = if near_delta(w, b.o, 16) { 10 } else { 0 };
    n + open / 4 - delta
}

/// A provoke's distance band: 6 to 16 (it walks), 16 to 36, 36 to 64 (the jar's follow range;
/// past 16 it teleports toward the bot).
pub fn band(d: f64) -> usize {
    if d < 16.0 {
        0
    } else if d < 36.0 {
        1
    } else {
        2
    }
}

/// The middle of the endermen seen, as a cell, or `None` with none.
fn centroid(seen: &[[f64; 3]]) -> Option<path::Cell> {
    if seen.is_empty() {
        return None;
    }
    #[allow(clippy::cast_precision_loss)]
    let n = seen.len() as f64;
    let m = |i: usize| seen.iter().map(|p| p[i]).sum::<f64>() / n;
    Some(path::feet_cell([m(0), m(1), m(2)]))
}

/// Blocks from the feet a forest floor cell walked to lies at least (a new place to search).
const FLOOR_MIN: i32 = 8;
/// Cells each way on the plane a floor block is dug for filler from where the bot stands.
const FLOOR_REACH: i32 = 2;
/// Cells round the feet a drop or lava below makes a ledge, how far a safe cell is looked for,
/// and the walks off ledges the hunt makes.
const LEDGE_NEAR: i32 = 2;
const LEDGE_SEARCH: i32 = 8;
const LEDGE_TRIES: u32 = 6;
/// Moves down off fungus caps the hunt makes (each a walk or a dig).
const CAP_TRIES: u32 = 24;
/// Eye to a block's middle within which the old shell is dug back from where the bot stands.
const RECLAIM_REACH: f64 = 4.4;
/// The look-out: after this many ticks at O with none in sight, up to this far from O, for at
/// most this long.
const LOOK_OUT_AFTER: u64 = 600;
const LOOK_OUT: i32 = 4;
const LOOK_OUT_TICKS: u64 = 200;
/// Ticks of one yaw sweep at O.
const SWEEP_TICKS: u64 = 400;
/// Blocks within which the feet after a walk on count as not moved.
const WALK_ON_STILL: i32 = 4;
/// The piglin kinds a bot in iron with no gold is the foe of.
const PIGLINS: [EntityKind; 2] = [EntityKind::Piglin, EntityKind::PiglinBrute];
/// A new place is never within this of a piglin in sight (race 45588764709 lane 1's restock).
const PIGLIN_AWAY: f64 = 24.0;
/// Walks to one new-ground place before it is dropped.
const NEW_GROUND_TRIES: u32 = 3;
/// Blocks round the hunt point a move's new place stays within.
const HUNT_AREA: i32 = 96;
/// The hunt point: within this it is reached; searches with no route, and walks, before the
/// hunt stays where it is.
const HUNT_POINT_NEAR: f64 = 16.0;
const HUNT_POINT_TRIES: u32 = 3;
const HUNT_POINT_WALKS: u32 = 24;
/// Blocks a walk on for a new site goes at least.
const WALK_ON: i32 = 24;
/// Blocks within which an angry enderman, ticking or frozen, stops a new provoke.
const ANGRY_PRESENT: f64 = 128.0;
/// Ticks within which an enderman hurt by no one (a magma block, fire, lava) is no provoke
/// target: such a hurt teleports it (the 26.1.2 jar), and the frontier race 45544744907's one
/// provoke stared 80 server ticks at one that magma was hurting and teleporting.
const HURT_SKIP: u64 = 100;
/// Blocks round the feet the site search looks for places with endermen seen, and how many.
const SUPPLY_REACH: i32 = 64;
const ANCHORS: usize = 3;
/// Blocks within which an angry enderman or a hostile holds the reclaim's digs.
const RECLAIM_CLEAR: f64 = 16.0;
/// Ticks a site moved from is avoided by the site search.
/// 24,000, not 6,000: a move's restock and walk took ~6,000 ticks and every bot then built
/// again within 20 of the site it left (race 45602491038 lanes 1, 3: 10 and 18 off); new
/// endermen come about 0.4 a minute (P1), so a used-up site needs about 20 minutes.
const MOVED_KEPT: u64 = 24_000;
/// Blocks out to which the burrow's note measures the nearest delta ground.
const DELTA_MEASURE: i32 = 32;
/// Blocks from delta ground a forest-floor walk target keeps.
const DELTA_AVOID: i32 = 8;
/// Ticks a sighting is kept for the mouth's side.
const SIGHTINGS_KEPT: u64 = 2400;
/// Ticks one step out of B takes at most, counted as one step.
const STEP_OUT_TICKS: u64 = 40;
/// Ticks between two notes of one standing row (a hold, a wait).
const NOTE_EVERY: u64 = 600;
/// An enderman's torso over its feet: the strike's aim (the plan's section 3: about 30 degrees off
/// its eye from B, far outside the stare's cone).
pub const TORSO: f64 = 1.2;

/// The fight in hand.
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Fight {
    pub id: i32,
    pub since: u64,
    pub last_fall: u64,
    pub last_health: Option<f32>,
    pub out_since: Option<u64>,
    pub falls: u32,
    pub swings: u32,
    pub weapon_before: i32,
    pub fall_noted: bool,
}

/// The hunt's state, one field of `Plan` (`Plan::pearls`).
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct PearlState {
    /// The hunt has begun (`Plan::hunting` held once).
    #[serde(default)]
    pub started: bool,
    #[serde(default)]
    pub burrow: Option<Burrow>,
    #[serde(default)]
    pub built: bool,
    /// Sites given up (a walk or a build that failed) and the burrows moved from.
    #[serde(default)]
    pub refused: Vec<[i32; 3]>,
    #[serde(default)]
    pub moved_from: Vec<[i32; 3]>,
    /// When each move was made (`moved_from`'s ticks).
    #[serde(default)]
    pub moved_at: Vec<u64>,
    #[serde(default)]
    pub site_at: Option<u64>,
    #[serde(default)]
    pub site_none_noted: Option<u64>,
    #[serde(default)]
    pub blocks: u32,
    /// Burrow blocks placed at the site in hand (built and mended; seals not counted).
    #[serde(default)]
    pub site_blocks: u32,
    /// Built burrow cells found open (the `BlockTaken` fact's cells), each noted once until
    /// mended.
    #[serde(default)]
    pub taken: Vec<[i32; 3]>,
    #[serde(default)]
    pub place_at: u64,
    #[serde(default)]
    pub walks: u32,
    /// When the burrow was built (the idle clock starts here or at the last kill).
    #[serde(default)]
    pub since: Option<u64>,
    #[serde(default)]
    pub hunt_start: Option<u64>,
    #[serde(default)]
    pub last_kill: Option<u64>,
    #[serde(default)]
    pub moves: u32,
    #[serde(default)]
    pub moves_no_kill: u32,
    #[serde(default)]
    pub over: bool,
    /// The provoke in hand: the target and the server tick it began.
    #[serde(default)]
    pub provoke: Option<(i32, u64)>,
    /// Where the provoke in hand is made from: M, or O when none is in sight from M.
    #[serde(default)]
    pub provoke_from: Option<[i32; 3]>,
    /// Provoke tries per enderman, and until when it is skipped.
    #[serde(default)]
    pub tries: Vec<(i32, u32, u64)>,
    #[serde(default)]
    pub provokes: u32,
    /// Endermen stared at on purpose (the provoke row's targets): their stared-at flag is no
    /// near miss.
    #[serde(default)]
    pub provoked: Vec<i32>,
    /// Ticking endermen's feet seen lately, with the tick: the burrow's mouth faces the most.
    #[serde(default)]
    pub sightings: Vec<(u64, [f64; 3])>,
    /// The tick a provoke's creepy flag came, until the bot is back in B (noted).
    #[serde(default)]
    pub back_since: Option<u64>,
    /// Forest floor cells walked to (or tried) for a new site search.
    #[serde(default)]
    pub floor_tried: Vec<[i32; 3]>,
    /// Weapon uses spent in fights, summed at each kill.
    #[serde(default)]
    pub weapon_spent: u64,
    /// The base's wait (`Some(Task::Idle)`) began here; the base is not asked before this tick.
    #[serde(default)]
    pub base_wait_since: Option<u64>,
    #[serde(default)]
    pub base_skip_until: Option<u64>,
    /// When an angry enderman with no burrow built was first seen (the pocket's hold).
    #[serde(default)]
    pub threat_since: Option<u64>,
    /// Pearl items seen within 12 of O, by id (each noted once).
    #[serde(default)]
    pub drops: Vec<i32>,
    /// Ticks waited at O, and when an enderman was last within `seen_near` of B.
    #[serde(default)]
    pub at_o_ticks: u64,
    #[serde(default)]
    pub last_near: Option<u64>,
    /// Filler is being restocked (netherrack dug near the burrow).
    #[serde(default)]
    pub restock: bool,
    /// Cells this hunt put a block into (the shell, supports, seals): what a reclaim digs back.
    #[serde(default)]
    pub placed: Vec<[i32; 3]>,
    /// A move's old shell being dug back, and blocks reclaimed so far.
    #[serde(default)]
    pub reclaim: bool,
    #[serde(default)]
    pub reclaimed: u32,
    /// The walls and roof of the burrow moved from (at or above B's level): dug back too, so a
    /// shell not in `placed` is reclaimed (one a resumed snapshot's earlier run built: race
    /// 45602491038 reclaimed 0 blocks at every move and restocked ~4,000 ticks instead).
    #[serde(default)]
    pub old_shell: Vec<[i32; 3]>,
    /// The floors of the old B, M and O: never dug by the reclaim (the orchestrator).
    #[serde(default)]
    pub old_floors: Vec<[i32; 3]>,
    /// Where the last walk on for a site began, and how many in a row left the feet there.
    #[serde(default)]
    pub walk_on: Option<([i32; 3], u32)>,
    /// Moves made down off fungus caps.
    #[serde(default)]
    pub cap_tries: u32,
    /// Walks off ledges made.
    #[serde(default)]
    pub ledge_tries: u32,
    /// The feet's height when the restock began: no dig below it.
    #[serde(default)]
    pub restock_floor: Option<i32>,
    /// A pillar out of a hole in hand: the feet cell it began from.
    #[serde(default)]
    pub pillar: Option<[i32; 3]>,
    /// The look-out: the last tick a provoke candidate was in sight, the spot and when it was
    /// taken, and the counts.
    #[serde(default)]
    pub last_cand: Option<u64>,
    #[serde(default)]
    pub look_out: Option<([i32; 3], u64)>,
    #[serde(default)]
    pub look_outs: u32,
    #[serde(default)]
    pub look_out_provokes: u32,
    /// Provokes and angers by distance band (6-16, 16-36, 36-64), and the band of the one in
    /// hand.
    #[serde(default)]
    pub band_provokes: [u32; 3],
    #[serde(default)]
    pub band_angers: [u32; 3],
    #[serde(default)]
    pub provoke_band: Option<usize>,
    /// The pocket from magma cubes is held.
    #[serde(default)]
    pub cube_pocket: bool,
    /// A seal for a parked enderman opens at this tick whatever is near.
    #[serde(default)]
    pub seal_until: Option<u64>,
    /// The hunt point reached (or given up), and the walks and the searches with no route.
    #[serde(default)]
    pub at_hunt_point: bool,
    /// Kills at the burrow in hand.
    #[serde(default)]
    pub site_kills: u32,
    /// New-ground places walked to after a move, and the walks begun to each.
    #[serde(default)]
    pub ground_tries: Vec<([i32; 3], u32)>,
    #[serde(default)]
    pub hunt_point_walks: u32,
    #[serde(default)]
    pub hunt_point_fails: u32,
    #[serde(default)]
    pub angers: u32,
    #[serde(default)]
    pub fight: Option<Fight>,
    #[serde(default)]
    pub kills: u32,
    #[serde(default)]
    pub falls: u64,
    #[serde(default)]
    pub swings: u64,
    #[serde(default)]
    pub last_swing: Option<u64>,
    /// The mouth is sealed (a breach, a fight given up, the hunt over) and why.
    #[serde(default)]
    pub sealed: Option<String>,
    #[serde(default)]
    pub breaches: u32,
    /// The last enderman hit on the bot taken into account, server tick.
    #[serde(default)]
    pub hit_seen: u64,
    #[serde(default)]
    pub step_outs: u32,
    #[serde(default)]
    pub last_step_out: Option<u64>,
    #[serde(default)]
    pub step_outs_held: u32,
    #[serde(default)]
    pub pearls_seen: i32,
    #[serde(default)]
    pub picked: u32,
    #[serde(default)]
    pub last_pickup: Option<u64>,
    /// The collect in hand: the item and when it began; items given up.
    #[serde(default)]
    pub collect: Option<(i32, u64)>,
    #[serde(default)]
    pub collect_refused: Vec<i32>,
    /// A parked target's look from M runs until this tick.
    #[serde(default)]
    pub parked_until: Option<u64>,
    #[serde(default)]
    pub start_filler: i32,
    #[serde(default)]
    pub start_picks: i32,
    #[serde(default)]
    pub start_weapons: i32,
    /// The last row that acted, and when a standing row was last noted.
    #[serde(default)]
    pub row: String,
    #[serde(default)]
    pub noted: BTreeMap<String, u64>,
    #[serde(default)]
    pub weapon_short_noted: bool,
    /// The provoke row's target this tick: the one look `keep_look` lets stare.
    #[serde(skip)]
    pub provoking: Option<u32>,
}

/// An enderman whose creepy flag (index 17) is set: it has the bot as its target.
pub fn creepy(w: &World, id: i32) -> bool {
    w.entities
        .get(&id)
        .is_some_and(|e| matches!(e.metadata.get(&CREEPY_INDEX), Some(MetaValue::Bool(true))))
}

/// The angry endermen within 64, ticking or frozen (an angry frozen enderman is present: the
/// issue's rule), the nearest first; one that hit the bot within `defend::PROVOKED_TICKS` counts
/// as angry too (the nearest).
pub fn angry_endermen(w: &World) -> Vec<Sighting> {
    let all = endermen::endermen(w);
    let mut v: Vec<Sighting> = all
        .iter()
        .filter(|s| s.dist <= endermen::APPROACH_FROM && creepy(w, s.id))
        .copied()
        .collect();
    if v.is_empty()
        && defend::provoked(w, EntityKind::Enderman)
        && let Some(s) = all.iter().find(|s| s.dist <= endermen::APPROACH_FROM)
    {
        v.push(*s);
    }
    v
}

/// The eye of a body standing in a cell's middle.
pub fn eye_in(c: [i32; 3]) -> [f64; 3] {
    let p = Burrow::centre(c);
    [p[0], p[1] + physics::EYE_HEIGHT, p[2]]
}

fn eye_of(e: [f64; 3]) -> [f64; 3] {
    [e[0], e[1] + look::ENDERMAN_EYE, e[2]]
}

fn torso_of(e: [f64; 3]) -> [f64; 3] {
    [e[0], e[1] + TORSO, e[2]]
}

/// The calm ticking endermen the provoke row may take from an eye at `eye`: within
/// `provoke_min` to `provoke_max`, whose eye the eye sees past every block seen; the nearest
/// first.
pub fn provoke_candidates(w: &World, eye: [f64; 3], r: &PearlRules) -> Vec<Sighting> {
    provoke_candidates_from(w, eye, r.provoke_min, r)
}

/// `provoke_candidates` no nearer than `min`, eye to eye.
pub fn provoke_candidates_from(
    w: &World,
    eye: [f64; 3],
    min: f64,
    r: &PearlRules,
) -> Vec<Sighting> {
    endermen::endermen(w)
        .into_iter()
        .filter(|s| s.ticking && !creepy(w, s.id))
        // Not on or within 1 of magma, lava or fire: it is hurt there and teleports.
        .filter(|s| {
            let f = path::feet_cell(s.pos);
            !(-1..=1).any(|dx| {
                (-1..=0).any(|dy| {
                    (-1..=1).any(|dz| {
                        w.block_at(world::BlockPos::new(f.0 + dx, f.1 + dy, f.2 + dz))
                            .is_some_and(|k| burrow::HURTS.contains(&k))
                    })
                })
            })
        })
        .filter(|s| {
            w.entities.get(&s.id).is_none_or(|e| {
                !e.hurts
                    .iter()
                    .any(|h| !h.by_bot && w.session.server_ticks < h.at_tick + HURT_SKIP)
            })
        })
        .filter(|s| {
            let d = dist(eye, eye_of(s.pos));
            d >= min && d <= r.provoke_max
        })
        .filter(|s| line_open(w, eye, eye_of(s.pos)))
        .collect()
}

fn dist(a: [f64; 3], b: [f64; 3]) -> f64 {
    (0..3).map(|i| (a[i] - b[i]).powi(2)).sum::<f64>().sqrt()
}

fn flat(a: [f64; 3], b: [f64; 3]) -> f64 {
    ((a[0] - b[0]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
}

/// Main inventory slots (9 to 44) with nothing in them.
pub fn free_slots(w: &World) -> usize {
    (9i16..=44)
        .filter(|s| !w.inventory.slots.contains_key(s))
        .count()
}

/// The weapon slots held (window 0's 9 to 44) with their damage and uses left.
fn weapons(w: &World) -> Vec<(i16, ItemKind, f64, i32)> {
    w.inventory
        .slots
        .iter()
        .filter(|(s, _)| (9..=44).contains(*s))
        .filter_map(|(s, (k, _))| {
            let wp = data::generated::item_props::weapon_of(*k)?;
            (wp.damage > 0.0).then(|| (*s, *k, wp.damage, w.inventory.uses_left(*s).unwrap_or(0)))
        })
        .collect()
}

/// The weapon to strike with: the hardest-hitting with more than `min` uses left, the most
/// worn of those first (a sword under `min` is switched at once). `None` with none.
pub fn weapon_slot(w: &World, min: i32) -> Option<i16> {
    weapons(w)
        .into_iter()
        .filter(|(_, _, _, u)| *u > min)
        .max_by(|a, b| a.2.total_cmp(&b.2).then(b.3.cmp(&a.3)).then(b.0.cmp(&a.0)))
        .map(|(s, _, _, _)| s)
}

/// Uses left on every weapon held, summed.
pub fn weapon_uses(w: &World) -> i32 {
    weapons(w).iter().map(|x| x.3).sum()
}

/// The next step to hold the weapon in `slot`: `None` once the selected hotbar slot is it.
fn hold_slot(w: &World, slot: i16) -> Option<world::Act> {
    if w.click.is_some() || w.window.is_some() {
        return None;
    }
    if (36..=44).contains(&slot) {
        let i = u8::try_from(slot - 36).ok()?;
        return (w.held != i).then_some(world::Act::Hold { slot: i });
    }
    // In the main inventory: a number-key swap into the selected hotbar slot, one click.
    Some(world::Act::Click {
        slot,
        button: i8::try_from(w.held).unwrap_or(0),
        mode: 2,
    })
}

/// The controls of a step to the middle of cell `c` from feet at `pos`, and whether it is there.
/// Sneaking for the last part: no overshoot, no step off an edge.
pub fn step_to(pos: [f64; 3], c: [i32; 3]) -> (Controls, bool) {
    let t = Burrow::centre(c);
    let d = flat(pos, t);
    if d < STEP_DONE {
        return (Controls::default(), true);
    }
    (
        Controls {
            forward: true,
            sneak: d < 0.7,
            yaw: yaw_to(pos, t),
            pitch: Some(0.0),
            jump: false,
        },
        false,
    )
}

/// Blocks behind B's middle (toward the back wall) the fight holds the body at: pressed against
/// the wall it rests 0.2 back. On a real server the bot drifted 0.14 toward M once its forward
/// key was let go, and the enderman's reach came within 0.23 of it, not 0.37 (P4's parity row
/// e04, parity 45531046147).
pub const HOLD_BACK: f64 = 0.1;

/// Between strikes, the body pressed back against B's back wall (facing it, sneaking: no stare
/// at a wall), unless it is already `HOLD_BACK` behind B's middle or the wall is missing; else
/// `c`.
pub fn press_back(w: &World, pos: [f64; 3], b: Burrow, c: Controls) -> Controls {
    let bc = Burrow::centre(b.b);
    let off = (pos[0] - bc[0]) * f64::from(b.dir[0]) + (pos[2] - bc[2]) * f64::from(b.dir[1]);
    let back = [b.b[0] - b.dir[0], b.b[1], b.b[2] - b.dir[1]];
    let wall = w
        .block_at(world::BlockPos::new(back[0], back[1], back[2]))
        .and_then(data::generated::shapes::collision)
        == Some(data::types::Collision::Full);
    if off <= -HOLD_BACK || !wall {
        return c;
    }
    Controls {
        forward: true,
        sneak: true,
        yaw: b.out_yaw() + 180.0,
        pitch: Some(0.0),
        jump: false,
    }
}

/// The look straight along `yaw` at `pitch`.
fn look(yaw: f64, pitch: f32) -> Controls {
    Controls {
        yaw,
        pitch: Some(pitch),
        ..Controls::default()
    }
}

fn arr(c: path::Cell) -> [i32; 3] {
    [c.0, c.1, c.2]
}

fn cells(v: &[[i32; 3]]) -> Vec<path::Cell> {
    v.iter().map(|c| (c[0], c[1], c[2])).collect()
}

/// What a burrow block's placing did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Placed {
    Building,
    Done,
    Cannot(&'static str),
}

impl Plan {
    /// The hunt's numbers: the goal table's `[pearls]`, else the defaults.
    fn pearl_rules(&self) -> PearlRules {
        self.table
            .as_ref()
            .map_or_else(PearlRules::default, |t| t.pearls.clone())
    }

    /// Whether the hunt runs: agent P2's `EnderPhase::Hunt` (its hand-over, "warped forest reached:
    /// on to the hunt"), and from then on.
    pub(crate) fn hunting(&mut self, _w: &World) -> bool {
        if !self.pearls.started && self.ender.phase == endermen::EnderPhase::Hunt {
            self.pearls.started = true;
        }
        self.pearls.started
    }

    /// The bot stands in its built burrow (B or M): the hunt's rows answer an angry enderman
    /// there, not `ender_threat`'s pocket.
    pub(crate) fn in_burrow(&self, w: &World) -> bool {
        let (Some(b), Some(body)) = (self.pearls.burrow, w.body) else {
            return false;
        };
        self.pearls.built && b.inside(arr(path::stand_cell(w, body.pos)))
    }

    /// A note once a `NOTE_EVERY` for a standing condition `key`.
    fn pearl_note(&mut self, tick: u64, key: &str, what: String, why: String) {
        if self
            .pearls
            .noted
            .get(key)
            .is_some_and(|t| tick < t + NOTE_EVERY)
        {
            return;
        }
        self.pearls.noted.insert(key.to_owned(), tick);
        self.note(tick, what, why);
    }

    /// A note once for `key` in the run.
    fn pearl_note_once(&mut self, tick: u64, key: &str, what: String, why: String) {
        if self.pearls.noted.contains_key(key) {
            return;
        }
        self.pearls.noted.insert(key.to_owned(), tick);
        self.note(tick, what, why);
    }

    /// The row that acts this tick, noted when it changes (strike and hold are one fight).
    fn row(&mut self, tick: u64, id: &str, why: impl FnOnce() -> String) {
        let fight = |r: &str| r == "strike" || r == "hold";
        if self.pearls.row == id || (fight(&self.pearls.row) && fight(id)) {
            self.pearls.row = id.to_owned();
            return;
        }
        self.pearls.row = id.to_owned();
        let why = why();
        self.note(tick, format!("pearls row {id}"), why);
    }

    /// Drops a task the hunt's rows cut in on (a walk, a base step), saying so.
    fn cut_task(&mut self, tick: u64, row: &str) {
        if !matches!(self.task, Task::Idle) {
            let name = self.task.name();
            self.task = Task::Idle;
            self.note(
                tick,
                format!("{name} cut by {row}"),
                "an enderman's row comes first".to_owned(),
            );
        }
    }

    /// The hunt's call site (one fenced call in `step_inner`, after the always-rows): rows 1 to 5
    /// and 7 to 9 as controls; rows 6 and 7's tasks begun through `pearls_task`. `None` lets the
    /// task in hand run, or (out of the hunt) everything else.
    pub(crate) fn pearls_step(&mut self, w: &World, tick: u64) -> Option<Controls> {
        if !self.hunting(w) {
            return None;
        }
        self.pearls.provoking = None;
        // The planner's give-up (12 failed walks in a row) never ends the hunt: the frontier race
        // 45548022681 lane 5 stopped for good at tick ~12,000 of 36,000.
        if self.stuck {
            self.stuck = false;
            self.failed_in_row = 0;
            self.note(
                tick,
                "stuck cleared: the hunt goes on".to_owned(),
                "the walks' give-up does not end the hunt".to_owned(),
            );
        }
        let body = w.body?;
        let r = self.pearl_rules();
        self.pearls_account(w, tick);
        let feet = arr(path::stand_cell(w, body.pos));
        // 1. pearls-done.
        if craft::count(w, ItemKind::EnderPearl) >= r.pearls_wanted {
            return Some(self.row_done(w, tick, &r, feet));
        }
        // A magma cube pressing (within `cube_near`): the sealed pocket where the bot stands, held
        // until none is within `cube_clear` (the orchestrator; race 45560431011 lane 2 died
        // fighting cubes with 3 pearls held).
        let cubes: Vec<f64> = w
            .entities
            .values()
            .filter(|e| !e.dead && e.kind == EntityKind::MagmaCube)
            .map(|e| dist(e.pos, body.pos))
            .collect();
        let near = |r: f64| cubes.iter().any(|d| *d <= r);
        // Piglins out of the burrow too (restocking, walking): a bot in iron and no gold is their
        // foe, and the hunt has no heal (race 45588764709 lane 1: two piglins at a restock took
        // it 19.8 to 9.6 in 20 ticks, and every lane of the pearls_6 frontier hunted at 9.6).
        let out_of_b = !self.in_burrow(w);
        let piglins: Vec<f64> = w
            .entities
            .values()
            .filter(|e| !e.dead && PIGLINS.contains(&e.kind))
            .map(|e| dist(e.pos, body.pos))
            .collect();
        let piglin_near = |r: f64| out_of_b && piglins.iter().any(|d| *d <= r);
        let near_any = |a: f64, b: f64| near(a) || piglin_near(b);
        if near_any(r.cube_near, r.piglin_near)
            || (self.pearls.cube_pocket && near_any(r.cube_clear, r.piglin_clear))
        {
            if !self.pearls.cube_pocket {
                self.pearls.cube_pocket = true;
                self.note(
                    tick,
                    "pocket from magma cube or piglin".to_owned(),
                    format!(
                        "cube {:.1}, piglin {:.1}; sealed where the bot stands until none within {} (cubes) or {} (piglins)",
                        cubes.iter().copied().fold(f64::MAX, f64::min),
                        piglins.iter().copied().fold(f64::MAX, f64::min),
                        r.cube_clear,
                        r.piglin_clear
                    ),
                );
            }
            self.cut_task(tick, "pocket");
            self.pocket_step(w, tick);
            return Some(look(w.player.yaw.into(), EYES_DOWN));
        }
        if self.pearls.cube_pocket {
            self.pearls.cube_pocket = false;
            self.note(
                tick,
                "magma cubes and piglins gone: the hunt goes on".to_owned(),
                format!(
                    "no cube within {}, no piglin within {}",
                    r.cube_clear, r.piglin_clear
                ),
            );
        }
        let angry = angry_endermen(w);
        let inside = self.in_burrow(w);
        if let Some(b) = self.pearls.burrow.filter(|_| inside) {
            // 2. breach.
            let hurt = w
                .hits
                .iter()
                .filter(|h| {
                    h.cause == Some(EntityKind::Enderman) && h.at_tick > self.pearls.hit_seen
                })
                .map(|h| h.at_tick)
                .max();
            if let Some(at) = hurt {
                self.pearls.hit_seen = at;
                self.pearls.breaches += 1;
                self.pearls.sealed = Some("hurt by an enderman".to_owned());
                self.note(
                    tick,
                    "breach: hurt by an enderman".to_owned(),
                    format!(
                        "at {feet:?} (B {:?}), health {:?}; {} breaches in the run; seal M",
                        b.b, w.player.health, self.pearls.breaches
                    ),
                );
            }
            let near = angry
                .iter()
                .find(|s| s.dist <= r.breach_near)
                .filter(|_| feet != b.b);
            if let Some(s) = near
                && self.pearls.sealed.is_none()
            {
                self.pearls.breaches += 1;
                self.pearls.sealed =
                    Some(format!("enderman {} within {:.1} out of B", s.id, s.dist));
                self.note(
                    tick,
                    format!(
                        "breach: enderman {} {:.1} off with the bot out of B",
                        s.id, s.dist
                    ),
                    format!(
                        "at {feet:?}; seal M from B; {} breaches",
                        self.pearls.breaches
                    ),
                );
            }
            if self.pearls.sealed.is_some() || self.pearls.over {
                self.cut_task(tick, "breach");
                return Some(self.row_breach(w, tick, &r, b, feet, &angry));
            }
            // 3 and 4. strike and hold.
            if !angry.is_empty() {
                self.cut_task(tick, "hold");
                return Some(self.row_fight(w, tick, &r, b, feet, &angry));
            }
            self.pearls.fight = None;
            // An angry one beyond the fight's 64, ticking or frozen, is present too: in B,
            // never in M (P4's closed loop stood in M with an angry frozen one 80 off; the real
            // server hit M 4 times in 6).
            if let Some(s) = endermen::endermen(w)
                .into_iter()
                .find(|s| s.dist <= ANGRY_PRESENT && creepy(w, s.id))
            {
                self.cut_task(tick, "hold");
                self.row(tick, "hold", || {
                    format!(
                        "enderman {} angry {:.1} off, ticking {}",
                        s.id, s.dist, s.ticking
                    )
                });
                if feet != b.b {
                    return Some(step_to(body.pos, b.b).0);
                }
                return Some(press_back(w, body.pos, b, look(b.out_yaw(), 0.0)));
            }
        }
        // 5. collect (it may take the bot out of the burrow onto O and beside it).
        if let Some(b) = self.pearls.burrow.filter(|_| self.pearls.built)
            && angry.is_empty()
            && !endermen::endermen(w)
                .iter()
                .any(|s| dist(s.pos, Burrow::centre(b.o)) <= r.near_o)
            && let Some(c) = self.row_collect(w, tick, &r, b, feet)
        {
            self.cut_task(tick, "collect");
            return Some(c);
        }
        // Out of the burrow but by its line (back from a collect): home by steps.
        if let Some(b) = self.pearls.burrow.filter(|_| self.pearls.built)
            && !b.inside(feet)
            && flat(body.pos, Burrow::centre(b.b))
                <= f64::from(r.look_out_reach.max(LOOK_OUT)) + 3.0
            && matches!(self.task, Task::Idle)
            && (!angry.is_empty() || (self.pearls.provoke_from.is_none() && feet != b.o))
        {
            // An angry one with the bot out (a provoke from O or the look-out): back to B, by
            // O first from off the burrow's line (a straight line from a look-out hits the shell).
            if !angry.is_empty() {
                self.pearls.provoke_from = None;
                self.pearls.look_out = None;
                let on_line = feet == b.o || feet == b.m;
                return Some(step_to(body.pos, if on_line { b.b } else { b.o }).0);
            }
            let next = if feet == b.o { b.m } else { b.o };
            let next = if flat(body.pos, Burrow::centre(b.m)) < 0.8 {
                b.b
            } else {
                next
            };
            return Some(step_to(body.pos, next).0);
        }
        // Out of a built burrow with an angry enderman: issue 17's sealed pocket where the bot
        // stands (`ender_threat`), not a walk or a base step in the open.
        if !angry.is_empty() && !inside {
            // At B of a burrow going up: the build goes on (the shell is the answer).
            if self.pearls.burrow.is_some_and(|b| b.b == feet)
                && let Some(c) = self.row_standing(w, tick, &r, feet)
            {
                return Some(c);
            }
            // Else issue 17's pocket where it stands, for `threat_hold_ticks` at most: anger
            // with a target lasts, and P4's closed loop held a pocket to the cap.
            let since = *self.pearls.threat_since.get_or_insert(tick);
            if tick.saturating_sub(since) < r.threat_hold_ticks {
                self.cut_task(tick, "pocket");
                if since == tick {
                    self.note(
                        tick,
                        format!("enderman {} angry with no burrow: a pocket", angry[0].id),
                        format!(
                            "{:.1} off; held up to {} ticks, then the hunt goes on",
                            angry[0].dist, r.threat_hold_ticks
                        ),
                    );
                }
                self.pocket_step(w, tick);
                return Some(look(w.player.yaw.into(), EYES_DOWN));
            }
            if since + r.threat_hold_ticks == tick {
                self.note(
                    tick,
                    "pocket hold over: the hunt goes on".to_owned(),
                    format!(
                        "{} ticks with enderman {} angry",
                        r.threat_hold_ticks, angry[0].id
                    ),
                );
            }
        } else if angry.is_empty() {
            self.pearls.threat_since = None;
        }
        // In a hole (every side shut at the head's height, so no step out): a pillar up, a block
        // under the feet at the top of each jump (P4's seed 21 ended 9 below the floor with every
        // walk failing NoPath).
        if !self.pearls.built
            && matches!(self.task, Task::Idle)
            && !self.pearls.burrow.is_some_and(|b| b.inside(feet))
            && let Some(c) = self.out_of_hole(w, tick, feet)
        {
            return Some(c);
        }
        // Rows 6 and 7 that are tasks (the base, a walk to B, a dig to reopen the mouth).
        if !matches!(self.task, Task::Idle) {
            return None;
        }
        let provoking_out = self.pearls.provoke_from.is_some();
        if self.pearls.burrow.is_some_and(|b| {
            b.inside(feet)
                || (self.pearls.built && feet == b.o)
                || (self.pearls.built && provoking_out)
        }) && let Some(c) = self.row_standing(w, tick, &r, feet)
        {
            return Some(c);
        }
        if tick < self.retry_at {
            return Some(Controls::default());
        }
        match self.pearls_task(w, tick) {
            // The base waits (an act in flight, nothing seen): stand, and ask again next tick.
            Some(Task::Idle) => Some(Controls::default()),
            Some(t) => {
                self.begin(t);
                None
            }
            None => {
                self.retry_at = tick + crate::RETRY_TICKS;
                Some(Controls::default())
            }
        }
    }

    /// Kills, pearls picked up, a fight's health falls: counted every tick of the hunt.
    fn pearls_account(&mut self, w: &World, tick: u64) {
        if self.pearls.hunt_start.is_none() {
            self.pearls.hunt_start = Some(tick);
            self.pearls.start_filler = endermen::filler_held(w);
            self.pearls.start_picks = endermen::pickaxe_uses_left(w);
            self.pearls.start_weapons = weapon_uses(w);
            self.pearls.pearls_seen = craft::count(w, ItemKind::EnderPearl);
            self.pearls.hit_seen = w
                .hits
                .iter()
                .filter(|h| h.cause == Some(EntityKind::Enderman))
                .map(|h| h.at_tick)
                .max()
                .unwrap_or(0);
            self.note(
                tick,
                "pearls hunt begins".to_owned(),
                format!(
                    "health {:?}, food {:?}; weapon uses {}, pickaxe uses {}, filler {}, pearls {}, free slots {}",
                    w.player.health,
                    w.player.food,
                    self.pearls.start_weapons,
                    self.pearls.start_picks,
                    self.pearls.start_filler,
                    self.pearls.pearls_seen,
                    free_slots(w)
                ),
            );
        }
        let pearls = craft::count(w, ItemKind::EnderPearl);
        if pearls > self.pearls.pearls_seen {
            let n = pearls - self.pearls.pearls_seen;
            self.pearls.picked += u32::try_from(n).unwrap_or(0);
            self.pearls.last_pickup = Some(tick);
            self.pearls.collect = None;
            self.note(
                tick,
                format!("pearl picked ({pearls} held)"),
                format!(
                    "{} picked in the run, {} kills",
                    self.pearls.picked, self.pearls.kills
                ),
            );
        }
        self.pearls.pearls_seen = pearls;
        // Pearl items seen by the burrow, each once: dropped against picked (P4's closed loops got
        // 0.42 a kill where the loot gives 0.5).
        if let Some(b) = self.pearls.burrow {
            let o = Burrow::centre(b.o);
            let new: Vec<(i32, [f64; 3])> = w
                .entities
                .iter()
                .filter(|(id, e)| {
                    e.item().is_some_and(|(k, _)| k == ItemKind::EnderPearl)
                        && !self.pearls.drops.contains(id)
                        && dist(e.pos, o) <= 12.0
                })
                .map(|(id, e)| (*id, e.pos))
                .collect();
            for (id, at) in new {
                self.pearls.drops.push(id);
                self.note(
                    tick,
                    format!("pearl dropped (item {id})"),
                    format!(
                        "at ({:.1}, {:.1}, {:.1}), {:.1} from O; {} dropped, {} picked, {} kills",
                        at[0],
                        at[1],
                        at[2],
                        dist(at, o),
                        self.pearls.drops.len(),
                        self.pearls.picked,
                        self.pearls.kills
                    ),
                );
            }
        }
        // Ticking endermen seen, every 20 ticks, kept 600 (the mouth's side at site choice).
        if tick.is_multiple_of(20) {
            self.pearls
                .sightings
                .retain(|(t, _)| tick < t + SIGHTINGS_KEPT);
            for s in endermen::endermen(w)
                .iter()
                .filter(|s| s.ticking && s.dist <= 64.0)
            {
                self.pearls.sightings.push((tick, s.pos));
            }
        }
        // A provoke that angered its target (the fight's rows take it from B on).
        if let Some((id, at)) = self.pearls.provoke
            && creepy(w, id)
        {
            self.pearls.back_since = Some(tick);
            self.pearls.provoke = None;
            self.pearls.provoke_from = None;
            self.pearls.angers += 1;
            if let Some(i) = self.pearls.provoke_band.take() {
                self.pearls.band_angers[i] += 1;
            }
            self.note(
                tick,
                format!("provoked enderman {id}"),
                format!(
                    "its creepy flag after {} server ticks; {} angers of {} provokes; by band 6-16 {}/{}, 16-36 {}/{}, 36-64 {}/{}; back to B",
                    w.session.server_ticks.saturating_sub(at),
                    self.pearls.angers,
                    self.pearls.provokes,
                    self.pearls.band_angers[0],
                    self.pearls.band_provokes[0],
                    self.pearls.band_angers[1],
                    self.pearls.band_provokes[1],
                    self.pearls.band_angers[2],
                    self.pearls.band_provokes[2]
                ),
            );
        }
        let Some(mut f) = self.pearls.fight.clone() else {
            return;
        };
        let e = w.entities.get(&f.id);
        let health = e.and_then(world::Entity::health);
        if let (Some(now), Some(was)) = (health, f.last_health)
            && now < was
        {
            f.falls += 1;
            f.last_fall = tick;
            f.fall_noted = false;
            self.pearls.falls += 1;
        }
        f.last_health = health.or(f.last_health);
        let dead = e.is_some_and(|e| e.dead) || health.is_some_and(|h| h <= 0.0);
        if dead {
            self.pearls.kills += 1;
            self.pearls.site_kills += 1;
            self.pearls.last_kill = Some(tick);
            // The base tries again at once after a kill (P5's pause), and so does the hunt's guard.
            self.base_resume();
            self.pearls.base_skip_until = None;
            self.pearls.moves_no_kill = 0;
            self.pearls.fight = None;
            let held = craft::count(w, ItemKind::EnderPearl).max(1);
            // Weapon uses spent fight by fight (the base makes swords after the hunt begins: race
            // 45537373222 wrote -245 a pearl from the run's start).
            self.pearls.weapon_spent +=
                u64::try_from((f.weapon_before - weapon_uses(w)).max(0)).unwrap_or(0);
            let weapon = i32::try_from(self.pearls.weapon_spent).unwrap_or(i32::MAX);
            let filler = (self.pearls.start_filler - endermen::filler_held(w)).max(0);
            let picks = (self.pearls.start_picks - endermen::pickaxe_uses_left(w)).max(0);
            self.note(
                tick,
                format!("enderman {} killed", f.id),
                format!(
                    "{} hits ({} swings) in {} ticks; kill {} of the run; health {:?}; per pearl ({} held): hits {:.1}, weapon uses {:.1}, filler {:.1}, pickaxe uses {:.1}",
                    f.falls,
                    f.swings,
                    tick.saturating_sub(f.since),
                    self.pearls.kills,
                    w.player.health,
                    craft::count(w, ItemKind::EnderPearl),
                    self.pearls.falls as f64 / f64::from(held),
                    f64::from(weapon) / f64::from(held),
                    f64::from(filler) / f64::from(held),
                    f64::from(picks) / f64::from(held),
                ),
            );
            return;
        }
        if e.is_none() || !creepy(w, f.id) && !defend::provoked(w, EntityKind::Enderman) {
            self.pearls.fight = None;
            self.note(
                tick,
                format!(
                    "fight with enderman {} over: {}",
                    f.id,
                    if e.is_none() { "gone" } else { "calm" }
                ),
                format!(
                    "{} hits in {} ticks, no kill",
                    f.falls,
                    tick.saturating_sub(f.since)
                ),
            );
            return;
        }
        self.pearls.fight = Some(f);
    }

    /// Row 1: the wanted pearls held. The mouth sealed from B, eyes down, the aim met.
    fn row_done(&mut self, w: &World, tick: u64, r: &PearlRules, feet: [i32; 3]) -> Controls {
        self.row(tick, "pearls-done", || {
            format!("{} pearls held", r.pearls_wanted)
        });
        self.pearls.fight = None;
        if let Some(b) = self.pearls.burrow.filter(|_| self.pearls.built)
            && b.inside(feet)
        {
            if feet == b.m {
                let body_pos = w.body.map_or([0.0; 3], |x| x.pos);
                return step_to(body_pos, b.b).0;
            }
            let mouth: Vec<[i32; 3]> = b.mouth().into_iter().collect();
            if self.place_burrow(w, tick, &mouth, "seal") == Placed::Building {
                return look(b.out_yaw(), EYES_DOWN);
            }
        }
        if !self.met {
            self.met = true;
            self.note(
                tick,
                "pearls met".to_owned(),
                format!(
                    "{} pearls held after {} kills; health {:?}",
                    craft::count(w, ItemKind::EnderPearl),
                    self.pearls.kills,
                    w.player.health
                ),
            );
        }
        look(w.player.yaw.into(), EYES_DOWN)
    }

    /// Row 2: the mouth sealed from B, eyes down, until no angry enderman ticks within
    /// `calm_range` (then the build row reopens it); for good once the hunt is over.
    fn row_breach(
        &mut self,
        w: &World,
        tick: u64,
        r: &PearlRules,
        b: Burrow,
        feet: [i32; 3],
        angry: &[Sighting],
    ) -> Controls {
        let why = self
            .pearls
            .sealed
            .clone()
            .unwrap_or_else(|| "the hunt is over".to_owned());
        self.row(tick, "breach", || {
            format!("{why}; seal M from B, eyes down")
        });
        let pos = w.body.map_or([0.0; 3], |x| x.pos);
        if feet != b.b {
            return step_to(pos, b.b).0;
        }
        let mouth: Vec<[i32; 3]> = b.mouth().into_iter().collect();
        let placed = self.place_burrow(w, tick, &mouth, "seal");
        let near = angry
            .iter()
            .filter(|s| s.ticking && s.dist <= r.calm_range)
            .count();
        let timed_out = self.pearls.seal_until.is_some_and(|u| tick >= u);
        if placed == Placed::Done && (near == 0 || timed_out) && !self.pearls.over {
            self.pearls.seal_until = None;
            self.pearls.sealed = None;
            self.pearls.fight = None;
            self.note(
                tick,
                "breach: sealed and calm".to_owned(),
                format!(
                    "no angry enderman ticking within {}; the mouth reopens",
                    r.calm_range
                ),
            );
        } else {
            self.pearl_note(
                tick,
                "breach",
                "breach: sealed".to_owned(),
                format!(
                    "{why}; {near} angry ticking within {}; {placed:?}",
                    r.calm_range
                ),
            );
        }
        look(b.out_yaw(), EYES_DOWN)
    }

    /// Rows 3 and 4: the fight from B. A full charge on a target in reach: the torso, attack then
    /// swing; else hold in B looking level at the mouth at torso height. The fight's invariants:
    /// a health fall every `fall_window` in reach, a parked target looked at from M after
    /// `parked_ticks`, the fight given up with the mouth sealed after `fight_cap`.
    fn row_fight(
        &mut self,
        w: &World,
        tick: u64,
        r: &PearlRules,
        b: Burrow,
        feet: [i32; 3],
        angry: &[Sighting],
    ) -> Controls {
        let pos = w.body.map_or([0.0; 3], |x| x.pos);
        let target = self
            .pearls
            .fight
            .as_ref()
            .map(|f| f.id)
            .filter(|id| angry.iter().any(|s| s.id == *id))
            .or_else(|| angry.first().map(|s| s.id));
        let Some(id) = target else {
            return look(b.out_yaw(), 0.0);
        };
        let Some(s) = angry.iter().find(|s| s.id == id).copied() else {
            return look(b.out_yaw(), 0.0);
        };
        if self.pearls.fight.as_ref().is_none_or(|f| f.id != id) {
            self.pearls.fight = Some(Fight {
                id,
                since: tick,
                last_fall: tick,
                last_health: w.entities.get(&id).and_then(world::Entity::health),
                weapon_before: weapon_uses(w),
                ..Fight::default()
            });
            self.note(
                tick,
                format!("fight enderman {id} from B"),
                format!(
                    "{:.1} off, ticking {}; {} angry; health {:?}; weapon uses {}",
                    s.dist,
                    s.ticking,
                    angry.len(),
                    w.player.health,
                    weapon_uses(w)
                ),
            );
        }
        let since = self.pearls.fight.as_ref().map_or(tick, |f| f.since);
        if tick.saturating_sub(since) > r.fight_cap {
            self.pearls.sealed = Some(format!(
                "fight with enderman {id} over {} ticks",
                r.fight_cap
            ));
            self.note(
                tick,
                format!("fight given up: enderman {id}"),
                format!(
                    "{} ticks; the mouth sealed until it calms or wanders",
                    tick.saturating_sub(since)
                ),
            );
            return look(b.out_yaw(), EYES_DOWN);
        }
        self.pearls.parked_until = None;
        // On the burrow's top: 2 or so from B over the roof, out of reach (P4's note).
        if b.on_top(arr(path::feet_cell(s.pos))) {
            self.pearl_note_once(
                tick,
                &format!("roof {id}"),
                format!("enderman {id} on the roof"),
                format!(
                    "at ({:.1}, {:.1}, {:.1}) over the burrow at {:?}; held in B",
                    s.pos[0], s.pos[1], s.pos[2], b.b
                ),
            );
        }
        if feet != b.b {
            return step_to(pos, b.b).0;
        }
        if let Some(at) = self.pearls.back_since.take() {
            self.note(
                tick,
                format!(
                    "back in B {} ticks after the creepy flag",
                    tick.saturating_sub(at)
                ),
                format!("enderman {id} {:.1} off", s.dist),
            );
        }
        self.row(tick, "hold", || {
            format!("enderman {id} angry, {:.1} off", s.dist)
        });
        let eye = [pos[0], pos[1] + physics::EYE_HEIGHT, pos[2]];
        let reach = defend::box_distance_of(eye, s.pos, EntityKind::Enderman);
        let torso = torso_of(s.pos);
        let clear = look::clear(w, eye, torso);
        let in_reach = reach <= defend::STRIKE_REACH && clear;
        // The weapon in the hand from the fight's start: a change of the held item resets the
        // attack strength (race 45534591345 lane 1: the first swing went out the tick after the
        // switch, `attack.weak`, 1.2 damage).
        if let Some(slot) = weapon_slot(w, r.weapon_min_uses)
            && let Some(a) = hold_slot(w, slot)
        {
            if self.acts.is_empty() {
                self.acts.push(a);
                self.pearls.last_swing = Some(tick);
            }
            return press_back(w, pos, b, look(b.out_yaw(), 0.0));
        }
        // Close by the mouth and out of reach: where, once a fight (the closed loop's question).
        if !in_reach && s.dist <= 3.5 {
            self.pearl_note_once(
                tick,
                &format!("close {id}"),
                format!("enderman {id} close but out of reach"),
                format!(
                    "it at ({:.2}, {:.2}, {:.2}), the bot at ({:.2}, {:.2}, {:.2}), O {:?}; eye to box {reach:.2}, line to the torso clear {clear}",
                    s.pos[0], s.pos[1], s.pos[2], pos[0], pos[1], pos[2], b.o
                ),
            );
        }
        if !in_reach {
            let f = self.pearls.fight.get_or_insert_with(Fight::default);
            let out = *f.out_since.get_or_insert(tick);
            if tick.saturating_sub(out) >= r.parked_ticks {
                f.out_since = Some(tick);
                // Never a look from M with an angry one present: the bot in M was hit once by one
                // that came back at an angle (P4's parity run 45535279839); B never was.
                // The mouth sealed `parked_seal_ticks`: it loses its path, may wander and come
                // back through O (the orchestrator; race 45572123445 lane 6 had one parked 3.6
                // off beside the shell).
                {
                    self.pearls.sealed = Some(format!("enderman {id} parked out of reach"));
                    self.pearls.seal_until = Some(tick + r.parked_seal_ticks);
                    self.note(
                        tick,
                        format!("enderman {id} parked: the mouth sealed"),
                        format!(
                            "out of reach {} ticks, {:.1} off; {} angry; sealed {} ticks",
                            r.parked_ticks,
                            s.dist,
                            angry.len(),
                            r.parked_seal_ticks
                        ),
                    );
                }
            }
            // Level at the mouth, at torso height over O; pressed against the back wall.
            let o = Burrow::centre(b.o);
            let (yaw, pitch) = look_at(eye, [o[0], o[1] + TORSO, o[2]]);
            return press_back(w, pos, b, look(yaw, pitch));
        }
        if let Some(f) = self.pearls.fight.as_mut() {
            // Into reach: the health-fall window starts here, not at the provoke.
            if f.out_since.take().is_some() {
                f.last_fall = tick;
            }
            if tick.saturating_sub(f.last_fall) >= r.fall_window && !f.fall_noted {
                f.fall_noted = true;
                let (falls, swings) = (f.falls, f.swings);
                self.note(
                    tick,
                    format!("fight: no health fall on enderman {id}"),
                    format!(
                        "{} ticks in reach with no fall; {falls} hits, {swings} swings",
                        r.fall_window
                    ),
                );
            }
        }
        let (yaw, pitch) = look_at(eye, torso);
        #[allow(clippy::cast_possible_truncation)]
        let staring = look::stares(w, eye, yaw as f32, pitch).is_some();
        let Some(slot) = weapon_slot(w, r.weapon_min_uses) else {
            if !self.pearls.weapon_short_noted {
                self.pearls.weapon_short_noted = true;
                self.note(
                    tick,
                    "no weapon over its minimum".to_owned(),
                    format!(
                        "no weapon with more than {} uses: hold in B, the base step makes more",
                        r.weapon_min_uses
                    ),
                );
            }
            return look(yaw, pitch);
        };
        if let Some(a) = hold_slot(w, slot) {
            if self.acts.is_empty() {
                self.acts.push(a);
            }
            return look(yaw, pitch);
        }
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let ready = (crate::hunt::swing_ticks(w) * crate::dig::slowdown(w)).ceil() as u64 + 1;
        let charged = self
            .pearls
            .last_swing
            .is_none_or(|t| tick.saturating_sub(t) >= ready);
        if charged && !staring && self.acts.is_empty() && w.click.is_none() {
            self.row(tick, "strike", String::new);
            self.pearls.last_swing = Some(tick);
            self.pearls.swings += 1;
            self.swings_sent += 1;
            if let Some(f) = self.pearls.fight.as_mut() {
                f.swings += 1;
            }
            // The attack first, then the swing: swing-first made every hit about 1 damage
            // (clojurecraft).
            self.acts.push(world::Act::Attack { entity: id });
            self.acts.push(world::Act::Swing);
            return look(yaw, pitch);
        }
        press_back(w, pos, b, look(yaw, pitch))
    }

    /// Whether a step out of B may be taken now (the bounce counter): one a `step_out_every`,
    /// or any after a kill or a pickup since the last. Counted, and noted when held back.
    fn step_out_ok(&mut self, tick: u64, r: &PearlRules) -> bool {
        let last = self.pearls.last_step_out;
        // A step out begun is walked to its end (a step is some 15 ticks sneaking): the closed
        // loop held the bot in B for good when every tick of one step counted as another.
        if last.is_some_and(|l| tick < l + STEP_OUT_TICKS) {
            return true;
        }
        let event = self
            .pearls
            .last_kill
            .max(self.pearls.last_pickup)
            .zip(last)
            .is_some_and(|(e, l)| e >= l);
        let ok = last.is_none_or(|l| tick >= l + r.step_out_every) || event;
        if ok {
            self.pearls.last_step_out = Some(tick);
            self.pearls.step_outs += 1;
        } else {
            self.pearls.step_outs_held += 1;
            self.pearl_note(
                tick,
                "bounce",
                "step out held: the bounce counter".to_owned(),
                format!(
                    "{} steps out, {} held back; one a {} ticks without a kill or pickup",
                    self.pearls.step_outs, self.pearls.step_outs_held, r.step_out_every
                ),
            );
        }
        ok
    }

    /// Row 5: a pearl on the ground within `collect_range` of O, a free slot: step to M, then
    /// to it within `collect_reach` of B; given up after `collect_ticks`.
    fn row_collect(
        &mut self,
        w: &World,
        tick: u64,
        r: &PearlRules,
        b: Burrow,
        feet: [i32; 3],
    ) -> Option<Controls> {
        if free_slots(w) == 0 {
            self.pearl_note(
                tick,
                "full",
                "no slot for a pearl".to_owned(),
                "every main slot full: the junk drop first".to_owned(),
            );
            return None;
        }
        let o = Burrow::centre(b.o);
        let bc = Burrow::centre(b.b);
        let items: Vec<(i32, [f64; 3])> = w
            .entities
            .iter()
            .filter(|(id, e)| {
                e.item().is_some_and(|(k, _)| k == ItemKind::EnderPearl)
                    && !self.pearls.collect_refused.contains(id)
            })
            .map(|(id, e)| (*id, e.pos))
            .filter(|(_, p)| dist(*p, o) <= r.collect_range)
            .collect();
        let near = items
            .iter()
            .filter(|(_, p)| flat(*p, bc) <= r.collect_reach && (p[1] - bc[1]).abs() <= 1.5)
            .min_by(|a, b2| {
                flat(a.1, bc)
                    .total_cmp(&flat(b2.1, bc))
                    .then(a.0.cmp(&b2.0))
            })
            .copied();
        let Some((id, p)) = near else {
            if let Some((id, _)) = self.pearls.collect.take() {
                self.note(
                    tick,
                    format!("pearl {id} gone or out of reach"),
                    "back to B".to_owned(),
                );
            }
            return None;
        };
        let since = match self.pearls.collect {
            Some((c, at)) if c == id => at,
            _ => {
                self.pearls.collect = Some((id, tick));
                self.note(
                    tick,
                    format!("collect pearl {id}"),
                    format!(
                        "{:.1} from B; none angry; {} free slots",
                        flat(p, bc),
                        free_slots(w)
                    ),
                );
                tick
            }
        };
        if tick.saturating_sub(since) > r.collect_ticks {
            self.pearls.collect = None;
            self.pearls.collect_refused.push(id);
            self.note(
                tick,
                format!("collect pearl {id} given up"),
                format!("{} ticks; it stays", r.collect_ticks),
            );
            return None;
        }
        self.row(tick, "collect", || format!("pearl {id} on the ground"));
        let pos = w.body.map_or([0.0; 3], |x| x.pos);
        let m = Burrow::centre(b.m);
        // From B, M first (the line out of the mouth); from M, straight at it.
        if feet == b.b && flat(p, m) > 0.5 {
            return Some(step_to(pos, b.m).0);
        }
        Some(step_to(pos, arr(path::feet_cell(p))).0)
    }

    /// Rows 7 (the immediate parts: build and mend from B), 8 and 9 for a bot in its burrow.
    fn row_standing(
        &mut self,
        w: &World,
        tick: u64,
        r: &PearlRules,
        feet: [i32; 3],
    ) -> Option<Controls> {
        let b = self.pearls.burrow?;
        let pos = w.body.map_or([0.0; 3], |x| x.pos);
        // 7. a block missing: built or mended from B.
        let missing = burrow::missing(w, &b);
        if self.pearls.built {
            for c in &missing {
                if !self.pearls.taken.contains(c) {
                    self.pearls.taken.push(*c);
                    self.note(
                        tick,
                        format!("burrow block taken at {c:?}"),
                        "a built burrow cell is open with no dig of ours: mended from B".to_owned(),
                    );
                }
            }
        }
        if missing.is_empty() {
            self.pearls.taken.clear();
        }
        if !missing.is_empty() {
            let (c, there) = step_to(pos, b.b);
            if feet != b.b || !there {
                return Some(c);
            }
            self.row(tick, "build", || {
                format!("{} burrow blocks missing at {:?}", missing.len(), b.b)
            });
            return match self.place_burrow(w, tick, &missing, "burrow") {
                Placed::Building | Placed::Done => Some(look(b.out_yaw(), 0.0)),
                Placed::Cannot("no burrow block held") => {
                    if !self.pearls.restock {
                        self.pearls.restock = true;
                        self.note(
                            tick,
                            "no burrow block held: restock".to_owned(),
                            format!(
                                "{} filler; netherrack dug near until {}",
                                endermen::filler_held(w),
                                r.restock_to
                            ),
                        );
                    }
                    None
                }
                Placed::Cannot(why) => {
                    self.refuse_site(tick, b, why);
                    Some(Controls::default())
                }
            };
        }
        if !self.pearls.built {
            self.pearls.built = true;
            self.pearls.since = Some(tick);
            self.note(
                tick,
                format!("burrow built at {:?}", b.b),
                format!(
                    "mouth toward ({}, {}), {} blocks placed; delta ground {} from O",
                    b.dir[0],
                    b.dir[1],
                    self.pearls.site_blocks,
                    delta_distance(w, b.o, DELTA_MEASURE)
                        .map_or_else(|| format!("over {DELTA_MEASURE}"), |d| format!("{d}"))
                ),
            );
        }
        // The mouth closed (a seal of a breach that is over): reopened by `pearls_task`'s dig.
        if b.mouth().iter().any(|c| {
            w.block_at(world::BlockPos::new(c[0], c[1], c[2]))
                .and_then(data::generated::shapes::collision)
                .is_some_and(|k| k != data::types::Collision::Empty)
        }) {
            return None;
        }
        // The hunt's clocks: a kill a `kill_window`, else the burrow moves.
        let last = self.pearls.last_kill.or(self.pearls.since).unwrap_or(tick);
        let idle = tick.saturating_sub(last);
        // Moves are the expensive thing now (filler): only when no enderman has been seen within
        // `seen_near` of B for `burrow_idle_ticks` (or no kill in `kill_window`).
        let bc = Burrow::centre(b.b);
        // Only one in sight from O counts: an enderman parked out of sight 26 off kept a burrow
        // from moving for 4,000 ticks with no provoke (the frontier race 45551462309).
        if endermen::endermen(w)
            .iter()
            .any(|s| dist(s.pos, bc) <= r.seen_near && look::clear(w, eye_in(b.o), eye_of(s.pos)))
        {
            self.pearls.last_near = Some(tick);
        }
        let quiet = tick.saturating_sub(self.pearls.last_near.unwrap_or(last).max(last));
        // A site that has had kills has used its locals up: moved after `burrow_idle_after_kill`.
        let idle_limit = if self.pearls.site_kills > 0 {
            r.burrow_idle_after_kill
        } else {
            r.burrow_idle_ticks
        };
        let calm_near = quiet < idle_limit;
        if idle >= r.kill_window || quiet >= idle_limit {
            self.move_burrow(tick, r, b, idle, calm_near);
            return Some(Controls::default());
        }
        // Any other hostile within `hostile_near` of O: in B too (the frontier race 45553808358
        // lane 6 stood at O by the deltas and magma cubes took it from 18.3 to 6.6).
        let o_at = Burrow::centre(b.o);
        if let Some((id, e)) = w.entities.iter().find(|(_, e)| {
            !e.dead
                && e.kind != EntityKind::Enderman
                && defend::hostile(e.kind)
                // Not a neutral one (a zombified piglin held a bot in B with nothing to fear,
                // race 45560431011): the survival rules' neutral kinds.
                && !self
                    .table
                    .as_ref()
                    .is_some_and(|t| t.survival.neutral.contains(&e.kind))
                && dist(e.pos, o_at) <= r.hostile_near
        }) {
            let (id, kind) = (*id, e.kind);
            self.provoke_hold(tick);
            self.row(tick, "hold", || format!("{kind:?} {id} near O: in B"));
            if feet != b.b {
                return Some(step_to(pos, b.b).0);
            }
            return Some(press_back(w, pos, b, look(b.out_yaw(), 0.0)));
        }
        // Any enderman within `near_o` of O, calm or not: the bot is in B, never out (P4's real
        // runs on the solid shell: B never hit in 9 rows over 4 runs; M hit in 2 of 4 runs, once
        // each, by one that came round angry).
        let o = Burrow::centre(b.o);
        if let Some(s) = endermen::endermen(w)
            .into_iter()
            .find(|s| dist(s.pos, o) <= r.near_o)
        {
            self.pearls.provoke = None;
            self.pearls.provoke_from = None;
            self.row(tick, "hold", || {
                format!("enderman {} {:.1} from O: in B", s.id, dist(s.pos, o))
            });
            if feet != b.b {
                return Some(step_to(pos, b.b).0);
            }
            return Some(press_back(w, pos, b, look(b.out_yaw(), 0.0)));
        }
        // 8. provoke.
        if let Some(c) = self.row_provoke(w, tick, r, b, feet) {
            return Some(c);
        }
        // 9. wait: in M, eyes level.
        self.row(tick, "wait", || {
            format!("burrow at {:?}; no calm ticking enderman in sight", b.b)
        });
        self.pearl_note(
            tick,
            "wait",
            "waiting at the burrow".to_owned(),
            format!(
                "{idle} ticks since the last kill or the build; nearest enderman {}; {} kills, {} pearls",
                endermen::endermen(w)
                    .first()
                    .map_or("none".to_owned(), |s| format!("{} at {:.1}, ticking {}", s.id, s.dist, s.ticking)),
                self.pearls.kills,
                craft::count(w, ItemKind::EnderPearl)
            ),
        );
        // At O, not in M: O sees all round where M sees one cone, and a calm enderman is harmless
        // unless stared at (keep_off guards every look); the yaw sweeps 90 either side of the
        // mouth's line every `SWEEP_TICKS` (the orchestrator; every burrow of race 45548022681
        // waited 2,400 ticks with endermen 25 to 60 off out of M's sight).
        if feet == b.o {
            let (c, there) = step_to(pos, b.o);
            if !there {
                return Some(c);
            }
            self.pearls.at_o_ticks += 1;
            self.pearl_note(
                tick,
                "at o",
                "waiting at O".to_owned(),
                format!(
                    "{} ticks waited at O in the run, {} provokes, {} kills",
                    self.pearls.at_o_ticks, self.pearls.provokes, self.pearls.kills
                ),
            );
            #[allow(clippy::cast_precision_loss)]
            let phase = (tick % SWEEP_TICKS) as f64 / SWEEP_TICKS as f64 * std::f64::consts::TAU;
            return Some(look(b.out_yaw() + 90.0 * phase.sin(), 0.0));
        }
        if feet == b.b && self.step_out_ok(tick, r) {
            return Some(step_to(pos, b.o).0);
        }
        if feet == b.m {
            return Some(step_to(pos, b.o).0);
        }
        Some(look(b.out_yaw(), 0.0))
    }

    /// Row 8: a calm ticking enderman in sight from M: its eye until the creepy flag, at most
    /// `provoke_ticks` server ticks; `provoke_tries` tries an enderman, then skipped a while.
    fn row_provoke(
        &mut self,
        w: &World,
        tick: u64,
        r: &PearlRules,
        b: Burrow,
        feet: [i32; 3],
    ) -> Option<Controls> {
        let server = w.session.server_ticks;
        let pos = w.body.map_or([0.0; 3], |x| x.pos);
        // An angry enderman alive anywhere within 128, ticking or frozen, is present: no new
        // provoke (it resumes the moment its chunk ticks; P4's closed loop provoked a calm one
        // with an angry frozen one 80 off).
        if self.pearls.provoke.is_none()
            && let Some(s) = endermen::endermen(w)
                .into_iter()
                .find(|s| s.dist <= ANGRY_PRESENT && creepy(w, s.id))
        {
            self.pearl_note(
                tick,
                "angry far",
                format!("no provoke: enderman {} angry {:.1} off", s.id, s.dist),
                format!(
                    "ticking {}; an angry one, frozen or not, is present",
                    s.ticking
                ),
            );
            return None;
        }
        // Its stared-at flag (index 18) seen set during a provoke: noted once a provoke.
        if let Some((id, at)) = self.pearls.provoke
            && w.entities.get(&id).is_some_and(|e| {
                matches!(
                    e.metadata.get(&endermen::STARED_INDEX),
                    Some(MetaValue::Bool(true))
                )
            })
        {
            self.pearl_note_once(
                tick,
                &format!("stared {id} {at}"),
                format!("provoke: enderman {id} stared-at flag set"),
                format!("{} server ticks into the stare", server.saturating_sub(at)),
            );
        }
        if let Some((id, at)) = self.pearls.provoke {
            let from = self.pearls.provoke_from.unwrap_or(b.m);
            let seen = w
                .entities
                .get(&id)
                .is_some_and(|e| !e.dead && look::clear(w, eye_in(from), eye_of(e.pos)));
            if server.saturating_sub(at) > r.provoke_ticks || !seen || feet != from {
                self.pearls.provoke = None;
                self.pearls.provoke_from = None;
                // Past 36 one try, then skipped (the orchestrator: 2 in 5 angered there).
                let far = self.pearls.provoke_band.take() == Some(2);
                let t = self.provoke_try(id, tick, r, far);
                self.note(
                    tick,
                    format!("provoke enderman {id}: no anger"),
                    format!(
                        "{} server ticks, in sight {seen}; try {t} of {}",
                        server.saturating_sub(at),
                        r.provoke_tries
                    ),
                );
            } else if let Some(e) = w.entities.get(&id) {
                let eye = [pos[0], pos[1] + physics::EYE_HEIGHT, pos[2]];
                let (yaw, pitch) = look_at(eye, eye_of(e.pos));
                self.pearls.provoking = u32::try_from(id).ok();
                return Some(look(yaw, pitch));
            }
        }
        // From M first; with none in sight there, from O at one far enough that the step back
        // to B (two cells, some 10 ticks) is made before it can come (the 26.1.2 jar: within 16 it
        // walks; past 16 it teleports after 15 goal ticks). M's mouth sees one way only: three
        // dev-forest bots waited 3,300 to 4,000 ticks with endermen 15 to 30 off out of its sight
        // (race 45534591345).
        let skipped = |s: &Sighting| {
            self.pearls
                .tries
                .iter()
                .any(|(i, _, until)| *i == s.id && tick < *until)
        };
        // One a provoke from M failed on is tried from O next (the frontier race 45548022681 lane
        // 6: three stares from M at one 13 off in our sight, its stared-at flag never set: the
        // server's line from M was cut by what we do not see).
        let failed_m = |s: &Sighting| {
            self.pearls
                .tries
                .iter()
                .any(|(i, n, _)| *i == s.id && *n > 0)
        };
        let from_m = provoke_candidates(w, eye_in(b.m), r)
            .into_iter()
            .find(|s| !skipped(s) && !failed_m(s))
            .map(|s| (s, b.m));
        let from_o = || {
            let far = PearlRules {
                provoke_max: r.provoke_out_max,
                ..r.clone()
            };
            provoke_candidates_from(w, eye_in(b.o), r.provoke_out_min, &far)
                .into_iter()
                .find(|s| !skipped(s))
                .map(|s| (s, b.o))
        };
        let found = from_m.or_else(from_o);
        if found.is_some() {
            self.pearls.last_cand = Some(tick);
        }
        let Some((cand, from)) = found else {
            return self.look_out(w, tick, r, b, feet);
        };
        self.row(tick, "provoke", || {
            format!("enderman {} calm and ticking {:.1} off", cand.id, cand.dist)
        });
        if feet == b.b {
            if self.step_out_ok(tick, r) {
                self.pearls.provoke_from = Some(from);
                return Some(step_to(pos, from).0);
            }
            return Some(look(b.out_yaw(), 0.0));
        }
        self.pearls.provoke_from = Some(from);
        let (c, there) = step_to(pos, from);
        if !there {
            return Some(c);
        }
        if !burrow::missing(w, &b).is_empty() {
            self.pearls.provoke_from = None;
            return None;
        }
        self.pearls.provoke = Some((cand.id, server));
        self.pearls.provoke_band = Some(band(cand.dist));
        self.pearls.band_provokes[band(cand.dist)] += 1;
        self.pearls.provokes += 1;
        if !self.pearls.provoked.contains(&cand.id) {
            self.pearls.provoked.push(cand.id);
        }
        self.note(
            tick,
            format!("provoke enderman {}", cand.id),
            format!(
                "from {} at {from:?}, {:.1} off, ticking; up to {} server ticks; burrow checked whole",
                if from == b.m { "M" } else { "O" },
                cand.dist,
                r.provoke_ticks
            ),
        );
        let eye = [pos[0], pos[1] + physics::EYE_HEIGHT, pos[2]];
        let (yaw, pitch) = look_at(eye, eye_of(cand.pos));
        self.pearls.provoking = u32::try_from(cand.id).ok();
        Some(look(yaw, pitch))
    }

    /// A provoke in hand dropped for a hold.
    fn provoke_hold(&mut self, _tick: u64) {
        self.pearls.provoke = None;
        self.pearls.provoke_from = None;
    }

    /// The look-out (the orchestrator): from O, with no calm enderman in sight for
    /// `LOOK_OUT_AFTER` ticks while endermen were seen 25 to 60 off, a step out up to
    /// `LOOK_OUT` from O toward them, on O's own floor with no drop, to look; a provoke from there
    /// at one 12 to 36 off in line, then straight back to B (the hunt's home steps). Over after
    /// `LOOK_OUT_TICKS` there with none in sight.
    fn look_out(
        &mut self,
        w: &World,
        tick: u64,
        r: &PearlRules,
        b: Burrow,
        feet: [i32; 3],
    ) -> Option<Controls> {
        let pos = w.body.map_or([0.0; 3], |x| x.pos);
        let o = Burrow::centre(b.o);
        let started = self.pearls.last_cand.or(self.pearls.since).unwrap_or(tick);
        if tick.saturating_sub(started) < LOOK_OUT_AFTER
            || (feet != b.o && self.pearls.look_out.is_none())
        {
            return None;
        }
        let far: Vec<[f64; 3]> = self
            .pearls
            .sightings
            .iter()
            .map(|(_, p)| *p)
            .filter(|p| (25.0..=60.0).contains(&flat(*p, o)))
            .collect();
        let c = centroid(&far)?;
        let spot = match self.pearls.look_out {
            Some((l, _)) => l,
            None => {
                let (dx, dz) = (f64::from(c.0) + 0.5 - o[0], f64::from(c.2) + 0.5 - o[2]);
                let len = (dx * dx + dz * dz).sqrt().max(1e-6);
                let mut cells = path::Cells::new(w);
                let shell = b.shell();
                #[allow(clippy::cast_possible_truncation)]
                // Every cell of the straight line out to it standable: a step goes straight (race
                // 45560431011: look-outs 4 off past a stem or a cap, "no way there" each time).
                let at_k = |k: i32| {
                    [
                        b.o[0] + (dx / len * f64::from(k)).round() as i32,
                        b.o[1],
                        b.o[2] + (dz / len * f64::from(k)).round() as i32,
                    ]
                };
                let reach = (1..=r.look_out_reach.max(1))
                    .take_while(|k| {
                        let l = at_k(*k);
                        cells.standable((l[0], l[1], l[2]))
                            && !shell.contains(&l)
                            && !b.inside(l)
                            && !ledge(w, (l[0], l[1], l[2]), 1)
                    })
                    .last();
                let pick = reach.map(at_k);
                let Some(l) = pick else {
                    self.pearls.last_cand = Some(tick);
                    return None;
                };
                self.pearls.look_out = Some((l, tick));
                self.pearls.provoke_from = Some(l);
                self.pearls.look_outs += 1;
                self.note(
                    tick,
                    format!("look-out at {l:?}"),
                    format!(
                        "none in sight {} ticks; {} endermen seen 25 to 60 off; look-out {} in the run",
                        tick.saturating_sub(started),
                        far.len(),
                        self.pearls.look_outs
                    ),
                );
                l
            }
        };
        if feet != spot {
            let since = self.pearls.look_out.map_or(tick, |x| x.1);
            if tick.saturating_sub(since) > LOOK_OUT_TICKS {
                self.end_look_out(tick, "no way there");
                return Some(step_to(pos, b.o).0);
            }
            return Some(step_to(pos, spot).0);
        }
        let out = PearlRules {
            provoke_max: if flat(Burrow::centre(spot), o) > f64::from(LOOK_OUT) + 0.5 {
                r.look_out_far_max
            } else {
                r.provoke_max
            },
            ..r.clone()
        };
        // Out past the near look-out, a target no nearer than `look_out_far_min` to the spot: the
        // walk back is longer (the orchestrator: from 24 an angry one needs 50 ticks or more).
        let far_spot = flat(Burrow::centre(spot), o) > f64::from(LOOK_OUT) + 0.5;
        let min = if far_spot {
            r.look_out_far_min
        } else {
            r.provoke_out_min
        };
        let cand = provoke_candidates_from(w, eye_in(spot), min, &out)
            .into_iter()
            .find(|s| {
                !self
                    .pearls
                    .tries
                    .iter()
                    .any(|(i, _, until)| *i == s.id && tick < *until)
            });
        let Some(cand) = cand else {
            let since = self.pearls.look_out.map_or(tick, |x| x.1);
            if tick.saturating_sub(since) > LOOK_OUT_TICKS {
                self.end_look_out(tick, "none in sight from it");
                return Some(step_to(pos, b.o).0);
            }
            #[allow(clippy::cast_precision_loss)]
            let phase = (tick % SWEEP_TICKS) as f64 / SWEEP_TICKS as f64 * std::f64::consts::TAU;
            return Some(look(
                yaw_to(o, [f64::from(c.0), o[1], f64::from(c.2)]) + 60.0 * phase.sin(),
                0.0,
            ));
        };
        let server = w.session.server_ticks;
        self.pearls.provoke = Some((cand.id, server));
        self.pearls.provoke_band = Some(band(cand.dist));
        self.pearls.band_provokes[band(cand.dist)] += 1;
        self.pearls.provoke_from = Some(spot);
        self.pearls.look_out = None;
        self.pearls.provokes += 1;
        self.pearls.look_out_provokes += 1;
        if !self.pearls.provoked.contains(&cand.id) {
            self.pearls.provoked.push(cand.id);
        }
        self.note(
            tick,
            format!("provoke enderman {} from the look-out", cand.id),
            format!(
                "at {spot:?}, {:.1} off; {} look-out provokes of {} look-outs",
                cand.dist, self.pearls.look_out_provokes, self.pearls.look_outs
            ),
        );
        let eye = [pos[0], pos[1] + physics::EYE_HEIGHT, pos[2]];
        let (yaw, pitch) = look_at(eye, eye_of(cand.pos));
        self.pearls.provoking = u32::try_from(cand.id).ok();
        Some(look(yaw, pitch))
    }

    /// The look-out given up: back to O, the wait's clock restarted.
    fn end_look_out(&mut self, tick: u64, why: &str) {
        self.pearls.look_out = None;
        self.pearls.provoke_from = None;
        self.pearls.last_cand = Some(tick);
        self.note(tick, "look-out over".to_owned(), why.to_owned());
    }

    /// One more provoke try on `id`; past `provoke_tries` it is skipped `provoke_skip_ticks`.
    fn provoke_try(&mut self, id: i32, tick: u64, r: &PearlRules, far: bool) -> u32 {
        let i = match self.pearls.tries.iter().position(|t| t.0 == id) {
            Some(i) => i,
            None => {
                self.pearls.tries.push((id, 0, 0));
                self.pearls.tries.len() - 1
            }
        };
        self.pearls.tries[i].1 += 1;
        let n = self.pearls.tries[i].1;
        if n >= r.provoke_tries || far {
            self.pearls.tries[i] = (id, 0, tick + r.provoke_skip_ticks);
            self.note(
                tick,
                format!("enderman {id} skipped"),
                format!(
                    "{n} provokes with no anger; skipped {} ticks",
                    r.provoke_skip_ticks
                ),
            );
        }
        n
    }

    /// The burrow moved: no kill in `kill_window`, or no calm enderman in `burrow_idle_ticks`.
    /// Two moves without a kill end the hunt (the mouth sealed, held in B).
    fn move_burrow(&mut self, tick: u64, r: &PearlRules, b: Burrow, idle: u64, calm: bool) {
        if self.pearls.moves_no_kill >= r.moves_without_kill {
            self.pearls.over = true;
            self.pearls.sealed = Some("the hunt is over".to_owned());
            self.note(
                tick,
                "pearls hunt over".to_owned(),
                format!(
                    "{} moves without a kill; {} kills, {} pearls; sealed in",
                    self.pearls.moves_no_kill, self.pearls.kills, self.pearls.pearls_seen
                ),
            );
            return;
        }
        self.pearls.moves += 1;
        self.pearls.moves_no_kill += 1;
        self.pearls.reclaim = true;
        self.pearls.reclaimed = 0;
        self.pearls.moved_from.push(b.b);
        self.pearls.moved_at.push(tick);
        self.pearls.old_shell = b.shell().into_iter().filter(|c| c[1] >= b.b[1]).collect();
        self.pearls.old_floors = vec![
            [b.b[0], b.b[1] - 1, b.b[2]],
            [b.m[0], b.m[1] - 1, b.m[2]],
            [b.o[0], b.o[1] - 1, b.o[2]],
        ];
        self.pearls.burrow = None;
        self.pearls.built = false;
        self.pearls.since = None;
        self.pearls.last_kill = None;
        self.pearls.site_at = None;
        self.note(
            tick,
            "burrow moved: none came".to_owned(),
            format!(
                "{idle} ticks since the last kill or the build (calm in sight {calm}); move {} ({} without a kill); a new site {}+ off",
                self.pearls.moves, self.pearls.moves_no_kill, r.burrow_move
            ),
        );
    }

    /// A site given up: its B refused, the burrow forgotten.
    fn refuse_site(&mut self, tick: u64, b: Burrow, why: &str) {
        self.pearls.refused.push(b.b);
        self.pearls.burrow = None;
        self.pearls.built = false;
        self.pearls.site_at = None;
        self.note(
            tick,
            format!("burrow site at {:?} given up", b.b),
            format!("{why}; {} sites refused", self.pearls.refused.len()),
        );
    }

    /// The next block of `cells` (a burrow's missing shell, the mouth's seal) from where the
    /// body stands, one a `BUILD_EVERY`, with a block no enderman can take.
    fn place_burrow(&mut self, w: &World, tick: u64, want: &[[i32; 3]], what: &str) -> Placed {
        // The cells no support block may go into: B, M and O two high (O three), less the cells
        // wanted (a seal fills M).
        let keep: Vec<[i32; 3]> = self
            .pearls
            .burrow
            .map(|b| {
                let mut v = Vec::new();
                for c in [b.b, b.m, b.o] {
                    for y in 0..3 {
                        v.push([c[0], c[1] + y, c[2]]);
                    }
                }
                v.retain(|c| !want.contains(c) && !b.shell().contains(c));
                v
            })
            .unwrap_or_default();
        let keep_open = &keep[..];
        let Some(body) = w.body else {
            return Placed::Building;
        };
        let missing: Vec<[i32; 3]> = want
            .iter()
            .filter(|c| {
                w.block_at(world::BlockPos::new(c[0], c[1], c[2]))
                    .and_then(data::generated::shapes::collision)
                    != Some(data::types::Collision::Full)
            })
            .copied()
            .collect();
        if missing.is_empty() {
            return Placed::Done;
        }
        let Some((cell, click, face)) =
            cover::fill_block(w, &cells(&missing), body.pos, &cells(keep_open))
        else {
            return Placed::Cannot("nothing to click for a burrow block");
        };
        if !body.on_ground {
            return Placed::Building;
        }
        if what == "burrow" && self.pearls.site_blocks >= self.pearl_rules().build_max {
            return Placed::Cannot("burrow blocks over the cap");
        }
        let Some(item) = BURROW_BLOCKS
            .iter()
            .copied()
            .find(|k| craft::count(w, *k) > 0)
        else {
            return Placed::Cannot("no burrow block held");
        };
        if w.window.is_some() && w.click.is_none() && self.acts.is_empty() {
            self.acts.push(world::Act::CloseWindow);
            return Placed::Building;
        }
        if w.window.is_some()
            || w.click.is_some()
            || !self.acts.is_empty()
            || tick < self.pearls.place_at
        {
            return Placed::Building;
        }
        match hold::toward(w, item) {
            hold::Hold::Held => {
                self.pearls.place_at = tick + BUILD_EVERY;
                self.pearls.blocks += 1;
                if what == "burrow" {
                    self.pearls.site_blocks += 1;
                }
                let placed = [cell.0, cell.1, cell.2];
                if !self.pearls.placed.contains(&placed) {
                    self.pearls.placed.push(placed);
                }
                self.acts.push(world::Act::UseOn {
                    pos: world::BlockPos::new(click.0, click.1, click.2),
                    face,
                });
                let mended = if self.pearls.built && what == "burrow" {
                    "burrow mended at"
                } else {
                    "block at"
                };
                self.note(
                    tick,
                    format!("{what} {mended} {cell:?}"),
                    format!(
                        "{item:?} on {click:?} face {face}; {} left to place; {} in the run",
                        missing.len(),
                        self.pearls.blocks
                    ),
                );
                Placed::Building
            }
            hold::Hold::Act(a) => {
                self.acts.push(a);
                Placed::Building
            }
            hold::Hold::Wait => Placed::Building,
            hold::Hold::Missing => Placed::Cannot("no burrow block held"),
        }
    }

    /// A walk to a seen warped nylium cell with room over it, `FLOOR_MIN` to `SITE_RADIUS` off and
    /// at least `burrow_move` from every burrow moved from, not tried before: the forest's own
    /// floor, where the next site search starts. The nearest first, two tried a call.
    fn walk_to_forest_floor(
        &mut self,
        w: &World,
        tick: u64,
        feet: [i32; 3],
        r: &PearlRules,
    ) -> Option<Task> {
        let dim = w.dimension.as_ref()?.name.clone();
        let (cx, cz) = (feet[0] >> 4, feet[2] >> 4);
        // Nearest the endermen seen lately when there are any: spawns are where they are.
        let seen: Vec<[f64; 3]> = self.pearls.sightings.iter().map(|(_, p)| *p).collect();
        let toward = centroid(&seen);
        let mut cells: Vec<(i32, [i32; 3])> = Vec::new();
        for dx in -2..=2 {
            for dz in -2..=2 {
                for c in w
                    .memory
                    .indexed_in_column(&dim, cx + dx, cz + dz, data::BlockKind::WarpedNylium)
                    .unwrap_or_default()
                {
                    let at = [c.0, c.1 + 1, c.2];
                    let d2 = (at[0] - feet[0]).pow(2) + (at[2] - feet[2]).pow(2);
                    let far = self.pearls.moved_from.iter().all(|m| {
                        (m[0] - at[0]).pow(2) + (m[2] - at[2]).pow(2) >= r.burrow_move.pow(2)
                    });
                    if !near_delta(w, at, DELTA_AVOID)
                        && (FLOOR_MIN * FLOOR_MIN..=burrow::SITE_RADIUS * burrow::SITE_RADIUS)
                            .contains(&d2)
                        && far
                        && !self.pearls.floor_tried.contains(&at)
                        && w.block_at(world::BlockPos::new(at[0], at[1], at[2]))
                            .and_then(data::generated::shapes::collision)
                            == Some(data::types::Collision::Empty)
                    {
                        let key = match toward {
                            Some(t) => (at[0] - t.0).pow(2) + (at[2] - t.2).pow(2),
                            None => d2 + (at[1] - feet[1]).pow(2),
                        };
                        cells.push((key, at));
                    }
                }
            }
        }
        cells.sort();
        for (_, at) in cells.into_iter().take(2) {
            self.pearls.floor_tried.push(at);
            let c = (at[0], at[1], at[2]);
            let mut walk = Walk::new(c, Goal::Near { pos: c, range: 2.0 }, Arrive::Goal, tick);
            if walk.route_now(w, tick) {
                self.walks += 1;
                self.note(
                    tick,
                    format!("walk to the forest floor at {at:?}"),
                    "no burrow site here; a site is looked for from there".to_owned(),
                );
                return Some(Task::Explore(walk));
            }
        }
        None
    }

    /// The pillar out of a hole: with every side of the feet shut one up (no step out) and
    /// open sky over the head for two, jump, and at the top put a burrow block into the cell the
    /// feet left (clicking the floor's top). `None` when not in a hole.
    fn out_of_hole(&mut self, w: &World, tick: u64, feet: [i32; 3]) -> Option<Controls> {
        let body = w.body?;
        let at = |c: [i32; 3]| {
            w.block_at(world::BlockPos::new(c[0], c[1], c[2]))
                .and_then(data::generated::shapes::collision)
        };
        let open = |c: [i32; 3]| at(c) == Some(data::types::Collision::Empty);
        let start = match self.pearls.pillar {
            Some(p) => p,
            None => {
                let walled = [[1, 0], [-1, 0], [0, 1], [0, -1]]
                    .iter()
                    .all(|d| !open([feet[0] + d[0], feet[1] + 1, feet[2] + d[1]]));
                let sky =
                    open([feet[0], feet[1] + 2, feet[2]]) && open([feet[0], feet[1] + 3, feet[2]]);
                if !walled || !sky || !body.on_ground {
                    return None;
                }
                self.pearls.pillar = Some(feet);
                self.note(
                    tick,
                    "out of the hole".to_owned(),
                    format!("every side of the feet {feet:?} shut a block up; a pillar up"),
                );
                feet
            }
        };
        // Up a block from where it began, on the ground: done (the walks take it from here).
        if body.on_ground && feet[1] > start[1] {
            self.pearls.pillar = None;
            return None;
        }
        let Some(item) = BURROW_BLOCKS
            .iter()
            .copied()
            .find(|k| craft::count(w, *k) > 0)
        else {
            self.pearls.pillar = None;
            return None;
        };
        if let hold::Hold::Act(a) = hold::toward(w, item) {
            if self.acts.is_empty() {
                self.acts.push(a);
            }
            return Some(look(w.player.yaw.into(), EYES_DOWN));
        }
        // At the top of the jump: the block into the cell the feet left.
        if !body.on_ground
            && body.pos[1] >= f64::from(start[1]) + 1.0
            && open(start)
            && self.acts.is_empty()
            && w.click.is_none()
        {
            self.acts.push(world::Act::UseOn {
                pos: world::BlockPos::new(start[0], start[1] - 1, start[2]),
                face: 1,
            });
        }
        Some(Controls {
            jump: body.on_ground,
            pitch: Some(EYES_DOWN),
            yaw: w.player.yaw.into(),
            ..Controls::default()
        })
    }

    /// Down off a fungus cap: a walk to the floor seen below, else the cap block under the feet
    /// dug (`base::dig_down`: a landing within the safe fall, no lava).
    fn cap_down(&mut self, w: &World, tick: u64, feet: [i32; 3]) -> Option<Task> {
        self.pearls.cap_tries += 1;
        let table = self.table.clone()?;
        let targets = crate::base::floor_targets(w, &table.base, |c| self.refused_now(c, tick));
        for c in targets.into_iter().take(crate::CANDIDATES_PER_TICK) {
            let mut walk = Walk::new(c, Goal::Block { pos: c }, Arrive::Goal, tick);
            if walk.route_now(w, tick) {
                self.walks += 1;
                self.note(
                    tick,
                    "hunt: down off the fungus cap".to_owned(),
                    format!(
                        "walk from {feet:?} on {:?} to the floor at {c:?}; try {} of {CAP_TRIES}",
                        w.block_under_player(),
                        self.pearls.cap_tries
                    ),
                );
                return Some(Task::Explore(walk));
            }
            self.refuse(c, tick);
        }
        let under = crate::base::dig_down(w)?;
        self.note(
            tick,
            "hunt: down off the fungus cap".to_owned(),
            format!(
                "dig {under:?} under the feet {feet:?}: no floor with a way; try {} of {CAP_TRIES}",
                self.pearls.cap_tries
            ),
        );
        Some(Task::Dig(crate::dig::Dig::new(w, under, tick)))
    }

    /// The next block of the old shell to dig back (a cell placed by this hunt, full, within
    /// reach of the eye, not under the feet), with a wooden pickaxe; `None` and the reclaim over
    /// (noted with its count) when none is left or no wooden pickaxe is held.
    fn reclaim_step(&mut self, w: &World, tick: u64, feet: [i32; 3]) -> Option<Task> {
        let body = w.body?;
        let eye = [body.pos[0], body.pos[1] + physics::EYE_HEIGHT, body.pos[2]];
        // No dig with an angry enderman or another hostile within `RECLAIM_CLEAR` (the
        // orchestrator): the reclaim waits, its cells kept, and the rows answer the threat.
        let threat = angry_endermen(w).iter().any(|s| s.dist <= RECLAIM_CLEAR)
            || w.entities.values().any(|e| {
                !e.dead
                    && e.kind != EntityKind::Enderman
                    && crate::defend::hostile(e.kind)
                    && dist(e.pos, body.pos) <= RECLAIM_CLEAR
            });
        if threat {
            return None;
        }
        let full = |c: [i32; 3]| {
            w.block_at(world::BlockPos::new(c[0], c[1], c[2]))
                .and_then(data::generated::shapes::collision)
                == Some(data::types::Collision::Full)
        };
        let next = self
            .pearls
            .placed
            .iter()
            .chain(self.pearls.old_shell.iter())
            .copied()
            // Never the column the bot stands in (the block under or over it).
            .filter(|c| {
                (c[0], c[2]) != (feet[0], feet[2])
                    && !self.pearls.old_floors.contains(c)
                    && full(*c)
            })
            .map(|c| {
                (
                    dist(
                        eye,
                        [
                            f64::from(c[0]) + 0.5,
                            f64::from(c[1]) + 0.5,
                            f64::from(c[2]) + 0.5,
                        ],
                    ),
                    c,
                )
            })
            .filter(|(d, _)| *d <= RECLAIM_REACH)
            .min_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)))
            .map(|(_, c)| c);
        let pick = crate::base::spare_pick(w);
        let (Some(c), Some(_)) = (next, pick) else {
            self.pearls.reclaim = false;
            self.pearls.placed.retain(|c| full(*c));
            self.pearls.old_shell.clear();
            self.note(
                tick,
                format!("reclaimed {} blocks", self.pearls.reclaimed),
                format!(
                    "{}; {} burrow blocks held",
                    if pick.is_none() {
                        "no wooden pickaxe held"
                    } else {
                        "none left in reach"
                    },
                    burrow_filler(w)
                ),
            );
            return None;
        };
        self.pearls.reclaimed += 1;
        self.pearls.placed.retain(|p| *p != c);
        self.pearls.old_shell.retain(|p| *p != c);
        Some(Task::Dig(crate::dig::Dig::with_tool(
            w,
            (c[0], c[1], c[2]),
            tick,
            ItemKind::WoodenPickaxe,
        )))
    }

    /// A walk to dig exposed netherrack (or nylium, which drops netherrack to a pickaxe) near the
    /// feet, with a pickaxe above the reserve; `None` with none (noted once a `NOTE_EVERY`).
    fn restock_task(&mut self, w: &World, tick: u64, filler: i32) -> Option<Task> {
        let reserve = self
            .table
            .as_ref()
            .map_or(0, |t| t.survival.pickaxe_reserve);
        let uses = endermen::pickaxe_uses_left(w);
        if uses <= reserve && crate::base::spare_pick(w).is_none() {
            self.pearl_note(
                tick,
                "restock",
                "no restock: the pickaxe reserve".to_owned(),
                format!("{uses} pickaxe uses left, reserve {reserve}; {filler} filler"),
            );
            return None;
        }
        let kinds = [data::BlockKind::Netherrack, data::BlockKind::WarpedNylium];
        // The floor counts: a floor cell beside the feet, in reach where the bot stands, dug
        // straight (P4's seed 22 stood on a floor of exposed netherrack and found none "with a
        // way"): never the cell under the bot, never one over a hole deeper than 1 or by lava.
        // Never below the floor the restock began on: digs beside the feet, walked into for
        // their drops, carved a pit 9 deep (P4's seed 21, 0e9fa0f).
        let floor_y = *self
            .pearls
            .restock_floor
            .get_or_insert(w.body.map_or(0, |b| path::stand_cell(w, b.pos).1));
        if let Some(body) = w.body.filter(|b| path::stand_cell(w, b.pos).1 >= floor_y) {
            let f = path::stand_cell(w, body.pos);
            let under = (f.0, f.1 - 1, f.2);
            let at = |c: path::Cell| w.block_at(world::BlockPos::new(c.0, c.1, c.2));
            let solid = |c: path::Cell| {
                at(c).and_then(data::generated::shapes::collision)
                    == Some(data::types::Collision::Full)
            };
            let mut floor: Vec<(i32, path::Cell)> = Vec::new();
            for dx in -FLOOR_REACH..=FLOOR_REACH {
                for dz in -FLOOR_REACH..=FLOOR_REACH {
                    let c = (f.0 + dx, f.1 - 1, f.2 + dz);
                    if c == under
                        || !at(c).is_some_and(|k| kinds.contains(&k))
                        || self.refused_now(c, tick)
                        || !solid((c.0, c.1 - 1, c.2))
                    {
                        continue;
                    }
                    let lava = [(1, 0), (-1, 0), (0, 1), (0, -1)].iter().any(|(ax, az)| {
                        at((c.0 + ax, c.1, c.2 + az)) == Some(data::BlockKind::Lava)
                            || at((c.0 + ax, c.1 - 1, c.2 + az)) == Some(data::BlockKind::Lava)
                    });
                    if !lava {
                        floor.push((dx * dx + dz * dz, c));
                    }
                }
            }
            floor.sort();
            if let Some((_, c)) = floor.first().copied() {
                self.refuse(c, tick);
                self.note(
                    tick,
                    format!("restock: dig the floor at {c:?}"),
                    format!("{filler} filler held; beside the feet {f:?}"),
                );
                let dig = match crate::base::spare_pick(w) {
                    Some(_) => crate::dig::Dig::with_tool(w, c, tick, ItemKind::WoodenPickaxe),
                    None => crate::dig::Dig::new(w, c, tick),
                };
                return Some(Task::Dig(dig));
            }
        }
        let cands: Vec<path::Cell> = crate::exposed_near(w, &kinds)
            .into_iter()
            .filter(|c| c.1 >= floor_y - 1 && !self.refused_now(*c, tick))
            .take(3)
            .collect();
        for c in cands {
            let mut walk = Walk::to_dig(c, tick);
            if walk.route_now(w, tick) {
                self.walks += 1;
                self.note(
                    tick,
                    format!("restock: dig {c:?}"),
                    format!("{filler} filler held; {uses} pickaxe uses left (reserve {reserve})"),
                );
                return Some(Task::ToLog(walk));
            }
            self.refuse(c, tick);
        }
        self.pearl_note(
            tick,
            "restock",
            "no restock: no netherrack seen with a way".to_owned(),
            format!("{filler} filler held"),
        );
        None
    }

    /// A dig out of a pocket: with every side of the feet shut at the feet or the head, the shut
    /// cells on the side whose standing run beyond is longest (`cover::open_runs` from the cell
    /// beside), the feet's level first. `None` with a side open.
    fn unpocket(&mut self, w: &World, tick: u64, feet: [i32; 3]) -> Option<Task> {
        let open = |c: [i32; 3]| {
            w.block_at(world::BlockPos::new(c[0], c[1], c[2]))
                .and_then(data::generated::shapes::collision)
                == Some(data::types::Collision::Empty)
        };
        let sides = [[1, 0], [-1, 0], [0, 1], [0, -1]];
        let shut = |d: [i32; 2]| {
            let c = [feet[0] + d[0], feet[1], feet[2] + d[1]];
            !open(c) || !open([c[0], c[1] + 1, c[2]])
        };
        if !sides.iter().all(|d| shut(*d)) {
            return None;
        }
        let run = |d: [i32; 2]| {
            let c = (feet[0] + d[0], feet[1], feet[2] + d[1]);
            cover::open_runs(w, c)
                .iter()
                .find(|(dd, _)| *dd == (d[0], d[1]))
                .map_or(0, |(_, n)| *n)
        };
        let best = sides.into_iter().max_by_key(|d| (run(*d), d[0], d[1]))?;
        let c = [feet[0] + best[0], feet[1], feet[2] + best[1]];
        let cell = if open(c) { [c[0], c[1] + 1, c[2]] } else { c };
        self.note(
            tick,
            format!("out of the pocket: dig {cell:?}"),
            format!(
                "every side of the feet {feet:?} shut; toward ({}, {}), {} open beyond; {} pickaxe uses left",
                best[0],
                best[1],
                run(best),
                endermen::pickaxe_uses_left(w)
            ),
        );
        Some(Task::Dig(crate::dig::Dig::new(
            w,
            (cell[0], cell[1], cell[2]),
            tick,
        )))
    }

    /// The hunt's tasks, for a planner with no task in hand (agent P2's `EnderPhase::Hunt` calls
    /// this; so does `pearls_step`): the base (row 6), then the build row's walks and digs: a site
    /// found and walked to, the mouth reopened, a walk away after a move. `None` with nothing to
    /// begin (the standing rows act).
    pub fn pearls_task(&mut self, w: &World, tick: u64) -> Option<Task> {
        let body = w.body?;
        // Off a ledge first, before the base or any wait, with no burrow built: a fireball's
        // knock-back off a ledge into lava killed a no-heal bot at tick 140 (race 45548022681
        // lane 3, pearls-hunt wall 3).
        if !self.pearls.built
            && self.pearls.ledge_tries < LEDGE_TRIES
            && let Some(body) = w.body
        {
            let f = path::stand_cell(w, body.pos);
            if ledge(w, f, LEDGE_NEAR) {
                let mut cells = path::Cells::new(w);
                let mut safe: Vec<(i32, path::Cell)> = Vec::new();
                for dx in -LEDGE_SEARCH..=LEDGE_SEARCH {
                    for dz in -LEDGE_SEARCH..=LEDGE_SEARCH {
                        for dy in -2..=2 {
                            let c = (f.0 + dx, f.1 + dy, f.2 + dz);
                            if cells.standable(c) && !ledge(w, c, LEDGE_NEAR) {
                                safe.push((dx * dx + dy * dy + dz * dz, c));
                            }
                        }
                    }
                }
                safe.sort();
                for (_, c) in safe.into_iter().take(3) {
                    let mut walk = Walk::new(c, Goal::Block { pos: c }, Arrive::Goal, tick);
                    if walk.route_now(w, tick) {
                        self.pearls.ledge_tries += 1;
                        self.walks += 1;
                        self.note(
                            tick,
                            "off the ledge".to_owned(),
                            format!(
                                "the feet {f:?} within {LEDGE_NEAR} of a drop of 3 or more or lava below; to {c:?}"
                            ),
                        );
                        return Some(Task::Explore(walk));
                    }
                }
                self.pearls.ledge_tries += 1;
            }
        }
        // The hunt point first, before the first burrow (Bridger's ruling of 2026-11-24 allows
        // steering by known geography to win): P1's measured spot in the forest's main body
        // (race 45534565509: a ticking enderman within 32 in 60 of 61 rows, no cubes), by the far
        // walk (piglins shunned at 24, cubes at 8); 3 failed searches and the hunt stays here.
        if let Some([hx, hz]) = self.pearl_rules().hunt_point
            && self.pearls.burrow.is_none()
            && !self.pearls.at_hunt_point
        {
            let feet_c = path::stand_cell(w, body.pos);
            let d = f64::from((hx - feet_c.0).pow(2) + (hz - feet_c.2).pow(2)).sqrt();
            if d <= HUNT_POINT_NEAR
                || self.pearls.hunt_point_fails >= HUNT_POINT_TRIES
                || self.pearls.hunt_point_walks >= HUNT_POINT_WALKS
            {
                self.pearls.at_hunt_point = true;
                self.note(
                    tick,
                    "hunt point: here".to_owned(),
                    format!(
                        "{d:.0} from ({hx}, {hz}); {} walks, {} with no route",
                        self.pearls.hunt_point_walks, self.pearls.hunt_point_fails
                    ),
                );
            } else {
                let c = (hx, feet_c.1, hz);
                let goal = Goal::Near {
                    pos: c,
                    range: HUNT_POINT_NEAR,
                };
                let mut walk = Walk::far_to(c, goal, tick);
                if walk.route_now(w, tick) {
                    self.pearls.hunt_point_walks += 1;
                    self.walks += 1;
                    self.note(
                        tick,
                        "to the hunt point".to_owned(),
                        format!(
                            "({hx}, {hz}), {d:.0} off; walk {}",
                            self.pearls.hunt_point_walks
                        ),
                    );
                    return Some(Task::Explore(walk));
                }
                self.pearls.hunt_point_fails += 1;
                self.note(
                    tick,
                    "no way to the hunt point".to_owned(),
                    format!(
                        "({hx}, {hz}), {d:.0} off; try {} of {HUNT_POINT_TRIES}",
                        self.pearls.hunt_point_fails
                    ),
                );
                return None;
            }
        }
        // 6. base: its tasks, but not a wait of more than `base_wait_ticks` (the frontier race
        // 45542558792: all 8 bots waited 2,400 ticks a step for a stem with a way to it, and
        // every step after wood needs wood); then the hunt goes on and the base is asked again
        // after `base_retry_ticks`.
        let r = self.pearl_rules();
        if self.pearls.base_skip_until.is_none_or(|t| tick >= t) {
            match self.base_task(w, tick) {
                Some(Task::Idle) => {
                    let since = *self.pearls.base_wait_since.get_or_insert(tick);
                    if tick.saturating_sub(since) < r.base_wait_ticks {
                        return Some(Task::Idle);
                    }
                    self.pearls.base_wait_since = None;
                    self.pearls.base_skip_until = Some(tick + r.base_retry_ticks);
                    self.note(
                        tick,
                        "base waits too long: the hunt goes on".to_owned(),
                        format!(
                            "{} ticks with no base task; weapon uses {}, pickaxe uses {}; the base asked again in {}",
                            r.base_wait_ticks,
                            weapon_uses(w),
                            endermen::pickaxe_uses_left(w),
                            r.base_retry_ticks
                        ),
                    );
                }
                Some(t) => {
                    self.pearls.base_wait_since = None;
                    return Some(t);
                }
                None => {}
            }
        }
        let feet = arr(path::stand_cell(w, body.pos));
        // On a fungus cap: down to the forest floor first (P5's floor targets, else the cap block
        // under the feet dug, never a fall over 3): a cap is no site and no start of a walk on
        // (race 45548022681: four bots sat on caps at y 71).
        if self.pearls.burrow.is_none()
            && crate::base::on_cap(w)
            && self.pearls.cap_tries < CAP_TRIES
            && let Some(t) = self.cap_down(w, tick, feet)
        {
            return Some(t);
        }
        // A move's old shell dug back first, with a wooden pickaxe only (P5's spare_pick: never the
        // stone pickaxes' reserve), nearest first, never the block under the feet.
        if self.pearls.burrow.is_none()
            && self.pearls.reclaim
            && let Some(t) = self.reclaim_step(w, tick, feet)
        {
            return Some(t);
        }
        // Filler low (under `restock_below`, or a burrow block wanted and none held): netherrack
        // dug near with a pickaxe above the reserve, until `restock_to`.
        let filler = burrow_filler(w);
        if filler < r.restock_below {
            self.pearls.restock = true;
        }
        if filler >= r.restock_to {
            self.pearls.restock = false;
            self.pearls.restock_floor = None;
        }
        if self.pearls.restock {
            if let Some(t) = self.restock_task(w, tick, filler) {
                return Some(t);
            }
            if filler == 0 {
                return None;
            }
        }
        // Walled in where it stands (a pocket of its own from before the hunt): out by the side
        // whose floor beyond runs longest, one block at a time.
        if self.pearls.burrow.is_none()
            && let Some(t) = self.unpocket(w, tick, feet)
        {
            return Some(t);
        }
        if self.pearls.burrow.is_none() {
            if self.pearls.site_at.is_some_and(|t| tick < t + r.site_every) {
                return None;
            }
            self.pearls.site_at = Some(tick);
            // A refused site is never tried again (within 1); a moved-from one is left by
            // `burrow_move` (a new place for spawns). Searched from the feet where they are.
            let seen: Vec<[f64; 3]> = self.pearls.sightings.iter().map(|(_, p)| *p).collect();
            // A site moved from may be taken again after `MOVED_KEPT` (the frontier's one good
            // site made the kills; every move from it drifted to no site, race 45560431011).
            let recent_moves: Vec<[i32; 3]> = self
                .pearls
                .moved_from
                .iter()
                .zip(self.pearls.moved_at.iter().chain(std::iter::repeat(&0)))
                .filter(|(_, at)| tick < *at + MOVED_KEPT)
                .map(|(m, _)| *m)
                .collect();
            // From the feet and up to `ANCHORS` places within `SUPPLY_REACH` where endermen were
            // seen most (the orchestrator: the edge site was productive but by the deltas, and
            // its view saw few): each place's best site scored by the endermen seen within 40
            // of it, the open floor round O, less delta ground within 16; the best is taken.
            let mut anchors = vec![feet];
            let away = self.pearls.moved_from.last().map(|m| (*m, r.burrow_move));
            anchors.extend(supply_anchors(w, &seen, feet, away, r.hunt_point));
            let mut best: Option<(i64, Burrow)> = None;
            let mut why: Vec<(&'static str, u32)> = Vec::new();
            for a in anchors {
                let (f, y) = burrow::site_toward_skipping(
                    w,
                    &w.memory,
                    a,
                    &recent_moves,
                    r.burrow_move,
                    &self.pearls.refused,
                    &seen,
                );
                if why.is_empty() {
                    why = y;
                }
                if let Some(b) = f {
                    let score = supply_score(w, &seen, b);
                    // A far anchor's site counts only with a plain route to its B now: one
                    // refused for no route cost each search 100 ticks and the build came at
                    // tick 1,640 to 4,540, not 820 (race 45572123445).
                    let reachable = a == feet || {
                        let c = (b.b[0], b.b[1], b.b[2]);
                        let mut walk = Walk::new(c, Goal::Block { pos: c }, Arrive::Goal, tick);
                        walk.route_now(w, tick)
                    };
                    if reachable && best.is_none_or(|(sc, _)| score > sc) {
                        best = Some((score, b));
                    }
                }
            }
            let found = best.map(|(_, b)| b);
            let reasons = why
                .iter()
                .map(|(k, n)| format!("{n} {k}"))
                .collect::<Vec<_>>()
                .join(", ");
            let Some(b) = found else {
                if self
                    .pearls
                    .site_none_noted
                    .is_none_or(|t| tick >= t + NOTE_EVERY)
                {
                    self.pearls.site_none_noted = Some(tick);
                    self.note(
                        tick,
                        "no burrow site".to_owned(),
                        format!(
                            "none within {} of {feet:?}; refused: {reasons}",
                            burrow::SITE_RADIUS
                        ),
                    );
                }
                // Walk on to warped nylium seen (the forest's floor), never off into another biome
                // (race 45534591345 lane 0 walked away from its feet on a cap into the basalt
                // deltas, 71 blocks bridged, and died to a magma cube).
                // After a move: a far walk to the best place 48 or more from the old B (endermen
                // seen there lately, no delta ground, near the hunt point).
                if let Some(last) = self.pearls.moved_from.last().copied()
                    && let Some(a) =
                        supply_anchors(w, &seen, feet, Some((last, r.burrow_move)), r.hunt_point)
                            .into_iter()
                            // A place three walks failed to reach is dropped (race 45588764709
                            // lane 4 began the same far walk 15 times, stuck at its first cell).
                            .find(|a| {
                                self.pearls
                                    .ground_tries
                                    .iter()
                                    .all(|(g, n)| g != a || *n < NEW_GROUND_TRIES)
                            })
                {
                    match self.pearls.ground_tries.iter_mut().find(|(g, _)| *g == a) {
                        Some(e) => e.1 += 1,
                        None => self.pearls.ground_tries.push((a, 1)),
                    }
                    let c = (a[0], a[1], a[2]);
                    let mut walk = Walk::far_to(c, Goal::Near { pos: c, range: 6.0 }, tick);
                    if walk.route_now(w, tick) {
                        self.walks += 1;
                        self.note(
                            tick,
                            format!("to new ground at {a:?}"),
                            format!(
                                "{}+ from the old burrow {last:?}: new chunks, new spawns",
                                r.burrow_move
                            ),
                        );
                        return Some(Task::Explore(walk));
                    }
                }
                if let Some(t) = self.walk_to_forest_floor(w, tick, feet, &r) {
                    return Some(t);
                }
                // Toward the endermen seen lately (their centroid), else away from the last burrow.
                let from = self.pearls.moved_from.last().copied().unwrap_or(feet);
                // In one walk of 24 or more: toward their middle when it is farther, else away
                // from the feet (P4's seed 23 walked on about a block a 100 ticks for 6,000).
                let here = (feet[0], feet[1], feet[2]);
                let goal = match centroid(&seen) {
                    // At the feet's height: the middle of endermen seen on caps and ledges was at y 87
                    // over a floor at 46 (race 45551462309).
                    Some(c) if path::flat_distance(c, here) >= f64::from(WALK_ON) => Goal::Near {
                        pos: (c.0, feet[1], c.2),
                        range: 12.0,
                    },
                    _ if self.pearls.moved_from.is_empty() => Goal::Away {
                        pos: here,
                        range: f64::from(WALK_ON),
                    },
                    _ => Goal::Away {
                        pos: (from[0], from[1], from[2]),
                        range: f64::from(r.burrow_move + 8),
                    },
                };
                let target = match goal {
                    Goal::Near { pos, .. } => pos,
                    _ => (from[0], from[1], from[2]),
                };
                // A walk on that left the feet where they were (the frontier race 45548022681: a
                // bot on a fungus cap at y 71 "explored to" the endermen's middle 46 off at once,
                // a hundred times) escalates: the far walk, then the heading.
                let stalled = self.pearls.walk_on.is_some_and(|(at, _)| {
                    (0..3).all(|i| (at[i] - feet[i]).abs() <= WALK_ON_STILL)
                });
                let n = if stalled {
                    self.pearls.walk_on.map_or(0, |x| x.1) + 1
                } else {
                    0
                };
                self.pearls.walk_on = Some((feet, n));
                if n >= 2 {
                    self.note(
                        tick,
                        format!("walk on stalled {n} times: the heading"),
                        format!("the feet still at {feet:?}"),
                    );
                    if let Some(t) = self.explore_walk(w, tick, "the walk on for a site stalled") {
                        return Some(t);
                    }
                }
                // A plain route first: the far walk bridges (one bot laid 52 bridge cells toward
                // the endermen's middle in race 45548022681).
                let mut walk = Walk::new(target, goal, Arrive::Goal, tick);
                // The far walk shuns magma cubes at 8 (path::SHUN_MAGMA); a plain route does not:
                // with one within 24 the far walk goes first (race 45560431011 lane 2 died to cubes
                // on a walk on).
                let cubes_near = w.entities.values().any(|e| {
                    !e.dead
                        && e.kind == EntityKind::MagmaCube
                        && w.body.is_some_and(|b| dist(b.pos, e.pos) <= 24.0)
                });
                // After a move, the far walk only: it shuns piglins at 24 and cubes at 8 (race
                // 45572123445 lane 1 died to piglins on a walk after its move).
                let moved = !self.pearls.moved_from.is_empty();
                if n > 0 || cubes_near || moved || !walk.route_now(w, tick) {
                    walk = Walk::far_to(target, goal, tick);
                }
                if walk.route_now(w, tick) {
                    self.walks += 1;
                    self.note(
                        tick,
                        "walk on for a burrow site".to_owned(),
                        format!("toward {target:?} ({} endermen seen lately)", seen.len()),
                    );
                    return Some(Task::Explore(walk));
                }
                return self.explore_walk(w, tick, "no burrow site and no walk away");
            };
            // A site is taken only with a route to its B (the frontier race 45544744907: every
            // bot took a site 23 off with no walk to it, three times in 60 ticks, then the next,
            // and drifted 30 to 90 blocks off the forest).
            let mut first: Option<Walk> = None;
            if b.b != feet {
                let c = (b.b[0], b.b[1], b.b[2]);
                let mut walk = Walk::new(c, Goal::Block { pos: c }, Arrive::Goal, tick);
                if !walk.route_now(w, tick) {
                    walk = Walk::far_to(c, Goal::Block { pos: c }, tick);
                    if !walk.route_now(w, tick) {
                        self.pearls.refused.push(b.b);
                        self.pearls.site_at = None;
                        self.note(
                            tick,
                            format!("burrow site at {:?} refused: no route", b.b),
                            format!("from {feet:?}; {} sites refused", self.pearls.refused.len()),
                        );
                        return None;
                    }
                }
                first = Some(walk);
            }
            self.pearls.burrow = Some(b);
            self.pearls.built = false;
            self.pearls.walks = 0;
            self.pearls.site_blocks = 0;
            self.pearls.site_kills = 0;
            self.note(
                tick,
                format!(
                    "burrow site at {:?}, mouth toward ({}, {})",
                    b.b, b.dir[0], b.dir[1]
                ),
                format!(
                    "nearest the feet {feet:?} within {}; {} of {} endermen seen lately on the mouth's side; refused: {reasons}",
                    burrow::SITE_RADIUS,
                    burrow::side_score(b.b, b.dir, &seen),
                    seen.len()
                ),
            );
            if let Some(walk) = first {
                self.pearls.walks = 1;
                self.walks += 1;
                self.note(
                    tick,
                    format!("walk to the burrow at {:?}", b.b),
                    format!("walk 1 of {}", r.walk_tries),
                );
                return Some(Task::Explore(walk));
            }
        }
        let b = self.pearls.burrow?;
        if !b.inside(feet) {
            if self.pearls.walks >= r.walk_tries {
                self.refuse_site(tick, b, "no walk to B");
                return None;
            }
            self.pearls.walks += 1;
            let c = (b.b[0], b.b[1], b.b[2]);
            let mut walk = Walk::new(c, Goal::Block { pos: c }, Arrive::Goal, tick);
            if walk.route_now(w, tick) {
                self.walks += 1;
                self.note(
                    tick,
                    format!("walk to the burrow at {:?}", b.b),
                    format!("walk {} of {}", self.pearls.walks, r.walk_tries),
                );
                return Some(Task::Explore(walk));
            }
            self.note(
                tick,
                format!("no way to the burrow at {:?}", b.b),
                format!("try {} of {}", self.pearls.walks, r.walk_tries),
            );
            return None;
        }
        // The mouth closed after a breach: reopened, a pickaxe use a block.
        if self.pearls.sealed.is_none() && burrow::missing(w, &b).is_empty() {
            for c in b.mouth() {
                let shut = w
                    .block_at(world::BlockPos::new(c[0], c[1], c[2]))
                    .and_then(data::generated::shapes::collision)
                    .is_some_and(|k| k != data::types::Collision::Empty);
                if shut {
                    self.note(
                        tick,
                        format!("burrow reopened: dig {c:?}"),
                        format!(
                            "the seal is over; {} pickaxe uses left",
                            endermen::pickaxe_uses_left(w)
                        ),
                    );
                    return Some(Task::Dig(crate::dig::Dig::new(w, (c[0], c[1], c[2]), tick)));
                }
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn burrow_blocks_are_none_an_enderman_can_take() {
        let holdable = data::generated::tags::block_tag("enderman_holdable").unwrap_or(&[]);
        assert!(!holdable.is_empty(), "the tag is generated");
        for item in BURROW_BLOCKS {
            let block = data::BlockKind::from_name(item.name().unwrap_or_default());
            assert!(block.is_some_and(|b| !holdable.contains(&b)), "{item:?}");
        }
        assert!(burrow::holdable(data::BlockKind::WarpedNylium));
        assert!(burrow::holdable(data::BlockKind::Dirt));
    }

    #[test]
    fn a_step_ends_at_the_cells_middle() {
        let (c, there) = step_to([0.5, 64.0, 0.5], [1, 64, 0]);
        assert!(!there && c.forward);
        assert!((c.yaw - -90.0).abs() < 1e-6, "east is yaw -90: {}", c.yaw);
        let (_, there) = step_to([1.45, 64.0, 0.55], [1, 64, 0]);
        assert!(there);
    }
}
