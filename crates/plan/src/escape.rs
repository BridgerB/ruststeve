//! Escapes (issue 12b, brain/ruststeve/design/escapes.md): the always-rows `escape-water`,
//! `escape-lava` and `escape-fire` (clojurecraft's `trap.clj` and `swim.clj`).
//!
//! - **The trap**, re-derived every tick with hysteresis (`Traps::tick`): lava on any contact; a
//!   fire block the body touches; water once the eyes have been under for `TRAP_AFTER_TICKS` or
//!   the body afloat for `AFLOAT_AFTER_TICKS`. A water or lava trap holds until dry land has been
//!   held `DRY_TICKS_TO_CLEAR` ticks (a one-tick bob is not out).
//! - **Water** (`swim-out`): one bank, chosen once and held (the siblings re-picked every tick
//!   and swam in circles): the nearest standable dry cell within `BANK_RADIUS` on the plane, from
//!   `BANK_DROP` below the water's surface to `BANK_RISE` above it. Forward and jump into it: the
//!   physics' out-of-liquid impulse, from pressing into the bank with free space over it, is the
//!   only lift out of water. No progress for `STUCK_TICKS` → dig what is in the way (a lid over
//!   the head, then the wall at head height toward the bank; never the block at feet height, the
//!   bank the impulse needs), else the bank is refused and another chosen. No bank at all → carve a
//!   notch two high into a wall just over the water's surface (`NOTCH_RADIUS`).
//! - **Lava** (`flee`): the nearest standable cell with no lava beside or under it within
//!   `FLEE_RADIUS`, at once, forward and jump.
//! - **Fire**: the nearest standable cell within `FIRE_RADIUS` (a fire cell is never standable).

use data::BlockKind;
use physics::Controls;
use world::{Act, BlockPos, World};

use crate::dig::{Dig, DigStatus};
use crate::path::{Cell, Cells, Class, feet_cell};
use crate::walk::yaw_to;

/// The eyes under water this long is a trap (clojurecraft's `trap-after-ticks`); wading is not.
pub const TRAP_AFTER_TICKS: u32 = 20;
/// Off the ground over water this long (3 s) is a trap, the eyes in air or not.
pub const AFLOAT_AFTER_TICKS: u32 = 60;
/// Dry land held this long ends a water or lava trap (steve's settle).
pub const DRY_TICKS_TO_CLEAR: u32 = 6;
/// How far on the plane a bank is looked for (clojurecraft's `bank-radius`).
pub const BANK_RADIUS: i32 = 16;
/// A bank's feet at most this far above the water's surface cell (`bank-rise`) ...
pub const BANK_RISE: i32 = 1;
/// ... or this far below it: a roofed pool's air pocket is sideways and down (`bank-drop`).
pub const BANK_DROP: i32 = 3;
/// Ticks without progress toward the bank before digging what is in the way (`stuck-ticks`).
pub const STUCK_TICKS: u64 = 40;
/// Blocks nearer the bank that count as progress (`progress`).
pub const PROGRESS: f64 = 0.05;
/// With no bank, look again this often (`search-every-ticks`).
pub const SEARCH_EVERY_TICKS: u64 = 20;
/// How far on the plane a wall is notched into when there is no bank (`notch-radius`).
pub const NOTCH_RADIUS: i32 = 4;
/// How far a lava flee looks for footing (`flee-radius`).
pub const FLEE_RADIUS: i32 = 6;
/// How far a fire escape looks for footing: a step or two.
pub const FIRE_RADIUS: i32 = 2;
/// How far up a column the water's surface is looked for.
const SURFACE_SEARCH: i32 = 24;
/// A swim off an island lands at least this far away, and at most this.
pub const SWIM_FROM: i32 = 10;
pub const SWIM_TO: i32 = 32;
/// Trail entries kept.
const TRAIL: usize = 12;
/// Banks refused before the list starts over.
const MAX_REFUSED: usize = 8;

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
pub enum TrapKind {
    Water,
    Lava,
    Fire,
    /// The eyes inside a full block (in_wall damage: a fallen block, issue 13b).
    Wall,
}

impl TrapKind {
    pub fn parse(s: &str) -> Option<TrapKind> {
        match s {
            "water" => Some(TrapKind::Water),
            "lava" => Some(TrapKind::Lava),
            "fire" => Some(TrapKind::Fire),
            "wall" => Some(TrapKind::Wall),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            TrapKind::Water => "water",
            TrapKind::Lava => "lava",
            TrapKind::Fire => "fire",
            TrapKind::Wall => "wall",
        }
    }
}

/// The trap counters, the planner's state.
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Traps {
    pub wet: u32,
    pub afloat: u32,
    pub dry: u32,
    pub kind: Option<TrapKind>,
}

