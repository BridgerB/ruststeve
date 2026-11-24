//! The travel tunnel (docs/problems/nether-tunnel.md): the way to a place no walk reaches, cut
//! through rock. A route of feet cells is searched from the feet to within reach of the target,
//! each move a level step, a stair up or a stair down (the same three cuts `stairs` makes), and
//! the staircase task cuts it a few steps at a time (`Stairs::along`).
//!
//! What the search may know (strict no X-ray): a cell that is open, and a block with an open cell
//! beside it (an exposed face), are read as they are; lava and water are read where the top of
//! their column is exposed (a lake under its own surface). Everything else is rock: unknown,
//! taken as diggable and as a floor. What rock hides (a lava pocket, magma, gravel) is met by the
//! staircase's own refusal one step ahead, as before; the planner then blocks that cell and the
//! route is searched again round it.
//!
//! The route keeps out of trouble by cost, not by rule: a step under `SAFE_Y` (the Nether's lava
//! sea and the magma round it) costs more, so a long route climbs first and comes down at the far
//! end; a step in the open (a cavern, a ledge over lava) costs more than one cut through rock.

use std::cmp::Reverse;
use std::collections::{BTreeMap, BTreeSet, BinaryHeap};

use data::BlockKind;
use world::{BlockPos, World};

use crate::path::{self, Cell, Class};
use crate::stairs::{self, Stairs};
use crate::{Plan, Task};

/// Feet under this height pay `LOW_COST` a step. The Nether's lava sea fills to y 31 and magma
/// lies in the netherrack to about y 37 round it (race 45194253932 lane 5's world: the survey in
/// docs/problems/nether-tunnel.md); 40 leaves a floor at 39, two blocks over the highest magma
/// seen there.
pub const SAFE_Y: i32 = 40;
/// The extra cost of a step under `SAFE_Y`: four stairs up (4 each) are repaid within three
/// steps, so any route longer than that climbs first.
pub const LOW_COST: u32 = 6;
/// The extra cost of a step in the open (more than a tunnel's own two open sides round the feet
/// and head): a cut through rock (3) is cheaper than a walk across a cavern floor (1 + 4), a
/// short crossing still cheaper than a long way round.
pub const OPEN_COST: u32 = 4;
/// Nodes a search may expand: the 110-block route of the survey world took about 6,000.
pub const MAX_NODES: u32 = 60_000;
/// The search box: this far beyond the feet and the target on the plane, and this far over the
/// higher of them (and `SAFE_Y`).
pub const MARGIN: i32 = 40;
pub const RISE: i32 = 16;
/// The box is never wider than this on a side (the grids are two bytes a cell).
pub const MAX_SPAN: i32 = 288;
/// Within this of the target's centre the route is done: under `world::stage::FORTRESS_NEAR` (6),
/// so the stage is met a step before the route ends.
pub const REACH: f64 = 5.0;
/// A partial route (the target not reached within the nodes, or beyond the loaded chunks) is
/// taken when it ends this much nearer on the plane than it starts.
pub const PARTIAL_GAIN: i32 = 8;
/// Segments begun without the feet coming nearer the target (on the plane) before the tunnel is
/// given up and its target refused: 16 segments are up to 96 cells, and the way round the cavern
/// of the survey world runs 25 cells with no gain.
pub const MAX_STALLS: u32 = 16;
/// Steps that ended short at one cell, with nothing the staircase refuses there, before the cell
/// is blocked: one short segment may be a task cut by a meal or a fight.
pub const SHORT_STRIKES: u32 = 2;

/// Steps a stretch cuts before the planner looks again: `stairs::STAIR_DEPTH`.
pub const STRETCH: usize = 6;

const UNREAD: u8 = 0;
const UNLOADED: u8 = 1;
const OPEN: u8 = 2;
const LAVA: u8 = 3;
const WATER: u8 = 4;
/// A full block a walk stands on, or anything unseen.
const ROCK: u8 = 5;
/// Seen, dug like rock, never stood on: magma, soul sand, anything the route calls awkward.
const HOT: u8 = 6;
/// Seen gravel or sand: a floor, never left over an opened cell.
const FALLS: u8 = 7;
/// Seen, neither open nor a floor (a plant, a fire).
const ODD: u8 = 8;
/// Never opened: a portal's own blocks (a step in is a trip home) and its frame, bedrock.
const WALL: u8 = 9;

