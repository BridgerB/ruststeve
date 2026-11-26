//! Issue 19d, the way home (docs/problems/eyes-home.md): from 12 pearls in the Nether's warped
//! forest to the overworld's surface, fed, holding the eyes. Eyes do not fly in the Nether and no
//! portal can be cast there (water evaporates; obsidian needs a diamond pickaxe), so the way home
//! is the bot's own entry portal, about 475 blocks west through the basalt deltas, with its
//! overworld side deep by the lava sea.
//!
//! `EnderPhase::Home`, entered under the race with 12 pearls held, in order, each with its notes:
//! 1. **the top-up**: the hunt goes on to `pearls_wanted` (14) or `top_up_ticks` from Home's start
//!    (Bridger, 2026-11-25), whichever is first ("home: pearl top-up over");
//! 2. **the kit** (`base::HOME_STEPS`): every pearl an eye (Q3's `eyes` row, else `home-eyes`),
//!    stems to `home_logs`, a table placed with sticks, spare wooden pickaxes, sticks, a table to
//!    carry;
//! 3. **the trip**: the nearest nether portal on record in memory at any distance (`portal_far`,
//!    a ring over memory's index, never only the loaded chunks), else the config's geography
//!    (`[home] portal`); the heading's legs run to `[home] way`'s points, then the portal, with
//!    the travel's rules (piglins and cubes shunned, bridges, the tunnel's spare);
//! 4. **the approach**: within `approach` of the portal, the walk into it (`Task::Enter`), else a
//!    far walk beside it (bridging the lava), else a tunnel to it;
//! 5. **the overworld**: stairs up in rock to the open sky (never the caves: overworld-leg wall
//!    1), stone pickaxes made from the cobblestone on the way, then food to `fed` (hunt and eat),
//!    a hut at night, then a rest toward `heal_to`. Home is over fed on the surface; the eyes'
//!    phase (Q3, Q4) takes over (`Plan::home_done`).

use data::{BlockKind, ItemKind};
use world::World;

use crate::endermen::{self, EnderPhase};
use crate::heading::Signs;
use crate::path::{self, Cell, Goal};
use crate::walk::{Arrive, Walk};
use crate::{Aim, Plan, Task, base, craft, eat};

pub const NETHER: &str = "minecraft:the_nether";
pub const OVERWORLD: &str = "minecraft:overworld";

/// `[home]` in `config/goals.toml`: the way home's numbers, each with its source there. The
/// defaults are the config's (a test holds them equal).
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct HomeRules {
    /// Pearls the top-up hunts to in Home.
    pub pearls_wanted: i32,
    /// Ticks from Home's start the top-up may run.
    pub top_up_ticks: u64,
    /// Points (x, z) the heading runs to in turn before the portal: known geography.
    pub way: Vec<[i32; 2]>,
    /// The entry portal's cell by known geography, used only when memory holds no portal.
    pub portal: Option<[i32; 3]>,
    /// Known bridges the chain built, each `[x_east, x_west, feet_y, z]`: one cell wide along z,
    /// walked west from `x_east` to `x_west` at `feet_y` with no new bridging.
    #[serde(default)]
    pub bridges: Vec<[i32; 4]>,
    /// Chunk columns round the feet the far portal lookup reads memory's index in.
    pub portal_chunks: i32,
    /// Ticks between two far lookups.
    pub portal_every: u64,
    /// Blocks on the plane from the portal inside which the approach replaces the heading.
    pub approach: i32,
    /// Ticks between two notes of the trip's state.
    pub note_every: u64,
    /// The food bar Home ends at on the surface.
    pub fed: i32,
    /// Health rested toward once fed, for at most `heal_ticks`.
    pub heal_to: f32,
    pub heal_ticks: u64,
    /// Walks to look for animals on the surface before the look is over for a while.
    pub animal_walks: u32,
    /// Stone pickaxe uses (the stone and iron ones, summed) under which the climb makes more
    /// from the cobblestone it dug, at most `pick_runs` times.
    pub climb_pick_uses: i32,
    pub pick_runs: u32,
}

impl Default for HomeRules {
    fn default() -> Self {
        Self {
            pearls_wanted: 14,
            top_up_ticks: 6000,
            bridges: vec![[400, 311, 37, 194], [307, 201, 41, 194]],
            way: vec![
                [477, 152],
                [448, 152],
                [426, 150],
                [417, 159],
                [407, 173],
                [399, 194],
                [368, 194],
                [339, 194],
                [310, 194],
                [280, 194],
                [250, 194],
                [220, 194],
                [190, 195],
                [167, 208],
                [146, 208],
                [130, 206],
                [100, 205],
                [83, 188],
                [60, 175],
                [60, 140],
                [60, 110],
                [55, 100],
                [45, 95],
                [31, 95],
                [30, 76],
                [30, 62],
                [25, 56],
                [24, 41],
                [24, 22],
                [24, 1],
                [24, -11],
            ],
            portal: Some([22, 34, -20]),
            portal_chunks: 48,
            portal_every: 1200,
            approach: 24,
            note_every: 1200,
            fed: 18,
            heal_to: 18.0,
            heal_ticks: 2400,
            animal_walks: 12,
            climb_pick_uses: 30,
            pick_runs: 24,
        }
    }
}

/// The way home's state, one field of `EnderState` (`EnderState::home`).
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct HomeState {
    /// When Home began.
    #[serde(default)]
    pub since: Option<u64>,
    #[serde(default)]
    pub top_up_over: bool,
    #[serde(default)]
    pub kit_begun: bool,
    #[serde(default)]
    pub kit_over: bool,
    /// The portal aimed at, where it came from ("memory" or "geography"), and when it was read.
    #[serde(default)]
    pub portal: Option<Cell>,
    #[serde(default)]
    pub portal_from: Option<String>,
    #[serde(default)]
    pub portal_at: Option<u64>,
    #[serde(default)]
    pub trip_begun: bool,
    #[serde(default)]
    pub noted_at: Option<u64>,
    /// The approach's tries (each a walk, a far walk or a tunnel begun) and the enters begun.
    #[serde(default)]
    pub approach_tries: u32,
    #[serde(default)]
    pub enters: u32,
    /// Filler blocks placed into lava beside the portal's feet cells.
    #[serde(default)]
    pub seals: u32,
    /// Overworld: when the bot arrived, when it reached the surface, when it was fed.
    #[serde(default)]
    pub arrived_at: Option<u64>,
    #[serde(default)]
    pub climb_begun: bool,
    /// Where the climb began (the feet's cell): the rock rule's start.
    #[serde(default)]
    pub climb_from: Option<Cell>,
    #[serde(default)]
    pub surface_at: Option<u64>,
    #[serde(default)]
    pub fed_at: Option<u64>,
    #[serde(default)]
    pub pick_runs: u32,
    #[serde(default)]
    pub animal_walks: u32,
    /// The night a hut was begun for (the day count), so one hut a night.
    #[serde(default)]
    pub hut_night: Option<i64>,
    /// The night's cook begun (or found short of fuel) and its end noted.
    #[serde(default)]
    pub cook_noted: bool,
    #[serde(default)]
    pub cook_ended: bool,
    /// Walks to a log for the cook's fuel.
    #[serde(default)]
    pub fuel_walks: u32,
    /// The last cell stood on with filler to dig in reach, when it was last looked for, and the
    /// walks back to it.
    #[serde(default)]
    pub ground: Option<Cell>,
    #[serde(default)]
    pub ground_at: Option<u64>,
    #[serde(default)]
    pub ground_walks: u32,
    /// The bridge blocks laid on the trip (the last `LAID_KEPT`), and those dug back.
    #[serde(default)]
    pub laid: Vec<Cell>,
    #[serde(default)]
    pub unbridged: u32,
    /// Runs of stone pickaxes made on the trip (from blackstone).
    #[serde(default)]
    pub trip_pick_runs: u32,
    /// The way points' chunk columns reached, kept past any one heading.
    #[serde(default)]
    pub ways_passed: Vec<(i32, i32)>,
    /// A pocket from magma cubes in hand (since), the tick the last cube left `CUBE_CLEAR`, and
    /// the pockets made.
    #[serde(default)]
    pub cube_pocket: Option<u64>,
    #[serde(default)]
    pub cube_clear_since: Option<u64>,
    #[serde(default)]
    pub cube_pockets: u32,
    /// No cube pocket before this tick (over the danger line), and the pockets given up.
    #[serde(default)]
    pub cube_resume_at: Option<u64>,
    #[serde(default)]
    pub cube_breakouts: u32,
    /// Walks on the old bridges, step blocks placed, and the last note of a bridge that failed.
    #[serde(default)]
    pub bridge_walks: u32,
    #[serde(default)]
    pub steps_placed: u32,
    /// Blocks laid into gaps of an old bridge.
    #[serde(default)]
    pub gaps_laid: u32,
    #[serde(default)]
    pub bridge_noted_at: Option<u64>,
    /// Floor blocks laid beside the feet with a cube near.
    #[serde(default)]
    pub rails: u32,
    /// The last tick a blaze or ghast had the bot in sight on the trip.
    #[serde(default)]
    pub shooter_seen_at: Option<u64>,
    /// Pockets not sealed for a ledge.
    #[serde(default)]
    pub ledge_skips: u32,
    /// The tunnel out of a cube pocket in hand (its start and end), and the tunnels dug.
    #[serde(default)]
    pub cube_tunnel: Option<(Cell, Cell)>,
    #[serde(default)]
    pub cube_tunnels: u32,
    /// The point the trip aims at now (a way point, else the portal).
    #[serde(default)]
    pub aim: Option<(i32, i32)>,
    #[serde(default)]
    pub done: bool,
    /// Large and medium cubes left unstruck by a lava edge (counted), and the last note.
    #[serde(default)]
    pub big_cubes_spared: u32,
    #[serde(default)]
    pub big_cube_noted_at: Option<u64>,
    /// The old bridges' spans whose west end was reached with no span beyond: walked out, never
    /// walked back to (race 45753006452 lane 0 paced 4 blocks between the west span's end and a
    /// restock walk east for 2,500 ticks).
    #[serde(default)]
    pub spans_out: Vec<usize>,
    /// Filler a restock dug for and did not get (no cell where the leg began): run at the next
    /// place with cells to dig (`heading_with`), and the owed restocks begun.
    #[serde(default)]
    pub filler_owed: Option<i64>,
    #[serde(default)]
    pub owed_restocks: u32,
    /// The trip's axis and the last feet it was read from (`home_axis`), and guard blocks laid.
    #[serde(default)]
    pub axis: Option<(i32, i32)>,
    #[serde(default)]
    pub last_feet: Option<Cell>,
    #[serde(default)]
    pub guards: u32,
    /// The spans' guard restock done, and whether it has begun.
    #[serde(default)]
    pub guard_restock_done: bool,
    #[serde(default)]
    pub span_restocked: bool,
    /// Cube pockets not made, or left, by a lava edge over the danger line.
    #[serde(default)]
    pub edge_pocket_skips: u32,
    /// The guard cell clicked last and its clicks, and the guard cells given up.
    #[serde(default)]
    pub guard_try: Option<(Cell, u32)>,
    #[serde(default)]
    pub guard_refused: Vec<Cell>,
    /// Blocks laid or dug to step up onto a span from below.
    #[serde(default)]
    pub step_ups: u32,
    /// Routes back over the heading's own cut taken, the only way toward the point.
    #[serde(default)]
    pub cut_routes_taken: u32,
    /// The provoked enderman held from and since when, and those left after `ENDER_HOLD_MAX`.
    #[serde(default)]
    pub ender_hold: Option<(i32, u64)>,
    #[serde(default)]
    pub enders_left: Vec<i32>,
    /// Endermen fought from the pocket at the cap, and the last one noted still near.
    #[serde(default)]
    pub ender_fights: u32,
    /// Covered tunnels begun under the danger line on the trip.
    #[serde(default)]
    pub danger_tunnels: u32,
    #[serde(default)]
    pub ender_near_noted: Option<i32>,
    /// Walks for wood by hand, and whether a gather to `WOOD_GATHER` is in hand.
    #[serde(default)]
    pub wood_walks: u32,
    #[serde(default)]
    pub wood_gathering: bool,
    /// Walks and tunnels cut for cover when a shooter first saw the bot.
    #[serde(default)]
    pub shooter_cuts: u32,
    /// Walks out of the portal's own cell on arrival.
    #[serde(default)]
    pub portal_exits: u32,
    /// Wooden pickaxe runs on the overworld's surface.
    #[serde(default)]
    pub surface_pick_runs: u32,
    // >>> Q6 (issue 19): the last leg's state (`crate::portal_way`).
    #[serde(default)]
    pub way_in: crate::portal_way::WayIn,
    // <<< Q6
    // >>> Q11 (issue 19): the fortress stretch's state (`crate::fortress_way`).
    #[serde(default)]
    pub fortress: crate::fortress_way::FortressState,
    // <<< Q11
    // >>> Q7: the overworld arrival's state (the shaft, the climb's table).
    #[serde(default)]
    pub arrival: crate::arrival::Arrival,
    // <<< Q7
}

/// Lava cells sealed beside the portal at most: two rows of five in front of it and its sides
/// (the row (21 to 25, 34, -19) was lava by tick 25,700: docs/problems/nether-walk.md), with room.
pub const SEALS: u32 = 24;
/// Walks to a log for the cook's fuel at most (the coordinator, 2026-11-25: the overland route is
/// forest and plains, logs are near), and how far a log seen may be (32, the coordinator's).
pub const FUEL_WALKS: u32 = 6;
pub const FUEL_LOG_REACH: f64 = 32.0;
/// Food under which raw meat is eaten as it is rather than cooked first by day: 6 is where
/// sprinting stops (vanilla).
pub const COOK_FOOD_FLOOR: i32 = 6;
/// Magma cubes on the trip (the coordinator, 2026-11-25; race 45698618366 lane 3 took five hits
/// in 140 ticks): one within `CUBE_FIGHT` is fought over the danger line; `CUBE_CROWD` within
/// `CUBE_NEAR` (or any within `CUBE_CLEAR` under the line) seals a pocket, held until none is
/// within `CUBE_CLEAR` for `CUBE_CLEAR_TICKS`. The legs shun them at `path::SHUN_MAGMA` 8.
pub const CUBE_FIGHT: f64 = 3.0;
/// The floor cell beside the feet to lay (and the face of the block stood on to click), with a
/// mob that knocks back near: a cube within `CUBE_CLEAR`, or a `KNOCKERS` mob within
/// `KNOCK_RANGE` with the bot in sight (race 45743712658: lane 1 knocked into lava by a cube at
/// tick 4,220, lane 3 by a blaze at 3,940, both on the one-wide span). `None` with none near,
/// no solid block stood on, or no open side.
pub fn rail_wanted(w: &World) -> Option<(Cell, i8, bool, bool)> {
    let body = w.body?;
    let cube = w.entities.values().any(|e| {
        !e.dead && e.kind == data::EntityKind::MagmaCube && dist(e.pos, body.pos) <= CUBE_CLEAR
    });
    let eyes: Vec<[f64; 3]> = w
        .entities
        .values()
        .filter(|e| !e.dead && KNOCKERS.contains(&e.kind) && dist(e.pos, body.pos) <= KNOCK_RANGE)
        .map(|e| [e.pos[0], e.pos[1] + 1.2, e.pos[2]])
        .collect();
    let knocker = !eyes.is_empty() && crate::cover::seen_body(w, &eyes, body.pos);
    if !cube && !knocker {
        return None;
    }
    let feet = path::feet_cell(body.pos);
    let under = (feet.0, feet.1 - 1, feet.2);
    let mut cells = path::Cells::new(w);
    if cells.at(under) != path::Class::Solid {
        return None;
    }
    // Only on a side that drops to lava within RAIL_DROP (a fall onto ground is a block or two,
    // not a death), and down to RAIL_FLOOR of filler, under the trip's keep: life over the
    // trip (the coordinator, 2026-11-26; race 45749508716 lane 3 held 10 under the keep of 24,
    // laid none, and was knocked off the west span into lava by a split cube).
    if path::bridge_blocks(w) <= RAIL_FLOOR {
        return None;
    }
    [((1, 0), 5i8), ((-1, 0), 4), ((0, 1), 3), ((0, -1), 2)]
        .into_iter()
        .map(|((dx, dz), f)| ((feet.0 + dx, feet.1 - 1, feet.2 + dz), f))
        .find(|(c, _)| {
            matches!(cells.at(*c), path::Class::Lava | path::Class::Clear)
                && ((0..=RAIL_DROP).any(|d| cells.at((c.0, c.1 - d, c.2)) == path::Class::Lava)
                    // >>> E3 (issue 20e): in the End a drop to the void wants the rail too.
                    || (crate::end::in_end(w) && crate::end::void_column(w, *c)))
            // <<< E3
        })
        .map(|(c, f)| (c, f, cube, knocker))
}

/// Mobs whose hit or blast knocks the bot back a block: with one within `KNOCK_RANGE` in sight,
/// the floor beside the feet is laid before the next step (`home_rail`).
pub const KNOCKERS: &[data::EntityKind] = &[
    data::EntityKind::MagmaCube,
    data::EntityKind::Blaze,
    data::EntityKind::Ghast,
    data::EntityKind::Skeleton,
    data::EntityKind::Piglin,
    data::EntityKind::ZombifiedPiglin,
];
pub const KNOCK_RANGE: f64 = 16.0;

