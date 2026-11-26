//! Issue 19, the last leg of the way home (agent Q6, docs/problems/eyes-portal.md): from about 64
//! blocks out to the bot's own Nether portal's front, by rock. The entry portal (cells x 22 to 23,
//! y 34 to 36, z -20) stands on a ledge over the lava sea; its one dry standing cell (22, 35, -19)
//! has netherrack behind it at (22, 35, -18) (`home::portal_front`). The heading's legs could not
//! close the last 48 blocks (race 45727347488: lane 1 swung east and west at y 97, 60 over the
//! portal, its legs north turned by the crimson forest's signs for 6,000 ticks; lane 0 walked past
//! it on the east at y 44 and was shot down by soul sand valley skeletons). So within `reach` of
//! the portal, with its front seen, the heading stands down:
//! 1. **the tunnel** (`tunnel_toward`) to the front's tunnel cell, through rock and down the 60
//!    blocks, no biome sign asked; it ends within the tunnel's reach (5) of the cell;
//! 2. **the cut**: an exact route of level steps and stairs (each the staircase's own refusals:
//!    no lava beside, a floor, nothing falling) from there to the tunnel cell itself;
//! 3. at the tunnel cell or the standing cell the way home's approach takes over (`home_approach`:
//!    the seals round the standing cell, the walk into the portal, `Task::Enter`).
//!
//! Each try is noted ("portal way: ..."); the cut is given up after `cut_tries` that come no
//! nearer (the progress invariant), and then the approach's own ways are left.

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
}

impl Default for WayRules {
    fn default() -> Self {
        Self {
            reach: 64,
            cut_range: 12,
            cut_nodes: 6000,
            cut_tries: 4,
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

    /// The last leg (module doc): `Some` a tunnel or a cut toward the portal's front while within
    /// `reach` of `portal` (`d` on the plane) with its front seen and the feet not yet at the
    /// tunnel cell or the standing cell; `None` hands back to the trip and the approach.
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
        let front: Front = home::portal_front(w, &lit, body.pos)?;
        if feet == front.stand || feet == front.tunnel {
            return None;
        }
        if !self.ender.home.way_in.begun {
            self.ender.home.way_in.begun = true;
            self.note(
                tick,
                format!("portal way: the last leg to the front at {:?}", front.tunnel),
                format!(
                    "from {feet:?}, {d} off on the plane; standing cell {:?}, the heading stands down",
                    front.stand
                ),
            );
        }
        let gap = span(feet, front.tunnel);
        // The cut, near: an exact route to the tunnel cell, while it keeps coming nearer.
        if gap <= r.cut_range && !self.ender.home.way_in.cut_over {
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
                        front.tunnel, r.cut_tries
                    ),
                );
                return None;
            }
            let mut keep_out = lit.clone();
            keep_out.push(front.stand);
            if let Some(route) = cut(w, feet, front.tunnel, &keep_out, r.cut_range, r.cut_nodes) {
                self.ender.home.way_in.cuts += 1;
                self.note(
                    tick,
                    format!("portal way: cut to the front at {:?}", front.tunnel),
                    format!(
                        "{} steps from {feet:?}, {gap} off; cut {}",
                        route.len().saturating_sub(1),
                        self.ender.home.way_in.cuts
                    ),
                );
                return Some(Task::Stairs(Box::new(Stairs::along(route, tick))));
            }
            self.note(
                tick,
                format!("portal way: no cut to {:?}", front.tunnel),
                format!("from {feet:?}, {gap} off; the tunnel next"),
            );
        }
        // The tunnel, far: through rock to within reach of the tunnel cell.
        let t = self.tunnel_toward(
            w,
            front.tunnel,
            ("the portal's front", "the way home's last leg"),
            tick,
        )?;
        self.ender.home.way_in.tunnels += 1;
        self.note(
            tick,
            format!("portal way: tunnel to the front at {:?}", front.tunnel),
            format!(
                "from {feet:?}, {d} off on the plane, {gap} from the cell; tunnel {}",
                self.ender.home.way_in.tunnels
            ),
        );
        Some(t)
    }
}
