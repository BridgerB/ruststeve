//! Issue 19, the last leg of the way home (agent Q6, docs/problems/eyes-portal.md): from about
//! 120 blocks out to the bot's own Nether portal's front, in rock. The entry portal (cells x 22 to
//! 23, y 34 to 36, z -20) stands on a ledge over the lava sea under the soul sand valley's rim; its
//! one dry standing cell (22, 35, -19) has netherrack behind it at (22, 35, -18)
//! (`home::portal_front`). The heading's legs could not close the way (race 45727347488: lane 1
//! swung east and west at y 97, 60 over the portal, its legs north turned by crimson signs for
//! 6,000 ticks; lane 0 took 8 skeleton arrows in the valley, walked past the portal on the east at
//! y 44 and fell off a ledge, shot, in its pocket). So within `reach` of the portal the heading
//! stands down:
//! 1. **dug in**: standing in the open (air in the two cells over the head), the bot digs straight
//!    down `dig_in` blocks where it stands first (each floor one the staircase takes: no lava
//!    beside, a floor under), out of the skeletons' line;
//! 2. **the tunnel** (`tunnel_toward_covered`) to the front's tunnel cell (to the portal while its
//!    chunk is not loaded), through rock and down the 60 blocks, keeping 2 of rock over the head
//!    where it can (`tunnel::COVER_COST`), no biome sign asked; it ends within the tunnel's reach
//!    (5) of the cell;
//! 3. **the cut**: an exact route of level steps and stairs (each the staircase's own refusals) to
//!    the tunnel cell itself;
//! 4. at the tunnel cell or the standing cell the way home's approach takes over (`home_approach`:
//!    the seals round the standing cell, the walk into the portal, `Task::Enter`).
//!
//! Also (`ledge_dig`): a pocket held on a ledge (air under a side cell or the feet) first digs down
//! one where the bot stands, so a hit does not knock it off (lane 0's death).
//!
//! Each step is noted ("portal way: ...", with the pickaxe uses the leg has spent); the cut is
//! given up after `cut_tries` that come no nearer (the progress invariant).

use std::collections::{BTreeMap, VecDeque};

use world::World;

use crate::home::{self, Front};
use crate::path::{self, Cell};
use crate::stairs::Stairs;
use crate::{Plan, Task};

/// `[portal_way]` in `config/goals.toml`; the defaults are the config's (a test holds them equal).
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct WayRules {
    /// Blocks on the plane from the portal inside which the last leg replaces the heading.
    pub reach: i32,
    /// Blocks (each axis summed) from the tunnel cell inside which the exact cut is searched.
    pub cut_range: i32,
    /// Cells the cut's search may visit.
    pub cut_nodes: usize,
    /// Cuts begun with no step nearer the tunnel cell before the cut is given up.
    pub cut_tries: u32,
    /// Blocks dug straight down where the bot stands in the open before the tunnel, and the
    /// times it digs in at most.
    pub dig_in: i32,
    pub dig_ins: u32,
    /// Blocks dug down under a pocket on a ledge at most, over the way home.
    pub ledge_digs: u32,
}

impl Default for WayRules {
    fn default() -> Self {
        Self {
            reach: 120,
            cut_range: 12,
            cut_nodes: 6000,
            cut_tries: 4,
            dig_in: 3,
            dig_ins: 4,
            ledge_digs: 4,
        }
    }
}