/// A `KNOCKERS` mob within `KNOCK_RANGE`, seen or not.
pub fn knocker_near(w: &World) -> bool {
    w.body.is_some_and(|b| {
        w.entities
            .values()
            .any(|e| !e.dead && KNOCKERS.contains(&e.kind) && dist(e.pos, b.pos) <= KNOCK_RANGE)
    })
}
/// A rail is laid only on a side that drops to lava within this.
pub const RAIL_DROP: i32 = 8;
/// Rails are laid while more filler than this is held (Bridger, 2026-11-26: "do the rails
/// always": down to 0).
pub const RAIL_FLOOR: i64 = 0;

/// A magma cube's size: its own metadata index 16 (`Slime.ID_SIZE`, an int: 1 small, 2 medium,
/// 4 large), sent only when not the default 1.
pub fn cube_size(e: &world::Entity) -> i32 {
    match e.metadata.get(&16) {
        Some(protocol::metadata::MetaValue::Int(s)) => *s,
        Some(_) | None => 1,
    }
}

/// The span's guard restock digs at most this over the keep (the coordinator: 2 stacks).
pub const GUARD_EXTRA: i64 = 128;
/// Clicks for one guard block before it is given up (`MAX_PLACE_TRIES` of the walk's bridge is 8;
/// a guard clicked every `SHELTER_EVERY_TICKS` 10 is answered within a tick or two).
pub const GUARD_TRIES: u32 = 6;
/// Guard cells given up, kept (the last).
pub const GUARD_REFUSED_KEPT: usize = 32;
/// Blocks laid or dug to step up onto a span from below, a trip.
pub const STEP_UPS: u32 = 48;
/// On the way home a pocket from one provoked enderman is held at most this long (a minute, as
/// the cube pocket's `CUBE_HOLD_MAX`), then it is left.
pub const ENDER_HOLD_MAX: u64 = 1200;
/// At the cap an enderman this near is not left: the hold goes on; this near, it is fought from the
/// pocket over the danger line (Q9's race 45776886803: released at 2 off, dead in 60 ticks).
pub const ENDER_RELEASE: f64 = 16.0;
pub const ENDER_FIGHT: f64 = 3.0;
/// Under the danger line a covered tunnel goes this far toward the point aimed at, a leg at a time.
pub const DANGER_LEG: i32 = 24;
/// Basalt kept on the trip when the pack fills (the guard's and the bridges' filler with the
/// netherrack's 128; the deltas' restock digs it).
pub const BASALT_KEPT: i32 = 64;
/// Netherrack kept on the trip (three stacks: the spans' guard restock wants up to 152; Q10's race
/// 45777976793 dropped 36 at 128 and walked back over them four times).
pub const NETHERRACK_KEPT: i32 = 192;
/// Wood by hand (`Plan::home_wood`; Q10's race 45768707240 lane 2 ran out of pickaxes 17 off the
/// portal): under this many pickaxe uses, logs or stems within `WOOD_REACH` are gathered by hand
/// to `WOOD_GATHER`, at most `WOOD_WALKS` walks a trip; within `WOOD_LEG_FROM` of the portal the
/// last leg's budget (`WOOD_USES_A_BLOCK`, Q6's 2.5) is wanted. A wooden pickaxe is 59 uses.
pub const WOOD_SHORT_USES: i32 = 60;
pub const WOOD_REACH: i32 = 24;
pub const WOOD_GATHER: i32 = 8;
pub const WOOD_WALKS: u32 = 48;
pub const WOOD_LEG_FROM: i32 = 160;
pub const WOOD_USES_A_BLOCK: f64 = 2.5;
pub const WOOD_PICK_USES: i32 = 59;
/// A stem with no walk to it is tunnelled to with at least this many pickaxe uses left.
pub const WOOD_TUNNEL_USES: i32 = 10;

/// The next block of the guard (`Plan::home_guard`): for each side across `axis`, an open
/// feet-level cell (no floor under it, or only a rail over a drop) is guarded. Its block is clicked onto a
/// guard block beside it along the axis, else onto the floor-level block under it (the rail),
/// else that rail is laid first, clicked onto the side of the block stood on. `(cell to fill,
/// block clicked, face)`; `None` with nothing to guard, no axis, no solid block stood on, or
/// no filler (Bridger: down to 0).
pub fn guard_wanted(w: &World, axis: Option<(i32, i32)>) -> Option<(Cell, Cell, i8)> {
    guard_next(w, axis, &[])
}

/// `guard_wanted`, skipping the cells in `refused` (guards given up after `GUARD_TRIES` clicks).
pub fn guard_next(
    w: &World,
    axis: Option<(i32, i32)>,
    refused: &[Cell],
) -> Option<(Cell, Cell, i8)> {
    let (ax, az) = axis?;
    let body = w.body?;
    // Down to `HOME_KEEP` with no knocker near, kept for the rails that a knocker calls for (race
    // 45783532164 lane 0 guarded 41 cells of the west span to filler 0, then a cube came and no
    // rail could be laid); down to 0 with one near.
    let floor = if rail_wanted(w).is_some() || knocker_near(w) {
        0
    } else {
        crate::heading::HOME_KEEP
    };
    if path::bridge_blocks(w) <= floor {
        return None;
    }
    let feet = path::stand_cell(w, body.pos);
    let under = (feet.0, feet.1 - 1, feet.2);
    let mut cells = path::Cells::new(w);
    if cells.at(under) != path::Class::Solid {
        return None;
    }
    let face_to = |(dx, dz): (i32, i32)| -> i8 {
        match (dx, dz) {
            (1, _) => 5,
            (-1, _) => 4,
            (_, 1) => 3,
            _ => 2,
        }
    };
    let mut side = |(sx, sz): (i32, i32)| -> Option<(Cell, Cell, i8)> {
        let t = (feet.0 + sx, feet.1, feet.2 + sz);
        // Open: nothing at the feet's level and no floor under it, or only a floor-level rail
        // over a drop (Bridger, 2026-11-26: "do the rails always": any open side, not only
        // lava under it).
        if cells.at(t) != path::Class::Clear
            || (1..=2).all(|d| cells.at((t.0, t.1 - d, t.2)) == path::Class::Solid)
        {
            return None;
        }
        let rail = (t.0, t.1 - 1, t.2);
        // >>> Q11: never a block in the body's own box (race 45760537530: the server refused the
        // same guard 300 times while blazes burned the bot); the rail under it still, if missing.
        if crate::fortress_way::body_in_cell(w, t) {
            return (cells.at(rail) != path::Class::Solid
                && !crate::fortress_way::body_in_cell(w, rail))
            .then_some((rail, under, face_to((sx, sz))));
        }
        // <<< Q11
        // A guard block beside it along the axis: clicked on the face toward it.
        for (dx, dz) in [(ax, az), (-ax, -az)] {
            let n = (t.0 + dx, t.1, t.2 + dz);
            if cells.at(n) == path::Class::Solid {
                return Some((t, n, face_to((-dx, -dz))));
            }
        }
        if cells.at(rail) == path::Class::Solid {
            return Some((t, rail, 1));
        }
        Some((rail, under, face_to((sx, sz))))
    };
    [(az, ax), (-az, -ax)]
        .into_iter()
        .find_map(|s| side(s).filter(|g| !refused.contains(&g.0)))
}

/// A lava edge at the feet or at a cell beside them on the same level: a span, its join, a
/// bridge over the sea, or a rail laid beside one (a cube's knock-back lands there).
pub fn near_lava_edge(w: &World) -> bool {
    w.body.is_some_and(|b| {
        let feet = path::stand_cell(w, b.pos);
        let mut cells = path::Cells::new(w);
        [(0, 0), (1, 0), (-1, 0), (0, 1), (0, -1)]
            .iter()
            .any(|(dx, dz)| crate::walk::lava_edge(&mut cells, (feet.0 + dx, feet.1, feet.2 + dz)))
    })
}

/// The feet stand at a lava edge (`walk::lava_edge`): a span, its join, a bridge over the sea.
pub fn at_lava_edge(w: &World) -> bool {
    w.body.is_some_and(|b| {
        let mut cells = path::Cells::new(w);
        let feet = path::stand_cell(w, b.pos);
        crate::walk::lava_edge(&mut cells, feet)
            // >>> E3 (issue 20e): idle at a void edge in the End, sneak as at lava.
            || (crate::end::in_end(w) && crate::end::void_edge(w, feet))
        // <<< E3
    })
}

/// A skeleton within this has the trip take cover as from a blaze (its bow's 15 and margin).
pub const SKELETON_COVER: f64 = 24.0;

/// A skeleton within `SKELETON_COVER` with the bot in sight (Q6's race 45727347488: eight
/// arrows, 19.8 to 8.6, in the soul sand valley south of the portal).
pub fn skeleton_sees(w: &World) -> bool {
    let Some(body) = w.body else {
        return false;
    };
    let eyes: Vec<[f64; 3]> = w
        .entities
        .values()
        .filter(|e| !e.dead && e.kind == data::EntityKind::Skeleton)
        .filter(|e| dist(e.pos, body.pos) <= SKELETON_COVER)
        .map(|e| [e.pos[0], e.pos[1] + 1.74, e.pos[2]])
        .collect();
    !eyes.is_empty() && crate::cover::seen_body(w, &eyes, body.pos)
}

/// A ghast shoots from farther than the cover rule's 48 and from outside the ticking chunks
/// (race 45742350492 lane 0: two fireballs from 46 and 56, 19.8 to 15.6, no cover): on the way
/// home one within `GHAST_COVER` with the bot in sight, ticking or not, sends it to cover.
pub const GHAST_COVER: f64 = 72.0;

pub fn ghast_sees(w: &World) -> bool {
    let Some(body) = w.body else {
        return false;
    };
    let eyes: Vec<[f64; 3]> = w
        .entities
        .values()
        .filter(|e| !e.dead && e.kind == data::EntityKind::Ghast)
        .filter(|e| dist(e.pos, body.pos) <= GHAST_COVER)
        .map(|e| [e.pos[0], e.pos[1] + 2.0, e.pos[2]])
        .collect();
    !eyes.is_empty() && crate::cover::seen_body(w, &eyes, body.pos)
}

/// Air under the feet, or under a cell beside them: a ledge, where no pocket is sealed.
pub fn on_ledge(w: &World) -> bool {
    let Some(body) = w.body else {
        return false;
    };
    let (x, y, z) = path::feet_cell(body.pos);
    let air = |c: Cell| {
        matches!(
            w.block_at(world::BlockPos::new(c.0, c.1, c.2)),
            Some(BlockKind::Air | BlockKind::CaveAir)
        )
    };
    air((x, y - 1, z))
        || [(1, 0), (-1, 0), (0, 1), (0, -1)]
            .iter()
            .any(|(dx, dz)| air((x + dx, y, z + dz)) && air((x + dx, y - 1, z + dz)))
}

/// A blaze or ghast out of sight this long ends the trip's cover (ten seconds).
pub const SHOOTER_CLEAR_TICKS: u64 = 200;
/// The trip's wooden pickaxes from the kit's stems: a table placed, then wooden pickaxes to the
/// base's wood uses (150).
pub const TRIP_WOOD_STEPS: [base::Step; 2] = [base::Step::Table, base::Step::WoodPicks];

fn picking_now(p: &Plan) -> bool {
    (p.base.steps == base::CLIMB_STEPS.to_vec() || p.base.steps == TRIP_WOOD_STEPS.to_vec())
        && !p.base.over
}

/// Whether `c` is a cell of the span `[x_east, x_west, feet_y, z]` or within 1 of its line at
/// its height (its floor and the feet cells over it, one each side).
pub fn near_span([e, wst, y, z]: [i32; 4], c: Cell) -> bool {
    // West of the span by up to JOIN_REACH (the join to the next span is built there) and down to
    // 4 under its feet (race 45749508716 lane 2 dug its own step at (309, 34..35, 194)).
    (c.2 - z).abs() <= 1 && c.0 <= e + 1 && c.0 >= wst - JOIN_REACH && c.1 >= y - 5 && c.1 <= y + 4
}

/// West of a span, the join to the next is built this far out: never a restock cell.
pub const JOIN_REACH: i32 = 6;

/// A walk on an old bridge goes this far at a time.
pub const BRIDGE_LEG: i32 = 24;
/// A known bridge this far below (race 45738826162: 5 under the west span; race 45768618919: 7) is
/// joined from below.
pub const BRIDGE_BELOW: i32 = 10;
/// A known bridge this near on the plane (across it) is walked onto first.
pub const BRIDGE_NEAR: i32 = 8;

/// Bridge blocks the trip keeps on record (the last laid), to dig back when stranded.
pub const LAID_KEPT: usize = 64;

/// A cell as `{:?}` prints it, "(x, y, z)".
pub fn parse_cell(s: &str) -> Option<Cell> {
    let mut it = s
        .trim()
        .strip_prefix('(')?
        .strip_suffix(')')?
        .split(',')
        .map(|p| p.trim().parse::<i32>());
    let c = (it.next()?.ok()?, it.next()?.ok()?, it.next()?.ok()?);
    it.next().is_none().then_some(c)
}

/// How often the trip notes the last ground with filler to dig (a second).
pub const GROUND_EVERY: u64 = 20;
pub const CUBE_NEAR: f64 = 6.0;
pub const CUBE_CROWD: usize = 2;
pub const CUBE_CLEAR: f64 = 8.0;
pub const CUBE_CLEAR_TICKS: u64 = 200;
/// A cube pocket over the danger line is held at most this long (a minute), then the trip goes on
/// with no pocket for `CUBE_RESUME_TICKS` (cubes wait outside a sealed pocket for good).
pub const CUBE_HOLD_MAX: u64 = 1200;
pub const CUBE_RESUME_TICKS: u64 = 1200;
/// The tunnel out of a cube pocket: this long, ended once `CUBE_TUNNEL_OUT` along with no cube
/// within `CUBE_CLEAR` (a cube loses the bot past 16 or so).
pub const CUBE_TUNNEL_LEN: i32 = 20;
pub const CUBE_TUNNEL_OUT: i32 = 12;

/// The tunnel out of a cube pocket's axis: toward `aim` (the longer axis first), never toward the
/// side the cubes are on (one more than a block that way); with every side taken, toward `aim`.
pub fn cube_tunnel_dir(feet: Cell, aim: (i32, i32), cubes: &[[f64; 3]]) -> (i32, i32) {
    let (dx, dz) = (aim.0 - feet.0, aim.1 - feet.2);
    let x = (if dx < 0 { -1 } else { 1 }, 0);
    let z = (0, if dz < 0 { -1 } else { 1 });
    let order = if dx.abs() >= dz.abs() {
        [x, z, (0, -z.1), (-x.0, 0)]
    } else {
        [z, x, (-x.0, 0), (0, -z.1)]
    };
    let cubes_on = |d: (i32, i32)| {
        cubes.iter().any(|c| {
            (c[0] - (f64::from(feet.0) + 0.5)) * f64::from(d.0)
                + (c[2] - (f64::from(feet.2) + 0.5)) * f64::from(d.1)
                > 1.0
        })
    };
    order
        .into_iter()
        .find(|d| !cubes_on(*d))
        .unwrap_or(order[0])
}

/// The distance between two positions.
pub fn dist(a: [f64; 3], b: [f64; 3]) -> f64 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
}

/// A seal is placed only this near the eye (`walk::REACH` 4 plus half a block for the far corner).
pub const SEAL_REACH: f64 = 4.5;

/// The nearest nether portal block on record in `dim` within `chunks` chunk columns of `at` (a
/// ring over memory's index of sightings, so a portal seen hundreds of blocks back counts; never a
/// block not seen), lowered to the bottom of its column of portal blocks (where the feet go).
pub fn portal_far(w: &World, dim: &str, at: [f64; 3], chunks: i32) -> Option<Cell> {
    let (cx, cz) = ((at[0].floor() as i32) >> 4, (at[2].floor() as i32) >> 4);
    let mut best: Option<(f64, Cell)> = None;
    for dx in -chunks..=chunks {
        for dz in -chunks..=chunks {
            let Some(cells) =
                w.memory
                    .indexed_in_column(dim, cx + dx, cz + dz, BlockKind::NetherPortal)
            else {
                continue;
            };
            for c in cells {
                let d = (0..3)
                    .map(|i| (f64::from([c.0, c.1, c.2][i]) + 0.5 - at[i]).powi(2))
                    .sum::<f64>()
                    .sqrt();
                if best.is_none_or(|(b, bc)| d < b || (d == b && c < bc)) {
                    best = Some((d, c));
                }
            }
        }
    }
    let (_, mut c) = best?;
    let below = |c: Cell| {
        w.memory
            .latest(dim, (c.0, c.1 - 1, c.2))
            .and_then(|s| data::block_of_state(s.state))
            == Some(BlockKind::NetherPortal)
    };
    while below(c) {
        c.1 -= 1;
    }
    Some(c)
}

/// The portal blocks on record within 3 of `c` (its other cells), the lowest first, then nearest.
pub fn portal_cells(w: &World, dim: &str, c: Cell) -> Vec<Cell> {
    let mut v: Vec<Cell> = w
        .memory
        .indexed_in_column(dim, c.0 >> 4, c.2 >> 4, BlockKind::NetherPortal)
        .unwrap_or_default()
        .into_iter()
        .chain(
            [(-1, 0), (1, 0), (0, -1), (0, 1)]
                .iter()
                .filter_map(|(dx, dz)| {
                    w.memory.indexed_in_column(
                        dim,
                        (c.0 >> 4) + dx,
                        (c.2 >> 4) + dz,
                        BlockKind::NetherPortal,
                    )
                })
                .flatten(),
        )
        .filter(|p| (p.0 - c.0).abs() <= 3 && (p.1 - c.1).abs() <= 3 && (p.2 - c.2).abs() <= 3)
        .collect();
    v.sort_by_key(|p| (p.1, (p.0 - c.0).abs() + (p.2 - c.2).abs(), *p));
    v.dedup();
    v
}

