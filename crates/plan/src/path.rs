//! A route over the block grid as a pure function of the world (brain/ruststeve/design/pathfinding.md):
//! a port of clojurecraft's `path.clj` at be85a92, constant for constant.
//!
//! A node is the feet cell: standing there means a floor under it and the body and head clear.
//! Moves are data (`TEMPLATES`, rotated to eight directions); goals are data (`Goal`). The search
//! is plain A* with a budget in node expansions, never time, so a recording replays to the same
//! route on any machine; running out returns the route to the node that came nearest, which a
//! walker follows before planning again as chunks arrive. Liquids are walls, never floors, no cell
//! beside or over lava is entered, and blocks the physics cannot simulate are awkward: neither
//! floor nor clear, routed around.

use std::collections::{BTreeMap, BTreeSet};

use data::BlockKind;
use data::generated::shapes::collision;
use data::types::Collision;
use world::{BlockPos, World};

/// A feet cell.
pub type Cell = (i32, i32, i32);

/// Blocks a drop may fall: vanilla damage starts above 3.
pub const MAX_FALL: i32 = 3;
/// One cell on the level.
pub const WALK_COST: f64 = 1.0;
/// One cell up.
pub const JUMP_COST: f64 = 2.0;
/// One cell on the level, cornerwise.
pub const DIAGONAL_COST: f64 = std::f64::consts::SQRT_2;
/// Added to the walk cost for each block of a drop.
pub const FALL_COST_PER_BLOCK: f64 = 0.5;
/// A carpet's height: a flat shape no taller than this the physics steps onto (clojurecraft's
/// `blocks/flat-shape`).
pub const FLAT_SHAPE: f64 = 0.0625;

/// How many nodes a search may spend, by what it is for (clojurecraft's `budgets`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Budget {
    /// A local stand, a walk to a candidate close by.
    Near,
    /// Is one place reachable at all.
    Reach,
    /// One plan, the default.
    Far,
}

impl Budget {
    pub fn expansions(self) -> u32 {
        match self {
            Budget::Near => 400,
            Budget::Reach => 300,
            Budget::Far => 6000,
        }
    }
}

/// What a cell is to a route.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Class {
    Solid,
    Clear,
    Water,
    Lava,
    /// A shape the physics cannot simulate, or one that hurts: neither floor nor clear.
    Awkward,
    /// Not loaded.
    Unknown,
}

/// Definition types the physics cannot simulate, or that hurt, slow or bounce (clojurecraft's
/// `awkward-types`, the same report names without their namespace).
const AWKWARD: &[&str] = &[
    "slab",
    "stair",
    "fence",
    "wall",
    "fence_gate",
    "trapdoor",
    "door",
    "iron_bars",
    "stained_glass_pane",
    "chain",
    "lantern",
    "snow_layer",
    "farmland",
    "dirt_path",
    "cactus",
    "magma",
    "sweet_berry_bush",
    "bed",
    "candle",
    "candle_cake",
    "cake",
    "skull",
    "wall_skull",
    "player_head",
    "player_wall_head",
    "wither_skull",
    "wither_wall_skull",
    "piglinwallskull",
    "flower_pot",
    "end_rod",
    "lightning_rod",
    "pointed_dripstone",
    "amethyst_cluster",
    "big_dripleaf",
    "big_dripleaf_stem",
    "scaffolding",
    "anvil",
    "bell",
    "brewing_stand",
    "campfire",
    "cauldron",
    "layered_cauldron",
    "lava_cauldron",
    "chest",
    "copper_chest",
    "trapped_chest",
    "ender_chest",
    "composter",
    "decorated_pot",
    "enchantment_table",
    "grindstone",
    "hopper",
    "lectern",
    "stonecutter",
    "turtle_egg",
    "sniffer_egg",
    "dragon_egg",
    "daylight_detector",
    "honey",
    "slime",
    "ice",
    "frosted_ice",
    "web",
    "soul_sand",
    "mud",
    "weathering_copper_slab",
    "weathering_copper_stair",
    "weathering_copper_trap_door",
    "weathering_copper_door",
    "weathering_copper_bar",
    "weathering_copper_chain",
    "weathering_copper_chest",
    "weathering_lantern",
    "weathering_lightning_rod",
    "weathering_copper_grate",
    "bamboo_stalk",
    "chorus_plant",
    "chorus_flower",
    "mangrove_roots",
    "heavy_core",
    "vault",
    "trial_spawner",
    "spawner",
    "conduit",
    "beacon",
    "shelf",
];

