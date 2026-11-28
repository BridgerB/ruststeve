//! The iron search (brain/ruststeve/design/iron-search.md, docs/problems/iron-search.md): what an
//! iron row does at its depth with no exposed iron in sight, before the old level tunnel.
//!
//! The old tunnel was the same tunnel every time: `Stairs::level` on the held heading, which a
//! tunnel that ends "done" never turns. At a face it could not cut it turned inside the task, back
//! into its own open tunnel, and walked it, each step counted as a stair: 2,727 of 3,711 "cut 6
//! stairs" tunnels of race 46130982111 ended in under 100 ticks with nothing dug, and rs-l00-b00
//! walked one six-block corridor for its last 13,600 ticks. In order, the search:
//!
//! 1. **Rises to the band**: feet under `rise_below` cut stairs up (deepslate digs twice as slow,
//!    and the middle band thins to nothing at y -24).
//! 2. **Takes a cave seen**: open cells in a cross with room over them and a floor under them (a
//!    one-wide tunnel never makes one), beyond the 24 blocks `exposed_near` reads and within
//!    `cave_reach`: walked to, or dug toward with no route. Its walls are then read from inside.
//! 3. **Cuts a strip in fresh rock**: only on a heading whose next `fresh_ahead` columns are rock
//!    and whose first step is not refused. A strip ends after `strip_len` blocks, or after
//!    `strip_stalls` tunnels in a row no further along, and turns a quarter, left and right in turn.
//! 4. **With no fresh rock on any heading**, stairs to another level, never the shuttle.
//!
//! Strict no X-ray: only open cells and blocks with an open face are read (the rule of
//! `exposed_near`, `tunnel.rs` and `Stairs::rock`); ore is still only taken through `exposed_near`.
//! Every choice writes a ledger note.

use data::{BlockKind, ItemKind};
use world::{BlockPos, World};

use crate::path::{self, Cell, Goal};
use crate::stairs::{self, Stairs};
use crate::walk::{Arrive, Walk};
use crate::{Plan, Task};

/// `[iron]` in `config/goals.toml`, each number with its source there; the defaults are the
/// config's (a test holds them equal).
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct IronRules {
    /// Feet under this height cut stairs up toward the band.
    pub rise_below: i32,
    /// Blocks a strip runs on one heading before it turns.
    pub strip_len: i32,
    /// Tunnels in a row no further along the strip before it turns.
    pub strip_stalls: u32,
    /// Columns ahead that must be rock for a heading to count as fresh.
    pub fresh_ahead: i32,
    /// A cave nearer than this on the plane is not sought (its walls are already in view).
    pub cave_from: i32,
    /// The farthest a cave is sought, on the plane.
    pub cave_reach: i32,
    /// Heights a cave is sought between (iron's middle band).
    pub cave_low: i32,
    pub cave_high: i32,
    /// A cave within this of one already taken is the same cave.
    pub cave_apart: i32,
    /// Ticks between two cave searches.
    pub cave_every: u64,
    /// Dig-toward tasks spent on one cave with no route before it is given up.
    pub cave_cuts: u32,
}

impl Default for IronRules {
    fn default() -> Self {
        Self {
            rise_below: 0,
            strip_len: 24,
            strip_stalls: 2,
            fresh_ahead: 3,
            cave_from: 24,
            cave_reach: 48,
            cave_low: -24,
            cave_high: 56,
            cave_apart: 16,
            cave_every: 200,
            cave_cuts: 8,
        }
    }
}

/// A strip in hand: its heading, where it began, the furthest along it the feet have been, the
/// tunnels in a row no further, and the raw iron held when it began.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Strip {
    pub dir: (i32, i32),
    pub from: Cell,
    pub best: i32,
    pub stalls: u32,
    pub raw_at: i32,
}

/// How often each path fired, for the case file and the tests.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct IronCounts {
    pub rises: u32,
    pub caves_walked: u32,
    pub caves_cut: u32,
    pub strips: u32,
    pub turned_open: u32,
    pub turned_empty: u32,
    pub turned_stalled: u32,
    pub no_fresh: u32,
}

/// The search's state, kept on the planner.
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct IronSearch {
    pub strip: Option<Strip>,
    /// The next strip's turn is to the left (they alternate, so strips step across new ground).
    pub left: bool,
    /// Caves taken (walked to, reached or given up), oldest first, at most `CAVES_KEPT`.
    pub caves: Vec<Cell>,
    /// The cave being dug toward and the tasks spent on it.
    pub cave_aim: Option<(Cell, u32)>,
    /// The tick the next cave search may run.
    pub cave_next: u64,
    pub counts: IronCounts,
}