/// The face a portal is entered from: `stand`, an open cell with a full block under it in front
/// of one of its two lowest rows; `tunnel`, the cell one further out (dug to when it is rock);
/// `normal`, the step from the portal's plane out to that face.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Front {
    pub stand: Cell,
    pub tunnel: Cell,
    pub normal: (i32, i32),
}

/// The face of `lit` (the portal's lit cells) with a standing cell, the nearer to `at` when both
/// have one; `None` when neither has. The entry portal's north face (z -21) has no floor over the
/// lava sea and its south face one dry standing cell, (22, 35, -19), with netherrack at
/// (22, 35, -18) behind it (Q1's probe 45688112814): its front is the south.
pub fn portal_front(w: &World, lit: &[Cell], at: [f64; 3]) -> Option<Front> {
    let low = lit.iter().map(|c| c.1).min()?;
    let first = *lit.first()?;
    // The plane runs along x when its cells differ in x, or when the low cell's x neighbours are
    // its frame (obsidian) or portal; its faces are then the two z sides.
    let along_x = lit.iter().any(|c| c.0 != first.0)
        || (lit.iter().all(|c| c.2 == first.2)
            && [first.0 - 1, first.0 + 1].iter().any(|x| {
                matches!(
                    w.block_at(world::BlockPos::new(*x, first.1, first.2)),
                    Some(BlockKind::Obsidian | BlockKind::NetherPortal)
                )
            }));
    let normals = if along_x {
        [(0, 1), (0, -1)]
    } else {
        [(1, 0), (-1, 0)]
    };
    let mut cells = path::Cells::new(w);
    let d2 = |c: Cell| {
        let d = [
            f64::from(c.0) + 0.5 - at[0],
            f64::from(c.1) - at[1],
            f64::from(c.2) + 0.5 - at[2],
        ];
        d[0] * d[0] + d[1] * d[1] + d[2] * d[2]
    };
    let mut best: Option<(f64, Front)> = None;
    for normal in normals {
        for c in lit.iter().filter(|c| c.1 == low || c.1 == low + 1) {
            let s = (c.0 + normal.0, c.1, c.2 + normal.1);
            if lit.contains(&s)
                || cells.at(s) != path::Class::Clear
                || cells.at((s.0, s.1 + 1, s.2)) == path::Class::Lava
                || cells.at((s.0, s.1 - 1, s.2)) != path::Class::Solid
            {
                continue;
            }
            let f = Front {
                stand: s,
                tunnel: (s.0 + normal.0, s.1, s.2 + normal.1),
                normal,
            };
            let d = d2(s);
            if best.is_none_or(|(b, _)| d < b) {
                best = Some((d, f));
            }
        }
    }
    best.map(|(_, f)| f)
}

/// How far `c` lies out of the portal's plane toward `front`'s face (0 in the plane).
fn out_of_plane(front: &Front, c: Cell) -> i32 {
    let plane = (
        front.stand.0 - front.normal.0,
        front.stand.2 - front.normal.1,
    );
    (c.0 - plane.0) * front.normal.0 + (c.2 - plane.1) * front.normal.1
}

/// Ticks of furnace fuel held or made where the bot stands: planks and coal by their burn time, a
/// log that burns as the four planks it makes, sticks over `make::STICK_RESERVE` at 100. Warped
/// and crimson wood is in `#non_flammable_wood` and burns nothing itself, but a stick is fuel
/// whatever planks made it (the generated fuel table: stick 100, no warped item): a stem is 4
/// planks, 8 sticks, 800 ticks; a warped plank 2 sticks, 200.
pub fn fuel_ticks_held(counts: &crate::make::Counts) -> i32 {
    let sticks = counts.get(&ItemKind::Stick).copied().unwrap_or(0);
    let stick = data::generated::fuel::fuel_ticks(ItemKind::Stick)
        .and_then(|t| i32::try_from(t).ok())
        .unwrap_or(0);
    burning_fuel_ticks(counts) + (sticks - crate::make::STICK_RESERVE).max(0) * stick
}

/// `fuel_ticks_held` without the sticks already held.
fn burning_fuel_ticks(counts: &crate::make::Counts) -> i32 {
    let stick = data::generated::fuel::fuel_ticks(ItemKind::Stick)
        .and_then(|t| i32::try_from(t).ok())
        .unwrap_or(0);
    let planks = data::generated::tags::item_tag("planks").unwrap_or(&[]);
    let coals = data::generated::tags::item_tag("coals").unwrap_or(&[]);
    let logs = data::generated::tags::item_tag("logs").unwrap_or(&[]);
    counts
        .iter()
        .filter(|(_, n)| **n > 0)
        .map(|(k, n)| {
            let t = data::generated::fuel::fuel_ticks(*k)
                .and_then(|t| i32::try_from(t).ok())
                .unwrap_or(0);
            if (planks.contains(k) || coals.contains(k)) && t > 0 {
                n * t
            } else if logs.contains(k) && t > 0 {
                // Four planks of 300 ticks each (the generated fuel table's planks).
                n * 4 * 300
            } else if logs.contains(k) {
                // Nether wood: four planks, eight sticks.
                n * 8 * stick
            } else if planks.contains(k) {
                n * 2 * stick
            } else {
                0
            }
        })
        .sum()
}

fn dimension(w: &World) -> Option<&str> {
    w.dimension.as_ref().map(|d| d.name.as_str())
}

fn flat(a: Cell, b: Cell) -> i32 {
    let (dx, dz) = (f64::from(a.0 - b.0), f64::from(a.2 - b.2));
    (dx * dx + dz * dz).sqrt().round() as i32
}

/// Eyes, pearls and rods held, for the notes.
pub fn carried(w: &World) -> String {
    format!(
        "eyes {}, pearls {}, rods {}, powder {}",
        craft::count(w, ItemKind::EnderEye),
        craft::count(w, ItemKind::EnderPearl),
        craft::count(w, ItemKind::BlazeRod),
        craft::count(w, ItemKind::BlazePowder)
    )
}

impl Plan {
    fn home_rules(&self) -> HomeRules {
        self.table
            .as_ref()
            .map_or_else(HomeRules::default, |t| t.home.clone())
    }

    fn home_state(&self, w: &World) -> String {
        format!(
            "{}; {}",
            carried(w),
            endermen::travel_state(w, self.table.as_ref().map(|t| &t.survival))
        )
    }

    /// The top-up is over in the way home: the hunt's rows stand down.
    pub(crate) fn home_hunt_over(&self) -> bool {
        self.ender.phase == EnderPhase::Home && self.ender.home.top_up_over
    }

    /// Home is over (fed on the overworld's surface): the eyes' phase is next (Q3, Q4).
    pub fn home_done(&self) -> bool {
        self.ender.phase == EnderPhase::Home && self.ender.home.done
    }

    /// Into Home, under the race: in the Nether with the pearls' count held (pearls and eyes
    /// together: a resumed bot may hold eyes), or in the overworld resumed at `pearls_12` or past
    /// it (the top-up, the kit and the trip are then behind it).
    pub(crate) fn home_begin(&mut self, w: &World, tick: u64) {
        if self.aim != Aim::Race || self.ender.phase == EnderPhase::Home {
            return;
        }
        let wanted = self.table.as_ref().map_or(12, |t| t.pearls.pearls_wanted);
        let held = craft::count(w, ItemKind::EnderPearl) + craft::count(w, ItemKind::EnderEye);
        match dimension(w) {
            Some(NETHER) if held >= wanted => {
                // Resumed past the kit (a snapshot on the trip, race 45725323112): the pearls are
                // eyes and the kit is held, so no top-up and no kit again (it walked 258 back to
                // the hunt point).
                let eyes = craft::count(w, ItemKind::EnderEye);
                let made = eyes >= wanted;
                let kit = made
                    && craft::count(w, ItemKind::Stick)
                        >= self.table.as_ref().map_or(8, |t| t.base.home_sticks)
                    && craft::count(w, ItemKind::CraftingTable) >= 1;
                self.ender.phase = EnderPhase::Home;
                self.ender.home = HomeState {
                    since: Some(tick),
                    top_up_over: made,
                    kit_begun: kit,
                    kit_over: kit,
                    ..HomeState::default()
                };
                if made {
                    // The way points before the nearest one are behind a bot resumed mid-trip.
                    let feet = w.body.map(|b| path::feet_cell(b.pos)).unwrap_or_default();
                    let way = self.home_rules().way;
                    let nearest = way
                        .iter()
                        .enumerate()
                        .min_by_key(|(_, [x, z])| flat(feet, (*x, feet.1, *z)))
                        .map(|(i, _)| i);
                    if let Some(k) = nearest {
                        self.ender.home.ways_passed =
                            way.iter().take(k).map(|[x, z]| (x >> 4, z >> 4)).collect();
                    }
                    self.note(
                        tick,
                        "home: resumed with the eyes made".to_owned(),
                        format!("{eyes} eyes held: no top-up; the kit held: {kit}"),
                    );
                }
                let s = self.home_state(w);
                self.note(
                    tick,
                    "home: 12 pearls held, on to the way home".to_owned(),
                    format!(
                        "{held} pearls and eyes held of {wanted}; the top-up to {} first; {s}",
                        self.home_rules().pearls_wanted
                    ),
                );
            }
            Some(OVERWORLD)
                if w.stage_reached
                    .is_some_and(|s| s >= world::stage::Stage::Pearls12) =>
            {
                self.ender.phase = EnderPhase::Home;
                self.ender.home = HomeState {
                    since: Some(tick),
                    top_up_over: true,
                    kit_begun: true,
                    kit_over: true,
                    trip_begun: true,
                    ..HomeState::default()
                };
                let s = self.home_state(w);
                self.note(
                    tick,
                    "home: resumed in the overworld".to_owned(),
                    format!(
                        "the stage {:?}; the climb and the food; {s}",
                        w.stage_reached
                    ),
                );
            }
            Some(_) | None => {}
        }
    }

    /// The top-up's end, in Home: `wanted` pearls held, or `top_up_ticks` since Home began. True
    /// once over (the hunt's rows then stand down).
    pub(crate) fn home_top_up_end(&mut self, w: &World, tick: u64, wanted: i32) -> bool {
        if self.ender.phase != EnderPhase::Home {
            return false;
        }
        if self.ender.home.top_up_over {
            return true;
        }
        let held = craft::count(w, ItemKind::EnderPearl);
        let spent = tick.saturating_sub(self.ender.home.since.unwrap_or(tick));
        let clock = self.home_rules().top_up_ticks;
        if held < wanted && spent < clock {
            return false;
        }
        self.ender.home.top_up_over = true;
        self.pearls.fight = None;
        self.pearls.provoking = None;
        let s = self.home_state(w);
        self.note(
            tick,
            "home: pearl top-up over".to_owned(),
            format!(
                "{} after {spent} ticks of {clock}: {held} pearls of {wanted}, {} kills in all; {s}",
                if held >= wanted { "the count" } else { "the clock" },
                self.pearls.kills
            ),
        );
        true
    }

    // >>> home (Q2): the planner's one entry, called first in `nether_task`.
    /// The way home's next task in either dimension, under the race; `None` when not on it.
    /// Once Home is over, `Task::Idle` in the overworld (never back into the portal) until the
    /// eyes' phase takes the planner.
    pub(crate) fn home_race_task(&mut self, w: &World, tick: u64) -> Option<Task> {
        if self.aim != Aim::Race {
            return None;
        }
        self.home_begin(w, tick);
        if self.ender.phase != EnderPhase::Home {
            return None;
        }
        self.home_task(w, tick)
    }
    // <<< home

    /// Home's next task by dimension.
    pub(crate) fn home_task(&mut self, w: &World, tick: u64) -> Option<Task> {
        // The planner's give-up (failed walks in a row) never ends the way home.
        if self.stuck {
            self.stuck = false;
            self.failed_in_row = 0;
            self.note(
                tick,
                "stuck cleared: the way home goes on".to_owned(),
                "the walks' give-up does not end the way home".to_owned(),
            );
        }
        match dimension(w) {
            Some(NETHER) => self.home_nether(w, tick),
            Some(OVERWORLD) => self.home_overworld(w, tick),
            Some(_) | None => None,
        }
    }

    fn home_nether(&mut self, w: &World, tick: u64) -> Option<Task> {
        if !self.ender.home.top_up_over {
            return Some(self.pearls_task(w, tick).unwrap_or(Task::Idle));
        }
        if !self.ender.home.kit_over {
            if !self.ender.home.kit_begun {
                self.ender.home.kit_begun = true;
                self.base = base::BaseState {
                    steps: base::HOME_STEPS.to_vec(),
                    ..base::BaseState::default()
                };
                let s = self.home_state(w);
                self.note(
                    tick,
                    "home: the kit begun".to_owned(),
                    format!(
                        "the eyes, stems, spare wooden pickaxes, sticks and a table to carry; {} logs, {} sticks, {} tables; {s}",
                        base::logs_held(w),
                        craft::count(w, ItemKind::Stick),
                        craft::count(w, ItemKind::CraftingTable)
                    ),
                );
            }
            if let Some(t) = self.base_task(w, tick) {
                return Some(t);
            }
            self.ender.home.kit_over = true;
            let s = self.home_state(w);
            self.note(
                tick,
                format!(
                    "home: the kit {}",
                    if self.base.over {
                        "over"
                    } else {
                        "paused: on without it"
                    }
                ),
                format!(
                    "{} logs, {} sticks, {} tables, {} wooden pickaxes; {s}",
                    base::logs_held(w),
                    craft::count(w, ItemKind::Stick),
                    craft::count(w, ItemKind::CraftingTable),
                    craft::count(w, ItemKind::WoodenPickaxe)
                ),
            );
        }
        if let Some(t) = self.home_recover(w, tick) {
            return Some(t);
        }
        self.home_trip(w, tick)
    }

    /// The travel's recovery (issue 18c's rules, `endermen_task`'s): at or under the danger line
    /// with nothing to heal with, a sealed pocket where the bot stands, never a death as a plan;
    /// under `travel_health` with a ticking ghast or blaze in sight, cover first.
    fn home_recover(&mut self, w: &World, tick: u64) -> Option<Task> {
        let rules = self.table.as_ref().map(|t| t.survival.clone())?;
        let health = w.player.health?;
        let heal = crate::can_heal(w.player.food, eat::best_food(w, &rules).is_some());
        if let Some(t) = self.home_cubes(w, tick, health, rules.danger_health) {
            return Some(t);
        }
        if health <= rules.danger_health && !heal {
            if self
                .ender
                .held_noted_at
                .is_none_or(|t| tick >= t + endermen::NOTE_EVERY)
            {
                self.ender.held_noted_at = Some(tick);
                self.note(
                    tick,
                    "home: hold in a pocket, nothing to heal with".to_owned(),
                    format!(
                        "health {health} at or under {}, food {:?}; never a death as a plan",
                        rules.danger_health, w.player.food
                    ),
                );
            }
            // >>> Q6 (issue 19): on the last leg in rock, quiet and not burning, the leg goes on
            // (the coordinator, 2026-11-25); never a pocket on a ledge (race 45727347488 lane 0
            // fell off its own, shot): a dig down one first (`crate::portal_way`).
            if let Some(t) = self.danger_carry_on(w, tick) {
                return Some(t);
            }
            if self.danger_at_door(w, tick)
                && let Some(portal) = self.ender.home.portal
                && let Some(t) = self.home_approach(w, tick, portal)
            {
                return Some(t);
            }
            if let Some(t) = self.ledge_dig(w, tick) {
                return Some(t);
            }
            // <<< Q6
            // >>> Q8: the same carry-on on the whole trip (the coordinator, 2026-11-26: B14 lane 3 held
            // a danger pocket at 5.6 from tick 9,240 to the cap; the overworld has food).
            if let Some(t) = self.home_danger_tunnel(w, tick) {
                return Some(t);
            }
            // <<< Q8
            if on_ledge(w) {
                // Never a pocket on a ledge (Q6's race 45727347488: knocked off one while sealing).
                self.ender.home.ledge_skips += 1;
                if self.ender.home.ledge_skips % 20 == 1 {
                    self.note(
                        tick,
                        "home: no pocket on a ledge".to_owned(),
                        format!(
                            "air beside or under the feet; skip {}",
                            self.ender.home.ledge_skips
                        ),
                    );
                }
                return None;
            }
            self.pocket_step(w, tick);
            self.retry_at = tick + 1;
            return Some(Task::Idle);
        }
        // A blaze or ghast in sight on the trip: cover at any health, held until it has been out
        // of sight SHOOTER_CLEAR_TICKS; never a fight (the defence ignores them in Home).
        let seen = endermen::seen_by_far_shooter(w)
            || (self.ender.home.top_up_over && (skeleton_sees(w) || ghast_sees(w)));
        if seen && self.ender.home.top_up_over {
            self.ender.home.shooter_seen_at = Some(tick);
        }
        let held = self
            .ender
            .home
            .shooter_seen_at
            .is_some_and(|t| tick < t + SHOOTER_CLEAR_TICKS);
        if held && !seen {
            self.retry_at = tick + crate::RETRY_TICKS;
            return Some(Task::Idle);
        }
        if seen && (health < rules.travel_health || self.ender.home.top_up_over) {
            let t = self.take_cover(w, tick)?;
            if self
                .ender
                .recover_noted_at
                .is_none_or(|t| tick >= t + endermen::NOTE_EVERY)
            {
                self.ender.recover_noted_at = Some(tick);
                self.note(
                    tick,
                    "home: cover from a shooter".to_owned(),
                    format!(
                        "health {health} under {} with a ghast or blaze in sight",
                        rules.travel_health
                    ),
                );
            }
            return Some(t);
        }
        None
    }