/// The last leg's state, `HomeState::way_in`. Every field defaults.
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct WayIn {
    /// Tunnels and cuts begun, for the ledger.
    pub tunnels: u32,
    pub cuts: u32,
    /// The nearest (each axis summed) a cut has brought the feet to the tunnel cell, and the cuts
    /// begun since without coming nearer.
    pub best: Option<i32>,
    pub stalls: u32,
    /// The cut given up (no nearer in `cut_tries`).
    pub cut_over: bool,
    /// The leg's first note said.
    pub begun: bool,
    /// The pickaxe uses held when the leg began.
    pub uses0: Option<i32>,
    /// The cell a dig-in began at (the open feet cell), the digs-in begun and the blocks dug.
    pub dig_from: Option<Cell>,
    pub dig_ins: u32,
    pub dug: u32,
    /// Blocks dug under a pocket on a ledge.
    pub ledge_dug: u32,
    /// Lava sealed round the standing cell's head, and the last tick a hostile was within
    /// `CARRY_CLEAR` (for the carry-on under the danger line), and the carry-ons begun.
    pub head_seals: u32,
    pub hostile_at: Option<u64>,
    pub carry_ons: u32,
    /// Pickaxe runs begun by the leg's budget, and when the last began.
    pub pick_runs: u32,
    pub pick_at: Option<u64>,
    /// The door last seen (the front's standing cell and tunnel cell), and the times it was
    /// dug open again (filled by an escape's seal).
    pub door: Option<(Cell, Cell)>,
    pub door_digs: u32,
    /// The door's box cells clicked, and how often (one still open after `BOX_CLICKS` is left).
    pub box_clicks: Vec<(Cell, u32)>,
}

/// Clicks on one box cell before it is left (`endermen::POCKET_CLICKS`'s rule: a cell clicked
/// and still open is not clicked for ever).
pub const BOX_CLICKS: u32 = 2;

/// Runs of the leg's own pickaxe budget at most, and the ticks between two (race 45742933645:
/// eight runs in 600 ticks at the pocket, each "table gave up", no room to place it).
pub const PICK_RUNS: u32 = 2;
pub const PICK_EVERY: u64 = 1200;
/// Times the door is dug open again at most.
pub const DOOR_DIGS: u32 = 3;

/// The door's box filled at most: its five cells, twice (a seal washed out by a fall from above),
/// with room.
pub const HEAD_SEALS: u32 = 12;
/// The carry-on under the danger line (the coordinator, 2026-11-25): no hostile within this, in
/// the last `CARRY_QUIET` ticks. Endermen and zombified piglins are neutral and never provoked by
/// the leg: they do not count.
pub const CARRY_CLEAR: f64 = 16.0;
pub const CARRY_QUIET: u64 = 200;
/// Pickaxe uses a block of the leg is budgeted (the coordinator, 2026-11-25: race 45737493003
/// lane 1 spent 267 uses on a leg of 156 blocks each axis summed, 1.7 a block; 2.5 with room).
pub const USES_PER_BLOCK: f64 = 2.5;

/// Blocks on the plane (each axis summed) a new dig-in is from the last: past a dig-in's own
/// shaft and the tunnel stretch begun from it (`tunnel::STRETCH` 6), with room.
const DIG_IN_APART: i32 = 8;

const DIRS: [(i32, i32); 4] = [(1, 0), (-1, 0), (0, 1), (0, -1)];

fn span(a: Cell, b: Cell) -> i32 {
    (a.0 - b.0).abs() + (a.1 - b.1).abs() + (a.2 - b.2).abs()
}

/// The shortest route of level steps and stairs (one up or down a step) from `from` to exactly
/// `to`, each step one the staircase takes (`tunnel::step_refusal` is `None`), never through a
/// cell of `keep_out`, within `range` (each axis summed) of `to`, at most `nodes` cells visited.
/// Breadth first in a fixed order: the same world gives the same route. `None` with no route.
pub fn cut(
    w: &World,
    from: Cell,
    to: Cell,
    keep_out: &[Cell],
    range: i32,
    nodes: usize,
) -> Option<Vec<Cell>> {
    if from == to {
        return Some(vec![from]);
    }
    let mut came: BTreeMap<Cell, Cell> = BTreeMap::new();
    let mut queue = VecDeque::from([from]);
    came.insert(from, from);
    let mut seen = 0;
    while let Some(c) = queue.pop_front() {
        seen += 1;
        if seen > nodes {
            return None;
        }
        for (dx, dz) in DIRS {
            for dy in [0, -1, 1] {
                let n = (c.0 + dx, c.1 + dy, c.2 + dz);
                if came.contains_key(&n)
                    || keep_out.contains(&n)
                    || span(n, to) > range
                    || crate::tunnel::step_refusal(w, c, n).is_some()
                {
                    continue;
                }
                came.insert(n, c);
                if n == to {
                    let mut route = vec![n];
                    let mut at = n;
                    while at != from {
                        at = *came.get(&at)?;
                        route.push(at);
                    }
                    route.reverse();
                    return Some(route);
                }
                queue.push_back(n);
            }
        }
    }
    None
}