/// Caves remembered as taken: a run reaches a few dozen at most.
pub const CAVES_KEPT: usize = 64;

/// What the strip planner chose, with the ledger notes it owes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StripPlan {
    /// A level tunnel on this heading.
    Cut((i32, i32)),
    /// No heading has fresh rock ahead.
    NoFresh,
}

/// The iron rows: their blocks name iron ore.
pub fn is_iron_row(blocks: &[BlockKind]) -> bool {
    blocks
        .iter()
        .any(|b| matches!(b, BlockKind::IronOre | BlockKind::DeepslateIronOre))
}

fn kind(w: &World, (x, y, z): Cell) -> Option<BlockKind> {
    w.block_at(BlockPos::new(x, y, z))
}

fn open(k: Option<BlockKind>) -> bool {
    matches!(
        k,
        Some(BlockKind::Air | BlockKind::CaveAir | BlockKind::VoidAir)
    )
}

fn full(k: Option<BlockKind>) -> bool {
    k.and_then(data::generated::shapes::collision) == Some(data::types::Collision::Full)
}

/// Feet under `rise_below`: cut stairs up toward the band first.
pub fn below_band(feet: Cell, rules: &IronRules) -> bool {
    feet.1 < rules.rise_below
}

/// With no fresh rock on any heading, the stairs go down (`true`) while the feet are more than a
/// staircase (six) over the rise line and the row's depth, else up: either way to another level's
/// rock.
pub fn no_fresh_down(feet: Cell, depth: Option<i32>, rules: &IronRules) -> bool {
    let floor = rules.rise_below + stairs::STAIR_DEPTH as i32;
    feet.1 > depth.map_or(floor, |d| d.min(floor))
}

/// A quarter turn to the right (east to south), as the stairs turn.
pub fn right((dx, dz): (i32, i32)) -> (i32, i32) {
    (-dz, dx)
}

/// A quarter turn to the left.
pub fn left((dx, dz): (i32, i32)) -> (i32, i32) {
    (dz, -dx)
}

/// A heading is fresh from `feet` when the feet and head cells of its next `ahead` columns are
/// rock (loaded, not open, no fluid: a full block) and its first level step is not refused by the
/// stairs' own rules. Only whether a cell is open is read past the first step: no X-ray.
pub fn fresh(w: &World, feet: Cell, dir: (i32, i32), ahead: i32) -> bool {
    let (x, y, z) = feet;
    let (dx, dz) = dir;
    let to = (x + dx, y, z + dz);
    let cells = [(x + dx, y + 1, z + dz), to];
    if stairs::refusal(w, &cells, (to.0, to.1 - 1, to.2)).is_some() {
        return false;
    }
    (1..=ahead).all(|k| {
        [0, 1].iter().all(|h| {
            let c = (x + dx * k, y + h, z + dz * k);
            w.columns
                .contains_key(&BlockPos::new(c.0, c.1, c.2).chunk())
                && full(kind(w, c))
        })
    })
}

/// The first fresh heading of `pref`, then a quarter to each side, then back.
pub fn fresh_heading(w: &World, feet: Cell, pref: (i32, i32), ahead: i32) -> Option<(i32, i32)> {
    [pref, right(pref), left(pref), right(right(pref))]
        .into_iter()
        .find(|d| fresh(w, feet, *d, ahead))
}

/// A cave cell: open with open over it, a full block under it, and open cells on all four sides
/// on the plane. A one-wide tunnel or staircase never has that cross; a cave, a ravine's floor
/// or a big room does.
pub fn cave_cell(w: &World, (x, y, z): Cell) -> bool {
    open(kind(w, (x, y, z)))
        && open(kind(w, (x, y + 1, z)))
        && full(kind(w, (x, y - 1, z)))
        && [(1, 0), (-1, 0), (0, 1), (0, -1)]
            .iter()
            .all(|(dx, dz)| open(kind(w, (x + dx, y, z + dz))))
}