    /// The trip's axis of travel (`HomeState::axis`): a known span's (west), else the last step
    /// of the feet along x or z. Read every tick on the trip.
    fn home_axis(&mut self, w: &World) -> Option<(i32, i32)> {
        let body = w.body?;
        let feet = path::stand_cell(w, body.pos);
        let r = self.home_rules();
        if r.bridges
            .iter()
            .any(|[e, wst, y, z]| near_span([*e, *wst, *y, *z], feet))
        {
            self.ender.home.axis = Some((-1, 0));
        } else if let Some(last) = self.ender.home.last_feet
            && last != feet
            && last.1 == feet.1
        {
            let (dx, dz) = (feet.0 - last.0, feet.2 - last.2);
            if dx.abs() + dz.abs() == 1 {
                self.ender.home.axis = Some((dx, dz));
            }
        }
        // No step yet: the axis toward the point aimed at.
        if self.ender.home.axis.is_none()
            && let Some((px, pz)) = self.ender.home.aim
        {
            let (dx, dz) = (px - feet.0, pz - feet.2);
            self.ender.home.axis = Some(if dx.abs() >= dz.abs() {
                (dx.signum(), 0)
            } else {
                (0, dz.signum())
            })
            .filter(|a| *a != (0, 0));
        }
        self.ender.home.last_feet = Some(feet);
        self.ender.home.axis
    }

    /// The guard (the coordinator, 2026-11-26; race 45758211666: lane 1 knocked off the west
    /// span 9 over the sea by a cube that spawned beside it): a block at the feet's level on each
    /// side across the trip's axis that is open (Bridger, 2026-11-26: "do the rails always"), laid
    /// before the next step, mob or no mob. Noted and counted; `None` with nothing to guard or no filler.
    fn home_guard(&mut self, w: &World, tick: u64) -> Option<Task> {
        let axis = self.home_axis(w);
        // Never while a restock is in hand: its digs open sides the guard would fill again (race
        // 45764179359: six lanes dug 58 to 74 cells by the join, filler flat at 47 to 59).
        if self.headed.as_ref().is_some_and(|h| h.restock.is_some()) {
            return None;
        }
        // Never a cell of the tunnel's own route, nor the floor of one (race 45779137282: the guard
        // filled (22, 43, -11), a stair of its own last leg).
        let mut skip = self.ender.home.guard_refused.clone();
        skip.extend(
            self.travel
                .route
                .iter()
                .flat_map(|c| [*c, (c.0, c.1 - 1, c.2)]),
        );
        let (at, click, face) = guard_next(w, axis, &skip)?;
        // The progress invariant: a guard clicked `GUARD_TRIES` times and still open is given up
        // (race 45760537530: one guard clicked 221 to 411 times, filler unchanged, blazes near).
        let tries = match self.ender.home.guard_try {
            Some((c, n)) if c == at => n,
            Some(_) | None => 0,
        };
        if tries >= GUARD_TRIES {
            self.ender.home.guard_try = None;
            self.ender.home.guard_refused.push(at);
            if self.ender.home.guard_refused.len() > GUARD_REFUSED_KEPT {
                self.ender.home.guard_refused.remove(0);
            }
            self.note(
                tick,
                format!("home: guard given up at {at:?}"),
                format!(
                    "{tries} clicks on {click:?} face {face}, still open; given up {}",
                    self.ender.home.guard_refused.len()
                ),
            );
            return None;
        }
        let item = endermen::FILLER
            .iter()
            .copied()
            .find(|k| craft::count(w, *k) > 0)?;
        if w.window.is_some()
            || w.click.is_some()
            || !self.acts.is_empty()
            || tick < self.shelter_at
        {
            return Some(Task::Idle);
        }
        match crate::hold::toward(w, item) {
            crate::hold::Hold::Held => {
                self.shelter_at = tick + crate::SHELTER_EVERY_TICKS;
                self.ender.home.guards += 1;
                self.ender.home.guard_try = Some((at, tries + 1));
                self.acts.push(world::Act::UseOn {
                    pos: world::BlockPos::new(click.0, click.1, click.2),
                    face,
                });
                if self.ender.home.guards % 10 == 1 {
                    self.note(
                        tick,
                        format!("home: guard laid at {at:?}"),
                        format!(
                            "a side across the axis {axis:?} is open; {item:?} on {click:?} face {face}; guard {}, filler {}",
                            self.ender.home.guards,
                            path::bridge_blocks(w)
                        ),
                    );
                }
                self.retry_at = tick + 1;
                Some(Task::Idle)
            }
            crate::hold::Hold::Act(a) => {
                self.acts.push(a);
                Some(Task::Idle)
            }
            crate::hold::Hold::Wait => Some(Task::Idle),
            crate::hold::Hold::Missing => None,
        }
    }

    /// Under the danger line on the trip with nothing to heal with: quiet (`carry_quiet`: not
    /// burning, no hostile within 16 for 200 ticks), no shooter in sight, 2 of rock over the head,
    /// a covered tunnel (`tunnel_toward_covered`) up to `DANGER_LEG` toward the point aimed at,
    /// every cell of its route covered; never a walk in the open, never a bridge. Noted and counted
    /// (`danger_tunnels`); `None` otherwise (the pocket holds).
    fn home_danger_tunnel(&mut self, w: &World, tick: u64) -> Option<Task> {
        if !self.ender.home.top_up_over
            || dimension(w) != Some(NETHER)
            || skeleton_sees(w)
            || ghast_sees(w)
            || endermen::seen_by_far_shooter(w)
            || !self.carry_quiet(w, tick)
        {
            return None;
        }
        let body = w.body?;
        let feet = path::stand_cell(w, body.pos);
        let rock = |c: Cell| {
            path::classify(w.state_at(world::BlockPos::new(c.0, c.1, c.2))) == path::Class::Solid
        };
        let covered = |c: Cell| rock((c.0, c.1 + 2, c.2)) && rock((c.0, c.1 + 3, c.2));
        if !covered(feet) {
            return None;
        }
        let (ax, az) = self.ender.home.aim?;
        let (dx, dz) = (ax - feet.0, az - feet.2);
        let len = f64::from(dx * dx + dz * dz).sqrt().max(1.0);
        let step = (f64::from(DANGER_LEG) / len).min(1.0);
        let target = (
            feet.0 + (f64::from(dx) * step).round() as i32,
            feet.1,
            feet.2 + (f64::from(dz) * step).round() as i32,
        );
        let t = self.tunnel_toward_covered(
            w,
            target,
            ("the danger tunnel", "on under the danger line"),
            tick,
        )?;
        let ok = match &t {
            Task::Stairs(st) => st.route.iter().all(|c| covered(*c)),
            _ => false,
        };
        if !ok {
            return None;
        }
        self.ender.home.danger_tunnels += 1;
        if self.ender.home.danger_tunnels % 10 == 1 {
            self.note(
                tick,
                "home: on under the danger line by covered tunnel".to_owned(),
                format!(
                    "health {:?}, nothing to heal with; quiet, no shooter in sight, 2 of rock over the head; toward {target:?}; tunnel {}",
                    w.player.health, self.ender.home.danger_tunnels
                ),
            );
        }
        Some(t)
    }

    /// A cube within `CUBE_CLEAR` and an edge beside the feet (no floor beside them: lava or air
    /// under the next cell): the floor beside the feet is laid first, a block clicked onto the side
    /// of the one stood on, so a cube's knock-back lands on a block (race 45733362772 lane 0: a
    /// magma cube knocked it into lava 560 ticks after the resume). Noted and counted; `None`
    /// with no cube near, no edge beside, or no filler held.
    fn home_rail(&mut self, w: &World, tick: u64) -> Option<Task> {
        let (target, face, cube, knocker) = rail_wanted(w)?;
        let body = w.body?;
        let feet = path::feet_cell(body.pos);
        let under = (feet.0, feet.1 - 1, feet.2);
        let item = endermen::FILLER
            .iter()
            .copied()
            .find(|k| craft::count(w, *k) > 0)?;
        if w.window.is_some()
            || w.click.is_some()
            || !self.acts.is_empty()
            || tick < self.shelter_at
        {
            return Some(Task::Idle);
        }
        match crate::hold::toward(w, item) {
            crate::hold::Hold::Held => {
                self.shelter_at = tick + crate::SHELTER_EVERY_TICKS;
                self.ender.home.rails += 1;
                self.acts.push(world::Act::UseOn {
                    pos: world::BlockPos::new(under.0, under.1, under.2),
                    face,
                });
                self.note(
                    tick,
                    format!("home: rail beside the feet at {target:?}"),
                    format!(
                        "a mob that knocks near (cube within {CUBE_CLEAR}: {cube}; one within {KNOCK_RANGE} in sight: {knocker}) and no floor beside {feet:?}; {item:?} on {under:?} face {face}; rail {}",
                        self.ender.home.rails
                    ),
                );
                self.retry_at = tick + 1;
                Some(Task::Idle)
            }
            crate::hold::Hold::Act(a) => {
                self.acts.push(a);
                Some(Task::Idle)
            }
            crate::hold::Hold::Wait => Some(Task::Idle),
            crate::hold::Hold::Missing => None,
        }
    }

    /// On a known bridge (`[home] bridges`): the walk west along it, 24 at a time on its own
    /// blocks, no new bridging; at the east span's west end the step blocks are placed first
    /// (`[home] steps`), then the walk up onto the west span. `None` off every bridge.
    fn home_bridge(&mut self, w: &World, tick: u64, r: &HomeRules) -> Option<Task> {
        let body = w.body?;
        let feet = path::feet_cell(body.pos);
        let out = self.ender.home.spans_out.clone();
        // >>> Q8: the guard's filler before the spans are walked, once, by the first span met: 2
        // a cell for what is left of them west of the feet, over the keep, at most `GUARD_EXTRA`
        // over it, dug where the bot stands (never a span, its join or the sea's floor:
        // `heading::restock_cells`).
        let near_any = r.bridges.iter().enumerate().any(|(i, [e, wst, y, z])| {
            !out.contains(&i)
                && (feet.2 - z).abs() <= BRIDGE_NEAR
                && feet.0 <= *e + 1
                && feet.0 >= *wst
                && (feet.1 - y).abs() <= BRIDGE_BELOW
        });
        if near_any && !self.ender.home.guard_restock_done {
            let left: i32 = r
                .bridges
                .iter()
                .map(|[e, wst, _, _]| ((*e).min(feet.0) - wst + 1).max(0))
                .sum();
            let need = (crate::heading::HOME_KEEP + 2 * i64::from(left))
                .min(crate::heading::HOME_KEEP + GUARD_EXTRA);
            let filler = path::bridge_blocks(w);
            let mut h = self.headed.take().unwrap_or_else(|| {
                crate::heading::Heading::new(
                    (-1, 0),
                    feet,
                    tick,
                    "the spans' guard restock".to_owned(),
                )
            });
            if h.restock.is_none() && filler < need && !self.ender.home.span_restocked {
                self.ender.home.span_restocked = true;
                h.restock = Some(crate::heading::Restock {
                    filler0: filler,
                    uses0: endermen::pickaxe_uses_left(w),
                    digs: 0,
                    began: tick,
                    to: need,
                });
                self.note(
                    tick,
                    format!("home: restock for the spans' guard at {feet:?}"),
                    format!(
                        "{left} span cells west of the feet: filler {filler}, {need} wanted (2 a cell over the keep)"
                    ),
                );
            }
            let t = self.heading_restock(w, &mut h, tick);
            self.headed = Some(h);
            if let Some(t) = t {
                return Some(t);
            }
            self.ender.home.guard_restock_done = true;
            // It went with what it had: nothing owed from it (race 45768618919: owed restocks ran
            // again and again on the join's island).
            self.ender.home.filler_owed = None;
        }
        // <<< Q8
        let on = r.bridges.iter().position(|[e, wst, y, z]| {
            (feet.2 - z).abs() <= 1 && feet.0 <= *e + 1 && feet.0 >= *wst && (feet.1 - y).abs() <= 1
        });
        // A span walked out is the heading's from then on.
        if on.is_some_and(|i| out.contains(&i)) {
            return None;
        }
        // Beside a span but not on it (race 45733362772's bots stood on their own bridge at y 36
        // a few blocks south of it): onto it first, at the feet's x.
        let Some(on) = on else {
            // >>> Q8: the join west of a span (up to `JOIN_REACH`) counts as beside it, and from the
            // join the next span's east end is the target (race 45768618919: four lanes stood on the
            // join's island at x 309 and 310, in no span's reach, the heading bridging the sea).
            let (i, [e, wst, y, z]) = r
                .bridges
                .iter()
                .enumerate()
                .find(|(i, [e, wst, y, z])| {
                    !out.contains(i)
                        && (feet.2 - z).abs() <= BRIDGE_NEAR
                        && feet.0 <= *e + 1
                        && feet.0 >= *wst - JOIN_REACH
                        && (feet.1 - y).abs() <= BRIDGE_BELOW
                })
                .map(|(i, s)| (i, *s))?;
            let target = match r.bridges.get(i + 1) {
                Some([e2, _, y2, z2]) if feet.0 < wst => (*e2, *y2, *z2),
                Some(_) | None => (feet.0.clamp(wst, e), y, z),
            };
            // <<< Q8
            let mut walk = Walk::new(
                target,
                Goal::Near {
                    pos: target,
                    range: 0.8,
                },
                Arrive::Goal,
                tick,
            );
            if !walk.route_now(w, tick) || walk.route != Some(path::Status::Found) {
                self.expanded += walk.expanded;
                // >>> Q8: up to 2 below and the walk finds no way: a step block laid toward the span
                // at the feet's level, on a full floor, then the walk again (Q10's races 45764230971
                // and 45764234506: 8 of 8 lanes at y 34 under the span top at 36, "tunnel route …
                // 0 steps, ends within reach", every tick).
                if (1..=BRIDGE_BELOW).contains(&(target.1 - feet.1))
                    && let Some(t) = self.home_step_up(w, tick, feet, target)
                {
                    return Some(t);
                }
                // <<< Q8
                // From below (race 45738826162: its own bridge 5 under the west span): the travel
                // tunnel's stairs up, laying blocks where there is no floor.
                if target.1 > feet.1 + 1
                    && let Some(t) =
                        self.tunnel_toward(w, target, ("the old bridge", "joined from below"), tick)
                {
                    self.note(
                        tick,
                        format!("home: up onto the old bridge at {target:?}"),
                        format!("from {feet:?}, {} below it", target.1 - feet.1),
                    );
                    return Some(t);
                }
                return None;
            }
            self.walks += 1;
            self.ender.home.bridge_walks += 1;
            self.note(
                tick,
                format!("home: onto the old bridge at {target:?}"),
                format!("from {feet:?}; walk {}", self.ender.home.bridge_walks),
            );
            return Some(Task::Explore(walk));
        };
        let [_, west, y, z] = r.bridges[on];
        // The way points along the bridges at or east of the feet are passed: the heading takes
        // over past the west end and must not aim back along them.
        for [px, pz] in &r.way {
            let near_line = r.bridges.iter().any(|[e, wst, _, bz]| {
                (pz - bz).abs() <= BRIDGE_NEAR && *px <= *e + 1 && *px >= *wst - BRIDGE_NEAR
            });
            let col = (px >> 4, pz >> 4);
            if near_line && *px >= feet.0 && !self.ender.home.ways_passed.contains(&col) {
                self.ender.home.ways_passed.push(col);
            }
        }
        let next = r.bridges.get(on + 1).copied();
        // At the span's west end with a span beyond: the step blocks, then the walk up.
        let target = if feet.0 <= west + 1
            && let Some([e2, _, y2, z2]) = next
        {
            if let Some(t) = self.home_step(w, tick, r) {
                return Some(t);
            }
            (e2, y2, z2)
        } else if feet.0 <= west + 1 {
            self.ender.home.spans_out.push(on);
            self.note(
                tick,
                format!("home: the old bridges walked out at {feet:?}"),
                format!(
                    "span {on}'s west end ({west}, {y}, {z}), no span beyond: the heading's from here"
                ),
            );
            return None;
        } else {
            ((feet.0 - BRIDGE_LEG).max(west), y, z)
        };
        // A cell gone from the span between the feet and the target (the bot's own restock dug a
        // 7-cell gap in it: Q1's probe 45740087576): the floor laid there from the cell before it,
        // kept to the span's line and height.
        if target.1 == y
            && let Some(gx) = (target.0..feet.0).rev().find(|x| {
                let mut cells = path::Cells::new(w);
                cells.at((*x, y - 1, z)) != path::Class::Solid
            })
        {
            let edge = (gx + 1, y, z);
            if feet != edge {
                let mut walk = Walk::new(
                    edge,
                    Goal::Near {
                        pos: edge,
                        range: 0.5,
                    },
                    Arrive::Goal,
                    tick,
                );
                if walk.route_now(w, tick) && walk.route == Some(path::Status::Found) {
                    self.walks += 1;
                    self.note(
                        tick,
                        format!("home: to the gap in the old bridge at {edge:?}"),
                        format!("the floor at ({gx}, {}, {z}) is gone", y - 1),
                    );
                    return Some(Task::Explore(walk));
                }
                self.expanded += walk.expanded;
            } else if let Some(t) = self.home_lay(w, tick, (gx, y - 1, z), (gx + 1, y - 1, z), 4) {
                return Some(t);
            }
        }
        let mut walk = Walk::new(
            target,
            Goal::Near {
                pos: target,
                range: 0.8,
            },
            Arrive::Goal,
            tick,
        );
        if !walk.route_now(w, tick) || walk.route != Some(path::Status::Found) {
            self.expanded += walk.expanded;
            // A few cells short on the span's line (race 45743712658 lane 0 stood at x 313 for
            // 8,000 ticks, "no walk" to (311, 37, 194)): a solid block in the feet or head cells
            // between is dug, never one with lava beside.
            if target.2 == feet.2 && (feet.0 - target.0).abs() <= 3 && feet.1 == target.1 {
                let mut cells = path::Cells::new(w);
                let step = if target.0 < feet.0 { -1 } else { 1 };
                let block = (1..=(feet.0 - target.0).abs())
                    .map(|i| feet.0 + step * i)
                    .flat_map(|x| [(x, feet.1, feet.2), (x, feet.1 + 1, feet.2)])
                    .find(|c| {
                        cells.at(*c) == path::Class::Solid
                            && ![(1, 0, 0), (-1, 0, 0), (0, 1, 0), (0, 0, 1), (0, 0, -1)]
                                .iter()
                                .any(|(dx, dy, dz)| {
                                    cells.at((c.0 + dx, c.1 + dy, c.2 + dz)) == path::Class::Lava
                                })
                    });
                if let Some(c) = block {
                    self.note(
                        tick,
                        format!("home: cleared the old bridge at {c:?}"),
                        format!("no walk from {feet:?} to {target:?}"),
                    );
                    return Some(Task::Dig(crate::dig::Dig::new(w, c, tick)));
                }
            }
            if self
                .ender
                .home
                .bridge_noted_at
                .is_none_or(|t| tick >= t + 600)
            {
                self.ender.home.bridge_noted_at = Some(tick);
                self.note(
                    tick,
                    format!("home: no walk on the old bridge to {target:?}"),
                    format!("from {feet:?}; the heading goes on"),
                );
            }
            return None;
        }
        self.walks += 1;
        self.ender.home.bridge_walks += 1;
        self.note(
            tick,
            format!("home: on the old bridge to {target:?}"),
            format!("from {feet:?}; walk {} on it", self.ender.home.bridge_walks),
        );
        Some(Task::Explore(walk))
    }