impl Plan {
    fn way_rules(&self) -> WayRules {
        self.table
            .as_ref()
            .map_or_else(WayRules::default, |t| t.portal_way.clone())
    }

    /// The pickaxe uses the leg has spent so far, for the notes.
    fn way_uses(&self, w: &World) -> String {
        let now = crate::endermen::pickaxe_uses_left(w);
        let spent = self.ender.home.way_in.uses0.map_or(0, |u| u - now);
        format!("pickaxe uses {spent} spent on the leg, {now} left")
    }

    /// The last leg (module doc): `Some` a dig, a tunnel or a cut toward the portal's front while
    /// within `reach` of `portal` (`d` on the plane), the feet not yet at the tunnel cell or the
    /// standing cell; `None` hands back to the trip and the approach. Called by the trip out of
    /// the approach's range, and inside it by the approach after its seals and its plain walk in.
    pub(crate) fn portal_way(
        &mut self,
        w: &World,
        tick: u64,
        portal: Cell,
        d: i32,
    ) -> Option<Task> {
        let r = self.way_rules();
        if d > r.reach {
            return None;
        }
        let body = w.body?;
        let feet = path::stand_cell(w, body.pos);
        let mut cells = home::portal_cells(w, home::NETHER, portal);
        if cells.is_empty() {
            cells.push(portal);
        }
        let lit: Vec<Cell> = cells
            .into_iter()
            .filter(|c| {
                w.block_at(world::BlockPos::new(c.0, c.1, c.2))
                    == Some(data::BlockKind::NetherPortal)
            })
            .collect();
        let front: Option<Front> = home::portal_front(w, &lit, body.pos);
        if front.is_some_and(|f| feet == f.stand || feet == f.tunnel) {
            return None;
        }
        if let Some(f) = front {
            self.ender.home.way_in.door = Some((f.stand, f.tunnel));
        }
        // No front, but the door was seen and its standing cell is filled now (race 45742933645
        // lane 1: the lava escape sealed lava in the standing cell itself, and the front was gone
        // for 4,500 ticks): dug open again from the tunnel cell.
        if front.is_none()
            && let Some((stand, tunnel)) = self.ender.home.way_in.door
        {
            let filled =
                path::classify(w.state_at(world::BlockPos::new(stand.0, stand.1, stand.2)))
                    == path::Class::Solid;
            if filled && feet == tunnel && self.ender.home.way_in.door_digs < DOOR_DIGS {
                self.ender.home.way_in.door_digs += 1;
                self.note(
                    tick,
                    format!("portal way: the door dug open at {stand:?}"),
                    format!(
                        "the standing cell filled; from the tunnel cell {tunnel:?}; dig {} of {DOOR_DIGS}",
                        self.ender.home.way_in.door_digs
                    ),
                );
                return Some(Task::Dig(crate::dig::Dig::new(w, stand, tick)));
            }
            // Beside the portal with no face: no tunnel at the portal itself (it spun 4,200
            // routes in place).
            if span(feet, portal) <= 6 {
                return None;
            }
        }
        // Every task begins with an empty cursor: a stack on it is put down first (`cursor_watch`
        // waits for no acts, and a dig sends one every tick).
        if w.inventory.carried.is_some() {
            return None;
        }
        // The portal's chunk not loaded (or no face seen yet): the tunnel runs at the portal
        // itself until the front is seen.
        let target = front.map_or(portal, |f| f.tunnel);
        if let Some(t) = self.way_picks(w, tick, span(feet, target)) {
            return Some(t);
        }
        if !self.ender.home.way_in.begun {
            self.ender.home.way_in.begun = true;
            self.ender.home.way_in.uses0 = Some(crate::endermen::pickaxe_uses_left(w));
            self.note(
                tick,
                format!("portal way: the last leg to {target:?}"),
                format!(
                    "from {feet:?}, {d} off on the plane; front {front:?}; the heading stands down"
                ),
            );
        }
        let gap = span(feet, target);
        // 1. Dug in: out of the open where the bot stands, short of the cut's range (by the
        // ledge the cut goes to the cell itself).
        if gap > r.cut_range
            && let Some(t) = self.way_dig_in(w, tick, feet, &r)
        {
            return Some(t);
        }
        // 3. The cut, near: an exact route to the tunnel cell, while it keeps coming nearer.
        if let Some(f) = front
            && gap <= r.cut_range
            && !self.ender.home.way_in.cut_over
        {
            let st = &mut self.ender.home.way_in;
            match st.best {
                Some(b) if gap >= b => st.stalls += 1,
                Some(_) | None => {
                    st.best = Some(gap);
                    st.stalls = 0;
                }
            }
            if st.stalls >= r.cut_tries {
                st.cut_over = true;
                let best = st.best;
                self.note(
                    tick,
                    "portal way: cut given up".to_owned(),
                    format!(
                        "no nearer than {best:?} to {:?} in {} cuts; the approach's own ways",
                        f.tunnel, r.cut_tries
                    ),
                );
                return None;
            }
            let mut keep_out = lit.clone();
            keep_out.push(f.stand);
            if let Some(route) = cut(w, feet, f.tunnel, &keep_out, r.cut_range, r.cut_nodes) {
                self.ender.home.way_in.cuts += 1;
                let uses = self.way_uses(w);
                self.note(
                    tick,
                    format!("portal way: cut to the front at {:?}", f.tunnel),
                    format!(
                        "{} steps from {feet:?}, {gap} off; cut {}; {uses}",
                        route.len().saturating_sub(1),
                        self.ender.home.way_in.cuts
                    ),
                );
                return Some(Task::Stairs(Box::new(Stairs::along(route, tick))));
            }
            self.note(
                tick,
                format!("portal way: no cut to {:?}", f.tunnel),
                format!("from {feet:?}, {gap} off; the tunnel next"),
            );
        }
        // 2. The tunnel: through rock, under cover, to within reach of the tunnel cell.
        // >>> Q8 (issue 19): to the front's tunnel cell the route ends with the feet in it (Q10's
        // race 45774326055: "0 steps (ends within reach)" 4 over it, 10,000 ticks idle).
        self.travel.feet_in = front.map(|f| f.tunnel);
        // <<< Q8
        let t = self.tunnel_toward_covered(
            w,
            target,
            ("the portal's front", "the way home's last leg"),
            tick,
        )?;
        self.ender.home.way_in.tunnels += 1;
        let uses = self.way_uses(w);
        self.note(
            tick,
            format!("portal way: tunnel to {target:?}"),
            format!(
                "from {feet:?}, {d} off on the plane, {gap} from the cell; tunnel {}; {uses}",
                self.ender.home.way_in.tunnels
            ),
        );
        Some(t)
    }