const SIDES: [(i32, i32, i32); 6] = [
    (1, 0, 0),
    (-1, 0, 0),
    (0, 1, 0),
    (0, -1, 0),
    (0, 0, 1),
    (0, 0, -1),
];
const DIRS: [(i32, i32); 4] = [(1, 0), (-1, 0), (0, 1), (0, -1)];

/// The world as the tunnel may know it, read once a cell inside a box.
struct Sight<'a> {
    w: &'a World,
    lo: Cell,
    dims: (i32, i32, i32),
    raw: Vec<u8>,
    seen: Vec<u8>,
}

impl<'a> Sight<'a> {
    fn new(w: &'a World, lo: Cell, hi: Cell) -> Self {
        let dims = (
            (hi.0 - lo.0 + 1).max(1),
            (hi.1 - lo.1 + 1).max(1),
            (hi.2 - lo.2 + 1).max(1),
        );
        let n =
            usize::try_from(i64::from(dims.0) * i64::from(dims.1) * i64::from(dims.2)).unwrap_or(0);
        Self {
            w,
            lo,
            dims,
            raw: vec![UNREAD; n],
            seen: vec![UNREAD; n],
        }
    }

    fn index(&self, (x, y, z): Cell) -> Option<usize> {
        let (dx, dy, dz) = (x - self.lo.0, y - self.lo.1, z - self.lo.2);
        if dx < 0 || dy < 0 || dz < 0 || dx >= self.dims.0 || dy >= self.dims.1 || dz >= self.dims.2
        {
            return None;
        }
        usize::try_from((dy * self.dims.2 + dz) * self.dims.0 + dx).ok()
    }

    /// The block as it is, whether or not it has been seen.
    fn raw(&mut self, c: Cell) -> u8 {
        let Some(i) = self.index(c) else {
            return UNLOADED;
        };
        if let Some(v) = self.raw.get(i).copied()
            && v != UNREAD
        {
            return v;
        }
        let state = self.w.state_at(BlockPos::new(c.0, c.1, c.2));
        let v = match state.and_then(data::block_of_state) {
            None => UNLOADED,
            Some(BlockKind::Air | BlockKind::CaveAir | BlockKind::VoidAir) => OPEN,
            Some(BlockKind::Lava) => LAVA,
            Some(
                BlockKind::NetherPortal
                | BlockKind::EndPortal
                | BlockKind::Obsidian
                | BlockKind::CryingObsidian
                | BlockKind::Bedrock,
            ) => WALL,
            Some(k) => match path::classify(state) {
                Class::Water => WATER,
                Class::Lava => LAVA,
                Class::Solid if stairs::falls(Some(k)) => FALLS,
                Class::Solid => ROCK,
                Class::Awkward => HOT,
                Class::Clear => ODD,
                Class::Unknown => UNLOADED,
            },
        };
        if let Some(slot) = self.raw.get_mut(i) {
            *slot = v;
        }
        v
    }

    fn exposed(&mut self, (x, y, z): Cell) -> bool {
        SIDES
            .iter()
            .any(|(dx, dy, dz)| self.raw((x + dx, y + dy, z + dz)) == OPEN)
    }

    /// The block as the tunnel may know it: see the module's note.
    fn at(&mut self, c: Cell) -> u8 {
        let Some(i) = self.index(c) else {
            return UNLOADED;
        };
        if let Some(v) = self.seen.get(i).copied()
            && v != UNREAD
        {
            return v;
        }
        let r = self.raw(c);
        let v = match r {
            HOT | FALLS => {
                if self.exposed(c) {
                    r
                } else {
                    ROCK
                }
            }
            LAVA | WATER => {
                // The top of this fluid's column, 24 cells up at most.
                let mut top = c;
                for _ in 0..24 {
                    let over = (top.0, top.1 + 1, top.2);
                    if self.raw(over) != r {
                        break;
                    }
                    top = over;
                }
                if self.exposed(top) { r } else { ROCK }
            }
            other => other,
        };
        if let Some(slot) = self.seen.get_mut(i) {
            *slot = v;
        }
        v
    }