fn kind_at(w: &World, (x, y, z): Cell) -> Option<BlockKind> {
    w.block_at(BlockPos::new(x, y, z))
}

fn is_water(w: &World, c: Cell) -> bool {
    matches!(
        kind_at(w, c),
        Some(BlockKind::Water | BlockKind::BubbleColumn)
    )
}

fn is_lava(w: &World, c: Cell) -> bool {
    kind_at(w, c) == Some(BlockKind::Lava)
}

/// Lava at the door: over the head, or beside the head or the feet, the first such cell. It is
/// in the body's cells within a second (blaze.md wall 22: Nether lava ran down a bot's own
/// stairs and took 20 health in 80 ticks, the trap firing only once the body was in it).
///
/// And lava one cell higher beside the head's column (the ring at y + 2), last: it falls beside
/// the head in 10 ticks and is on the feet in 30 (nether-walk.md: race 45204423376 lane 5 stood
/// at (29, 34, -22) with lava spreading at y 36 over (30, 36, -22) and (29, 36, -21); the check
/// saw nothing until it was beside the head, and the body burned 20 ticks later).
pub fn lava_door(feet: Cell, lava_at: impl Fn(Cell) -> bool) -> Option<Cell> {
    let (x, y, z) = feet;
    let beside = |h: i32| [(x + 1, h, z), (x - 1, h, z), (x, h, z + 1), (x, h, z - 1)];
    std::iter::once((x, y + 2, z))
        .chain(beside(y + 1))
        .chain(beside(y))
        .chain(beside(y + 2))
        .find(|c| lava_at(*c))
}

/// `lava_door` on the world.
pub fn lava_door_at(w: &World, feet: Cell) -> Option<Cell> {
    lava_door(feet, |c| is_lava(w, c))
}

/// Seals tried against lava at the door before the escape flees without one.
pub const MAX_LAVA_SEALS: u32 = 16;

fn full(k: Option<BlockKind>) -> bool {
    k.and_then(data::generated::shapes::collision) == Some(data::types::Collision::Full)
}

/// Does the body's box touch a fire block?
pub fn touching_fire(w: &World, pos: [f64; 3]) -> bool {
    let b = physics::aabb(pos);
    let e = 0.001;
    let f = |v: f64| v.floor() as i32;
    (f(b[0] + e)..=f(b[3] - e)).any(|x| {
        (f(b[1] + e)..=f(b[4] - e)).any(|y| {
            (f(b[2] + e)..=f(b[5] - e)).any(|z| {
                matches!(
                    kind_at(w, (x, y, z)),
                    Some(BlockKind::Fire | BlockKind::SoulFire)
                )
            })
        })
    })
}

/// On the ground, no part of the body in water, the eyes in air, a full block under the feet
/// (clojurecraft's `dry-land?`).
pub fn dry_land(w: &World, body: &physics::Body) -> bool {
    let [x, y, z] = body.pos;
    let under = (x.floor() as i32, (y - 0.5).floor() as i32, z.floor() as i32);
    // Soul sand is land too (a box 0.875 high): a bot out of Nether lava onto it is out.
    let land = full(kind_at(w, under)) || kind_at(w, under) == Some(BlockKind::SoulSand);
    body.on_ground && !body.in_water && !body.eye_in_water && land
}

/// The eyes in a full block (vanilla's `isInWall` takes the eye's cell).
pub fn eye_in_wall(w: &World, body: &physics::Body) -> bool {
    let eye = (
        body.pos[0].floor() as i32,
        (body.pos[1] + physics::EYE_HEIGHT).floor() as i32,
        body.pos[2].floor() as i32,
    );
    full(kind_at(w, eye))
}

/// Off the ground with water in the body or in the cells at or under the feet.
fn afloat(w: &World, body: &physics::Body) -> bool {
    let (x, y, z) = feet_cell(body.pos);
    !body.on_ground && (body.in_water || is_water(w, (x, y, z)) || is_water(w, (x, y - 1, z)))
}

impl Traps {
    /// The trap this tick (clojurecraft's `trapped-tick`).
    pub fn tick(&mut self, w: &World, body: &physics::Body, door: bool) -> Option<TrapKind> {
        let at_door = door && lava_door(feet_cell(body.pos), |c| is_lava(w, c)).is_some();
        self.wet = if body.eye_in_water { self.wet + 1 } else { 0 };
        self.afloat = if afloat(w, body) { self.afloat + 1 } else { 0 };
        self.dry = if dry_land(w, body) { self.dry + 1 } else { 0 };
        let held = self
            .kind
            .filter(|k| *k != TrapKind::Fire && self.dry < DRY_TICKS_TO_CLEAR);
        self.kind = if body.in_lava || at_door {
            Some(TrapKind::Lava)
        } else if self.wet >= TRAP_AFTER_TICKS || self.afloat >= AFLOAT_AFTER_TICKS {
            Some(TrapKind::Water)
        } else if touching_fire(w, body.pos) {
            Some(TrapKind::Fire)
        } else if eye_in_wall(w, body) {
            Some(TrapKind::Wall)
        } else {
            held
        };
        self.kind
    }
}