    /// Standing in the open (air in either of the two cells over the head), a dig straight down
    /// where the bot stands, `dig_in` blocks, before the tunnel; each floor dug one the staircase
    /// takes (`stairs::refusal`: no fluid, no lava beside, a floor under it). `None` once dug in,
    /// under cover, or with no safe floor.
    fn way_dig_in(&mut self, w: &World, tick: u64, feet: Cell, r: &WayRules) -> Option<Task> {
        let open = |c: Cell| {
            matches!(
                w.block_at(world::BlockPos::new(c.0, c.1, c.2)),
                Some(data::BlockKind::Air | data::BlockKind::CaveAir)
            )
        };
        let st = &self.ender.home.way_in;
        let digging = st
            .dig_from
            .filter(|s| s.0 == feet.0 && s.2 == feet.2 && feet.1 > s.1 - r.dig_in);
        if digging.is_none() {
            let in_open = open((feet.0, feet.1 + 2, feet.2)) || open((feet.0, feet.1 + 3, feet.2));
            // One dig-in a place: its own shaft reads open over the head (race 45737493003 lane
            // 0 dug in 4 times in one column, 12 deep).
            let fresh = st
                .dig_from
                .is_none_or(|s| (s.0 - feet.0).abs() + (s.2 - feet.2).abs() > DIG_IN_APART);
            if !in_open || !fresh || st.dig_ins >= r.dig_ins {
                return None;
            }
        }
        let floor = (feet.0, feet.1 - 1, feet.2);
        let under = (feet.0, feet.1 - 2, feet.2);
        // >>> Q8: never a dig at an open cell (Q10's race 45777976793: "Gone" 336 times).
        if open(floor) {
            self.note(
                tick,
                format!("portal way: no dig in at {floor:?}"),
                "the cell is open already; the tunnel from where it stands".to_owned(),
            );
            return None;
        }
        // <<< Q8
        if let Some(why) = crate::stairs::refusal(w, &[floor], under) {
            // A dig-in stopped short counts as done here: no new one in this column.
            if let Some(s0) = digging {
                self.ender.home.way_in.dig_from = Some((s0.0, feet.1 + r.dig_in, s0.2));
            }
            self.note(
                tick,
                format!("portal way: no dig in at {floor:?}"),
                format!("{why}; the tunnel from where it stands"),
            );
            return None;
        }
        if digging.is_none() {
            self.ender.home.way_in.dig_from = Some(feet);
            self.ender.home.way_in.dig_ins += 1;
        }
        self.ender.home.way_in.dug += 1;
        let uses = self.way_uses(w);
        self.note(
            tick,
            format!("portal way: dig in at {floor:?}"),
            format!(
                "in the open at {feet:?}; dig-in {}, block {} of the leg; {uses}",
                self.ender.home.way_in.dig_ins, self.ender.home.way_in.dug
            ),
        );
        Some(Task::Dig(crate::dig::Dig::new(w, floor, tick)))
    }