    /// May a step open `cells` onto `floor`, by what is known.
    fn takes(&mut self, cells: &[Cell], floor: Cell) -> bool {
        if !matches!(self.at(floor), ROCK | FALLS) {
            return false;
        }
        for c in cells {
            if !matches!(self.at(*c), OPEN | ROCK | HOT | FALLS | ODD) {
                return false;
            }
            for (dx, dy, dz) in SIDES {
                if self.at((c.0 + dx, c.1 + dy, c.2 + dz)) == LAVA {
                    return false;
                }
            }
        }
        cells
            .iter()
            .max_by_key(|c| c.1)
            .is_none_or(|top| self.at((top.0, top.1 + 1, top.2)) != FALLS)
    }

    /// Open cells beside the feet and the head, of the eight.
    fn open_sides(&mut self, (x, y, z): Cell) -> usize {
        DIRS.iter()
            .flat_map(|(dx, dz)| [(x + dx, y, z + dz), (x + dx, y + 1, z + dz)])
            .filter(|c| self.at(*c) == OPEN)
            .count()
    }
}

/// The cells a step from `from` to the next feet cell `to` opens (top first) and the floor it
/// lands on: a level step, a stair up or a stair down, as `stairs` cuts them. `None` when `to` is
/// not one step away.
pub fn opened(from: Cell, to: Cell) -> Option<(Vec<Cell>, Cell)> {
    let (dx, dy, dz) = (to.0 - from.0, to.1 - from.1, to.2 - from.2);
    if dx.abs() + dz.abs() != 1 {
        return None;
    }
    let floor = (to.0, to.1 - 1, to.2);
    match dy {
        0 => Some((vec![(to.0, to.1 + 1, to.2), to], floor)),
        1 => Some((stairs::up_step(from, (dx, dz)).1, floor)),
        -1 => Some((
            vec![(to.0, from.1 + 1, to.2), (to.0, from.1, to.2), to],
            floor,
        )),
        _ => None,
    }
}

fn within_reach((x, y, z): Cell, target: Cell) -> bool {
    let d = [
        f64::from(target.0 - x),
        f64::from(target.1) + 0.5 - f64::from(y),
        f64::from(target.2 - z),
    ];
    (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt() <= REACH
}

fn flat(a: Cell, b: Cell) -> i32 {
    (a.0 - b.0).abs() + (a.2 - b.2).abs()
}

/// A tunnel route: the feet cells from the start (first) on.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Route {
    pub cells: Vec<Cell>,
    /// Whether it ends within reach of the target; else it is the nearest the search came.
    pub found: bool,
    pub expanded: u32,
}

