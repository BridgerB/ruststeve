//! Issue 19, the way home under cover (agent Q9, docs/problems/eyes-route.md): from the warped
//! forest to within `[portal_way] reach` of the entry portal by a route read from the frontier
//! world's region files, every leg cut by `tunnel_toward_covered` to a feet cell of
//! `[home] covered`. The way west along z 194 put a fall to lava beside the feet on 434 of its 907
//! steps (the spans over the deltas' sea, where magma cubes knocked bots in: eyes-home walls 12,
//! 20, 23, 27); the covered route north of the deltas (probe 45761861773, `results region-route`,
//! candidate `thrifty16`: 16 clear of every fortress column, 24 of the blaze spawner) has 11.
//!
//! 1. **Begun** on the first trip tick with points left: the points before the nearest one (to the
//!    feet, in 3D) are passed, so a bot resumed mid-route goes on from where it stands (Plan state
//!    is not in a snapshot).
//! 2. **A point is passed** with the feet within `PASS` of it on the plane and `PASS_UP` in height
//!    (the tunnel ends within `tunnel::REACH` 5 of its target); its chunk column joins Home's
//!    `ways_passed`, so the heading's fallback (`[home] way`, the same points on the plane) never
//!    aims back at it.
//! 3. **The leg**: `tunnel_toward_covered` to the next point, 2 of rock kept over the head where it
//!    can. A point no tunnel reaches `FAILS` times running is skipped (the progress invariant).
//! 4. **Pickaxes**: under `PICK_USES` uses left, a run of the trip's wooden pickaxes
//!    (`home::TRIP_WOOD_STEPS`) from held stems or planks, within the way home's `pick_runs`, at
//!    most `PICK_RUNS` here and `PICK_EVERY` apart.
//! 5. Within `[portal_way] reach` of the portal it hands over to Q6's last leg; with no points
//!    left, to the trip's own ways.
//!
//! Every step is noted ("covered way: ...").

use world::World;

use crate::path::{self, Cell};
use crate::{Plan, Task, home};

/// The covered way's state, `HomeState::covered`. Every field defaults.
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct CoveredState {
    /// Whether the points before the nearest were marked passed.
    pub begun: bool,
    /// Points of `[home] covered` passed, in order.
    pub passed: usize,
    /// Tunnels begun toward the current point with no route found, running.
    pub fails: u32,
    /// Tunnels begun in all, points skipped, pickaxe runs made and the last one's tick.
    pub tunnels: u32,
    pub skipped: u32,
    pub pick_runs: u32,
    pub pick_at: Option<u64>,
    /// The pickaxe uses held when the covered way began, for the notes.
    pub uses0: Option<i32>,
}

/// A point is passed within this on the plane (the tunnel's `REACH` 5, and a cell of room).
pub const PASS: f64 = 6.0;
/// And within this in height (a stair round the target's own cell).
pub const PASS_UP: i32 = 4;
/// Tunnels with no route toward one point before it is skipped (`tunnel::SHORT_STRIKES` is 2; a
/// third with the chunks loaded).
pub const FAILS: u32 = 3;
/// Pickaxe uses under which a wooden run is made: a 24-block leg cut through rock at Q6's measured
/// 1.7 a block (race 45742933645 lane 1) is about 41; 100 is two legs and a margin.
pub const PICK_USES: i32 = 100;
/// Wooden runs at most on the covered way (the trip's `pick_runs` 8 bound them all; Q6's last leg
/// keeps its own 2).
pub const PICK_RUNS: u32 = 5;
/// Ticks between two runs (Q6's `PICK_EVERY`).
pub const PICK_EVERY: u64 = 1200;
/// The trip's portal is `[home] portal` within this each axis (memory's lowered bottom cell and the
/// geography's lower inside cell differ by a cell or two: Q1's probe 45686594347).
pub const SAME_PORTAL: i32 = 4;
/// Tunnels between two "tunnel" notes after the first toward a point.
pub const NOTE_TUNNELS: u32 = 10;