    /// A pocket about to be held on a ledge (open air under the feet's cell beside, a drop, or no
    /// floor under the feet): a dig down one where the bot stands first, its floor one the
    /// staircase takes, at most `ledge_digs` over the way home. Lane 0 of race 45727347488 held
    /// a pocket at (77, 43, -44) on an edge, was shot three times as it built and fell to its
    /// death. `None` off a ledge, past the count, or with no safe floor.
    pub(crate) fn ledge_dig(&mut self, w: &World, tick: u64) -> Option<Task> {
        let r = self.way_rules();
        if self.ender.home.way_in.ledge_dug >= r.ledge_digs {
            return None;
        }
        let body = w.body?;
        let feet = path::stand_cell(w, body.pos);
        let open = |c: Cell| {
            matches!(
                w.block_at(world::BlockPos::new(c.0, c.1, c.2)),
                Some(data::BlockKind::Air | data::BlockKind::CaveAir)
            )
        };
        let edge = *DIRS.iter().find(|(dx, dz)| {
            let side = (feet.0 + dx, feet.1, feet.2 + dz);
            open(side) && open((side.0, side.1 - 1, side.2))
        })?;
        let floor = (feet.0, feet.1 - 1, feet.2);
        let under = (feet.0, feet.1 - 2, feet.2);
        if let Some(why) = crate::stairs::refusal(w, &[floor], under) {
            self.note(
                tick,
                format!("pocket on a ledge, no dig down at {floor:?}"),
                format!("{why}; the drop on {edge:?}"),
            );
            self.ender.home.way_in.ledge_dug = r.ledge_digs;
            return None;
        }
        self.ender.home.way_in.ledge_dug += 1;
        self.note(
            tick,
            format!("pocket on a ledge: dig down at {floor:?}"),
            format!(
                "the drop on {edge:?} beside {feet:?}; dig {} of {}",
                self.ender.home.way_in.ledge_dug, r.ledge_digs
            ),
        );
        Some(Task::Dig(crate::dig::Dig::new(w, floor, tick)))
    }

