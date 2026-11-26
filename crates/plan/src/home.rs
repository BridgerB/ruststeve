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
                [128, 165],
                [118, 186],
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
            pick_runs: 8,
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
/// The trip's wooden pickaxes from the kit's stems: a table placed, then wooden pickaxes to the
/// base's wood uses (150).
pub const TRIP_WOOD_STEPS: [base::Step; 2] = [base::Step::Table, base::Step::WoodPicks];

fn picking_now(p: &Plan) -> bool {
    (p.base.steps == base::CLIMB_STEPS.to_vec() || p.base.steps == TRIP_WOOD_STEPS.to_vec())
        && !p.base.over
}

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
            self.pocket_step(w, tick);
            self.retry_at = tick + 1;
            return Some(Task::Idle);
        }
        if health < rules.travel_health && endermen::seen_by_far_shooter(w) {
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
            && self.ender.home.trip_pick_runs < r.pick_runs
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

    /// The overworld: the climb by stairs in rock, then food, the night's hut, a rest; then over.
    fn home_overworld(&mut self, w: &World, tick: u64) -> Option<Task> {
        let r = self.home_rules();
        let body = w.body?;
        let feet = path::feet_cell(body.pos);
        if self.ender.home.done {
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
    /// short (noted), or the night's cook over.
    fn home_cook(
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
        if self.base.steps == base::CLIMB_STEPS.to_vec()
            && !self.base.over
            && let Some(t) = self.base_task(w, tick)
        {
            return Some(t);
        }
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