    /// Up to 2 below a span with no walk onto it: a stair of laid blocks toward it, one at the
    /// feet's level, the next one higher, each on a full floor with two open cells over it; a
    /// solid cell over a step (no lava beside) dug first. At most `STEP_UPS` blocks a trip. `None`
    /// with nothing to lay or dig (the walk and the tunnel are tried as before).
    fn home_step_up(&mut self, w: &World, tick: u64, feet: Cell, target: Cell) -> Option<Task> {
        if self.ender.home.step_ups >= STEP_UPS {
            return None;
        }
        // Toward the span when there is room for the climb, else along the trip (west, the spans'
        // axis): the last step comes level beside the span (race 45768618919: lanes 4 to 7 below
        // the west span, one off its line).
        let dy = target.1 - feet.1;
        let (dx, dz) = if (target.2 - feet.2).abs() >= dy {
            (0, (target.2 - feet.2).signum())
        } else {
            (-1, 0)
        };
        let mut cells = path::Cells::new(w);
        let lava_by = |cells: &mut path::Cells, c: Cell| {
            [(1, 0, 0), (-1, 0, 0), (0, 1, 0), (0, 0, 1), (0, 0, -1)]
                .iter()
                .any(|(x, y, z)| cells.at((c.0 + x, c.1 + y, c.2 + z)) == path::Class::Lava)
        };
        for i in 1..=(target.1 - feet.1) {
            let c = (feet.0 + dx * i, feet.1 + i - 1, feet.2 + dz * i);
            let under = (c.0, c.1 - 1, c.2);
            match cells.at(c) {
                path::Class::Solid => {
                    // The step is there: its head room.
                    if let Some(h) = [(c.0, c.1 + 1, c.2), (c.0, c.1 + 2, c.2)]
                        .into_iter()
                        .find(|h| cells.at(*h) == path::Class::Solid)
                    {
                        if lava_by(&mut cells, h) {
                            return None;
                        }
                        self.ender.home.step_ups += 1;
                        self.note(
                            tick,
                            format!("home: head room dug over the step up at {h:?}"),
                            format!("from {feet:?} toward {target:?}"),
                        );
                        return Some(Task::Dig(crate::dig::Dig::new(w, h, tick)));
                    }
                    // Onto it: the stair goes on from there (from the feet, a step at a time).
                    let top = (c.0, c.1 + 1, c.2);
                    let mut walk = Walk::new(
                        top,
                        Goal::Near {
                            pos: top,
                            range: 0.5,
                        },
                        Arrive::Goal,
                        tick,
                    );
                    if walk.route_now(w, tick) && walk.route == Some(path::Status::Found) {
                        self.walks += 1;
                        self.note(
                            tick,
                            format!("home: up the step at {top:?}"),
                            format!("from {feet:?} toward {target:?}"),
                        );
                        return Some(Task::Explore(walk));
                    }
                    self.expanded += walk.expanded;
                }
                path::Class::Clear => {
                    // On the floor under it, else the floor itself first, clicked onto the last
                    // step's side.
                    let prev = (c.0 - dx, c.1 - 1, c.2 - dz);
                    let face_to = match (dx, dz) {
                        (1, _) => 5,
                        (-1, _) => 4,
                        (_, 1) => 3,
                        _ => 2,
                    };
                    let (at, click, face) = if cells.at(under) == path::Class::Solid {
                        (c, under, 1)
                    } else if cells.at(under) == path::Class::Clear
                        && cells.at(prev) == path::Class::Solid
                    {
                        (under, prev, face_to)
                    } else {
                        return None;
                    };
                    let item = endermen::FILLER
                        .iter()
                        .copied()
                        .find(|k| craft::count(w, *k) > 0)?;
                    if w.window.is_some()
                        || w.click.is_some()
                        || !self.acts.is_empty()
                        || tick < self.shelter_at
                    {
                        return Some(Task::Idle);
                    }
                    return match crate::hold::toward(w, item) {
                        crate::hold::Hold::Held => {
                            self.shelter_at = tick + crate::SHELTER_EVERY_TICKS;
                            self.ender.home.step_ups += 1;
                            self.acts.push(world::Act::UseOn {
                                pos: world::BlockPos::new(click.0, click.1, click.2),
                                face,
                            });
                            self.note(
                                tick,
                                format!("home: step up toward the old bridge at {at:?}"),
                                format!(
                                    "from {feet:?}, {} below {target:?}; {item:?} on {click:?} face {face}; {} laid",
                                    target.1 - feet.1,
                                    self.ender.home.step_ups
                                ),
                            );
                            self.retry_at = tick + 1;
                            Some(Task::Idle)
                        }
                        crate::hold::Hold::Act(a) => {
                            self.acts.push(a);
                            Some(Task::Idle)
                        }
                        crate::hold::Hold::Wait => Some(Task::Idle),
                        crate::hold::Hold::Missing => None,
                    };
                }
                path::Class::Water
                | path::Class::Lava
                | path::Class::Awkward
                | path::Class::Unknown => return None,
            }
        }
        None
    }

    /// One block laid at `at`, clicked on `click`'s face `face` with held filler; noted. `None`
    /// with no filler held.
    fn home_lay(&mut self, w: &World, tick: u64, at: Cell, click: Cell, face: i8) -> Option<Task> {
        let item = endermen::FILLER
            .iter()
            .copied()
            .find(|k| craft::count(w, *k) > 0)?;
        if w.window.is_some()
            || w.click.is_some()
            || !self.acts.is_empty()
            || tick < self.shelter_at
        {
            return Some(Task::Idle);
        }
        match crate::hold::toward(w, item) {
            crate::hold::Hold::Held => {
                self.shelter_at = tick + crate::SHELTER_EVERY_TICKS;
                self.ender.home.gaps_laid += 1;
                self.acts.push(world::Act::UseOn {
                    pos: world::BlockPos::new(click.0, click.1, click.2),
                    face,
                });
                self.note(
                    tick,
                    format!("home: the old bridge's gap laid at {at:?}"),
                    format!(
                        "{item:?} on {click:?} face {face}; {} laid",
                        self.ender.home.gaps_laid
                    ),
                );
                self.retry_at = tick + 1;
                Some(Task::Idle)
            }
            crate::hold::Hold::Act(a) => {
                self.acts.push(a);
                Some(Task::Idle)
            }
            crate::hold::Hold::Wait => Some(Task::Idle),
            crate::hold::Hold::Missing => None,
        }
    }

    /// The join from the span the bot is on to the next one west, built from the world as it is
    /// (the snapshot's world is not the frontier's: race 45738826162): from the bot at the span's
    /// west end, each column between rises by one from the last feet toward the next span's feet;
    /// a column lower than that gets blocks placed from its top up (bottom first), any solid cell
    /// in the two over the new feet is dug (none with lava beside). The bot stands at the span's
    /// west end first, so nothing is placed in its own body. `None` once the join is whole.
    fn home_step(&mut self, w: &World, tick: u64, r: &HomeRules) -> Option<Task> {
        let body = w.body?;
        let feet = path::feet_cell(body.pos);
        let on = r.bridges.iter().position(|[e, wst, y, z]| {
            (feet.2 - z).abs() <= 1 && feet.0 <= *e + 1 && feet.0 >= *wst && (feet.1 - y).abs() <= 1
        })?;
        let [_, west, y, z] = r.bridges[on];
        let [e2, _, y2, _] = *r.bridges.get(on + 1)?;
        let stand = (west, y, z);
        let mut cells = path::Cells::new(w);
        let solid = |cells: &mut path::Cells, c: Cell| cells.at(c) == path::Class::Solid;
        let lava_by = |cells: &mut path::Cells, c: Cell| {
            [
                (1, 0, 0),
                (-1, 0, 0),
                (0, 1, 0),
                (0, -1, 0),
                (0, 0, 1),
                (0, 0, -1),
            ]
            .iter()
            .any(|(dx, dy, dz)| cells.at((c.0 + dx, c.1 + dy, c.2 + dz)) == path::Class::Lava)
        };
        // What the join wants, column by column going west: a place, a dig or nothing.
        let mut want: Option<(Cell, bool)> = None;
        let mut last = y;
        for x in (e2 + 1..west).rev() {
            let f = (last + 1).min(y2.max(last));
            last = f;
            let top = f - 1;
            // The column's own top at or under the new floor: the cells from it up are placed.
            let base = (top - 6..=top)
                .rev()
                .find(|yy| solid(&mut cells, (x, *yy, z)));
            let from = base.map_or(top - 6, |b| b + 1);
            if let Some(c) = (from..=top)
                .map(|yy| (x, yy, z))
                .find(|c| !solid(&mut cells, *c))
            {
                want = Some((c, true));
                break;
            }
            if let Some(c) = [(x, f, z), (x, f + 1, z)]
                .into_iter()
                .find(|c| solid(&mut cells, *c))
            {
                want = Some((c, false));
                break;
            }
        }
        let (c, place) = want?;
        // The span's own last cell gone (dug by a restock): laid from the cell east of it.
        if !solid(&mut cells, (stand.0, stand.1 - 1, stand.2)) && feet == (west + 1, y, z) {
            return self.home_lay(
                w,
                tick,
                (stand.0, stand.1 - 1, stand.2),
                (west + 1, y - 1, z),
                4,
            );
        }
        if feet != stand {
            let mut walk = Walk::new(
                stand,
                Goal::Near {
                    pos: stand,
                    range: 0.5,
                },
                Arrive::Goal,
                tick,
            );
            if walk.route_now(w, tick) && walk.route == Some(path::Status::Found) {
                self.walks += 1;
                self.note(
                    tick,
                    format!("home: to the bridge's end at {stand:?} for the step"),
                    format!("from {feet:?}; {c:?} wanted"),
                );
                return Some(Task::Explore(walk));
            }
            self.expanded += walk.expanded;
            return None;
        }
        let eye = [body.pos[0], body.pos[1] + 1.62, body.pos[2]];
        let reach = (0..3)
            .map(|i| (eye[i] - (f64::from([c.0, c.1, c.2][i]) + 0.5)).powi(2))
            .sum::<f64>()
            .sqrt();
        let under = (c.0, c.1 - 1, c.2);
        let blocked = lava_by(&mut cells, c)
            || reach > crate::heading::RESTOCK_REACH
            || (place && !solid(&mut cells, under));
        if blocked {
            // A climb with nothing to build on (the step dug away, open air to the lava sea):
            // the travel tunnel's stairs up to the next span, laying blocks as they go.
            if let Some(t) = self.tunnel_toward(
                w,
                (e2, y2, z),
                ("the next span", "the join cannot be built"),
                tick,
            ) {
                self.note(
                    tick,
                    format!("home: stairs up to the next span at {:?}", (e2, y2, z)),
                    format!("the step at {c:?} cannot be built from {feet:?}"),
                );
                return Some(t);
            }
            if self
                .ender
                .home
                .bridge_noted_at
                .is_none_or(|t| tick >= t + 600)
            {
                self.ender.home.bridge_noted_at = Some(tick);
                self.note(
                    tick,
                    format!("home: the step at {c:?} cannot be made"),
                    format!(
                        "{}, under {:?}, {reach:.1} off, lava beside {}",
                        if place { "a place" } else { "a dig" },
                        cells.at(under),
                        lava_by(&mut cells, c)
                    ),
                );
            }
            return None;
        }
        if !place {
            self.ender.home.steps_placed += 1;
            self.note(
                tick,
                format!("home: head room over the step at {c:?}"),
                "a block over a step's new feet dug before the walk up".to_owned(),
            );
            return Some(Task::Dig(crate::dig::Dig::new(w, c, tick)));
        }
        let item = endermen::FILLER
            .iter()
            .copied()
            .find(|k| craft::count(w, *k) > 0)?;
        if w.window.is_some()
            || w.click.is_some()
            || !self.acts.is_empty()
            || tick < self.shelter_at
        {
            return Some(Task::Idle);
        }
        match crate::hold::toward(w, item) {
            crate::hold::Hold::Held => {
                self.shelter_at = tick + crate::SHELTER_EVERY_TICKS;
                self.ender.home.steps_placed += 1;
                self.acts.push(world::Act::UseOn {
                    pos: world::BlockPos::new(under.0, under.1, under.2),
                    face: 1,
                });
                self.note(
                    tick,
                    format!("home: step block at {c:?}"),
                    format!(
                        "joining the old bridges from {stand:?}; {item:?} on {under:?}; {} made",
                        self.ender.home.steps_placed
                    ),
                );
                self.retry_at = tick + 1;
                Some(Task::Idle)
            }
            crate::hold::Hold::Act(a) => {
                self.acts.push(a);
                Some(Task::Idle)
            }
            crate::hold::Hold::Wait => Some(Task::Idle),
            crate::hold::Hold::Missing => None,
        }
    }

    /// The tunnel out of a cube pocket in hand: on through rock toward its end until
    /// `CUBE_TUNNEL_OUT` along with no cube within `CUBE_CLEAR`; `None` with none in hand, or it
    /// ended (noted). With no tunnel route the near cube is fought (the defence's rest window).
    fn home_cube_tunnel(&mut self, w: &World, tick: u64) -> Option<Task> {
        let (start, target) = self.ender.home.cube_tunnel?;
        let body = w.body?;
        let feet = path::feet_cell(body.pos);
        let along = flat(start, feet);
        let clear = !w.entities.values().any(|e| {
            !e.dead && e.kind == data::EntityKind::MagmaCube && dist(e.pos, body.pos) <= CUBE_CLEAR
        });
        if (along >= CUBE_TUNNEL_OUT && clear) || feet == target {
            self.ender.home.cube_tunnel = None;
            self.note(
                tick,
                "home: out of the cube tunnel".to_owned(),
                format!(
                    "{along} along from {start:?} at {feet:?}; no cube within {CUBE_CLEAR}: {clear}"
                ),
            );
            return None;
        }
        if let Some(t) = self.tunnel_toward(
            w,
            target,
            ("the cube tunnel", "out of a cube pocket through rock"),
            tick,
        ) {
            return Some(t);
        }
        self.ender.home.cube_tunnel = None;
        self.note(
            tick,
            "home: no cube tunnel".to_owned(),
            format!("no way through rock from {feet:?} to {target:?}: the near cube is fought"),
        );
        None
    }