/// An escape in hand.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Escape {
    pub kind: TrapKind,
    /// The cell the feet are headed for: a bank, footing, or a notch being cut.
    pub target: Option<Cell>,
    pub refused: Vec<Cell>,
    /// The best distance to the target so far, and when.
    pub best: f64,
    pub best_at: u64,
    pub next_search: u64,
    /// A block being dug out of the way, and the notch's cells still to dig.
    pub dig: Option<Dig>,
    #[serde(default)]
    pub queue: Vec<Cell>,
    pub digs: u32,
    pub started: u64,
    pub yaw: f64,
    /// A bucket being filled before the swim out (wall 10), and whether one was tried.
    #[serde(default)]
    pub fill: Option<Box<crate::scoop::Scoop>>,
    #[serde(default)]
    pub fill_tried: bool,
    /// Seals sent over the head against water pouring down a shaft (wall 11), and the last one.
    #[serde(default)]
    pub seals: u32,
    #[serde(default)]
    pub sealed_at: u64,
    /// What the escape chose and why, the last `TRAIL` entries: printed when it stalls (an escape
    /// held 25,000 ticks beside a dry cell with nothing in the ledger to say why: race 45085497535).
    #[serde(default)]
    pub trail: Vec<String>,
    /// How many long-escape reports the planner has written (one each `ESCAPE_REPORT_TICKS`).
    #[serde(default)]
    pub reports: u64,
}

/// Seals tried before the escape goes on without one.
pub const MAX_SEALS: u32 = 6;
/// Ticks between two seals: the block update arrives in a tick or two.
pub const SEAL_EVERY_TICKS: u64 = 10;
/// Blocks a seal is made of, the commonest first: what a digging bot carries.
const SEAL_BLOCKS: &[data::ItemKind] = &[
    data::ItemKind::Cobblestone,
    data::ItemKind::CobbledDeepslate,
    data::ItemKind::Dirt,
    data::ItemKind::Andesite,
    data::ItemKind::Granite,
    data::ItemKind::Diorite,
    data::ItemKind::Netherrack,
];

/// Water over the cell, or past it on the far side from the bot (`away`, a unit step): digging
/// it lets more in (wall 11). The bot's own wet cells on the near side do not count.
fn holds_back_water(w: &World, (x, y, z): Cell, (ax, ay, az): Cell) -> bool {
    is_water(w, (x, y + 1, z)) || is_water(w, (x + ax, y + ay, z + az))
}

/// The cell to steer at on the way to `target` from `feet`: the target itself when it is straight
/// along x or z, else the cardinal neighbour toward it that `open` passes (the larger step first),
/// so a diagonal is taken round a corner. Neither open: the target, as before.
pub fn waypoint(feet: Cell, target: Cell, open: impl Fn(Cell) -> bool) -> Cell {
    let (dx, dz) = (target.0 - feet.0, target.2 - feet.2);
    if dx == 0 || dz == 0 {
        return target;
    }
    let along_x = (feet.0 + dx.signum(), feet.1, feet.2);
    let along_z = (feet.0, feet.1, feet.2 + dz.signum());
    let order = if dx.abs() >= dz.abs() {
        [along_x, along_z]
    } else {
        [along_z, along_x]
    };
    order.into_iter().find(|c| open(*c)).unwrap_or(target)
}

/// On the way to a diagonal `target`, a cardinal cell toward it the feet fit (not full) but the
/// head does not: its head-height block, to dig so the way round the corner is two high, never
/// one holding water back (blaze wall 14: the only way to the dry cell was a one-high stream).
pub fn corner_head(
    feet: Cell,
    target: Cell,
    full_at: impl Fn(Cell) -> bool,
    holds_back: impl Fn(Cell, Cell) -> bool,
) -> Option<Cell> {
    let (x, y, z) = feet;
    let (sx, sz) = ((target.0 - x).signum(), (target.2 - z).signum());
    if sx == 0 || sz == 0 {
        return None;
    }
    [((x + sx, y, z), (sx, 0, 0)), ((x, y, z + sz), (0, 0, sz))]
        .into_iter()
        .find_map(|(c, away)| {
            let head = (c.0, c.1 + 1, c.2);
            (!full_at(c) && full_at(head) && !holds_back(head, away)).then_some(head)
        })
}