/// Definition types and names a route stands on though they are not a full cube: a stairs
/// block, an end portal frame, the end portal (clojurecraft's `stood-on`).
const STOOD_ON_TYPES: &[&str] = &["stair"];
const STOOD_ON_BLOCKS: &[BlockKind] = &[BlockKind::EndPortalFrame, BlockKind::EndPortal];

/// Blocks a sheet of ice is made of: the floor when the bot stands on one.
const ICE: &[BlockKind] = &[
    BlockKind::Ice,
    BlockKind::FrostedIce,
    BlockKind::PackedIce,
    BlockKind::BlueIce,
];

/// What a block state is to a route (clojurecraft's `classify-id`).
pub fn classify(state: Option<u32>) -> Class {
    let Some(state) = state else {
        return Class::Unknown;
    };
    let Some(kind) = data::block_of_state(state) else {
        return Class::Unknown;
    };
    match kind {
        BlockKind::Water => return Class::Water,
        BlockKind::Lava => return Class::Lava,
        BlockKind::Fire | BlockKind::SoulFire => return Class::Awkward,
        _ => {}
    }
    let info = kind.info();
    // One layer of snow, the block's default state, has no collision: walked through.
    if kind == BlockKind::Snow && info.is_some_and(|i| i.default_state == state) {
        return Class::Clear;
    }
    let definition = info.map_or("", |i| i.definition);
    if STOOD_ON_TYPES.contains(&definition) || STOOD_ON_BLOCKS.contains(&kind) {
        return Class::Solid;
    }
    let shape = collision(kind);
    let tall_partial = matches!(shape, Some(Collision::Partial { top }) if top > FLAT_SHAPE);
    if AWKWARD.contains(&definition) || tall_partial {
        return Class::Awkward;
    }
    match shape {
        Some(Collision::Full) => Class::Solid,
        Some(Collision::Empty) | Some(Collision::Partial { .. }) | None => Class::Clear,
    }
}

/// The cells of a world as a route sees them, each read once and held for one question
/// (clojurecraft's `cells`): made inside a search, never kept.
pub struct Cells<'a> {
    world: &'a World,
    ice: bool,
    held: BTreeMap<Cell, Class>,
    /// `clear` answers, which read five neighbours for lava each: held too.
    clear: BTreeMap<Cell, bool>,
}

impl<'a> Cells<'a> {
    pub fn new(world: &'a World) -> Self {
        Self {
            world,
            ice: on_ice(world),
            held: BTreeMap::new(),
            clear: BTreeMap::new(),
        }
    }

    pub fn at(&mut self, c: Cell) -> Class {
        if let Some(k) = self.held.get(&c) {
            return *k;
        }
        let state = self.world.state_at(BlockPos::new(c.0, c.1, c.2));
        let mut k = classify(state);
        if self.ice
            && k == Class::Awkward
            && state
                .and_then(data::block_of_state)
                .is_some_and(|b| ICE.contains(&b))
        {
            k = Class::Solid;
        }
        self.held.insert(c, k);
        k
    }

    /// Lava in any of the four horizontal neighbours or under (clojurecraft's `lava-near?`).
    fn lava_near(&mut self, (x, y, z): Cell) -> bool {
        [(1, 0, 0), (-1, 0, 0), (0, 0, 1), (0, 0, -1), (0, -1, 0)]
            .iter()
            .any(|(dx, dy, dz)| self.at((x + dx, y + dy, z + dz)) == Class::Lava)
    }