/// The nearest cave cell from `cave_from` to `cave_reach` blocks off on the plane (Chebyshev,
/// every second column), within `SEARCH_HEIGHT` of the feet and inside the band, and not within
/// `cave_apart` of a cave already taken.
pub fn cave_near(w: &World, feet: Cell, rules: &IronRules, taken: &[Cell]) -> Option<Cell> {
    let (fx, fy, fz) = feet;
    let lo = (fy - crate::SEARCH_HEIGHT).max(rules.cave_low);
    let hi = (fy + crate::SEARCH_HEIGHT).min(rules.cave_high);
    let near_taken = |c: Cell| {
        taken.iter().any(|t| {
            (t.0 - c.0)
                .abs()
                .max((t.1 - c.1).abs())
                .max((t.2 - c.2).abs())
                <= rules.cave_apart
        })
    };
    // Ring by ring outward, so the first ring with a cave holds the nearest few.
    for r in (rules.cave_from..=rules.cave_reach).step_by(2) {
        let mut found: Vec<(i32, Cell)> = Vec::new();
        for dx in (-r..=r).step_by(2) {
            for dz in (-r..=r).step_by(2) {
                if dx.abs().max(dz.abs()) != r {
                    continue;
                }
                for y in lo..=hi {
                    let c = (fx + dx, y, fz + dz);
                    if cave_cell(w, c) && !near_taken(c) {
                        found.push((dx * dx + dz * dz + (y - fy).pow(2), c));
                    }
                }
            }
        }
        found.sort();
        if let Some((_, c)) = found.first() {
            return Some(*c);
        }
    }
    None
}

impl IronSearch {
    /// The strip's next tunnel from `feet`, with the notes to write. A strip the feet have left
    /// (more than a block off its line or its height) is ended; one that ran `strip_len` or made
    /// no progress for `strip_stalls` tunnels is ended and turned; a heading with open cells
    /// ahead is turned from.
    pub fn next_strip(
        &mut self,
        w: &World,
        feet: Cell,
        pref: (i32, i32),
        raw: i32,
        rules: &IronRules,
    ) -> (StripPlan, Vec<(String, String)>) {
        let mut notes = Vec::new();
        let mut turn_to = None;
        if let Some(mut s) = self.strip.take() {
            let (ox, oz) = (feet.0 - s.from.0, feet.2 - s.from.2);
            let along = ox * s.dir.0 + oz * s.dir.1;
            let side = ox * s.dir.1 - oz * s.dir.0;
            let gave = raw - s.raw_at;
            if side.abs() > 1 || (feet.1 - s.from.1).abs() > 1 {
                notes.push((
                    format!("strip left after {} blocks", s.best),
                    format!(
                        "the feet at {feet:?} are off its line from {:?}; it gave {gave} raw iron",
                        s.from
                    ),
                ));
            } else {
                if along > s.best {
                    s.best = along;
                    s.stalls = 0;
                } else {
                    s.stalls += 1;
                }
                let turn = if self.left { left(s.dir) } else { right(s.dir) };
                if s.best >= rules.strip_len {
                    self.counts.turned_empty += 1;
                    self.left = !self.left;
                    turn_to = Some(turn);
                    notes.push((
                        format!("strip turned after {} blocks", s.best),
                        format!(
                            "no exposed iron in sight along it; it gave {gave} raw iron; turn to {turn:?}"
                        ),
                    ));
                } else if s.stalls >= rules.strip_stalls {
                    self.counts.turned_stalled += 1;
                    self.left = !self.left;
                    turn_to = Some(turn);
                    notes.push((
                        format!("strip turned, stalled at {} blocks", s.best),
                        format!(
                            "{} tunnels in a row no further along; it gave {gave} raw iron; turn to {turn:?}",
                            s.stalls
                        ),
                    ));
                } else if fresh(w, feet, s.dir, rules.fresh_ahead) {
                    self.strip = Some(s);
                    return (StripPlan::Cut(s.dir), notes);
                } else {
                    self.counts.turned_open += 1;
                    turn_to = Some(s.dir);
                    notes.push((
                        format!("strip turned from open cells ahead at {} blocks", s.best),
                        format!("no fresh rock on {:?} from {feet:?}", s.dir),
                    ));
                }
            }
        }
        let want = turn_to.unwrap_or(pref);
        let Some(dir) = fresh_heading(w, feet, want, rules.fresh_ahead) else {
            self.counts.no_fresh += 1;
            notes.push((
                "no fresh rock on any heading".to_owned(),
                format!(
                    "every heading from {feet:?} is open, fluid or refused within {} blocks",
                    rules.fresh_ahead
                ),
            ));
            return (StripPlan::NoFresh, notes);
        };
        self.counts.strips += 1;
        self.strip = Some(Strip {
            dir,
            from: feet,
            best: 0,
            stalls: 0,
            raw_at: raw,
        });
        notes.push((
            format!("strip begun on {dir:?} at {feet:?}"),
            format!(
                "no exposed iron in sight; fresh rock ahead ({} strips this run)",
                self.counts.strips
            ),
        ));
        (StripPlan::Cut(dir), notes)
    }