/// A full block beside `target` to click, and the face of it that looks at `target` (faces as
/// the protocol numbers them: 0 down, 1 up, 2 north, 3 south, 4 west, 5 east). Never the cell
/// under `target`: that is the head.
pub fn against(target: Cell, full_at: impl Fn(Cell) -> bool) -> Option<(Cell, i8)> {
    let (x, y, z) = target;
    [
        ((x, y + 1, z), 0),
        ((x + 1, y, z), 4),
        ((x - 1, y, z), 5),
        ((x, y, z + 1), 2),
        ((x, y, z - 1), 3),
    ]
    .into_iter()
    .find(|(c, _)| full_at(*c))
}

/// Afloat with an empty bucket and no water bucket: the water the bot came for is here, so it
/// fills one before swimming out (blaze.md wall 10).
pub fn wants_fill(w: &World) -> bool {
    fill_wanted(&crate::held(w))
}

/// The rule over what is held: an empty bucket, and no water bucket yet.
pub fn fill_wanted(held: &crate::make::Counts) -> bool {
    let n = |k: data::ItemKind| held.get(&k).copied().unwrap_or(0);
    n(data::ItemKind::Bucket) >= 1 && n(data::ItemKind::WaterBucket) == 0
}

/// The cell `c`'s distance to the feet: on the plane, plus how far it is above them.
fn distance(pos: [f64; 3], (x, y, z): Cell) -> f64 {
    let dx = f64::from(x) + 0.5 - pos[0];
    let dz = f64::from(z) + 0.5 - pos[2];
    (dx * dx + dz * dz).sqrt() + (f64::from(y) - pos[1]).max(0.0)
}

/// The first cell over the feet's column that is not water: the water's surface.
fn surface(w: &World, (x, y, z): Cell) -> i32 {
    (y..y + SURFACE_SEARCH)
        .find(|yy| !is_water(w, (x, *yy, z)))
        .unwrap_or(y)
}

/// Cells within `r` on the plane, nearest first, at heights `ys`.
fn around((x, _, z): Cell, r: i32, ys: std::ops::RangeInclusive<i32>) -> Vec<Cell> {
    let mut v: Vec<Cell> = (-r..=r)
        .flat_map(|dx| (-r..=r).map(move |dz| (dx, dz)))
        .filter(|(dx, dz)| dx * dx + dz * dz <= r * r)
        .flat_map(|(dx, dz)| ys.clone().map(move |y| (x + dx, y, z + dz)))
        .collect();
    v.sort_by_key(|(cx, cy, cz)| ((cx - x).pow(2) + (cz - z).pow(2), -cy));
    v
}

impl Escape {
    pub fn new(kind: TrapKind, w: &World, tick: u64) -> Self {
        Self {
            kind,
            target: None,
            refused: Vec::new(),
            best: f64::MAX,
            best_at: tick,
            next_search: tick,
            dig: None,
            queue: Vec::new(),
            digs: 0,
            started: tick,
            yaw: w.player.yaw.into(),
            fill: None,
            fill_tried: false,
            seals: 0,
            sealed_at: 0,
            trail: Vec::new(),
            reports: 0,
        }
    }

    /// A swim off an island (issue 13b): the escape's swimming, held on dry footing across the
    /// water, between `SWIM_FROM` and `SWIM_TO` blocks away on the plane, the nearest first.
    pub fn swim(w: &World, tick: u64) -> Option<Self> {
        let body = w.body?;
        let feet = feet_cell(body.pos);
        let mut e = Self::new(TrapKind::Water, w, tick);
        let cells: Vec<Cell> = around(feet, SWIM_TO, feet.1 - 3..=feet.1 + 2)
            .into_iter()
            .filter(|(x, _, z)| (x - feet.0).pow(2) + (z - feet.2).pow(2) >= SWIM_FROM * SWIM_FROM)
            .collect();
        e.target = Some(e.footing(w, cells)?);
        Some(e)
    }

    /// The nearest standable cell from `cells` not refused.
    fn footing(&self, w: &World, cells: Vec<Cell>) -> Option<Cell> {
        let mut c = Cells::new(w);
        cells
            .into_iter()
            .find(|cell| !self.refused.contains(cell) && c.standable(*cell))
    }

    fn bank(&self, w: &World, feet: Cell) -> Option<Cell> {
        let top = surface(w, feet);
        self.footing(
            w,
            around(feet, BANK_RADIUS, top - BANK_DROP..=top + BANK_RISE),
        )
    }