    pub fn floor(&mut self, c: Cell) -> bool {
        self.at(c) == Class::Solid
    }

    pub fn clear(&mut self, c: Cell) -> bool {
        if let Some(v) = self.clear.get(&c) {
            return *v;
        }
        let v = self.at(c) == Class::Clear && !self.lava_near(c);
        self.clear.insert(c, v);
        v
    }

    pub fn standable(&mut self, (x, y, z): Cell) -> bool {
        self.floor((x, y - 1, z)) && self.clear((x, y, z)) && self.clear((x, y + 1, z))
    }
}

/// Do the feet stand on a sheet of ice (clojurecraft's `on-ice?`).
fn on_ice(w: &World) -> bool {
    let Some([x, y, z]) = w.player.pos else {
        return false;
    };
    let (cx, cy, cz) = (x.floor() as i32, (y + 0.5).floor() as i32, z.floor() as i32);
    let ice_at = |c: Cell| {
        w.block_at(BlockPos::new(c.0, c.1, c.2))
            .is_some_and(|b| ICE.contains(&b))
    };
    if [1, 2].iter().any(|d| ice_at((cx, cy - d, cz))) {
        return true;
    }
    let solid_under = w
        .block_at(BlockPos::new(cx, cy - 1, cz))
        .and_then(collision)
        .is_some_and(|c| c == Collision::Full);
    !solid_under
        && [(1, 0), (-1, 0), (0, 1), (0, -1)]
            .iter()
            .any(|(dx, dz)| ice_at((cx + dx, cy - 1, cz + dz)))
}

/// What a move needs of a cell relative to the node.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Need {
    Floor,
    Clear,
    NotFloor,
}

/// A move along +x; `MOVES` turns it to every direction.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Move {
    pub id: MoveKind,
    pub to: Cell,
    pub cost: f64,
    fall: bool,
    needs: &'static [(Need, Cell)],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum MoveKind {
    Walk,
    JumpUp,
    Drop,
    Diagonal,
}

/// clojurecraft's `templates`, in its order.
const TEMPLATES: [Move; 4] = [
    Move {
        id: MoveKind::Walk,
        to: (1, 0, 0),
        cost: WALK_COST,
        fall: false,
        needs: &[
            (Need::Floor, (1, -1, 0)),
            (Need::Clear, (1, 0, 0)),
            (Need::Clear, (1, 1, 0)),
        ],
    },
    Move {
        id: MoveKind::JumpUp,
        to: (1, 1, 0),
        cost: JUMP_COST,
        fall: false,
        needs: &[
            (Need::Clear, (0, 2, 0)),
            (Need::Floor, (1, 0, 0)),
            (Need::Clear, (1, 1, 0)),
            (Need::Clear, (1, 2, 0)),
        ],
    },
    Move {
        id: MoveKind::Drop,
        to: (1, 0, 0),
        cost: WALK_COST,
        fall: true,
        needs: &[
            (Need::Clear, (1, 0, 0)),
            (Need::Clear, (1, 1, 0)),
            (Need::NotFloor, (1, -1, 0)),
        ],
    },
    Move {
        id: MoveKind::Diagonal,
        to: (1, 0, 1),
        cost: DIAGONAL_COST,
        fall: false,
        needs: &[
            (Need::Floor, (1, -1, 1)),
            (Need::Clear, (1, 0, 1)),
            (Need::Clear, (1, 1, 1)),
            (Need::Clear, (1, 0, 0)),
            (Need::Clear, (1, 1, 0)),
            (Need::Clear, (0, 0, 1)),
            (Need::Clear, (0, 1, 1)),
        ],
    },
];

/// clojurecraft's `rotations`: identity, then quarter turns about y.
fn rotate(r: usize, (x, y, z): Cell) -> Cell {
    match r {
        0 => (x, y, z),
        1 => (-z, y, x),
        2 => (-x, y, -z),
        _ => (z, y, -x),
    }
}