    /// Magma cubes pressing on the trip: with `CUBE_CROWD` within `CUBE_NEAR`, or any within
    /// `CUBE_CLEAR` at or under the danger line, a sealed pocket where the bot stands, held until
    /// none is within `CUBE_CLEAR` for `CUBE_CLEAR_TICKS`. Each pocket counted and noted.
    fn home_cubes(&mut self, w: &World, tick: u64, health: f32, danger: f32) -> Option<Task> {
        if !self.ender.home.top_up_over {
            return None;
        }
        let body = w.body?;
        if let Some(t) = self.home_cube_tunnel(w, tick) {
            return Some(t);
        }
        // >>> Q8: the guard first, mob or none.
        if let Some(t) = self.home_guard(w, tick) {
            return Some(t);
        }
        // <<< Q8
        if let Some(t) = self.home_rail(w, tick) {
            return Some(t);
        }
        let cubes: Vec<f64> = w
            .entities
            .values()
            .filter(|e| !e.dead && e.kind == data::EntityKind::MagmaCube)
            .map(|e| dist(e.pos, body.pos))
            .collect();
        let near = cubes.iter().filter(|d| **d <= CUBE_NEAR).count();
        let within_clear = cubes.iter().any(|d| *d <= CUBE_CLEAR);
        // Over the danger line, a pocket given up waits `CUBE_RESUME_TICKS` before another: cubes
        // sit outside a sealed pocket for good (race 45707489694 lanes 2 and 3 held 5,000 and
        // 8,000 ticks with cubes at 4 to 7).
        let resting = self.ender.home.cube_resume_at.is_some_and(|t| tick < t) && health > danger;
        let press = !resting && (near >= CUBE_CROWD || (within_clear && health <= danger));
        // >>> Q8: over the danger line by a lava edge no cube pocket: the guard and the sneak hold
        // the bot and it walks on (race 45758211666 lanes 3 and 4 sat 4,000 to 16,000 ticks in
        // pockets on the join's island over the sea; the breakout's tunnel found no rock there).
        if health > danger && near_lava_edge(w) {
            if press || self.ender.home.cube_pocket.is_some() {
                self.ender.home.cube_pocket = None;
                self.ender.home.cube_clear_since = None;
                self.ender.home.edge_pocket_skips += 1;
                if self.ender.home.edge_pocket_skips % 20 == 1 {
                    self.note(
                        tick,
                        "home: no cube pocket by a lava edge, on behind the guard".to_owned(),
                        format!(
                            "{near} within {CUBE_NEAR}; health {health} over {danger}; skip {}",
                            self.ender.home.edge_pocket_skips
                        ),
                    );
                }
            }
            return None;
        }
        // <<< Q8
        let held = self.ender.home.cube_pocket.is_some();
        if press && !held {
            self.ender.home.cube_pocket = Some(tick);
            self.ender.home.cube_pockets += 1;
            self.note(
                tick,
                "home: pocket from magma cubes".to_owned(),
                format!(
                    "{near} within {CUBE_NEAR}, {} within {CUBE_CLEAR}; health {health} (danger {danger}); pocket {}",
                    cubes.iter().filter(|d| **d <= CUBE_CLEAR).count(),
                    self.ender.home.cube_pockets
                ),
            );
        }
        let since = self.ender.home.cube_pocket?;
        // Held long enough over the danger line: on, the near one fought, the rest left to the
        // shun, and no pocket for a while.
        if health > danger && tick.saturating_sub(since) >= CUBE_HOLD_MAX {
            self.ender.home.cube_pocket = None;
            self.ender.home.cube_clear_since = None;
            self.ender.home.cube_resume_at = Some(tick + CUBE_RESUME_TICKS);
            self.ender.home.cube_breakouts += 1;
            self.note(
                tick,
                "home: out of the cube pocket, the cubes stay".to_owned(),
                format!(
                    "held {CUBE_HOLD_MAX} ticks; {near} within {CUBE_NEAR}; health {health} over {danger}; no pocket for {CUBE_RESUME_TICKS}; breakout {}",
                    self.ender.home.cube_breakouts
                ),
            );
            // Out through the rock on the side away from them, toward the next point: cubes do
            // not follow through rock (the coordinator: a fight at 14 to 17 with no heal can kill).
            let feet = path::feet_cell(body.pos);
            let aim = self.ender.home.aim.unwrap_or((feet.0, feet.2));
            let near_cubes: Vec<[f64; 3]> = w
                .entities
                .values()
                .filter(|e| {
                    !e.dead
                        && e.kind == data::EntityKind::MagmaCube
                        && dist(e.pos, body.pos) <= 12.0
                })
                .map(|e| e.pos)
                .collect();
            let dir = cube_tunnel_dir(feet, aim, &near_cubes);
            let target = (
                feet.0 + dir.0 * CUBE_TUNNEL_LEN,
                feet.1,
                feet.2 + dir.1 * CUBE_TUNNEL_LEN,
            );
            self.ender.home.cube_tunnel = Some((feet, target));
            self.ender.home.cube_tunnels += 1;
            self.note(
                tick,
                "home: left the cube pocket by tunnel".to_owned(),
                format!(
                    "on ({}, {}) toward {target:?}, away from {} cubes within 12, toward the point {aim:?}; tunnel {}",
                    dir.0,
                    dir.1,
                    near_cubes.len(),
                    self.ender.home.cube_tunnels
                ),
            );
            return self.home_cube_tunnel(w, tick);
        }
        if within_clear {
            self.ender.home.cube_clear_since = None;
        } else {
            let clear = *self.ender.home.cube_clear_since.get_or_insert(tick);
            if tick.saturating_sub(clear) >= CUBE_CLEAR_TICKS {
                self.ender.home.cube_pocket = None;
                self.ender.home.cube_clear_since = None;
                self.note(
                    tick,
                    "home: out of the cube pocket".to_owned(),
                    format!(
                        "none within {CUBE_CLEAR} for {CUBE_CLEAR_TICKS} ticks; held {} ticks; health {health}",
                        tick.saturating_sub(since)
                    ),
                );
                return None;
            }
        }
        if on_ledge(w) {
            // Never a pocket on a ledge (Q6's race 45727347488: knocked off one while sealing).
            self.ender.home.ledge_skips += 1;
            if self.ender.home.ledge_skips % 20 == 1 {
                self.note(
                    tick,
                    "home: no pocket on a ledge".to_owned(),
                    format!(
                        "air beside or under the feet; skip {}",
                        self.ender.home.ledge_skips
                    ),
                );
            }
            return None;
        }
        self.pocket_step(w, tick);
        self.retry_at = tick + 1;
        Some(Task::Idle)
    }

    /// The portal aimed at: memory's nearest, read again every `portal_every`; else geography.
    fn home_portal(&mut self, w: &World, tick: u64) -> Option<Cell> {
        let r = self.home_rules();
        let body = w.body?;
        let fresh = self
            .ender
            .home
            .portal_at
            .is_some_and(|t| tick < t + r.portal_every);
        if fresh {
            return self.ender.home.portal;
        }
        self.ender.home.portal_at = Some(tick);
        let (found, from) = match portal_far(w, NETHER, body.pos, r.portal_chunks) {
            Some(c) => (Some(c), "memory"),
            None => (r.portal.map(|[x, y, z]| (x, y, z)), "geography"),
        };
        if found != self.ender.home.portal {
            let feet = path::feet_cell(body.pos);
            self.note(
                tick,
                match found {
                    Some(c) => format!("home: the portal at {c:?} ({from})"),
                    None => "home: no portal known".to_owned(),
                },
                format!(
                    "from {feet:?}; {} off on the plane; memory read {} chunks round",
                    found.map_or(-1, |c| flat(feet, c)),
                    r.portal_chunks
                ),
            );
        }
        self.ender.home.portal = found;
        self.ender.home.portal_from = Some(from.to_owned());
        found
    }

    /// Pickaxe uses the trip wants held: `WOOD_SHORT_USES`, or within `WOOD_LEG_FROM` of the portal
    /// the last leg's own budget (`WOOD_USES_A_BLOCK` a block to go on the plane).
    fn wood_wanted_uses(&self, w: &World) -> i32 {
        if dimension(w) != Some(NETHER) {
            return WOOD_SHORT_USES;
        }
        let Some(body) = w.body else {
            return WOOD_SHORT_USES;
        };
        let feet = path::feet_cell(body.pos);
        let leg = self
            .ender
            .home
            .portal
            .map(|p| flat(feet, p))
            .filter(|d| *d <= WOOD_LEG_FROM)
            .map_or(0, |d| (f64::from(d) * WOOD_USES_A_BLOCK).ceil() as i32);
        WOOD_SHORT_USES.max(leg)
    }

    /// Wood by hand (the forest base's gather, reused): with the pickaxe uses held and the wood
    /// held for more under what the trip wants (`wood_wanted_uses`), a log or stem seen within
    /// `WOOD_REACH` (warped and crimson stems make planks too) is walked to and dug by hand, until
    /// `WOOD_GATHER` logs are held or none is seen; the trip's pickaxe runs make them pickaxes.
    /// Noted and counted (`wood_walks`), at most `WOOD_WALKS` a trip.
    fn home_wood(&mut self, w: &World, tick: u64) -> Option<Task> {
        if self.ender.home.wood_walks >= WOOD_WALKS {
            return None;
        }
        let want = self.wood_wanted_uses(w);
        let uses = endermen::pickaxe_uses_left(w);
        let logs = base::logs_held(w);
        // A log is four planks, about one wooden pickaxe (`WOOD_PICK_USES`) with its sticks.
        let promised = uses + WOOD_PICK_USES * (logs + base::planks_held(w) / 4);
        let gathering = self.ender.home.wood_gathering && logs < WOOD_GATHER;
        if promised >= want && !gathering {
            self.ender.home.wood_gathering = false;
            return None;
        }
        let body = w.body?;
        let log = self.nearest_log(w, body.pos, 0.0, tick)?;
        let feet = path::feet_cell(body.pos);
        let d = flat(feet, log);
        if d > WOOD_REACH {
            self.ender.home.wood_gathering = false;
            return None;
        }
        let mut walk = Walk::to_dig(log, tick);
        // A found route only: a partial one walks the bot back up its own stairs toward a stem it
        // cannot reach (race 45779137282: twelve walks NoPath from the last leg's tunnel, each
        // undoing its stairs).
        if !walk.route_now(w, tick) || walk.route != Some(path::Status::Found) {
            self.expanded += walk.expanded;
            // With pickaxe uses left, a tunnel or stairs to it (Q10's race 45783611857: the stems
            // by the portal seen, every walk-only route failed).
            if uses >= WOOD_TUNNEL_USES
                && let Some(t) = self.tunnel_toward(w, log, ("a stem", "wood by hand"), tick)
            {
                self.ender.home.wood_gathering = true;
                self.ender.home.wood_walks += 1;
                self.note(
                    tick,
                    format!("home: wood by tunnel to {log:?}"),
                    format!(
                        "no walk; pickaxe uses {uses}, {logs} logs held, {want} wanted; {d} off; walk {} of {WOOD_WALKS}",
                        self.ender.home.wood_walks
                    ),
                );
                return Some(t);
            }
            self.refuse(log, tick);
            return None;
        }
        self.ender.home.wood_gathering = true;
        self.ender.home.wood_walks += 1;
        self.walks += 1;
        self.note(
            tick,
            format!("home: wood by hand at {log:?}"),
            format!(
                "pickaxe uses {uses} and {logs} logs held, {want} wanted; {d} off; walk {} of {WOOD_WALKS}",
                self.ender.home.wood_walks
            ),
        );
        Some(Task::ToLog(walk))
    }

    /// The trip west: the heading's legs to the way's points and the portal, then the approach.
    fn home_trip(&mut self, w: &World, tick: u64) -> Option<Task> {
        let r = self.home_rules();
        let body = w.body?;
        let feet = path::stand_cell(w, body.pos);
        // Stone pickaxes on the way from the deltas' blackstone (`#stone_tool_materials`), with
        // the kit's table and sticks, once the stone uses fall under the base's (the coordinator,
        // 2026-11-25: the long way wears them; the walls are full of blackstone).
        // The last ground with filler to dig, every GROUND_EVERY ticks.
        if self
            .ender
            .home
            .ground_at
            .is_none_or(|t| tick >= t + GROUND_EVERY)
        {
            self.ender.home.ground_at = Some(tick);
            if !crate::heading::restock_cells(w, Some((1, 0)), crate::heading::RESTOCK_REACH)
                .is_empty()
            {
                self.ender.home.ground = Some(path::feet_cell(body.pos));
            }
        }
        // >>> Q8: wood by hand when the pickaxes run short (Q10's race 45768707240 lane 2: 17 off
        // the portal with 0 uses, the kit's stems spent, a crimson forest round it).
        if let Some(t) = self.home_wood(w, tick) {
            return Some(t);
        }
        // <<< Q8
        // Stone and wood both count (the coordinator, 2026-11-25): the kit's 16 stems are 64
        // planks, about 20 wooden pickaxes of 59 uses; stone from blackstone when held.
        let picking = (self.base.steps == base::CLIMB_STEPS.to_vec()
            || self.base.steps == TRIP_WOOD_STEPS.to_vec())
            && !self.base.over;
        let want = self.table.as_ref().map_or(100, |t| t.base.pickaxe_uses);
        let stone = craft::count(w, ItemKind::Blackstone)
            + craft::count(w, ItemKind::Cobblestone)
            + craft::count(w, ItemKind::CobbledDeepslate);
        let wood = base::logs_held(w) + base::planks_held(w);
        let uses = endermen::pickaxe_uses_left(w);
        let steps = if stone >= 3 && craft::count(w, ItemKind::Stick) >= 2 {
            Some(base::CLIMB_STEPS.to_vec())
        } else if wood >= 2 {
            Some(TRIP_WOOD_STEPS.to_vec())
        } else {
            None
        };
        if !picking
            && uses < want
            && (self.ender.home.trip_pick_runs < r.pick_runs || wood >= 2)
            && let Some(steps) = steps
        {
            self.ender.home.trip_pick_runs += 1;
            self.base = base::BaseState {
                steps,
                ..base::BaseState::default()
            };
            self.note(
                tick,
                format!("home: pickaxes on the way ({})", self.ender.home.trip_pick_runs),
                format!(
                    "pickaxe uses {uses} (stone and wood) under {want}; {} logs and planks, {} blackstone, {} cobblestone, {} sticks, {} tables",
                    wood,
                    craft::count(w, ItemKind::Blackstone),
                    craft::count(w, ItemKind::Cobblestone),
                    craft::count(w, ItemKind::Stick),
                    craft::count(w, ItemKind::CraftingTable)
                ),
            );
        }
        // >>> Q8: Q7's table rule on the trip: a table past `arrival::TABLE_REACH` is refused for the
        // run, so the carried one is placed where the bot stands, and dug back after (Q9's race
        // 45770564199: each run walked 35 to 50 back east to the forest's table at (494, 41, -7)).
        if let Some(t) = self.arrival_table(w, tick, picking_now(self)) {
            return Some(t);
        }
        // <<< Q8
        if picking_now(self)
            && let Some(t) = self.base_task(w, tick)
        {
            return Some(t);
        }
        let Some(portal) = self.home_portal(w, tick) else {
            self.retry_at = tick + crate::RETRY_TICKS * 10;
            return Some(Task::Idle);
        };
        let d = flat(feet, portal);
        if !self.ender.home.trip_begun {
            self.ender.home.trip_begun = true;
            // A fresh heading for the trip (the forest's is over).
            self.headed = None;
            let s = self.home_state(w);
            self.note(
                tick,
                format!("home: the trip begun toward the portal at {portal:?}"),
                format!(
                    "{d} off on the plane ({}); {} way points; {s}",
                    self.ender.home.portal_from.as_deref().unwrap_or("?"),
                    r.way.len()
                ),
            );
        }
        if self
            .ender
            .home
            .noted_at
            .is_none_or(|t| tick >= t + r.note_every)
        {
            self.ender.home.noted_at = Some(tick);
            let s = self.home_state(w);
            self.note(
                tick,
                "home: on the way".to_owned(),
                format!(
                    "at {feet:?} in {:?}, {d} from the portal {portal:?}; {s}",
                    w.feet_biome()
                ),
            );
        }
        // >>> Q11 (issue 19): the fortress stretch's rules (docs/problems/eyes-fortress.md).
        if let Some(t) = self.fortress_way(w, tick, portal, d) {
            return Some(t);
        }
        // <<< Q11
        // >>> Q6 (issue 19): within `[portal_way] reach` and out of the approach's, the last leg
        // in rock to the front's tunnel cell (docs/problems/eyes-portal.md).
        if d > r.approach
            && let Some(t) = self.portal_way(w, tick, portal, d)
        {
            return Some(t);
        }
        // <<< Q6
        if let Some(t) = self.home_bridge(w, tick, &r) {
            return Some(t);
        }
        if d <= r.approach {
            return self.home_approach(w, tick, portal);
        }
        // The points passed are Home's, not the heading's: a heading turned out is dropped, and
        // race 45707489694 lane 3, past (490, 150) and 140 blocks on, was sent back east to it.
        for c in self
            .headed
            .as_ref()
            .map(|h| h.ways.clone())
            .unwrap_or_default()
        {
            if !self.ender.home.ways_passed.contains(&c) {
                self.ender.home.ways_passed.push(c);
            }
        }
        let passed = self.ender.home.ways_passed.clone();
        let next = r
            .way
            .iter()
            .map(|[x, z]| (*x, *z))
            .chain(std::iter::once((portal.0, portal.2)))
            .map(|p| ((p.0 >> 4, p.1 >> 4), p))
            .find(|(c, _)| !passed.contains(c))
            // The portal's own point stays the aim after its column is reached.
            .or(Some(((portal.0 >> 4, portal.2 >> 4), (portal.0, portal.2))));
        self.ender.home.aim = next.map(|(_, p)| p);
        let signs = Signs {
            // The forest at the feet would end the heading: the trip begins in it.
            feet_biome: w.feet_biome().filter(|b| *b != "warped_forest"),
            forest: None,
            valley: None,
            way: next,
        };
        let t = self.heading_with(w, signs, tick);
        for c in self
            .headed
            .as_ref()
            .map(|h| h.ways.clone())
            .unwrap_or_default()
        {
            if !self.ender.home.ways_passed.contains(&c) {
                self.ender.home.ways_passed.push(c);
            }
        }
        if let Some(t) = t {
            return Some(t);
        }
        // The heading turned out: a far walk toward the portal, then a fresh heading.
        self.headed = None;
        let goal = Goal::Near {
            pos: portal,
            range: f64::from(r.approach),
        };
        let mut walk = Walk::far_to(portal, goal, tick);
        if walk.route_now(w, tick) {
            self.walks += 1;
            self.note(
                tick,
                "home: far walk toward the portal".to_owned(),
                format!("the heading is over; {d} off"),
            );
            return Some(Task::Explore(walk));
        }
        self.expanded += walk.expanded;
        // Stranded short of filler (on its own bridge, race 45715674818 lane 3 at (293, 38, 188)
        // with 15, one under the 16 a far search needs to bridge at all): back to the last ground
        // that had filler to dig, where the heading's restock runs.
        if path::bridge_blocks(w) < crate::heading::HOME_KEEP + crate::heading::RESTOCK_BRIDGES
            && let Some(g) = self.ender.home.ground
            && flat(g, feet) > 2
        {
            let mut back = Walk::new(g, Goal::Near { pos: g, range: 1.5 }, Arrive::Goal, tick);
            if back.route_now(w, tick) && back.route == Some(path::Status::Found) {
                self.walks += 1;
                self.ender.home.ground_walks += 1;
                self.note(
                    tick,
                    format!("home: back to ground at {g:?} for filler"),
                    format!(
                        "no way on from {feet:?} with filler {}; walk {}",
                        path::bridge_blocks(w),
                        self.ender.home.ground_walks
                    ),
                );
                return Some(Task::Explore(back));
            }
            self.expanded += back.expanded;
        }
        // No ground known (or no way to it) and under what a far search needs to bridge: the
        // bridge's own last blocks behind the bot are dug back, never the one under the feet, for
        // a short retreat to land (the coordinator, 2026-11-25).
        if path::bridge_blocks(w) < path::BRIDGE_RESERVE
            && let Some(body) = w.body
        {
            let eye = [body.pos[0], body.pos[1] + 1.62, body.pos[2]];
            let under = (feet.0, feet.1 - 1, feet.2);
            let mut cells = path::Cells::new(w);
            let pick = self.ender.home.laid.iter().rev().copied().find(|c| {
                *c != under
                    && cells.at(*c) == path::Class::Solid
                    && (0..3)
                        .map(|i| (eye[i] - (f64::from([c.0, c.1, c.2][i]) + 0.5)).powi(2))
                        .sum::<f64>()
                        .sqrt()
                        <= crate::heading::RESTOCK_REACH
            });
            if let Some(c) = pick {
                self.ender.home.laid.retain(|l| *l != c);
                self.ender.home.unbridged += 1;
                self.note(
                    tick,
                    format!("home: dig back own bridge block at {c:?}"),
                    format!(
                        "stranded at {feet:?} with filler {}; dig {} of the run",
                        path::bridge_blocks(w),
                        self.ender.home.unbridged
                    ),
                );
                return Some(Task::Dig(crate::dig::Dig::new(w, c, tick)));
            }
        }
        self.retry_at = tick + crate::RETRY_TICKS * 10;
        self.note(
            tick,
            "home: no way on toward the portal".to_owned(),
            format!("the heading is over and no far walk from {feet:?}; {d} off"),
        );
        Some(Task::Idle)
    }