/// The default `[home] covered` (the config's; a test holds them equal): `results region-route`
/// candidate `thrifty16`'s feet cells every 24 steps (probes 45761861773 and 45762715583, its `way3`) on the frontier world
/// (race 45621953332 lane 7's snapshot), the first (the forest's start) and the points within
/// `[portal_way] reach` 120 of the portal left out.
pub fn default_covered() -> Vec<[i32; 3]> {
    vec![
        [481, 52, 21],
        [466, 72, 28],
        [457, 89, 39],
        [443, 98, 49],
        [425, 95, 53],
        [409, 88, 59],
        [392, 94, 62],
        [375, 83, 69],
        [360, 96, 70],
        [338, 97, 70],
        [319, 92, 75],
        [296, 85, 74],
        [286, 75, 60],
        [285, 79, 39],
        [261, 79, 39],
        [240, 82, 40],
        [222, 84, 42],
        [205, 84, 45],
        [190, 90, 42],
        [170, 85, 42],
        [148, 81, 40],
        [139, 80, 55],
        [124, 82, 64],
        [116, 84, 80],
        [104, 89, 74],
    ]
}

fn plane(a: Cell, b: [i32; 3]) -> f64 {
    f64::from(a.0 - b[0]).hypot(f64::from(a.2 - b[2]))
}

impl Plan {
    fn covered_points(&self) -> Vec<[i32; 3]> {
        self.table
            .as_ref()
            .map_or_else(default_covered, |t| t.home.covered.clone())
    }

    /// The covered way's next task on the trip (module doc), `None` with no points left, within
    /// the last leg's reach of `portal` (`d` on the plane), or a stack on the cursor.
    pub(crate) fn covered_way(
        &mut self,
        w: &World,
        tick: u64,
        portal: Cell,
        d: i32,
    ) -> Option<Task> {
        let points = self.covered_points();
        let reach = self.table.as_ref().map_or(120, |t| t.portal_way.reach);
        if points.is_empty() || d <= reach {
            return None;
        }
        // The points are the way to `[home] portal` (the frontier world's entry portal): used
        // only when the trip's portal is that one, within `SAME_PORTAL` each axis.
        let geo = self
            .table
            .as_ref()
            .map_or(Some([22, 34, -20]), |t| t.home.portal);
        let same = geo.is_some_and(|g| {
            (g[0] - portal.0).abs() <= SAME_PORTAL
                && (g[1] - portal.1).abs() <= SAME_PORTAL
                && (g[2] - portal.2).abs() <= SAME_PORTAL
        });
        if !same {
            return None;
        }
        let body = w.body?;
        let feet = path::stand_cell(w, body.pos);
        // 1. Begun: the points before the nearest passed (a resumed bot).
        if !self.ender.home.covered.begun {
            let nearest = points
                .iter()
                .enumerate()
                .min_by(|(_, a), (_, b)| {
                    let da = plane(feet, **a).hypot(f64::from(feet.1 - a[1]));
                    let db = plane(feet, **b).hypot(f64::from(feet.1 - b[1]));
                    da.total_cmp(&db)
                })
                .map_or(0, |(k, _)| k);
            let st = &mut self.ender.home.covered;
            st.begun = true;
            st.passed = nearest;
            st.uses0 = Some(crate::endermen::pickaxe_uses_left(w));
            for p in points.iter().take(nearest) {
                self.pass_column(*p);
            }
            self.note(
                tick,
                format!("covered way: begun at point {nearest} of {}", points.len()),
                format!(
                    "from {feet:?}, {d} off the portal on the plane; next {:?}; pickaxe uses {}",
                    points.get(nearest),
                    crate::endermen::pickaxe_uses_left(w)
                ),
            );
        }
        // 2. Points passed.
        while let Some(p) = points.get(self.ender.home.covered.passed).copied() {
            if plane(feet, p) > PASS || (feet.1 - p[1]).abs() > PASS_UP {
                break;
            }
            self.ender.home.covered.passed += 1;
            self.ender.home.covered.fails = 0;
            self.pass_column(p);
            let k = self.ender.home.covered.passed - 1;
            let uses = self.covered_uses(w);
            self.note(
                tick,
                format!("covered way: point {k} of {} passed at {p:?}", points.len()),
                format!("feet {feet:?}; {uses}"),
            );
        }
        let next = points.get(self.ender.home.covered.passed).copied()?;
        if w.inventory.carried.is_some() {
            return None;
        }
        // 4. Pickaxes before the leg.
        if let Some(t) = self.covered_picks(w, tick) {
            return Some(t);
        }
        // 3. The leg.
        let target: Cell = (next[0], next[1], next[2]);
        let k = self.ender.home.covered.passed;
        match self.tunnel_toward_covered(w, target, ("the covered way", "a covered leg"), tick) {
            Some(t) => {
                let st = &mut self.ender.home.covered;
                st.tunnels += 1;
                let first = st.fails == 0 && st.tunnels % NOTE_TUNNELS == 1;
                st.fails = 0;
                if first {
                    let n = st.tunnels;
                    let uses = self.covered_uses(w);
                    self.note(
                        tick,
                        format!("covered way: tunnel to point {k} at {target:?}"),
                        format!(
                            "from {feet:?}, {:.0} off on the plane; tunnel {n}; {uses}",
                            plane(feet, next)
                        ),
                    );
                }
                Some(t)
            }
            None => {
                let st = &mut self.ender.home.covered;
                st.fails += 1;
                if st.fails < FAILS {
                    return None;
                }
                st.fails = 0;
                st.passed += 1;
                st.skipped += 1;
                let n = st.skipped;
                self.pass_column(next);
                self.note(
                    tick,
                    format!("covered way: point {k} skipped at {target:?}"),
                    format!("no tunnel route from {feet:?} {FAILS} times; skip {n}"),
                );
                None
            }
        }
    }