/// One neighbour: the node a move reaches, its cost and the move.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Step {
    pub node: Cell,
    pub cost: f64,
    pub kind: MoveKind,
}

/// Where a drop from `cell` comes to rest, at most `MAX_FALL` down, on a floor only.
fn landing(cells: &mut Cells<'_>, (x, y, z): Cell) -> Option<(Cell, i32)> {
    for fall in 1..=MAX_FALL {
        let feet = (x, y - fall, z);
        let below = (x, y - fall - 1, z);
        if !cells.clear(feet) {
            return None;
        }
        if cells.floor(below) {
            return Some((feet, fall));
        }
        if cells.at(below) != Class::Clear {
            return None;
        }
    }
    None
}

/// Every node reachable from `node` in one move, templates in order, each in four rotations
/// (clojurecraft's `moves` is `(for [m templates rot rotations] ...)`).
pub fn neighbors(cells: &mut Cells<'_>, (x, y, z): Cell) -> Vec<Step> {
    let mut out = Vec::new();
    for m in &TEMPLATES {
        for r in 0..4 {
            let ok = m.needs.iter().all(|(need, off)| {
                let (dx, dy, dz) = rotate(r, *off);
                let c = (x + dx, y + dy, z + dz);
                match need {
                    Need::Floor => cells.floor(c),
                    Need::Clear => cells.clear(c),
                    Need::NotFloor => !cells.floor(c),
                }
            });
            if !ok {
                continue;
            }
            let (dx, dy, dz) = rotate(r, m.to);
            let there = (x + dx, y + dy, z + dz);
            if m.fall {
                if let Some((node, fall)) = landing(cells, there) {
                    out.push(Step {
                        node,
                        cost: m.cost + FALL_COST_PER_BLOCK * f64::from(fall),
                        kind: m.id,
                    });
                }
            } else {
                out.push(Step {
                    node: there,
                    cost: m.cost,
                    kind: m.id,
                });
            }
        }
    }
    out
}

/// Where a route should end (clojurecraft's goal kinds).
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum Goal {
    /// Within `range` of `pos`, octile on the plane plus height.
    Near { pos: Cell, range: f64 },
    /// At exactly `pos`.
    Block { pos: Cell },
    /// Above or below `pos` on the plane.
    Xz { pos: Cell },
    /// Farther than `range` from `pos`, height counted.
    Away { pos: Cell, range: f64 },
    /// Farther than `range` from `pos` on the plane alone.
    Afar { pos: Cell, range: f64 },
    /// Where a block dug at `pos` drops within the pickup box (clojurecraft's `:pickup`):
    /// within `range` of it on the plane, with `pos` at the feet's height or up to `rise` above.
    Pickup { pos: Cell, range: f64, rise: i32 },
}

/// Octile distance on the plane: diagonals at their real cost.
pub fn octile((ax, _, az): Cell, (bx, _, bz): Cell) -> f64 {
    let dx = f64::from((ax - bx).abs());
    let dz = f64::from((az - bz).abs());
    DIAGONAL_COST * dx.min(dz) + (dx.max(dz) - dx.min(dz))
}

/// Octile plus the height difference.
pub fn flat_distance(a: Cell, b: Cell) -> f64 {
    octile(a, b) + f64::from((a.1 - b.1).abs())
}

impl Goal {
    pub fn done(&self, node: Cell) -> bool {
        match *self {
            Goal::Near { pos, range } => flat_distance(node, pos) <= range,
            Goal::Block { pos } => node == pos,
            Goal::Xz { pos } => node.0 == pos.0 && node.2 == pos.2,
            Goal::Away { pos, range } => flat_distance(node, pos) > range,
            Goal::Afar { pos, range } => octile(node, pos) > range,
            Goal::Pickup { pos, range, rise } => {
                octile(node, pos) <= range && node.1 <= pos.1 && pos.1 <= node.1 + rise
            }
        }
    }

