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
            way: Vec::new(),
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
    /// Overworld: when the bot arrived, when it reached the surface, when it was fed.
    #[serde(default)]
    pub arrived_at: Option<u64>,
    #[serde(default)]
    pub climb_begun: bool,
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
    #[serde(default)]
    pub done: bool,
}

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
                self.ender.phase = EnderPhase::Home;
                self.ender.home = HomeState {
                    since: Some(tick),
                    ..HomeState::default()
                };
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
        self.home_trip(w, tick)
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
        let passed = self
            .headed
            .as_ref()
            .map(|h| h.ways.clone())
            .unwrap_or_default();
        let next = r
            .way
            .iter()
            .map(|[x, z]| (*x, *z))
            .chain(std::iter::once((portal.0, portal.2)))
            .map(|p| ((p.0 >> 4, p.1 >> 4), p))
            .find(|(c, _)| !passed.contains(c))
            // The portal's own point stays the aim after its column is reached.
            .or(Some(((portal.0 >> 4, portal.2 >> 4), (portal.0, portal.2))));
        let signs = Signs {
            // The forest at the feet would end the heading: the trip begins in it.
            feet_biome: w.feet_biome().filter(|b| *b != "warped_forest"),
            forest: None,
            valley: None,
            way: next,
        };
        if let Some(t) = self.heading_with(w, signs, tick) {
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
        let open: Vec<Cell> = lit
            .iter()
            .copied()
            .filter(|c| !self.refused_now(*c, tick))
            .collect();
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
        // A far walk to beside the portal: bridges over the lava in front of it.
        if n % 2 == 1 {
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
        // A tunnel at the portal's floor level from the dry side: the tunnel's stairs refuse
        // lava beside a cell they open.
        if let Some(t) = self.tunnel_toward(
            w,
            portal,
            ("the portal", "the way home's approach: no walk in"),
            tick,
        ) {
            self.note(
                tick,
                format!("home: approach {n} by tunnel to {portal:?}"),
                format!("from {feet:?}"),
            );
            return Some(t);
        }
        self.retry_at = tick + crate::RETRY_TICKS * 5;
        self.note(
            tick,
            format!("home: approach {n} found no way"),
            format!("from {feet:?} to {portal:?}: no walk, far walk or tunnel"),
        );
        Some(Task::Idle)
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
                self.note(
                    tick,
                    "home: a hut for the night".to_owned(),
                    format!("day time {:?}; at {feet:?}", w.day_time()),
                );
            }
            if let Some(t) = self.hut_step(w, tick) {
                return Some(t);
            }
        } else {
            self.dusk_noted = false;
        }
        let food = w.player.food.unwrap_or(0);
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

    /// Up to the open sky by stairs in rock (overworld-leg wall 1: never through the caves),
    /// stone pickaxes made from the stairs' own cobblestone when the stone uses run low.
    fn home_climb(&mut self, w: &World, tick: u64, r: &HomeRules) -> Option<Task> {
        if !self.ender.home.climb_begun {
            self.ender.home.climb_begun = true;
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
        if let Some(t) = self.stairs_to_surface(w, tick) {
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