    /// A passed point's chunk column in Home's `ways_passed` (the heading's fallback).
    fn pass_column(&mut self, p: [i32; 3]) {
        let col = (p[0] >> 4, p[2] >> 4);
        if !self.ender.home.ways_passed.contains(&col) {
            self.ender.home.ways_passed.push(col);
        }
    }

    fn covered_uses(&self, w: &World) -> String {
        let now = crate::endermen::pickaxe_uses_left(w);
        let spent = self.ender.home.covered.uses0.map_or(0, |u| u - now);
        format!("pickaxe uses {spent} spent on the covered way, {now} left")
    }

    /// Wooden pickaxes for the covered way (module doc, 4). `None` with enough, no wood, a run in
    /// hand, or the runs spent.
    fn covered_picks(&mut self, w: &World, tick: u64) -> Option<Task> {
        let uses = crate::endermen::pickaxe_uses_left(w);
        let runs = self.table.as_ref().map_or(8, |t| t.home.pick_runs);
        let wood = crate::base::logs_held(w) + crate::base::planks_held(w);
        let picking = self.base.steps == home::TRIP_WOOD_STEPS.to_vec() && !self.base.over;
        let st = &self.ender.home.covered;
        if uses >= PICK_USES
            || picking
            || wood < 2
            || self.ender.home.trip_pick_runs >= runs
            || st.pick_runs >= PICK_RUNS
            || st.pick_at.is_some_and(|t| tick < t + PICK_EVERY)
        {
            return None;
        }
        self.ender.home.covered.pick_at = Some(tick);
        self.ender.home.covered.pick_runs += 1;
        self.ender.home.trip_pick_runs += 1;
        self.base = crate::base::BaseState {
            steps: home::TRIP_WOOD_STEPS.to_vec(),
            ..crate::base::BaseState::default()
        };
        self.note(
            tick,
            format!(
                "covered way: pickaxes for the way ({})",
                self.ender.home.covered.pick_runs
            ),
            format!("pickaxe uses {uses} under {PICK_USES}; {wood} logs and planks"),
        );
        self.base_task(w, tick)
    }
}