    pub fn heuristic(&self, node: Cell) -> f64 {
        match *self {
            Goal::Near { pos, range } => (flat_distance(node, pos) - range).max(0.0),
            Goal::Block { pos } => flat_distance(node, pos),
            Goal::Xz { pos } => octile(node, pos),
            Goal::Away { pos, range } => (range - flat_distance(node, pos)).max(0.0),
            Goal::Afar { pos, range } => (range - octile(node, pos)).max(0.0),
            Goal::Pickup { pos, range, rise } => {
                let up = if pos.1 < node.1 {
                    node.1 - pos.1
                } else if pos.1 > node.1 + rise {
                    pos.1 - node.1 - rise
                } else {
                    0
                };
                (octile(node, pos) - range).max(0.0) + f64::from(up)
            }
        }
    }
}

/// How a search ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Status {
    Found,
    /// The budget ran out: the route goes to the node that came nearest.
    Partial,
    /// The frontier ran out.
    None,
}

/// A route: the waypoints after the start, its cost, how it ended and what it spent.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Route {
    pub waypoints: Vec<Cell>,
    pub cost: f64,
    pub status: Status,
    pub from: Option<Cell>,
    pub expanded: u32,
}

/// An f64 that orders totally, for the open set.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Ord64(f64);

impl Eq for Ord64 {}

impl PartialOrd for Ord64 {
    fn partial_cmp(&self, o: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(o))
    }
}

impl Ord for Ord64 {
    fn cmp(&self, o: &Self) -> std::cmp::Ordering {
        self.0.total_cmp(&o.0)
    }
}

/// A* from `start` toward `goal`, spending at most `max_nodes` expansions (clojurecraft's
/// `search`). The open set is ordered by (f, g, node), so ties break on the node itself.
pub fn search(cells: &mut Cells<'_>, start: Cell, goal: &Goal, max_nodes: u32) -> Route {
    let h0 = goal.heuristic(start);
    let mut open: BTreeSet<(Ord64, Ord64, Cell)> = BTreeSet::new();
    open.insert((Ord64(h0), Ord64(0.0), start));
    let mut g: BTreeMap<Cell, f64> = BTreeMap::new();
    g.insert(start, 0.0);
    let mut parents: BTreeMap<Cell, Cell> = BTreeMap::new();
    let mut closed: BTreeSet<Cell> = BTreeSet::new();
    let (mut best, mut best_h) = (start, h0);
    let mut expanded = 0u32;
    let done =
        |status, parents: &BTreeMap<Cell, Cell>, node: Cell, g: &BTreeMap<Cell, f64>, expanded| {
            let mut back = vec![node];
            let mut at = node;
            while let Some(p) = parents.get(&at) {
                back.push(*p);
                at = *p;
            }
            back.reverse();
            Route {
                waypoints: back.into_iter().skip(1).collect(),
                cost: g.get(&node).copied().unwrap_or(0.0),
                status,
                from: Some(start),
                expanded,
            }
        };
    loop {
        let Some(entry) = open.pop_first() else {
            return done(Status::None, &parents, best, &g, expanded);
        };
        if expanded >= max_nodes {
            return done(Status::Partial, &parents, best, &g, expanded);
        }
        let (_, Ord64(gn), node) = entry;
        if closed.contains(&node) {
            continue;
        }
        if goal.done(node) {
            return done(Status::Found, &parents, node, &g, expanded);
        }
        closed.insert(node);
        let hn = goal.heuristic(node);
        if hn < best_h {
            best = node;
            best_h = hn;
        }
        for s in neighbors(cells, node) {
            let g2 = gn + s.cost;
            if !closed.contains(&s.node) && g2 < g.get(&s.node).copied().unwrap_or(f64::MAX) {
                open.insert((Ord64(g2 + goal.heuristic(s.node)), Ord64(g2), s.node));
                g.insert(s.node, g2);
                parents.insert(s.node, node);
            }
        }
        expanded += 1;
    }
}