    /// A wall to notch: a cell at the water's surface, beside water, with a full floor, whose
    /// two cells hold something to dig and no fluid; the one quickest to break with what is held
    /// (dirt before stone by hand), then the nearest.
    fn notch(&self, w: &World, feet: Cell) -> Option<(Cell, Vec<Cell>)> {
        let top = surface(w, feet);
        let mut c = Cells::new(w);
        let held = crate::dig::held_item(w);
        let cost = |cells: &[Cell]| -> u64 {
            cells
                .iter()
                .map(|k| {
                    kind_at(w, *k)
                        .and_then(|b| crate::dig::break_ticks(b, held, false, false))
                        .unwrap_or(u64::MAX / 4)
                })
                .sum()
        };
        let mut found: Vec<(u64, usize, Cell, Vec<Cell>)> = around(feet, NOTCH_RADIUS, top..=top)
            .into_iter()
            .filter(|cell| !self.refused.contains(cell))
            .filter_map(|(x, y, z)| {
                let beside_water = [(1, 0), (-1, 0), (0, 1), (0, -1)].iter().any(|(dx, dz)| {
                    is_water(w, (x + dx, y, z + dz)) || is_water(w, (x + dx, y - 1, z + dz))
                });
                let cells = [(x, y, z), (x, y + 1, z)];
                // Never a cell holding water back: over it, or beside it on the level. Dug, it
                // floods the notch, and a body in water on the notch's floor is never on dry land
                // (race 45343519424 lane 2: the aquifer round (42, 55, -136) stood at three heights).
                let wet = |(kx, ky, kz): Cell| {
                    [(0, 1, 0), (1, 0, 0), (-1, 0, 0), (0, 0, 1), (0, 0, -1)]
                        .iter()
                        .any(|(dx, dy, dz)| is_water(w, (kx + dx, ky + dy, kz + dz)))
                };
                let ok = beside_water
                    && !cells.iter().any(|k| wet(*k))
                    && c.floor((x, y - 1, z))
                    && cells
                        .iter()
                        .all(|k| matches!(c.at(*k), Class::Solid | Class::Clear | Class::Awkward));
                let dig: Vec<Cell> = cells
                    .iter()
                    .copied()
                    .filter(|k| c.at(*k) != Class::Clear)
                    .collect();
                (ok && !dig.is_empty()).then_some(((x, y, z), dig))
            })
            .enumerate()
            .map(|(i, (cell, dig))| (cost(&dig), i, cell, dig))
            .collect();
        found.sort_by_key(|(cost, i, _, _)| (*cost, *i));
        found
            .into_iter()
            .next()
            .map(|(_, _, cell, dig)| (cell, dig))
    }

    /// What the escape did, for the note at its end: how long, its seals and digs, its trail.
    pub fn report(&self) -> String {
        format!(
            "{} seals, {} digs, {} refused; trail: {}",
            self.seals,
            self.digs,
            self.refused.len(),
            if self.trail.is_empty() {
                "nothing chosen".to_owned()
            } else {
                self.trail.join(" / ")
            }
        )
    }

    fn trail(&mut self, tick: u64, what: String) {
        self.trail.push(format!("{tick} {what}"));
        if self.trail.len() > TRAIL {
            self.trail.remove(0);
        }
    }

    fn start_dig(&mut self, w: &World, cell: Cell, tick: u64) {
        self.trail(tick, format!("dig {cell:?}"));
        let mut d = Dig::new(w, cell, tick);
        // Nothing is collected on the way out.
        d.drop = None;
        self.dig = Some(d);
        self.digs += 1;
    }

    /// The block in the way: a lid over the head with the eyes under water, then the wall at
    /// head height one step toward the target.
    fn in_the_way(&self, w: &World, body: &physics::Body, target: Cell) -> Option<Cell> {
        let (x, y, z) = feet_cell(body.pos);
        let lid = (x, y + 2, z);
        // Never a block with water on it: a stair cut into an aquifer pours down the shaft, and
        // digging the lid let more in until the bot drowned (blaze.md wall 11: 4 of 12).
        if body.eye_in_water && full(kind_at(w, lid)) && !holds_back_water(w, lid, (0, 1, 0)) {
            return Some(lid);
        }
        let dx = (target.0 - x).signum();
        let dz = (target.2 - z).signum();
        [(x + dx, target.1 + 1, z + dz), (x + dx, y + 1, z + dz)]
            .into_iter()
            .find(|c| {
                full(kind_at(w, *c))
                    && *c != (target.0, target.1 - 1, target.2)
                    && !holds_back_water(w, *c, (dx, 0, dz))
            })
            .or_else(|| {
                corner_head(
                    (x, y, z),
                    target,
                    |c| full(kind_at(w, c)),
                    |c, away| holds_back_water(w, c, away),
                )
            })
    }

