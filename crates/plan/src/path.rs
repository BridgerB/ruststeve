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
    // Soul sand is ground (docs/problems/nether-walk.md): a box 0.875 high the physics stands
    // on, lining every Nether shore. The feet rest at y + 0.875, which `feet_cell` reads as the
    // cell over it.
    if kind == BlockKind::SoulSand {
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
    /// May a search lay a held block as a floor (`MoveKind::Bridge`)? Only a far search
    /// (`Cells::far`), in the Nether, with `BRIDGE_RESERVE` blocks held.
    bridge: bool,
    /// Where the mobs a far route keeps clear of stand (`SHUN_KINDS`).
    shun: Vec<[f64; 3]>,
}

impl<'a> Cells<'a> {
    pub fn new(world: &'a World) -> Self {
        Self {
            world,
            ice: on_ice(world),
            held: BTreeMap::new(),
            clear: BTreeMap::new(),
            bridge: false,
            shun: Vec::new(),
        }
    }

    /// The cells for a far search (`plan_far`): it may bridge, and it keeps clear of the mobs
    /// that kill a bot on sight.
    pub fn far(world: &'a World) -> Self {
        Self {
            bridge: bridges(world),
            shun: world
                .entities
                .values()
                .filter(|e| SHUN_KINDS.contains(&e.kind))
                .map(|e| e.pos)
                .collect(),
            ..Self::new(world)
        }
    }

    /// Is `c` within `SHUN_RADIUS` of a mob to keep clear of? Never within `SHUN_FREE` of the
    /// search's `start`: a bot already beside one must still be given a way off.
    pub fn shunned(&self, c: Cell, start: Cell) -> bool {
        let at = |c: Cell| [f64::from(c.0) + 0.5, f64::from(c.1), f64::from(c.2) + 0.5];
        let d = |a: [f64; 3], b: [f64; 3]| {
            ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
        };
        !self.shun.is_empty()
            && d(at(c), at(start)) > SHUN_FREE
            && self.shun.iter().any(|m| d(at(c), *m) <= SHUN_RADIUS)
    }

    /// Can a held block be laid at `c` as a floor: air or lava there (a block replaces a fluid),
    /// never water (the overworld's walks are not changed here), fire or anything awkward.
    pub fn layable(&mut self, c: Cell) -> bool {
        self.bridge && self.replaceable(c)
    }

    fn replaceable(&mut self, c: Cell) -> bool {
        match self.at(c) {
            Class::Lava => true,
            Class::Clear => matches!(
                self.world.block_at(BlockPos::new(c.0, c.1, c.2)),
                Some(BlockKind::Air | BlockKind::CaveAir | BlockKind::VoidAir)
            ),
            Class::Solid | Class::Water | Class::Awkward | Class::Unknown => false,
        }
    }

    /// Clear, with no lava beside: the feet's cell of a bridge move, whose floor (lava, as like
    /// as not) is laid before the feet come.
    pub fn clear_over(&mut self, (x, y, z): Cell) -> bool {
        self.at((x, y, z)) == Class::Clear
            && ![(1, 0), (-1, 0), (0, 1), (0, -1)]
                .iter()
                .any(|(dx, dz)| self.at((x + dx, y, z + dz)) == Class::Lava)
    }