/// The node a player at feet position `pos` stands in.
pub fn feet_cell([x, y, z]: [f64; 3]) -> Cell {
    (x.floor() as i32, y.floor() as i32, z.floor() as i32)
}

/// The cells at the feet's height whose columns the body's box overlaps, the one under most of
/// it first (clojurecraft's `rests`): a box over a ledge's edge rests on the block beside.
fn rests(pos: [f64; 3]) -> Vec<Cell> {
    let [px, py, pz] = pos;
    let y = py.floor() as i32;
    let lo = |v: f64| (v - physics::HALF_WIDTH).floor() as i32;
    let hi = |v: f64| (v + physics::HALF_WIDTH).floor() as i32;
    let mut xs = vec![lo(px), hi(px)];
    xs.dedup();
    let mut zs = vec![lo(pz), hi(pz)];
    zs.dedup();
    let away = |(x, _, z): &Cell| {
        let (dx, dz) = (px - (f64::from(*x) + 0.5), pz - (f64::from(*z) + 0.5));
        (dx * dx + dz * dz).sqrt()
    };
    let mut out: Vec<Cell> = xs
        .iter()
        .flat_map(|x| zs.iter().map(move |z| (*x, y, *z)))
        .collect();
    out.sort_by(|a, b| away(a).total_cmp(&away(b)));
    out
}

/// Where a search from the feet begins (clojurecraft's `start-cell`).
fn start_cell(cells: &mut Cells<'_>, from: Cell, under_box: &[Cell]) -> Option<Cell> {
    if cells.standable(from) {
        return Some(from);
    }
    let (x, y, z) = from;
    let near = [0, -1, 1].iter().flat_map(|dy| {
        [0, -1, 1]
            .iter()
            .flat_map(move |dx| [0, -1, 1].iter().map(move |dz| (x + dx, y + dy, z + dz)))
    });
    under_box
        .iter()
        .copied()
        .chain(near)
        .find(|c| cells.standable(*c))
}

/// A route for the feet from `from` toward `goal` (clojurecraft's `plan`): pure and
/// deterministic. It may leave a supported, clear feet cell directly when that reaches the goal;
/// otherwise it starts from `start_cell`.
pub fn plan(world: &World, from: Cell, goal: &Goal, max_nodes: u32) -> Route {
    let mut cells = Cells::new(world);
    let under_box = world
        .player
        .pos
        .filter(|p| feet_cell(*p) == from)
        .map(rests)
        .unwrap_or_default();
    let start = start_cell(&mut cells, from, &under_box);
    let (x, y, z) = from;
    let supported = world
        .block_at(BlockPos::new(x, y - 1, z))
        .and_then(collision)
        .is_some_and(|c| c == Collision::Full);
    if start != Some(from) && supported && cells.clear(from) && cells.clear((x, y + 1, z)) {
        let departure = search(&mut cells, from, goal, max_nodes);
        if departure.status == Status::Found {
            return departure;
        }
    }
    match start {
        Some(s) => search(&mut cells, s, goal, max_nodes),
        None => Route {
            waypoints: Vec::new(),
            cost: 0.0,
            status: Status::None,
            from: None,
            expanded: 0,
        },
    }
}

#[cfg(test)]
mod pickup_tests {
    use super::*;

    #[test]
    fn pickup_is_beside_with_the_block_up_to_rise_above() {
        let g = Goal::Pickup {
            pos: (0, 70, 0),
            range: DIAGONAL_COST,
            rise: 2,
        };
        assert!(g.done((1, 70, 0)));
        assert!(g.done((1, 68, 1)));
        assert!(!g.done((1, 67, 1)), "the block too high");
        assert!(!g.done((1, 71, 0)), "the block below the feet");
        assert!(!g.done((2, 70, 0)), "too far");
        assert_eq!(g.heuristic((1, 70, 0)), 0.0);
        assert!(g.heuristic((5, 70, 0)) > 0.0);
    }
}