/// The cheapest tunnel found from `from` to within `REACH` of `target`, never stepping onto a
/// `blocked` cell: a weighted A* over level steps and stairs. `None` when there is no way that
/// ends `PARTIAL_GAIN` nearer. Pure and deterministic.
pub fn route(
    w: &World,
    from: Cell,
    target: Cell,
    blocked: &BTreeSet<Cell>,
    max_nodes: u32,
) -> Option<Route> {
    // The box: the feet and the target with a margin, the target pulled in when it is far.
    let pull = |f: i32, t: i32| f + (t - f).clamp(-(MAX_SPAN - 2 * MARGIN), MAX_SPAN - 2 * MARGIN);
    let near = (pull(from.0, target.0), target.1, pull(from.2, target.2));
    let lo = (
        from.0.min(near.0) - MARGIN,
        from.1.min(near.1) - 8,
        from.2.min(near.2) - MARGIN,
    );
    let hi = (
        from.0.max(near.0) + MARGIN,
        from.1.max(near.1).max(SAFE_Y) + RISE,
        from.2.max(near.2) + MARGIN,
    );
    let mut sight = Sight::new(w, lo, hi);
    // Twice the cheapest step a block: the search runs at the target and takes the first way.
    let h = |c: Cell| -> u32 {
        let d = flat(c, target).unsigned_abs() * 6;
        d + (c.1 - target.1).unsigned_abs()
    };
    // Each node: its cost and the node it was reached from.
    let mut best: BTreeMap<Cell, (u32, Cell)> = BTreeMap::new();
    let mut closed: BTreeSet<Cell> = BTreeSet::new();
    let mut heap: BinaryHeap<Reverse<(u32, u32, Cell)>> = BinaryHeap::new();
    best.insert(from, (0, from));
    heap.push(Reverse((h(from), 0, from)));
    let mut nearest = (flat(from, target), 0u32, from);
    let mut expanded = 0;
    let mut end = None;
    while let Some(Reverse((_, g, c))) = heap.pop() {
        if !closed.insert(c) {
            continue;
        }
        if within_reach(c, target) {
            end = Some(c);
            break;
        }
        let d = flat(c, target);
        if d < nearest.0 || (d == nearest.0 && g < nearest.1) {
            nearest = (d, g, c);
        }
        expanded += 1;
        if expanded >= max_nodes {
            break;
        }
        for (dx, dz) in DIRS {
            for dy in [0, 1, -1] {
                let to = (c.0 + dx, c.1 + dy, c.2 + dz);
                if closed.contains(&to) || blocked.contains(&to) {
                    continue;
                }
                let Some((cells, floor)) = opened(c, to) else {
                    continue;
                };
                if !sight.takes(&cells, floor) {
                    continue;
                }
                let digs = cells.iter().filter(|x| sight.at(**x) != OPEN).count();
                let mut cost = 1 + u32::try_from(digs).unwrap_or(3);
                if to.1 < SAFE_Y {
                    cost += LOW_COST;
                }
                if sight.open_sides(to) > 4 {
                    cost += OPEN_COST;
                }
                let g2 = g + cost;
                if best.get(&to).is_none_or(|(old, _)| g2 < *old) {
                    best.insert(to, (g2, c));
                    heap.push(Reverse((g2 + h(to), g2, to)));
                }
            }
        }
    }
    let found = end.is_some();
    let last = match end {
        Some(c) => c,
        None if nearest.0 + PARTIAL_GAIN <= flat(from, target) => nearest.2,
        None => return None,
    };
    let mut cells = vec![last];
    let mut at = last;
    while at != from {
        at = best.get(&at)?.1;
        cells.push(at);
    }
    cells.reverse();
    Some(Route {
        cells,
        found,
        expanded,
    })
}

/// The planner's memory of its tunnel: the target held, the route, the cells found bad.
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Travel {
    pub target: Option<Cell>,
    pub route: Vec<Cell>,
    pub blocked: BTreeSet<Cell>,
    /// The nearest the feet have come to the target on the plane, and the segments begun since.
    pub best: i32,
    pub stalls: u32,
    /// Where the last segment began (its index on the route).
    pub began: Option<usize>,
    /// The cell a segment last ended short of with nothing refused there, and how often.
    pub short: Option<(Cell, u32)>,
    /// Counted for the ledger: routes searched, segments begun, cells blocked.
    pub routes: u32,
    pub segments: u32,
    pub refused: u32,
}

/// What a stretch of route does, for the note: "stairs up", "level" or "stairs down".
fn phase(cells: &[Cell]) -> &'static str {
    let (Some(a), Some(b)) = (cells.first(), cells.last()) else {
        return "level";
    };
    match b.1 - a.1 {
        d if d > 0 => "stairs up",
        d if d < 0 => "stairs down",
        _ => "level",
    }
}

