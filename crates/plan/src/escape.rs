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
/// Banks refused before the list starts over.
const MAX_REFUSED: usize = 8;

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
pub enum TrapKind {
    Water,
    Lava,
    Fire,
}

impl TrapKind {
    pub fn parse(s: &str) -> Option<TrapKind> {
        match s {
            "water" => Some(TrapKind::Water),
            "lava" => Some(TrapKind::Lava),
            "fire" => Some(TrapKind::Fire),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            TrapKind::Water => "water",
            TrapKind::Lava => "lava",
            TrapKind::Fire => "fire",
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
    body.on_ground && !body.in_water && !body.eye_in_water && full(kind_at(w, under))
}

/// Off the ground with water in the body or in the cells at or under the feet.
fn afloat(w: &World, body: &physics::Body) -> bool {
    let (x, y, z) = feet_cell(body.pos);
    !body.on_ground && (body.in_water || is_water(w, (x, y, z)) || is_water(w, (x, y - 1, z)))
}

impl Traps {
    /// The trap this tick (clojurecraft's `trapped-tick`).
    pub fn tick(&mut self, w: &World, body: &physics::Body) -> Option<TrapKind> {
        self.wet = if body.eye_in_water { self.wet + 1 } else { 0 };
        self.afloat = if afloat(w, body) { self.afloat + 1 } else { 0 };
        self.dry = if dry_land(w, body) { self.dry + 1 } else { 0 };
        let held = self
            .kind
            .filter(|k| *k != TrapKind::Fire && self.dry < DRY_TICKS_TO_CLEAR);
        self.kind = if body.in_lava {
            Some(TrapKind::Lava)
        } else if self.wet >= TRAP_AFTER_TICKS || self.afloat >= AFLOAT_AFTER_TICKS {
            Some(TrapKind::Water)
        } else if touching_fire(w, body.pos) {
            Some(TrapKind::Fire)
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
        }
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
                let ok = beside_water
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

    fn start_dig(&mut self, w: &World, cell: Cell, tick: u64) {
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
        if body.eye_in_water && full(kind_at(w, lid)) {
            return Some(lid);
        }
        let dx = (target.0 - x).signum();
        let dz = (target.2 - z).signum();
        [(x + dx, target.1 + 1, z + dz), (x + dx, y + 1, z + dz)]
            .into_iter()
            .find(|c| full(kind_at(w, *c)) && *c != (target.0, target.1 - 1, target.2))
    }

    /// One tick: the controls and the act to send.
    pub fn step(&mut self, w: &World, tick: u64) -> (Controls, Option<Act>) {
        let Some(body) = w.body else {
            return (Controls::default(), None);
        };
        let feet = feet_cell(body.pos);
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
        if self.target.is_none() && tick >= self.next_search {
            self.next_search = tick + SEARCH_EVERY_TICKS;
            self.target = match self.kind {
                TrapKind::Water => self.bank(w, feet),
                TrapKind::Lava => {
                    self.footing(w, around(feet, FLEE_RADIUS, feet.1 - 1..=feet.1 + 2))
                }
                TrapKind::Fire => {
                    self.footing(w, around(feet, FIRE_RADIUS, feet.1 - 1..=feet.1 + 1))
                }
            };
            self.best = f64::MAX;
            self.best_at = tick;
            if self.target.is_none()
                && self.kind == TrapKind::Water
                && let Some((cell, dig)) = self.notch(w, feet)
            {
                self.target = Some(cell);
                // Popped from the end: the head-height cell first, then the feet's.
                self.queue = dig;
            }
        }
        let Some(target) = self.target else {
            // Nowhere yet: rise, and keep going the way the body faces.
            return (
                Controls {
                    forward: self.kind != TrapKind::Water,
                    jump: true,
                    sneak: false,
                    yaw: self.yaw,
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
        self.yaw = yaw_to(body.pos, centre);
        let flat = ((centre[0] - body.pos[0]).powi(2) + (centre[2] - body.pos[2]).powi(2)).sqrt();
        // On footing over the target's centre: stand still and let the trap clear.
        let there = flat < 0.2 && body.on_ground;
        (
            Controls {
                forward: !there,
                jump: body.in_water || body.in_lava || body.horizontal_collision,
                sneak: false,
                yaw: self.yaw,
            },
            None,
        )
    }
}