    /// One tick: the controls and the act to send.
    pub fn step(&mut self, w: &World, tick: u64) -> (Controls, Option<Act>) {
        let Some(body) = w.body else {
            return (Controls::default(), None);
        };
        let feet = feet_cell(body.pos);
        // In the water the bot wanted: fill the bucket first, once, then swim out.
        if self.kind == TrapKind::Water && !self.fill_tried && wants_fill(w) {
            self.fill_tried = true;
            self.fill = crate::scoop::Scoop::from_here(w, tick).map(Box::new);
        }
        if let Some(s) = &mut self.fill {
            let (mut c, act, status) = s.step(w, tick);
            match status {
                crate::scoop::ScoopStatus::Running => {
                    // Keep the head up while aiming.
                    c.jump = body.in_water;
                    return (c, act);
                }
                crate::scoop::ScoopStatus::Done | crate::scoop::ScoopStatus::Failed(_) => {
                    self.fill = None;
                }
            }
        }
        // Water pouring down onto the head (a stair cut into an aquifer): put a held block into
        // the cell over the head, so the shaft drains instead of filling (blaze.md wall 11).
        let over = (feet.0, feet.1 + 2, feet.2);
        if self.kind == TrapKind::Water
            && body.eye_in_water
            && is_water(w, over)
            && self.seals < MAX_SEALS
            && tick.saturating_sub(self.sealed_at) >= SEAL_EVERY_TICKS
            && let Some(item) = SEAL_BLOCKS
                .iter()
                .copied()
                .find(|k| crate::held(w).get(k).copied().unwrap_or(0) > 0)
            && let Some((click, face)) = against(over, |c| full(kind_at(w, c)))
        {
            // Still: a jump lifts the body into the cell sealed, and the server refuses it.
            let up = Controls::default();
            match crate::hold::toward(w, item) {
                crate::hold::Hold::Held => {
                    self.seals += 1;
                    self.trail(tick, format!("seal {over:?} on {click:?} face {face}"));
                    self.sealed_at = tick;
                    return (
                        up,
                        Some(Act::UseOn {
                            pos: BlockPos::new(click.0, click.1, click.2),
                            face,
                        }),
                    );
                }
                crate::hold::Hold::Act(a) => return (up, Some(a)),
                crate::hold::Hold::Wait => return (up, None),
                crate::hold::Hold::Missing => {}
            }
        }
        // Lava at the door and the body not yet in it: a held block into that cell (wall 22).
        if self.kind == TrapKind::Lava
            && !body.in_lava
            && self.seals < MAX_LAVA_SEALS
            && let Some(door) = lava_door(feet, |c| is_lava(w, c))
            && let Some(item) = SEAL_BLOCKS
                .iter()
                .copied()
                .find(|k| crate::held(w).get(k).copied().unwrap_or(0) > 0)
            && let Some((click, face)) = against(door, |c| full(kind_at(w, c))).or_else(|| {
                let under = (door.0, door.1 - 1, door.2);
                (under != (feet.0, feet.1 + 1, feet.2) && full(kind_at(w, under)))
                    .then_some((under, 1))
            })
        {
            let still = Controls::default();
            if tick.saturating_sub(self.sealed_at) < SEAL_EVERY_TICKS {
                return (still, None);
            }
            match crate::hold::toward(w, item) {
                crate::hold::Hold::Held => {
                    self.seals += 1;
                    self.trail(tick, format!("seal lava {door:?} on {click:?} face {face}"));
                    self.sealed_at = tick;
                    return (
                        still,
                        Some(Act::UseOn {
                            pos: BlockPos::new(click.0, click.1, click.2),
                            face,
                        }),
                    );
                }
                crate::hold::Hold::Act(a) => return (still, Some(a)),
                crate::hold::Hold::Wait => return (still, None),
                crate::hold::Hold::Missing => {}
            }
        }
        if let Some(d) = &mut self.dig {
            let (mut c, act, status) = d.step(w, tick);
            // Keep the head up while digging out of water.
            c.jump = body.in_water;
            match status {
                DigStatus::Running => return (c, act),
                DigStatus::Done | DigStatus::Failed(_) => {
                    self.dig = None;
                    // A cell that will not break: refuse where it led, look again.
                    if matches!(status, DigStatus::Failed(_))
                        && let Some(t) = self.target.take()
                    {
                        self.refused.push(t);
                        self.trail(tick, format!("dig failed; refused {t:?}"));
                        self.queue.clear();
                        self.next_search = tick;
                    }
                    self.best = f64::MAX;
                    self.best_at = tick;
                    if act.is_some() {
                        return (c, act);
                    }
                }
            }
        }
        // A notch's cells are dug from the surface, the eyes out of the water: under it the
        // break time is five times longer (the queue waits while the body rises).
        if !body.eye_in_water
            && let Some(next) = self.queue.pop()
        {
            self.start_dig(w, next, tick);
            return self.step(w, tick);
        }
        // In a wall: dig the eye's cell, then the feet's, nothing collected.
        if self.kind == TrapKind::Wall {
            let eye = (
                feet.0,
                (body.pos[1] + physics::EYE_HEIGHT).floor() as i32,
                feet.2,
            );
            let cell = [eye, feet].into_iter().find(|c| full(kind_at(w, *c)));
            if let Some(c) = cell {
                self.start_dig(w, c, tick);
                return self.step(w, tick);
            }
            return (Controls::default(), None);
        }
        if self.target.is_none() && tick >= self.next_search {
            self.next_search = tick + SEARCH_EVERY_TICKS;
            // A bank refused (no nearer, nothing on the way it may dig) is past a swim's reach:
            // a notch beside the body comes before the next bank. With a bank within reach on
            // record the list never ran out, and the notch was never cut (race 45343519424 lane
            // 2: nine banks 12 blocks off through stone holding water back, refused in turn
            // and the list cleared, from tick 14,780 to the run's end).
            let notch_first = (self.kind == TrapKind::Water && !self.refused.is_empty())
                .then(|| self.notch(w, feet))
                .flatten();
            if let Some((cell, _)) = &notch_first {
                self.trail(
                    tick,
                    format!(
                        "notch before another bank at {cell:?}: {} banks refused",
                        self.refused.len()
                    ),
                );
            }
            self.target = match self.kind {
                TrapKind::Water if notch_first.is_some() => None,
                TrapKind::Water => self.bank(w, feet),
                TrapKind::Lava => {
                    self.footing(w, around(feet, FLEE_RADIUS, feet.1 - 1..=feet.1 + 2))
                }
                TrapKind::Fire => {
                    self.footing(w, around(feet, FIRE_RADIUS, feet.1 - 1..=feet.1 + 1))
                }
                TrapKind::Wall => None,
            };
            self.best = f64::MAX;
            self.best_at = tick;
            if self.target.is_none()
                && self.kind == TrapKind::Water
                && let Some((cell, dig)) = notch_first.or_else(|| self.notch(w, feet))
            {
                self.target = Some(cell);
                // Popped from the end: the head-height cell first, then the feet's.
                self.queue = dig;
            }
            let found = match self.target {
                Some(t) => format!("target {t:?} from {feet:?}, {} queued", self.queue.len()),
                None => format!("no bank or notch from {feet:?}"),
            };
            self.trail(tick, found);
        }
        let Some(target) = self.target else {
            // Nowhere yet: rise, and keep going the way the body faces.
            return (
                Controls {
                    forward: self.kind != TrapKind::Water,
                    jump: true,
                    sneak: false,
                    yaw: self.yaw,
                    pitch: None,
                },
                None,
            );
        };
        let d = distance(body.pos, target);
        if d < self.best - PROGRESS {
            self.best = d;
            self.best_at = tick;
        } else if tick.saturating_sub(self.best_at) > STUCK_TICKS {
            match self.in_the_way(w, &body, target) {
                Some(cell) if self.kind == TrapKind::Water => self.start_dig(w, cell, tick),
                Some(_) | None => {
                    self.refused.push(target);
                    self.trail(
                        tick,
                        format!("no nearer in {STUCK_TICKS}; refused {target:?}"),
                    );
                    if self.refused.len() > MAX_REFUSED {
                        self.refused.clear();
                    }
                    self.target = None;
                    self.next_search = tick;
                }
            }
            self.best = f64::MAX;
            self.best_at = tick;
        }
        let centre = [
            f64::from(target.0) + 0.5,
            f64::from(target.1),
            f64::from(target.2) + 0.5,
        ];
        // Round a corner, not into it: a diagonal target is steered at through the open cardinal
        // cell first (wall 14: a bot pressed into the stone corner between it and a dry cell one
        // diagonal step away and refused every bank for 25,000 ticks).
        let open = |c: Cell| !full(kind_at(w, c)) && !full(kind_at(w, (c.0, c.1 + 1, c.2)));
        let aim = waypoint(feet, target, open);
        self.yaw = yaw_to(
            body.pos,
            [f64::from(aim.0) + 0.5, centre[1], f64::from(aim.2) + 0.5],
        );
        let flat = ((centre[0] - body.pos[0]).powi(2) + (centre[2] - body.pos[2]).powi(2)).sqrt();
        // On footing over the target's centre: stand still and let the trap clear.
        let there = flat < 0.2 && body.on_ground;
        (
            Controls {
                forward: !there,
                jump: body.in_water || body.in_lava || body.horizontal_collision,
                sneak: false,
                yaw: self.yaw,
                pitch: None,
            },
            None,
        )
    }
}