    /// The leg's pickaxe budget: under `USES_PER_BLOCK` uses a block to go (each axis summed),
    /// a run of the trip's wooden pickaxes (`home::TRIP_WOOD_STEPS`) from held stems or planks,
    /// within the way home's `pick_runs`. `None` with enough, no wood, or a run in hand (the
    /// trip runs it).
    fn way_picks(&mut self, w: &World, tick: u64, to_go: i32) -> Option<Task> {
        let uses = crate::endermen::pickaxe_uses_left(w);
        let need = (f64::from(to_go) * USES_PER_BLOCK).ceil();
        let runs = self.table.as_ref().map_or(8, |t| t.home.pick_runs);
        let wood = crate::base::logs_held(w) + crate::base::planks_held(w);
        let picking = self.base.steps == home::TRIP_WOOD_STEPS.to_vec() && !self.base.over;
        let st = &self.ender.home.way_in;
        if f64::from(uses) >= need
            || picking
            || wood < 2
            || self.ender.home.trip_pick_runs >= runs
            || st.pick_runs >= PICK_RUNS
            || st.pick_at.is_some_and(|t| tick < t + PICK_EVERY)
        {
            return None;
        }
        self.ender.home.way_in.pick_at = Some(tick);
        self.ender.home.trip_pick_runs += 1;
        self.ender.home.way_in.pick_runs += 1;
        self.base = crate::base::BaseState {
            steps: home::TRIP_WOOD_STEPS.to_vec(),
            ..crate::base::BaseState::default()
        };
        self.note(
            tick,
            format!(
                "portal way: pickaxes for the leg ({})",
                self.ender.home.way_in.pick_runs
            ),
            format!(
                "pickaxe uses {uses} under {need} for {to_go} blocks to go; {wood} logs and planks"
            ),
        );
        self.base_task(w, tick)
    }