    /// Within `approach` of the portal: the walk into it; else a far walk beside it (the far
    /// search bridges lava with held blocks); else a tunnel to it. Each try noted.
    fn home_approach(&mut self, w: &World, tick: u64, portal: Cell) -> Option<Task> {
        let body = w.body?;
        let feet = path::stand_cell(w, body.pos);
        let mut cells = portal_cells(w, NETHER, portal);
        if cells.is_empty() {
            cells.push(portal);
        }
        let lit: Vec<Cell> = cells
            .iter()
            .copied()
            .filter(|c| {
                w.block_at(world::BlockPos::new(c.0, c.1, c.2)) == Some(BlockKind::NetherPortal)
            })
            .collect();
        let loaded = w.block_at(world::BlockPos::new(portal.0, portal.1, portal.2));
        if loaded.is_some() && lit.is_empty() {
            self.note(
                tick,
                format!("home: the portal at {portal:?} is out"),
                format!(
                    "its cell holds {loaded:?}; flint and steel held {}",
                    craft::count(w, ItemKind::FlintAndSteel)
                ),
            );
            if let Some(t) = self.home_light(w, tick, portal) {
                return Some(t);
            }
        }
        // Lava beside or under the portal's feet cells (the row in front of it turned to lava:
        // docs/problems/nether-walk.md) is sealed with filler from where the bot stands first:
        // no route enters a cell with lava beside it.
        // The face with a floor is the only way in: the walk in starts on its side, the far walk
        // and the tunnel go to the cell behind its standing cell (the entry portal's north face
        // has no floor over the lava sea).
        let front = portal_front(w, &lit, body.pos);
        // >>> Q6 (issue 19): the lava by the standing cell's head first (`crate::portal_way`).
        if let Some(t) = self.head_seal(w, tick, front.as_ref()) {
            return Some(t);
        }
        // <<< Q6
        if let Some(t) = self.home_seal(w, tick, &lit, front.as_ref()) {
            return Some(t);
        }
        let on_side = front.is_none_or(|f| out_of_plane(&f, feet) >= 1);
        let open: Vec<Cell> = lit
            .iter()
            .copied()
            .filter(|c| on_side && !self.refused_now(*c, tick))
            .collect();
        let (goal_cell, goal_what) = match front {
            Some(f) => (
                f.tunnel,
                format!("{:?}, behind the standing cell {:?}", f.tunnel, f.stand),
            ),
            None => (portal, format!("{portal:?}, no face with a floor seen")),
        };
        for c in open {
            let mut walk = Walk::new(c, Goal::Block { pos: c }, Arrive::Goal, tick);
            if walk.route_now(w, tick) && walk.route == Some(path::Status::Found) {
                self.walks += 1;
                self.ender.home.enters += 1;
                let s = self.home_state(w);
                self.note(
                    tick,
                    format!("home: enter the portal at {c:?}"),
                    format!("enter {} from {feet:?}; {s}", self.ender.home.enters),
                );
                return Some(Task::Enter(Box::new(crate::Enter {
                    walk,
                    cell: c,
                    inside_since: None,
                    from: Some(NETHER.to_owned()),
                })));
            }
            self.expanded += walk.expanded;
        }
        // >>> Q6 (issue 19): no seal due and no plain walk in: the last leg in rock to the front's
        // tunnel cell, then exactly to it (`crate::portal_way`), before the far walk and the tunnel.
        if let Some(t) = self.portal_way(w, tick, portal, flat(feet, portal)) {
            return Some(t);
        }
        // <<< Q6
        self.ender.home.approach_tries += 1;
        let n = self.ender.home.approach_tries;
        // No face with a floor seen: a far walk to beside the portal, bridging the lava in front
        // of it. With one, never: a bridge can cross to the face without a floor.
        if front.is_none() && n % 2 == 1 {
            let goal = Goal::Near {
                pos: portal,
                range: 1.5,
            };
            let mut walk = Walk::far_to(portal, goal, tick);
            if walk.route_now(w, tick) {
                self.walks += 1;
                self.note(
                    tick,
                    format!("home: approach {n} by far walk to {portal:?}"),
                    format!(
                        "from {feet:?}, {} to bridge, no plain way into the portal",
                        walk.bridges_ahead()
                    ),
                );
                return Some(Task::Explore(walk));
            }
            self.expanded += walk.expanded;
        }
        // A tunnel to the cell behind the standing cell (else to the portal): the tunnel's stairs
        // refuse lava beside a cell they open.
        if let Some(t) = self.tunnel_toward(
            w,
            goal_cell,
            ("the portal", "the way home's approach: no walk in"),
            tick,
        ) {
            self.note(
                tick,
                format!("home: approach {n} by tunnel to {goal_cell:?}"),
                format!("from {feet:?} to {goal_what}; on the face's side {on_side}"),
            );
            return Some(t);
        }
        self.retry_at = tick + crate::RETRY_TICKS * 5;
        self.note(
            tick,
            format!("home: approach {n} found no way"),
            format!("from {feet:?} to {goal_what}: no walk, far walk or tunnel"),
        );
        Some(Task::Idle)
    }

    /// One filler block into a lava cell beside or under a portal feet cell (the lowest of `lit`),
    /// within reach of the eye, clicked on a full block beside it; noted "home: lava sealed at".
    /// `None` with no such lava in reach, no filler held, or `SEALS` placed.
    fn home_seal(
        &mut self,
        w: &World,
        tick: u64,
        lit: &[Cell],
        front: Option<&Front>,
    ) -> Option<Task> {
        let body = w.body?;
        if self.ender.home.seals >= SEALS {
            return None;
        }
        let low = lit.iter().map(|c| c.1).min()?;
        let eye = [body.pos[0], body.pos[1] + 1.62, body.pos[2]];
        let reach = |c: Cell| {
            (0..3)
                .map(|i| (eye[i] - (f64::from([c.0, c.1, c.2][i]) + 0.5)).powi(2))
                .sum::<f64>()
                .sqrt()
                <= SEAL_REACH
        };
        let mut cells = path::Cells::new(w);
        let around = |c: &Cell| {
            [(1, 0, 0), (-1, 0, 0), (0, 0, 1), (0, 0, -1), (0, -1, 0)]
                .map(|(dx, dy, dz)| (c.0 + dx, c.1 + dy, c.2 + dz))
        };
        // The standing cells in front of the portal: open cells beside its two lowest rows. The
        // entry portal's one dry standing cell is (22, 35, -19), with lava at (21, 35, -19) beside
        // it, under its neighbour (23, 35, -19) and at (24, 35, -19) beyond (Q1's probe
        // 45688112814 of the frontier snapshot's region files).
        let stand: Vec<Cell> = lit
            .iter()
            .filter(|c| c.1 == low || c.1 == low + 1)
            .flat_map(|c| around(c).into_iter().take(4))
            .filter(|n| !lit.contains(n))
            .filter(|n| front.is_none_or(|f| out_of_plane(f, *n) >= 1))
            .filter(|n| cells.at(*n) == path::Class::Clear)
            .collect();
        // With a front known, the lava behind the portal (the entry portal's north lava sea) is
        // never sealed: the bot never stands there.
        let mut lava: Vec<Cell> = Vec::new();
        for n in lit
            .iter()
            .filter(|c| c.1 == low)
            .chain(stand.iter())
            .flat_map(around)
            .filter(|n| front.is_none_or(|f| out_of_plane(f, *n) >= 0))
        {
            if !lava.contains(&n) && cells.at(n) == path::Class::Lava {
                lava.push(n);
            }
        }
        let feet = path::feet_cell(body.pos);
        let item = endermen::FILLER
            .iter()
            .copied()
            .find(|k| craft::count(w, *k) > 0)?;
        // A lava fall is sealed from the top down: a cell with lava over it is replaced by the
        // highest lava cell of its column (Q6's race 45742933645: the fall's foot sealed at
        // (24, 35, -19), the fall spread a level up and into the standing cell, 19.8 to 0.9);
        // a column whose top is out of reach is not touched.
        let lava: Vec<Cell> = lava
            .into_iter()
            .filter_map(|c| {
                let mut top = c;
                while cells.at((top.0, top.1 + 1, top.2)) == path::Class::Lava {
                    top = (top.0, top.1 + 1, top.2);
                }
                reach(top).then_some(top)
            })
            .collect();
        let (target, click, face) = lava
            .into_iter()
            .filter(|c| reach(*c) && *c != feet)
            .find_map(|c| {
                [
                    ((c.0, c.1 - 1, c.2), 1),
                    ((c.0 + 1, c.1, c.2), 4),
                    ((c.0 - 1, c.1, c.2), 5),
                    ((c.0, c.1, c.2 + 1), 2),
                    ((c.0, c.1, c.2 - 1), 3),
                    ((c.0, c.1 + 1, c.2), 0),
                ]
                .into_iter()
                .find(|(n, _)| cells.at(*n) == path::Class::Solid)
                .map(|(n, f)| (c, n, f))
            })?;
        if w.window.is_some()
            || w.click.is_some()
            || !self.acts.is_empty()
            || tick < self.shelter_at
        {
            return Some(Task::Idle);
        }
        match crate::hold::toward(w, item) {
            crate::hold::Hold::Held => {
                self.shelter_at = tick + crate::SHELTER_EVERY_TICKS;
                self.ender.home.seals += 1;
                self.acts.push(world::Act::UseOn {
                    pos: world::BlockPos::new(click.0, click.1, click.2),
                    face,
                });
                self.note(
                    tick,
                    format!("home: lava sealed at {target:?}"),
                    format!(
                        "beside the portal's feet cells; {item:?} on {click:?} face {face}; seal {} of {SEALS}",
                        self.ender.home.seals
                    ),
                );
                self.retry_at = tick + 1;
                Some(Task::Idle)
            }
            crate::hold::Hold::Act(a) => {
                self.acts.push(a);
                Some(Task::Idle)
            }
            crate::hold::Hold::Wait => Some(Task::Idle),
            crate::hold::Hold::Missing => None,
        }
    }

    /// The portal is out: flint and steel on the frame's bottom obsidian, from where the bot
    /// stands within reach, else a walk to beside it. `None` with no flint and steel held.
    fn home_light(&mut self, w: &World, tick: u64, portal: Cell) -> Option<Task> {
        if craft::count(w, ItemKind::FlintAndSteel) == 0 {
            return None;
        }
        let body = w.body?;
        let base_cell = (portal.0, portal.1 - 1, portal.2);
        if w.block_at(world::BlockPos::new(base_cell.0, base_cell.1, base_cell.2))
            != Some(BlockKind::Obsidian)
        {
            return None;
        }
        let eye = [body.pos[0], body.pos[1] + 1.62, body.pos[2]];
        let d = (0..3)
            .map(|i| {
                (eye[i] - (f64::from([base_cell.0, base_cell.1, base_cell.2][i]) + 0.5)).powi(2)
            })
            .sum::<f64>()
            .sqrt();
        if d > crate::walk::REACH {
            let goal = Goal::Near {
                pos: portal,
                range: 2.5,
            };
            let mut walk = Walk::far_to(portal, goal, tick);
            if walk.route_now(w, tick) {
                self.walks += 1;
                self.note(
                    tick,
                    "home: to the portal's frame to light it".to_owned(),
                    format!("{d:.1} from its bottom obsidian {base_cell:?}"),
                );
                return Some(Task::Explore(walk));
            }
            return None;
        }
        match crate::hold::toward(w, ItemKind::FlintAndSteel) {
            crate::hold::Hold::Held => {
                self.acts.push(world::Act::UseOn {
                    pos: world::BlockPos::new(base_cell.0, base_cell.1, base_cell.2),
                    face: 1,
                });
                self.retry_at = tick + crate::RETRY_TICKS;
                self.note(
                    tick,
                    format!("home: the portal lit at {base_cell:?}"),
                    "flint and steel on the frame's bottom obsidian, top face".to_owned(),
                );
                Some(Task::Idle)
            }
            crate::hold::Hold::Act(a) => {
                self.acts.push(a);
                Some(Task::Idle)
            }
            crate::hold::Hold::Wait => Some(Task::Idle),
            crate::hold::Hold::Missing => None,
        }
    }

    /// On the overworld's surface with fewer than `WOOD_SHORT_USES` pickaxe uses: logs by hand
    /// (`home_wood`), then wooden pickaxes from them at the carried table (`TRIP_WOOD_STEPS`, a
    /// far table refused). Noted; `None` with the uses held or nothing to make them from.
    fn home_surface_tools(&mut self, w: &World, tick: u64) -> Option<Task> {
        let uses = endermen::pickaxe_uses_left(w);
        let picking = picking_now(self);
        if uses >= WOOD_SHORT_USES && !picking {
            return None;
        }
        if !picking {
            if let Some(t) = self.home_wood(w, tick) {
                return Some(t);
            }
            let wood = base::logs_held(w) + base::planks_held(w);
            if wood < 2 {
                return None;
            }
            self.ender.home.surface_pick_runs += 1;
            self.base = base::BaseState {
                steps: TRIP_WOOD_STEPS.to_vec(),
                ..base::BaseState::default()
            };
            self.note(
                tick,
                format!(
                    "home: pickaxes on the surface ({})",
                    self.ender.home.surface_pick_runs
                ),
                format!(
                    "pickaxe uses {uses} under {WOOD_SHORT_USES}; {} logs, {} planks, {} sticks, {} tables",
                    base::logs_held(w),
                    base::planks_held(w),
                    craft::count(w, ItemKind::Stick),
                    craft::count(w, ItemKind::CraftingTable)
                ),
            );
        }
        if let Some(t) = self.arrival_table(w, tick, picking_now(self)) {
            return Some(t);
        }
        if picking_now(self) {
            return self.base_task(w, tick);
        }
        None
    }