    /// A waypoint a route may still use: standable, or (a bridge's) clear with a floor to lay.
    pub fn usable(&mut self, c: Cell, kind: MoveKind) -> bool {
        self.standable(c)
            || (kind == MoveKind::Bridge
                && self.clear_over(c)
                && self.clear((c.0, c.1 + 1, c.2))
                && self.layable((c.0, c.1 - 1, c.2)))
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

/// Blocks a bridge is laid with, the commonest first: full cubes that do not fall or burn, what a
/// digging bot carries by the hundred.
pub const BRIDGE_BLOCKS: &[data::ItemKind] = &[
    data::ItemKind::Cobblestone,
    data::ItemKind::CobbledDeepslate,
    data::ItemKind::Netherrack,
    data::ItemKind::Granite,
    data::ItemKind::Diorite,
    data::ItemKind::Andesite,
    data::ItemKind::Tuff,
    data::ItemKind::Dirt,
];
/// Blocks held before a search may bridge: fewer is not a crossing.
pub const BRIDGE_RESERVE: i64 = 16;
/// Added to a walk's cost for a cell bridged: a route takes ground up to four times as long
/// before it lays a block (and a block laid is a block spent).
pub const BRIDGE_COST: f64 = 3.0;

/// Mobs a far route keeps clear of: they attack on sight a bot with no gold on, and two piglins
/// took a bot in iron from 20 health to 0 in 100 ticks (race 45337580375 lane 1,
/// docs/problems/nether-walk.md).
pub const SHUN_KINDS: &[data::EntityKind] = &[
    data::EntityKind::Piglin,
    data::EntityKind::PiglinBrute,
    data::EntityKind::Hoglin,
];
/// A far route enters no cell within this of one: a piglin sees a player at 16.
pub const SHUN_RADIUS: f64 = 20.0;
/// Cells this near the search's start are free of the rule.
pub const SHUN_FREE: f64 = 6.0;

/// The bridge blocks held, every kind together.
pub fn bridge_blocks(w: &World) -> i64 {
    w.inventory
        .slots
        .values()
        .filter(|(k, _)| BRIDGE_BLOCKS.contains(k))
        .map(|(_, n)| i64::from(*n))
        .sum()
}

/// The next bridge block to hold: the kind with the most held.
pub fn bridge_item(w: &World) -> Option<data::ItemKind> {
    BRIDGE_BLOCKS
        .iter()
        .copied()
        .map(|k| {
            let n: i64 = w
                .inventory
                .slots
                .values()
                .filter(|(i, _)| *i == k)
                .map(|(_, n)| i64::from(*n))
                .sum();
            (n, k)
        })
        .filter(|(n, _)| *n > 0)
        .max_by_key(|(n, _)| *n)
        .map(|(_, k)| k)
}

/// May a route bridge in this world: in the Nether (its seas are lava and its ground is in
/// pieces; the overworld's walks are left as they were), with blocks to spare.
fn bridges(w: &World) -> bool {
    w.dimension
        .as_ref()
        .is_some_and(|d| d.name == "minecraft:the_nether")
        && bridge_blocks(w) >= BRIDGE_RESERVE
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
    /// Air or lava a held block can be laid in (never where bridging is off).
    Layable,
    /// Clear with no lava beside, whatever is under: over a floor still to lay.
    ClearOver,
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
    /// One cell on the level onto a floor laid first: a held block placed against the side of
    /// the block under the feet (docs/problems/nether-walk.md).
    Bridge,
}

/// clojurecraft's `templates`, in its order.
const TEMPLATES: [Move; 5] = [
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
    Move {
        id: MoveKind::Bridge,
        to: (1, 0, 0),
        cost: WALK_COST + BRIDGE_COST,
        fall: false,
        needs: &[
            (Need::Layable, (1, -1, 0)),
            (Need::ClearOver, (1, 0, 0)),
            (Need::Clear, (1, 1, 0)),
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
                    Need::Layable => cells.layable(c),
                    Need::ClearOver => cells.clear_over(c),
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
    /// The move that reaches each waypoint, one a waypoint.
    #[serde(default)]
    pub kinds: Vec<MoveKind>,
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
    search_weighted(cells, start, goal, max_nodes, 1.0)
}

/// `search` with the heuristic weighed: f = g + `weight` × h. At 1 it is A* and the route is the
/// cheapest; over 1 the search leans toward the goal and reaches far ones in a fraction of the
/// expansions, its route at most `weight` times the cheapest (docs/problems/nether-walk.md: a
/// fortress 110 blocks off is past 6,000 plain expansions).
pub fn search_weighted(
    cells: &mut Cells<'_>,
    start: Cell,
    goal: &Goal,
    max_nodes: u32,
    weight: f64,
) -> Route {
    let h0 = goal.heuristic(start);
    let mut open: BTreeSet<(Ord64, Ord64, Cell)> = BTreeSet::new();
    open.insert((Ord64(weight * h0), Ord64(0.0), start));
    let mut g: BTreeMap<Cell, f64> = BTreeMap::new();
    g.insert(start, 0.0);
    let mut parents: BTreeMap<Cell, (Cell, MoveKind)> = BTreeMap::new();
    let mut closed: BTreeSet<Cell> = BTreeSet::new();
    let (mut best, mut best_h) = (start, h0);
    let mut expanded = 0u32;
    let done = |status,
                parents: &BTreeMap<Cell, (Cell, MoveKind)>,
                node: Cell,
                g: &BTreeMap<Cell, f64>,
                expanded| {
        let mut back = Vec::new();
        let mut at = node;
        while let Some((p, kind)) = parents.get(&at) {
            back.push((at, *kind));
            at = *p;
        }
        back.reverse();
        Route {
            waypoints: back.iter().map(|(c, _)| *c).collect(),
            cost: g.get(&node).copied().unwrap_or(0.0),
            status,
            from: Some(start),
            expanded,
            kinds: back.iter().map(|(_, k)| *k).collect(),
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
            if cells.shunned(s.node, start) {
                continue;
            }
            let g2 = gn + s.cost;
            if !closed.contains(&s.node) && g2 < g.get(&s.node).copied().unwrap_or(f64::MAX) {
                open.insert((
                    Ord64(g2 + weight * goal.heuristic(s.node)),
                    Ord64(g2),
                    s.node,
                ));
                g.insert(s.node, g2);
                parents.insert(s.node, (node, s.kind));
            }
        }
        expanded += 1;
    }
}

/// Feet this far under a cell's floor line stand in that cell: on soul sand the feet rest at
/// y + 0.875, an eighth under the cell a route gives them (docs/problems/nether-walk.md).
pub const FEET_LIFT: f64 = 0.2;

/// The node a player at feet position `pos` stands in: the cell of the feet, or the one over it
/// when they rest on a box just short of a full block (soul sand).
pub fn feet_cell([x, y, z]: [f64; 3]) -> Cell {
    (
        x.floor() as i32,
        (y + FEET_LIFT).floor() as i32,
        z.floor() as i32,
    )
}

/// The cells at the feet's height whose columns the body's box overlaps, the one under most of
/// it first (clojurecraft's `rests`): a box over a ledge's edge rests on the block beside.
fn rests(pos: [f64; 3]) -> Vec<Cell> {
    let [px, py, pz] = pos;
    let y = (py + FEET_LIFT).floor() as i32;
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
    plan_weighted(world, from, goal, max_nodes, 1.0)
}

/// `plan` with the heuristic weighed (`search_weighted`).
pub fn plan_weighted(world: &World, from: Cell, goal: &Goal, max_nodes: u32, weight: f64) -> Route {
    plan_over(Cells::new(world), from, goal, max_nodes, weight)
}

/// A route to a goal far off (docs/problems/nether-walk.md): weighed, over `Cells::far`, so it
/// may bridge and keeps clear of the mobs that kill on sight. A plain `plan` does neither: an
/// explore's walk laid nine blocks out over a lava sea on its first try.
pub fn plan_far(world: &World, from: Cell, goal: &Goal, max_nodes: u32, weight: f64) -> Route {
    plan_over(Cells::far(world), from, goal, max_nodes, weight)
}

fn plan_over(mut cells: Cells<'_>, from: Cell, goal: &Goal, max_nodes: u32, weight: f64) -> Route {
    let world = cells.world;
    let under_box = world
        .player
        .pos
        .filter(|p| feet_cell(*p) == from)
        .map(rests)
        .unwrap_or_default();
    let (x, y, z) = from;
    let start = start_cell(&mut cells, from, &under_box);
    let supported = world
        .block_at(BlockPos::new(x, y - 1, z))
        .and_then(collision)
        .is_some_and(|c| c == Collision::Full);
    if start != Some(from) && supported && cells.clear(from) && cells.clear((x, y + 1, z)) {
        let departure = search_weighted(&mut cells, from, goal, max_nodes, weight);
        if departure.status == Status::Found {
            return departure;
        }
    }
    match start {
        Some(s) => search_weighted(&mut cells, s, goal, max_nodes, weight),
        None => Route {
            waypoints: Vec::new(),
            cost: 0.0,
            status: Status::None,
            from: None,
            expanded: 0,
            kinds: Vec::new(),
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