#[cfg(test)]
mod fill_tests {
    use super::*;

    /// Blaze wall 10: afloat with only an empty bucket, fill it; with water held, or no bucket,
    /// just swim out (a second empty bucket is kept for lava).
    #[test]
    fn an_empty_bucket_and_no_water_is_filled() {
        let c = |v: &[(data::ItemKind, i32)]| v.iter().copied().collect::<crate::make::Counts>();
        assert!(fill_wanted(&c(&[(data::ItemKind::Bucket, 1)])));
        assert!(!fill_wanted(&c(&[
            (data::ItemKind::Bucket, 1),
            (data::ItemKind::WaterBucket, 1)
        ])));
        assert!(!fill_wanted(&c(&[(data::ItemKind::StonePickaxe, 1)])));
    }

    /// Blaze wall 11: water over the head in a shaft is sealed by clicking the stone over it
    /// (its down face), else a wall beside it on the face that looks at it; never the head below.
    #[test]
    fn a_seal_clicks_the_stone_round_the_cell_over_the_head() {
        let over = (57, 26, -152);
        assert_eq!(
            against(over, |c| c == (57, 27, -152)),
            Some(((57, 27, -152), 0))
        );
        assert_eq!(
            against(over, |c| c == (58, 26, -152)),
            Some(((58, 26, -152), 4))
        );
        assert_eq!(
            against(over, |c| c == (57, 26, -153)),
            Some(((57, 26, -153), 3))
        );
        assert_eq!(against(over, |c| c == (57, 25, -152)), None);
    }