    /// Out of the portal's own cell: the first cell beside the feet (south, north, east, west)
    /// that is open with its head cell, on a full floor, and not a portal block, walked to. Noted
    /// and counted; `None` with none.
    fn home_out_of_portal(&mut self, w: &World, tick: u64, feet: Cell) -> Option<Task> {
        let mut cells = path::Cells::new(w);
        let portal = |c: Cell| {
            w.block_at(world::BlockPos::new(c.0, c.1, c.2)) == Some(BlockKind::NetherPortal)
        };
        let out = [(0, 1), (0, -1), (1, 0), (-1, 0)]
            .into_iter()
            .map(|(dx, dz)| (feet.0 + dx, feet.1, feet.2 + dz))
            .find(|c| {
                !portal(*c)
                    && cells.at(*c) == path::Class::Clear
                    && cells.at((c.0, c.1 + 1, c.2)) == path::Class::Clear
                    && cells.at((c.0, c.1 - 1, c.2)) == path::Class::Solid
            })
            // Else the south face (Q1's region read: the overworld portal's way out, z -107), open
            // at the feet and the head and no lava under it.
            .or_else(|| {
                let c = (feet.0, feet.1, feet.2 + 1);
                (!portal(c)
                    && cells.at(c) == path::Class::Clear
                    && cells.at((c.0, c.1 + 1, c.2)) == path::Class::Clear
                    && cells.at((c.0, c.1 - 1, c.2)) != path::Class::Lava)
                    .then_some(c)
            })
            // Over a drop (F2, race 45796865676: the room south of the portal floors at y -54,
            // so (125, -52, -107) stands on nothing and the walk to it was refused 162 times):
            // the landing under it, down to `stairs::MAX_DROP`, is the target.
            .map(|c| {
                (0..crate::stairs::MAX_DROP)
                    .map(|d| (c.0, c.1 - d, c.2))
                    .find(|l| cells.at((l.0, l.1 - 1, l.2)) == path::Class::Solid)
                    .unwrap_or(c)
            })?;
        let mut walk = Walk::new(
            out,
            Goal::Near {
                pos: out,
                range: 0.5,
            },
            Arrive::Goal,
            tick,
        );
        if !walk.route_now(w, tick) || walk.route != Some(path::Status::Found) {
            self.expanded += walk.expanded;
            // A plain step forward: the route is the one cell (F1, race 45791152151: six bots in
            // the portal, the step-out found no route from a portal block).
            walk = Walk::new(
                out,
                Goal::Near {
                    pos: out,
                    range: 0.5,
                },
                Arrive::Goal,
                tick,
            );
            walk.waypoints = Some(vec![out]);
            walk.more.kinds = vec![path::MoveKind::Walk];
            walk.route = Some(path::Status::Found);
        }
        self.walks += 1;
        self.ender.home.portal_exits += 1;
        self.note(
            tick,
            format!("home: out of the portal to {out:?}"),
            format!(
                "from {feet:?}, a portal block; exit {}",
                self.ender.home.portal_exits
            ),
        );
        Some(Task::Explore(walk))
    }

    /// The overworld: the climb by stairs in rock, then food, the night's hut, a rest; then over.
    fn home_overworld(&mut self, w: &World, tick: u64) -> Option<Task> {
        let r = self.home_rules();
        let body = w.body?;
        let feet = path::feet_cell(body.pos);
        if self.ender.home.done {
            // >>> overland and stronghold (Q4): over, with eyes held or makeable (pearls and
            // powder or rods) or a room known, the eyes' phase takes the planner
            // (`eyes_stronghold_task` crafts the eyes and holds the overworld); race 45701314227
            // idled 4,300 ticks here with 14 eyes and 12 frames on record.
            let count = |k: ItemKind| craft::count(w, k);
            let makeable = count(ItemKind::EnderPearl) > 0
                && (count(ItemKind::BlazePowder) > 0 || count(ItemKind::BlazeRod) > 0);
            if count(ItemKind::EnderEye) > 0 || makeable || self.stronghold.room.is_some() {
                return None;
            }
            // <<< overland and stronghold
            self.retry_at = tick + crate::RETRY_TICKS;
            return Some(Task::Idle);
        }
        if self.ender.home.arrived_at.is_none() {
            self.ender.home.arrived_at = Some(tick);
            // The climb's own counters start fresh.
            self.surface_target = None;
            self.surface_tries = 0;
            self.toward = None;
            let s = self.home_state(w);
            self.note(
                tick,
                format!("home: in the overworld at {feet:?}"),
                format!(
                    "{} ticks after Home began; open sky {}; {s}",
                    tick.saturating_sub(self.ender.home.since.unwrap_or(tick)),
                    world::stage::open_sky(w)
                ),
            );
        }
        // >>> Q8 (issue 19): standing in the portal's own cell, out of it first, by its south face
        // first (Q11's race 45771600648: three bots stood in the portal at (125, -52, -108), "no
        // stairs up from here", every heading refused, the shaft never begun).
        if self.ender.home.surface_at.is_none()
            && w.block_at(world::BlockPos::new(feet.0, feet.1, feet.2))
                == Some(BlockKind::NetherPortal)
            && let Some(t) = self.home_out_of_portal(w, tick, feet)
        {
            return Some(t);
        }
        // <<< Q8
        let rules = self.table.as_ref().map(|t| t.survival.clone())?;
        if self.ender.home.surface_at.is_none() {
            if world::stage::open_sky(w) {
                self.ender.home.surface_at = Some(tick);
                let s = self.home_state(w);
                self.note(
                    tick,
                    format!("home: on the surface at {feet:?}"),
                    format!(
                        "{} ticks after the arrival; {s}",
                        tick.saturating_sub(self.ender.home.arrived_at.unwrap_or(tick))
                    ),
                );
            } else {
                return self.home_climb(w, tick, &r);
            }
        }
        // >>> Q8: tools before the hand-over (the frontier bot reaches the surface with 0 pickaxe uses;
        // Q4's dig needs a pickaxe and its stone ones need cobblestone, which a hand dig never drops):
        // wood by hand from the trees seen, then wooden pickaxes at the carried table.
        if let Some(t) = self.home_surface_tools(w, tick) {
            return Some(t);
        }
        // <<< Q8
        // >>> Q7: on the surface by day, food 6 or more: over, the eyes take over (the done
        // branch above: Q4's exit with eyes held).
        match self.arrival_hand_over(w, tick) {
            Some(Some(t)) => return Some(t),
            Some(None) => return self.home_overworld(w, tick),
            None => {}
        }
        // <<< Q7
        // The night: a hut where the bot stands, one a night, and the night waited out in it.
        let dusk = self.dusk_near(w, tick) || w.is_night() == Some(true);
        if dusk {
            let day = w.day_time().unwrap_or(0) / 24_000;
            if self.ender.home.hut_night != Some(day) {
                self.ender.home.hut_night = Some(day);
                self.hut_done = false;
                self.hut_blocks = 0;
                // A cook a night: its task budget and its end are the night's.
                self.cook_over = false;
                self.cook_tasks = 0;
                self.ender.home.cook_noted = false;
                self.note(
                    tick,
                    "home: a hut for the night".to_owned(),
                    format!("day time {:?}; at {feet:?}", w.day_time()),
                );
            }
            if let Some(t) = self.hut_step(w, tick) {
                return Some(t);
            }
            if let Some(t) = self.home_cook(w, tick, &rules, true) {
                return Some(t);
            }
        } else {
            self.dusk_noted = false;
        }
        let food = w.player.food.unwrap_or(0);
        // By day the raw meat is cooked first (8 food a piece against 3), and a log fetched for
        // the fuel when short; under COOK_FOOD_FLOOR it is eaten as it is.
        if !dusk
            && food >= COOK_FOOD_FLOOR
            && let Some(t) = self.home_cook(w, tick, &rules, false)
        {
            return Some(t);
        }
        if food < r.fed {
            if let Some(f) = eat::best_food(w, &rules) {
                self.note(
                    tick,
                    format!("home: eat {}", f.name().unwrap_or_default()),
                    format!("food {food} under {}", r.fed),
                );
                return Some(Task::Eat(eat::Eat::new(w, f, tick)));
            }
            if let Some(h) = self.choose_prey(w, &rules, tick) {
                self.note(
                    tick,
                    format!("home: hunt {:?} {}", h.kind, h.prey),
                    format!("food {food} under {} and nothing safe to eat held", r.fed),
                );
                return Some(Task::Hunt(Box::new(h)));
            }
            if dusk {
                self.retry_at = tick + crate::RETRY_TICKS * 10;
                return Some(Task::Idle);
            }
            // >>> Q7: by day, far legs east for animals (fresh land), before the look round.
            if let Some(t) = self.arrival_food_walk(w, tick) {
                return Some(t);
            }
            // <<< Q7
            if self.ender.home.animal_walks < r.animal_walks {
                self.ender.home.animal_walks += 1;
                let why = format!(
                    "home: look for animals ({} of {}), food {food} under {}",
                    self.ender.home.animal_walks, r.animal_walks, r.fed
                );
                if let Some(t) = self.explore_walk(w, tick, &why) {
                    return Some(t);
                }
            }
            self.retry_at = tick + crate::RETRY_TICKS * 10;
            return Some(Task::Idle);
        }
        let fed_at = *self.ender.home.fed_at.get_or_insert(tick);
        if fed_at == tick {
            let s = self.home_state(w);
            self.note(
                tick,
                "home: fed".to_owned(),
                format!("food {food} of {} at {feet:?}; {s}", r.fed),
            );
        }
        let health = w.player.health.unwrap_or(0.0);
        if health < r.heal_to && tick.saturating_sub(fed_at) < r.heal_ticks {
            self.retry_at = tick + crate::RETRY_TICKS * 5;
            return Some(Task::Idle);
        }
        self.ender.home.done = true;
        let s = self.home_state(w);
        self.note(
            tick,
            "home: over, fed on the surface".to_owned(),
            format!(
                "at {feet:?}, health {health}, food {food}; {} ticks since Home began; {s}",
                tick.saturating_sub(self.ender.home.since.unwrap_or(tick))
            ),
        );
        Some(Task::Idle)
    }

    /// The night's cook in the hut (overworld-leg wall 4's cook phase, its `cook-*` rows): with
    /// raw meat held and the fuel to cook all of it (overworld planks, logs as their four planks,
    /// coal; warped wood is no fuel), table and furnace placed where the bot stands and the meat
    /// smelted, cooked meat 8 food a piece against raw's 3. `None` with nothing to cook, the fuel
    /// short (noted), or the night's cook over. (E2, issue 20c: the End kit's cook too.)
    pub(crate) fn home_cook(
        &mut self,
        w: &World,
        tick: u64,
        rules: &crate::make::SurvivalRules,
        night: bool,
    ) -> Option<Task> {
        let counts = crate::held(w);
        let raw: i32 = eat::raw_held(&counts, rules).iter().map(|r| r.1).sum();
        if self.cook_over || (raw == 0 && self.batch.is_none()) {
            if self.ender.home.cook_noted && !self.ender.home.cook_ended {
                self.ender.home.cook_ended = true;
                self.note(
                    tick,
                    "home: the cook over".to_owned(),
                    format!(
                        "{raw} raw left, {} food points held, after {} cook tasks",
                        eat::food_points(w, rules),
                        self.cook_tasks
                    ),
                );
            }
            return None;
        }
        let fuel = fuel_ticks_held(&counts);
        let need = raw * 200;
        if self.batch.is_none() && fuel < need {
            // By day: a log from a tree seen within FUEL_LOG_REACH, at most FUEL_WALKS walks.
            if !night
                && self.ender.home.fuel_walks < FUEL_WALKS
                && let Some(t) = self.home_fuel_log(w, tick, raw, need, fuel)
            {
                return Some(t);
            }
            if !self.ender.home.cook_noted {
                self.ender.home.cook_noted = true;
                self.ender.home.cook_ended = true;
                self.note(
                    tick,
                    "home: no cook, the fuel short".to_owned(),
                    format!(
                        "{raw} raw pieces want {need} ticks of fuel; {fuel} held or made of sticks; {} log walks",
                        self.ender.home.fuel_walks
                    ),
                );
            }
            return None;
        }
        if !self.ender.home.cook_noted {
            self.ender.home.cook_noted = true;
            self.ender.home.cook_ended = false;
            self.note(
                tick,
                format!("home: cook {}", if night { "in the hut" } else { "by day" }),
                format!(
                    "{raw} raw pieces, {fuel} ticks of fuel, {} furnaces and {} tables held",
                    craft::count(w, ItemKind::Furnace),
                    craft::count(w, ItemKind::CraftingTable)
                ),
            );
        }
        let origin = w.body?.pos;
        self.home_cook = true;
        let t = self.decide(w, origin, tick);
        self.home_cook = false;
        Some(t)
    }

    /// A walk to dig the nearest log seen within `FUEL_LOG_REACH` that burns, for the cook's fuel.
    fn home_fuel_log(
        &mut self,
        w: &World,
        tick: u64,
        raw: i32,
        need: i32,
        fuel: i32,
    ) -> Option<Task> {
        let body = w.body?;
        let log = self.nearest_log(w, body.pos, 0.0, tick)?;
        let feet = path::feet_cell(body.pos);
        let d = f64::from((log.0 - feet.0).pow(2) + (log.2 - feet.2).pow(2)).sqrt();
        let burns = w
            .block_at(world::BlockPos::new(log.0, log.1, log.2))
            .and_then(|b| b.name())
            .and_then(ItemKind::from_name)
            .and_then(data::generated::fuel::fuel_ticks)
            .is_some();
        if d > FUEL_LOG_REACH || !burns {
            return None;
        }
        self.ender.home.fuel_walks += 1;
        let mut walk = Walk::to_dig(log, tick);
        if !walk.route_now(w, tick) {
            self.expanded += walk.expanded;
            self.refuse(log, tick);
            return None;
        }
        self.walks += 1;
        self.note(
            tick,
            format!("home: a log for the cook's fuel at {log:?}"),
            format!(
                "{raw} raw pieces want {need} ticks of fuel, {fuel} held; {d:.0} off; walk {} of {FUEL_WALKS}",
                self.ender.home.fuel_walks
            ),
        );
        Some(Task::ToLog(walk))
    }

    /// Up to the open sky by stairs in rock (overworld-leg wall 1: never through the caves),
    /// stone pickaxes made from the stairs' own cobblestone when the stone uses run low.
    fn home_climb(&mut self, w: &World, tick: u64, r: &HomeRules) -> Option<Task> {
        if !self.ender.home.climb_begun {
            self.ender.home.climb_begun = true;
            self.ender.home.climb_from = w.body.map(|b| path::feet_cell(b.pos));
            let s = self.home_state(w);
            self.note(
                tick,
                "home: the climb begun".to_owned(),
                format!("stairs up in rock to the open sky, never the caves; {s}"),
            );
        }
        // Pickaxes from the pack: a table placed, then stone ones to the base's pickaxe uses.
        let climbing = self.base.steps == base::CLIMB_STEPS.to_vec() && !self.base.over;
        let low = base::reserve_pick_uses(w) < r.climb_pick_uses
            && craft::count(w, ItemKind::Cobblestone) + craft::count(w, ItemKind::CobbledDeepslate)
                >= 3;
        if !climbing && low && self.ender.home.pick_runs < r.pick_runs {
            self.ender.home.pick_runs += 1;
            self.base = base::BaseState {
                steps: base::CLIMB_STEPS.to_vec(),
                ..base::BaseState::default()
            };
            self.note(
                tick,
                format!(
                    "home: pickaxes for the climb ({})",
                    self.ender.home.pick_runs
                ),
                format!(
                    "stone pickaxe uses {} under {}; {} cobblestone, {} sticks, {} logs, {} planks",
                    base::reserve_pick_uses(w),
                    r.climb_pick_uses,
                    craft::count(w, ItemKind::Cobblestone),
                    craft::count(w, ItemKind::Stick),
                    base::logs_held(w),
                    base::planks_held(w)
                ),
            );
        }
        // >>> Q7: the run's table where the bot stands (a far one refused), taken along after; then
        // a shaft straight up where one is safe (crates/plan/src/arrival.rs).
        let running = self.base.steps == base::CLIMB_STEPS.to_vec() && !self.base.over;
        if let Some(t) = self.arrival_table(w, tick, running) {
            return Some(t);
        }
        // <<< Q7
        if self.base.steps == base::CLIMB_STEPS.to_vec()
            && !self.base.over
            && let Some(t) = self.base_task(w, tick)
        {
            return Some(t);
        }
        // >>> Q7
        if let Some(t) = self.arrival_climb(w, tick) {
            return Some(t);
        }
        // <<< Q7
        if let Some(mut t) = self.stairs_to_surface(w, tick) {
            // Dry and in rock: a stair breaking into a cave or beside water turns its heading
            // (the cave band, y -57 to 33 round the overworld portal: Q1's probe 45688112814).
            if let Task::Stairs(s) = &mut t {
                s.dry = true;
                s.rock = self.ender.home.climb_from;
            }
            return Some(t);
        }
        // Every heading refused: begin the round again later.
        self.surface_tries = 0;
        self.surface_target = None;
        self.retry_at = tick + crate::RETRY_TICKS * 10;
        self.note(
            tick,
            "home: no stairs up from here".to_owned(),
            format!(
                "every heading refused at {:?}; again shortly",
                w.body.map(|b| path::feet_cell(b.pos))
            ),
        );
        Some(Task::Idle)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn in_dimension(name: &str) -> World {
        let mut w = World::new();
        w.dimension = Some(world::Dimension {
            type_id: 0,
            name: name.to_owned(),
            min_y: Some(0),
            height: Some(256),
        });
        w
    }

    fn enter(from: Option<&str>) -> crate::Enter {
        let c = (0, 64, 0);
        crate::Enter {
            walk: Walk::new(c, Goal::Block { pos: c }, Arrive::Goal, 0),
            cell: c,
            inside_since: None,
            from: from.map(str::to_owned),
        }
    }

    fn done(e: &mut crate::Enter, w: &World) -> bool {
        matches!(e.step(w, 1).1, crate::walk::TaskStatus::Done)
    }

    #[test]
    fn an_enter_is_done_in_any_dimension_but_its_own() {
        // From the Nether: done in the overworld, not while still in the Nether.
        let mut e = enter(Some(NETHER));
        assert!(!done(&mut e, &in_dimension(NETHER)));
        assert!(done(&mut e, &in_dimension(OVERWORLD)));
        // From the overworld (the old way in): done in the Nether, not in the overworld.
        let mut e = enter(Some(OVERWORLD));
        assert!(!done(&mut e, &in_dimension(OVERWORLD)));
        assert!(done(&mut e, &in_dimension(NETHER)));
        // No start on record reads as the overworld's, as before the field.
        let mut e = enter(None);
        assert!(done(&mut e, &in_dimension(NETHER)));
        assert!(!done(&mut e, &in_dimension(OVERWORLD)));
    }
}
