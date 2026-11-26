//! Issue 19, the way home's fortress stretch (agent Q11, docs/problems/eyes-fortress.md): from
//! the blaze spawner's detour at about (100, 45, 205) past the fortress (its bridge at x 60, z 109
//! to 175) to within `[portal_way] reach` 120 of the entry portal, where Q6's last leg
//! (`crate::portal_way`) takes over. Called from fenced blocks in `crate::home` and
//! `crate::heading`; every rule here notes when it fires.
//!
//! 1. **The guard never aims at the body's own cells** (`body_in_cell`): race 45760537530, all
//!    three lanes that uploaded, stood for 3,000 to 4,000 ticks clicking the same guard block
//!    (filler unchanged: the server refuses a block inside the player's box) while blazes and a
//!    ghast burned lane 1 from 19.8 to 2.4. Lane 0 at x 98.8 (box to 99.1) guarded (99, 73, 182);
//!    lane 1 at x 81.7 guarded (82, 80, 162); lane 2 at z 168.1 on soul sand (box from 167.8)
//!    guarded (104, 69, 167). A side the body overhangs is not guarded (`home::guard_wanted`), and
//!    the skip is noted here ("fortress way: no guard in the body's own cell").
//! 2. **No fortress turn while the way steers home** (`home_way_steers`): the same race's three
//!    lanes turned south 48 blocks at tick 60 ("a fortress seen ahead (9300 nether bricks)" or "a
//!    ticking WitherSkeleton 33 off") in the one tick between a point reached and the next aimed
//!    at, then walked back under ghast and blaze fire. The way crosses the fortress by design.

use world::World;

use crate::path::Cell;
use crate::{Plan, home, path};

/// Fortress-stretch state, `HomeState::fortress`. Every field defaults.
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct FortressState {
    /// Ticks a guard side was skipped because the body's box is in it, and the last note's tick.
    pub guard_skips: u32,
    pub guard_noted_at: Option<u64>,
    /// Heading turns for a fortress refused while the way steered home.
    pub turns_refused: u32,
}

/// Ticks between two notes of the same rule (a minute of game time).
pub const NOTE_EVERY: u64 = 1200;

/// Whether the player's box (`physics::HALF_WIDTH` round the feet, `physics::HEIGHT` tall)
/// overlaps the cell's cube: the server refuses a block placed there (race 45760537530).
pub fn body_in_cell(w: &World, c: Cell) -> bool {
    let Some(body) = w.body else {
        return false;
    };
    // A hair inside the faces: a box exactly flush with a face does not block a placement.
    const EPS: f64 = 1e-4;
    let [px, py, pz] = body.pos;
    let hw = physics::HALF_WIDTH - EPS;
    let over = |lo: f64, hi: f64, cell: i32| lo < f64::from(cell) + 1.0 && hi > f64::from(cell);
    over(px - hw, px + hw, c.0)
        && over(py + EPS, py + physics::HEIGHT - EPS, c.1)
        && over(pz - hw, pz + hw, c.2)
}

/// The guard's open side cells across `axis` the body's box is in (what `home::guard_wanted`
/// skips), for the note.
pub fn guard_sides_in_body(w: &World, axis: Option<(i32, i32)>) -> Vec<Cell> {
    let (Some((ax, az)), Some(body)) = (axis, w.body) else {
        return Vec::new();
    };
    let feet = path::stand_cell(w, body.pos);
    let mut cells = path::Cells::new(w);
    [(az, ax), (-az, -ax)]
        .into_iter()
        .map(|(sx, sz)| (feet.0 + sx, feet.1, feet.2 + sz))
        .filter(|t| cells.at(*t) == path::Class::Clear && body_in_cell(w, *t))
        .collect()
}

/// Whether a travel point steers the way home now (Home's own aim set, or the heading's):
/// no fortress turn then.
pub fn home_way_steers(p: &Plan, heading_way: bool) -> bool {
    p.ender.phase == crate::endermen::EnderPhase::Home
        && (heading_way || p.ender.home.aim.is_some())
}

impl Plan {
    /// The stretch's per-tick notes on the trip (`home_trip`): the guard skips counted. Never a
    /// task of its own yet.
    pub(crate) fn fortress_way(&mut self, w: &World, tick: u64) -> Option<crate::Task> {
        let skipped = guard_sides_in_body(w, self.ender.home.axis);
        if !skipped.is_empty() {
            let f = &mut self.ender.home.fortress;
            f.guard_skips += 1;
            if f.guard_noted_at.is_none_or(|t| tick >= t + NOTE_EVERY) {
                f.guard_noted_at = Some(tick);
                let n = f.guard_skips;
                let pos = w.body.map(|b| b.pos);
                self.note(
                    tick,
                    format!("fortress way: no guard in the body's own cell {:?}", skipped[0]),
                    format!(
                        "the box at {pos:?} overhangs it (the server refuses a block there); skip {n}; {}",
                        home::carried(w)
                    ),
                );
            }
        }
        None
    }

    /// A fortress turn refused while the way steers home: counted and noted (`crate::heading`).
    pub(crate) fn fortress_turn_refused(&mut self, tick: u64, why: &str) {
        let f = &mut self.ender.home.fortress;
        f.turns_refused += 1;
        let n = f.turns_refused;
        if n % 20 == 1 {
            self.note(
                tick,
                "fortress way: no fortress turn on the way home".to_owned(),
                format!("{why}; the way crosses the fortress (refusal {n})"),
            );
        }
    }
}