    /// Blaze wall 14 (race 45091345688, lane 1): from (13, 31, 2) the dry cell (14, 31, 3) is a
    /// diagonal step past the stone at (13, 31, 3): steer through (14, 31, 2), the open one.
    #[test]
    fn a_diagonal_bank_is_reached_round_the_corner() {
        let stone = (13, 31, 3);
        let open = |c: Cell| c != stone && c != (13, 32, 3);
        assert_eq!(waypoint((13, 31, 2), (14, 31, 3), open), (14, 31, 2));
        assert_eq!(waypoint((13, 31, 2), (16, 30, 2), open), (16, 30, 2));
        assert_eq!(waypoint((13, 31, 2), (14, 31, 3), |_| false), (14, 31, 3));
    }

    /// Blaze wall 14 again (race 45096903034, lanes 2, 8, 9, 10 at (13, 31, 2)): the cardinal cell
    /// (14, 31, 2) is a one-high stream under stone, so the stone over it is dug; (13, 31, 3) is
    /// stone at the feet, no way. Holding water back, or a straight target: nothing.
    #[test]
    fn the_one_high_way_round_a_corner_is_dug_two_high() {
        let full = |c: Cell| matches!(c, (14, 32, 2) | (13, 31, 3) | (13, 32, 3));
        let dry = |_: Cell, _: Cell| false;
        assert_eq!(
            corner_head((13, 31, 2), (14, 31, 3), full, dry),
            Some((14, 32, 2))
        );
        assert_eq!(
            corner_head((13, 31, 2), (14, 31, 3), full, |_, _| true),
            None
        );
        assert_eq!(corner_head((13, 31, 2), (16, 31, 2), full, dry), None);
    }

    /// Blaze wall 22: lava over the head, beside the head or beside the feet is at the door;
    /// lava under a neighbour's floor, or two cells off, is not.
    #[test]
    fn lava_at_the_door_is_seen_over_and_beside_the_body() {
        let feet = (30, 31, -18);
        let at = |cells: &'static [Cell]| move |c: Cell| cells.contains(&c);
        assert_eq!(lava_door(feet, at(&[(30, 33, -18)])), Some((30, 33, -18)));
        assert_eq!(lava_door(feet, at(&[(30, 32, -19)])), Some((30, 32, -19)));
        assert_eq!(lava_door(feet, at(&[(30, 31, -19)])), Some((30, 31, -19)));
        // Over the head first, then the head's height, then the feet's.
        assert_eq!(
            lava_door(feet, at(&[(30, 31, -19), (31, 32, -18), (30, 33, -18)])),
            Some((30, 33, -18))
        );
        assert_eq!(lava_door(feet, at(&[(30, 30, -19), (30, 31, -20)])), None);
        // Lava a cell over the head's height, beside its column, is seen, and last.
        assert_eq!(lava_door(feet, at(&[(31, 33, -18)])), Some((31, 33, -18)));
        assert_eq!(
            lava_door(feet, at(&[(31, 33, -18), (30, 31, -19)])),
            Some((30, 31, -19))
        );
        assert_eq!(lava_door(feet, at(&[(31, 33, -19), (30, 34, -18)])), None);
    }
}
