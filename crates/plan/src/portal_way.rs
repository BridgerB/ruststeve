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
}

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
    /// standing cell; `None` hands back to the trip and the approach.
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
        // Every task begins with an empty cursor: a stack on it is put down first (`cursor_watch`
        // waits for no acts, and a dig sends one every tick).
        if w.inventory.carried.is_some() {
            return None;
        }
        // Within the approach with a plain walk into a lit cell, the approach walks in.
        let approach = self.table.as_ref().map_or(24, |t| t.home.approach);
        if d <= approach
            && lit.iter().any(|c| {
                let mut walk = crate::walk::Walk::new(
                    *c,
                    path::Goal::Block { pos: *c },
                    crate::walk::Arrive::Goal,
                    tick,
                );
                walk.route_now(w, tick) && walk.route == Some(path::Status::Found)
            })
        {
            return None;
        }
        // The portal's chunk not loaded (or no face seen yet): the tunnel runs at the portal
        // itself until the front is seen.
        let target = front.map_or(portal, |f| f.tunnel);
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
        // 1. Dug in: out of the open where the bot stands.
        if let Some(t) = self.way_dig_in(w, tick, feet, &r) {
            return Some(t);
        }
        let gap = span(feet, target);
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
            if !in_open || st.dig_ins >= r.dig_ins {
                return None;
            }
        }
        let floor = (feet.0, feet.1 - 1, feet.2);
        let under = (feet.0, feet.1 - 2, feet.2);
        if let Some(why) = crate::stairs::refusal(w, &[floor], under) {
            if digging.is_some() {
                self.ender.home.way_in.dig_from = None;
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
}