    /// Lava beside or over the standing cell's head (its four sides and the cell over it), sealed
    /// with filler from where the bot stands before the approach's own seals: race 45737493003
    /// lane 1 sealed (21, 35, -19) beside the standing cell, and the lava falling at (21, 36, -19)
    /// over it spread into the head cell (22, 36, -19) and set the bot alight in the tunnel cell.
    /// `None` with no such lava in reach, no filler, or `HEAD_SEALS` placed.
    pub(crate) fn head_seal(
        &mut self,
        w: &World,
        tick: u64,
        front: Option<&Front>,
    ) -> Option<Task> {
        let f = front?;
        let st = &self.ender.home.way_in;
        if st.head_seals >= HEAD_SEALS {
            return None;
        }
        let body = w.body?;
        let eye = [body.pos[0], body.pos[1] + 1.62, body.pos[2]];
        let reach = |c: Cell| {
            (0..3)
                .map(|i| (eye[i] - (f64::from([c.0, c.1, c.2][i]) + 0.5)).powi(2))
                .sum::<f64>()
                .sqrt()
                <= home::SEAL_REACH
        };
        let lava = |c: Cell| {
            w.block_at(world::BlockPos::new(c.0, c.1, c.2)) == Some(data::BlockKind::Lava)
        };
        let solid = |c: Cell| {
            path::classify(w.state_at(world::BlockPos::new(c.0, c.1, c.2))) == path::Class::Solid
        };
        let open = |c: Cell| {
            matches!(
                w.block_at(world::BlockPos::new(c.0, c.1, c.2)),
                Some(data::BlockKind::Air | data::BlockKind::CaveAir)
            )
        };
        let head = (f.stand.0, f.stand.1 + 1, f.stand.2);
        let feet = path::feet_cell(body.pos);
        // The door's box: the cells beside the standing cell and its head along the portal's
        // plane, and the cell over the head, lava or open, filled in that order (the top first):
        // race 45742933645 lane 1 sealed the foot of the fall at (24, 35, -19), and the fall
        // spread at (24, 36, -19) through (23, 36, -19) into the standing cell itself.
        let side = (f.normal.1, f.normal.0);
        let boxed = [
            (head.0, head.1 + 1, head.2),
            (head.0 + side.0, head.1, head.2 + side.1),
            (head.0 - side.0, head.1, head.2 - side.1),
            (f.stand.0 + side.0, f.stand.1, f.stand.2 + side.1),
            (f.stand.0 - side.0, f.stand.1, f.stand.2 - side.1),
        ];
        // Only by lava at the head's height or over it, beside or in the box (a fall that can
        // spread into the door); lava at the feet's height is the approach's own seals'.
        let high_lava = boxed.iter().any(|c| {
            [
                (0, 0, 0),
                (1, 0, 0),
                (-1, 0, 0),
                (0, 0, 1),
                (0, 0, -1),
                (0, 1, 0),
            ]
            .iter()
            .map(|(dx, dy, dz)| (c.0 + dx, c.1 + dy, c.2 + dz))
            .any(|n| n.1 >= head.1 && lava(n))
        });
        if !high_lava {
            return None;
        }
        let clicks = &self.ender.home.way_in.box_clicks;
        let (target, click, face) = boxed
            .into_iter()
            .filter(|c| {
                !clicks.iter().any(|(b, n)| b == c && *n >= BOX_CLICKS)
                    && (lava(*c) || open(*c))
                    && reach(*c)
                    && *c != feet
                    && *c != (feet.0, feet.1 + 1, feet.2)
            })
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
                .find(|(n, _)| solid(*n))
                .map(|(n, face)| (c, n, face))
            })?;
        let item = crate::endermen::FILLER
            .iter()
            .copied()
            .find(|k| crate::craft::count(w, *k) > 0)?;
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
                self.ender.home.way_in.head_seals += 1;
                match self
                    .ender
                    .home
                    .way_in
                    .box_clicks
                    .iter_mut()
                    .find(|(b, _)| *b == target)
                {
                    Some(e) => e.1 += 1,
                    None => self.ender.home.way_in.box_clicks.push((target, 1)),
                }
                self.acts.push(world::Act::UseOn {
                    pos: world::BlockPos::new(click.0, click.1, click.2),
                    face,
                });
                self.note(
                    tick,
                    format!("portal way: door boxed at {target:?}"),
                    format!(
                        "the box round the standing cell {:?} and its head; {item:?} on {click:?} face {face}; block {} of {HEAD_SEALS}",
                        f.stand,
                        self.ender.home.way_in.head_seals
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

    /// Under the danger line with nothing to heal with, on the last leg in rock (the coordinator,
    /// 2026-11-25; race 45737493003 lane 0 held a pocket at 8.0 for 10,000 ticks): the leg goes on
    /// when not burning, no hostile (`CARRY_CLEAR`, neutrals aside) in the last `CARRY_QUIET`
    /// ticks, the feet under 2 of rock, and the leg's next task a stretch of tunnel, a cut or a
    /// seal whose cells all keep 2 of rock over the head. Never a fight; else `None` and the
    /// pocket holds.
    pub(crate) fn danger_carry_on(&mut self, w: &World, tick: u64) -> Option<Task> {
        let body = w.body?;
        let feet = path::stand_cell(w, body.pos);
        let rock = |c: Cell| {
            path::classify(w.state_at(world::BlockPos::new(c.0, c.1, c.2))) == path::Class::Solid
        };
        let covered = |c: Cell| rock((c.0, c.1 + 2, c.2)) && rock((c.0, c.1 + 3, c.2));
        if !self.carry_quiet(w, tick) || !covered(feet) {
            return None;
        }
        let portal = self.ender.home.portal?;
        let d = {
            let (dx, dz) = (f64::from(feet.0 - portal.0), f64::from(feet.2 - portal.2));
            (dx * dx + dz * dz).sqrt().round() as i32
        };
        // The leg asked as it would be; its stairs kept only when every cell stays covered.
        let t = self.portal_way(w, tick, portal, d)?;
        let ok = match &t {
            Task::Stairs(st) => st.route.iter().all(|c| covered(*c)),
            Task::Idle => true,
            _ => false,
        };
        if !ok {
            return None;
        }
        self.ender.home.way_in.carry_ons += 1;
        if self.ender.home.way_in.carry_ons % 20 == 1 {
            self.note(
                tick,
                "portal way: on under the danger line, in rock".to_owned(),
                format!(
                    "health {:?}, nothing to heal with; no hostile within {CARRY_CLEAR} for {CARRY_QUIET} ticks, 2 of rock over the head; carry-on {}",
                    w.player.health, self.ender.home.way_in.carry_ons
                ),
            );
        }
        Some(t)
    }

    /// Under the danger line at the portal's door (the front's tunnel cell or its standing cell),
    /// quiet and not burning: the approach goes on (its seals and the step in), never a pocket in
    /// the doorway. Noted with the carry-ons.
    pub(crate) fn danger_at_door(&mut self, w: &World, tick: u64) -> bool {
        let Some(body) = w.body else {
            return false;
        };
        let Some(portal) = self.ender.home.portal else {
            return false;
        };
        let feet = path::stand_cell(w, body.pos);
        let lit: Vec<Cell> = home::portal_cells(w, home::NETHER, portal)
            .into_iter()
            .filter(|c| {
                w.block_at(world::BlockPos::new(c.0, c.1, c.2))
                    == Some(data::BlockKind::NetherPortal)
            })
            .collect();
        let at_door = home::portal_front(w, &lit, body.pos)
            .is_some_and(|f| feet == f.stand || feet == f.tunnel);
        if !at_door || !self.carry_quiet(w, tick) {
            return false;
        }
        self.ender.home.way_in.carry_ons += 1;
        if self.ender.home.way_in.carry_ons % 20 == 1 {
            self.note(
                tick,
                "portal way: on into the portal under the danger line".to_owned(),
                format!(
                    "health {:?} at the door {feet:?}; carry-on {}",
                    w.player.health, self.ender.home.way_in.carry_ons
                ),
            );
        }
        true
    }

    /// Not burning, and no hostile (neutrals aside) within `CARRY_CLEAR` in the last
    /// `CARRY_QUIET` ticks; the last one seen is kept.
    fn carry_quiet(&mut self, w: &World, tick: u64) -> bool {
        let Some(body) = w.body else {
            return false;
        };
        let near = w.entities.values().any(|e| {
            !e.dead
                && crate::defend::hostile(e.kind)
                && !matches!(
                    e.kind,
                    data::EntityKind::Enderman | data::EntityKind::ZombifiedPiglin
                )
                && (0..3)
                    .map(|i| (e.pos[i] - body.pos[i]).powi(2))
                    .sum::<f64>()
                    .sqrt()
                    <= CARRY_CLEAR
        });
        if near {
            self.ender.home.way_in.hostile_at = Some(tick);
        }
        let quiet = self
            .ender
            .home
            .way_in
            .hostile_at
            .is_none_or(|t| tick >= t + CARRY_QUIET);
        quiet && !w.player.on_fire
    }
}