    fn take_cave(&mut self, c: Cell) {
        self.caves.push(c);
        if self.caves.len() > CAVES_KEPT {
            self.caves.remove(0);
        }
    }
}

impl Plan {
    /// The iron search at the row's depth (module docs); `None` leaves the old tunnel.
    pub(crate) fn iron_search(
        &mut self,
        w: &World,
        pref: (i32, i32),
        depth: Option<i32>,
        tick: u64,
    ) -> Option<Task> {
        let rules = IronRules::default();
        let body = w.body?;
        let feet = path::feet_cell(body.pos);
        let raw = crate::craft::count(w, ItemKind::RawIron);
        if below_band(feet, &rules) {
            self.iron.strip = None;
            self.iron.counts.rises += 1;
            self.note(
                tick,
                format!("rise toward iron's band from y {}", feet.1),
                format!(
                    "no exposed iron in sight, the feet under y {}: deepslate digs twice as slow and the band thins",
                    rules.rise_below
                ),
            );
            return Some(Task::Stairs(Box::new(Stairs::up(pref, tick))));
        }
        if let Some(t) = self.iron_cave(w, feet, &rules, tick) {
            self.iron.strip = None;
            return Some(t);
        }
        let (plan, notes) = self.iron.next_strip(w, feet, pref, raw, &rules);
        for (what, why) in notes {
            self.note(tick, what, why);
        }
        match plan {
            StripPlan::Cut(dir) => Some(Task::Stairs(Box::new(Stairs::level(dir, tick)))),
            StripPlan::NoFresh => {
                let down = no_fresh_down(feet, depth, &rules);
                self.note(
                    tick,
                    format!("stairs {} for fresh rock", if down { "down" } else { "up" }),
                    format!("no fresh rock on any heading from {feet:?}"),
                );
                Some(Task::Stairs(Box::new(if down {
                    Stairs::new(pref, tick)
                } else {
                    Stairs::up(pref, tick)
                })))
            }
        }
    }

    /// A cave seen beyond `exposed_near`'s reach: walked into, or dug toward with no route.
    fn iron_cave(&mut self, w: &World, feet: Cell, rules: &IronRules, tick: u64) -> Option<Task> {
        let held = match self.iron.cave_aim {
            Some((c, n)) if n < rules.cave_cuts => Some((c, n)),
            Some((c, n)) => {
                self.iron.cave_aim = None;
                self.iron.take_cave(c);
                self.note(
                    tick,
                    format!("cave at {c:?} given up"),
                    format!("{n} cuts toward it and no route yet"),
                );
                None
            }
            None => None,
        };
        let (aim, cuts) = match held {
            Some(h) => h,
            None => {
                if tick < self.iron.cave_next {
                    return None;
                }
                self.iron.cave_next = tick + rules.cave_every;
                (cave_near(w, feet, rules, &self.iron.caves)?, 0)
            }
        };
        let flat = (aim.0 - feet.0).abs().max((aim.2 - feet.2).abs());
        if flat <= 3 && (aim.1 - feet.1).abs() <= 2 {
            self.iron.cave_aim = None;
            self.iron.take_cave(aim);
            self.note(
                tick,
                format!("in the cave at {aim:?}"),
                "its walls are in view now; look along it".to_owned(),
            );
            return None;
        }
        let mut walk = Walk::new(aim, Goal::Block { pos: aim }, Arrive::Goal, tick);
        if walk.route_now(w, tick) {
            self.walks += 1;
            self.iron.cave_aim = None;
            self.iron.take_cave(aim);
            self.iron.counts.caves_walked += 1;
            self.note(
                tick,
                format!("walk into the cave at {aim:?}"),
                format!("no exposed iron in sight; a cave seen {flat} blocks off, with a route"),
            );
            return Some(Task::Explore(walk));
        }
        self.expanded += walk.expanded;
        match self.dig_toward(
            w,
            aim,
            true,
            ("a cave", "a cave seen with no walk to it"),
            tick,
        ) {
            Some(t) => {
                self.iron.cave_aim = Some((aim, cuts + 1));
                self.iron.counts.caves_cut += 1;
                self.note(
                    tick,
                    format!("dig toward a cave at {aim:?}"),
                    format!(
                        "no exposed iron in sight; a cave seen {flat} blocks off with no route (cut {} of {})",
                        cuts + 1,
                        rules.cave_cuts
                    ),
                );
                Some(t)
            }
            None => {
                self.iron.cave_aim = None;
                self.iron.take_cave(aim);
                self.note(
                    tick,
                    format!("cave at {aim:?} refused"),
                    "no route and no cut toward it".to_owned(),
                );
                None
            }
        }
    }
}