impl Plan {
    /// The next stretch of the travel tunnel toward `target`, or `None` when there is no way (the
    /// target is then refused a while). Called each time the planner looks again: what the last
    /// stretch did is read from where the feet are on the route, a step that failed is blocked
    /// and the route searched again, and a tunnel that stops coming nearer is given up.
    pub(crate) fn travel_tunnel(
        &mut self,
        w: &World,
        target: Cell,
        (what, why): (&str, &str),
        tick: u64,
    ) -> Option<Task> {
        let body = w.body?;
        let feet = path::feet_cell(body.pos);
        if self.travel.target != Some(target) {
            // A new target, or the same fortress's nearer brick: the cells found bad stay bad.
            let keep = self.travel.target.is_some_and(|t| flat(t, target) <= 32);
            if !keep {
                self.travel.blocked.clear();
            }
            self.travel.target = Some(target);
            self.travel.best = flat(feet, target) + 1;
            self.travel.stalls = 0;
            self.travel.short = None;
            self.travel.route.clear();
            self.travel.began = None;
        }
        // What the last stretch did.
        if let Some(began) = self.travel.began.take() {
            match self.travel.route.iter().position(|c| *c == feet) {
                None => {
                    self.note(
                        tick,
                        format!("tunnel off its route at {feet:?}"),
                        "the route is searched again from here".to_owned(),
                    );
                    self.travel.route.clear();
                }
                Some(i) => {
                    let cut = i.saturating_sub(began);
                    let next = self.travel.route.get(i + 1).copied();
                    if let Some(next) = next
                        && cut < STRETCH
                    {
                        let refused = opened(feet, next)
                            .and_then(|(cells, floor)| stairs::refusal(w, &cells, floor));
                        let strikes = match self.travel.short {
                            Some((c, n)) if c == next => n + 1,
                            Some(_) | None => 1,
                        };
                        self.travel.short = Some((next, strikes));
                        if refused.is_some() || strikes >= SHORT_STRIKES {
                            self.travel.blocked.insert(next);
                            self.travel.refused += 1;
                            self.travel.short = None;
                            self.travel.route.clear();
                            self.note(
                                tick,
                                format!("tunnel step to {next:?} refused"),
                                format!(
                                    "{}; blocked, {} so far, the route searched again",
                                    refused.unwrap_or_else(|| format!(
                                        "the step ended short there {strikes} times"
                                    )),
                                    self.travel.refused
                                ),
                            );
                        }
                    }
                }
            }
        }
        // The progress invariant: nearer on the plane, or the stretches run out.
        let d = flat(feet, target);
        if d < self.travel.best {
            self.travel.best = d;
            self.travel.stalls = 0;
        } else {
            self.travel.stalls += 1;
        }
        if self.travel.stalls > MAX_STALLS {
            self.note(
                tick,
                format!("tunnel toward {what} at {target:?} given up"),
                format!(
                    "no nearer than {} in {} stretches ({} routes, {} stretches, {} cells blocked)",
                    self.travel.best,
                    self.travel.stalls,
                    self.travel.routes,
                    self.travel.segments,
                    self.travel.refused
                ),
            );
            self.travel.target = None;
            self.travel.route.clear();
            self.refuse(target, tick);
            return None;
        }
        let on = self.travel.route.iter().position(|c| *c == feet);
        let at = match on {
            Some(i) if i + 1 < self.travel.route.len() => i,
            Some(_) | None => {
                let Some(r) = route(w, feet, target, &self.travel.blocked, MAX_NODES) else {
                    self.note(
                        tick,
                        format!("no tunnel route toward {what} at {target:?}"),
                        format!(
                            "from {feet:?}, {} cells blocked; refused a while",
                            self.travel.blocked.len()
                        ),
                    );
                    self.travel.target = None;
                    self.travel.route.clear();
                    self.refuse(target, tick);
                    return None;
                };
                self.travel.routes += 1;
                self.expanded += u64::from(r.expanded);
                let top = r.cells.iter().map(|c| c.1).max().unwrap_or(feet.1);
                let end = r.cells.last().copied().unwrap_or(feet);
                self.note(
                    tick,
                    format!(
                        "tunnel route {} toward {what} at {target:?}: {} steps",
                        self.travel.routes,
                        r.cells.len().saturating_sub(1)
                    ),
                    format!(
                        "{} at {end:?}, up to y {top}, {} nodes, {} cells blocked",
                        if r.found {
                            "ends within reach"
                        } else {
                            "ends short (the nearest found)"
                        },
                        r.expanded,
                        self.travel.blocked.len()
                    ),
                );
                self.travel.route = r.cells;
                if self.travel.route.len() < 2 {
                    return None;
                }
                0
            }
        };
        let stretch: Vec<Cell> = self
            .travel
            .route
            .iter()
            .skip(at)
            .take(STRETCH + 1)
            .copied()
            .collect();
        self.travel.began = Some(at);
        self.travel.segments += 1;
        self.note(
            tick,
            format!("tunnel {} toward {what} at {target:?}", phase(&stretch)),
            format!(
                "{why}; step {at} of {}, y {}, {d} blocks off on the plane",
                self.travel.route.len().saturating_sub(1),
                feet.1
            ),
        );
        Some(Task::Stairs(Box::new(Stairs::along(stretch, tick))))
    }
}
