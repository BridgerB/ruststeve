//! Building from a plan that is data (issue 14c, brain/ruststeve/design/portal-cast.md): a port
//! of clojurecraft's `build.clj` plans (be85a92).
//!
//! A plan is a list of ops. Every op says what it does (`OpKind`), where (`pos`, and `stand`
//! where the bot stands to do it), with what (`item`), and when it is done (`done`, a predicate
//! over the world). Nothing about progress is stored: the op worked on is the first one not
//! satisfied, so a build resumes after a death, a chunk reload or a relaunch by reading the world.
//!
//! `portal_plan` casts the frame as a player does: the rock is the mould. Beside a lava pool, at
//! any depth, the frame's ten cells are holes cut into a face of the rock, each a block deep;
//! lava is poured into a hole, water into the cell directly over it against the same wall (it
//! lands on the source: obsidian) and scooped straight back. Positions are written for the bot
//! working from +z (south) and turned for the other facings. Never dig obsidian: ruststeve v1 dug
//! its own frame and restarted forever.

use data::{BlockKind, ItemKind};
use world::{BlockPos, World};

use crate::Cell;

/// The side of the frame the bot works from.
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
pub enum Facing {
    South,
    North,
    East,
    West,
}

impl Facing {
    pub const ALL: [Facing; 4] = [Facing::South, Facing::West, Facing::North, Facing::East];

    pub fn name(self) -> &'static str {
        match self {
            Facing::South => "south",
            Facing::North => "north",
            Facing::East => "east",
            Facing::West => "west",
        }
    }

    /// A point `[x z]` on the plane, written for south, turned for this facing (`turn`).
    fn turn(self, [x, z]: [f64; 2]) -> [f64; 2] {
        match self {
            Facing::South => [x, z],
            Facing::North => [-x, -z],
            Facing::East => [z, -x],
            Facing::West => [-z, x],
        }
    }
}

/// When an op is done: a predicate over the world (`holds?`).
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum Done {
    Solid(Cell),
    NotSolid(Cell),
    /// Solid and not a block that falls when the cell under it opens (gravel, sand).
    Firm(Cell),
    Block(Cell, BlockKind),
    LavaSource(Cell),
    NoWaterSource(Cell),
    Dimension(String),
    Or(Vec<Done>),
    And(Vec<Done>),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum OpKind {
    Place,
    Dig,
    Pour,
    Scoop,
    UseOn,
    StandIn,
    Verify,
}

/// Why an op is in the plan (clojurecraft's `:build/step`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Step {
    Landing,
    Room,
    Floor,
    FallClear,
    Wall,
    Hole,
    Over,
    Lava,
    Water,
    Scoop,
    Interior,
    Light,
    Enter,
    Verify,
}

impl Step {
    pub fn name(self) -> &'static str {
        match self {
            Step::Landing => "landing",
            Step::Room => "room",
            Step::Floor => "floor",
            Step::FallClear => "fall-clear",
            Step::Wall => "wall",
            Step::Hole => "hole",
            Step::Over => "over",
            Step::Lava => "lava",
            Step::Water => "water",
            Step::Scoop => "scoop",
            Step::Interior => "interior",
            Step::Light => "light",
            Step::Enter => "enter",
            Step::Verify => "verify",
        }
    }
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Op {
    pub kind: OpKind,
    pub pos: Option<Cell>,
    pub stand: Cell,
    pub item: Option<ItemKind>,
    /// Where the look points for a pour or a scoop.
    pub aim: Option<[f64; 3]>,
    /// The cell a pour fills.
    pub into: Option<Cell>,
    /// The frame cell `[dx dy]` a pour or a scoop casts.
    pub cell: Option<(i32, i32)>,
    /// The face a use-on clicks (1 up).
    pub face: Option<i8>,
    pub step: Step,
    pub done: Done,
}

/// The frame cells `[dx dy]` from the origin in cast order: the bottom pair, the two columns a
/// block a layer, the top pair. Never sorted by height: a sort interleaves the columns and the
/// bot crosses the frame for every block (steve cast.ts 2510-2521).
pub const FRAME_CELLS: [(i32, i32); 10] = [
    (1, 0),
    (2, 0),
    (0, 1),
    (3, 1),
    (0, 2),
    (3, 2),
    (0, 3),
    (3, 3),
    (1, 4),
    (2, 4),
];

/// The steps `[x feet]` in the row three in front of the frame the upper cells are cast from.
const STAIR: [(i32, i32); 5] = [(0, 1), (1, 2), (2, 3), (3, 2), (4, 1)];

/// Each frame cell → the cell `[x feet dz]` the bot stands in to cast it (`stands`).
fn stand_of((dx, dy): (i32, i32)) -> (i32, i32, i32) {
    match (dx, dy) {
        (1, 0) => (1, 0, 2),
        (2, 0) => (2, 0, 2),
        (0, 1) => (0, 0, 2),
        (3, 1) => (3, 0, 2),
        (0, 2) => (0, 1, 3),
        (3, 2) => (4, 1, 3),
        (0, 3) => (1, 2, 3),
        (3, 3) => (3, 2, 3),
        _ => (2, 3, 3),
    }
}

/// The lava's stand at most this far over the room's floor (`max-access`).
pub const MAX_ACCESS: i32 = 6;

/// Where the plan sits: its origin cell and facing.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Frame {
    pub origin: Cell,
    pub facing: Facing,
}

impl Frame {
    /// The absolute point at `[x y z]` from the origin (written for south), turned about the
    /// middle of the origin's cell (`point`).
    pub fn point(self, [x, y, z]: [f64; 3]) -> [f64; 3] {
        let (bx, by, bz) = self.origin;
        let [tx, tz] = self.facing.turn([x - 0.5, z - 0.5]);
        [
            f64::from(bx) + 0.5 + tx,
            f64::from(by) + y,
            f64::from(bz) + 0.5 + tz,
        ]
    }

    /// The absolute cell at `[dx dy dz]` (`cell`).
    pub fn at(self, (dx, dy, dz): (i32, i32, i32)) -> Cell {
        let p = self.point([f64::from(dx) + 0.5, f64::from(dy), f64::from(dz) + 0.5]);
        (
            p[0].floor() as i32,
            p[1].floor() as i32,
            p[2].floor() as i32,
        )
    }

    /// The point at offset `a` inside cell `c` (`aim`).
    fn aim(self, (dx, dy, dz): (i32, i32, i32), [ax, ay, az]: [f64; 3]) -> [f64; 3] {
        self.point([f64::from(dx) + ax, f64::from(dy) + ay, f64::from(dz) + az])
    }
}

fn cast(f: Frame, (dx, dy): (i32, i32)) -> Done {
    Done::Block(f.at((dx, dy, 0)), BlockKind::Obsidian)
}

fn layer_cast(f: Frame, cells: &[(i32, i32)]) -> Done {
    Done::And(cells.iter().map(|c| cast(f, *c)).collect())
}

fn place_op(pos: Cell, stand: Cell, guard: Option<Done>, step: Step) -> Op {
    Op {
        kind: OpKind::Place,
        pos: Some(pos),
        stand,
        item: Some(ItemKind::Cobblestone),
        aim: None,
        into: None,
        cell: None,
        face: None,
        step,
        done: match guard {
            Some(g) => Done::Or(vec![g, Done::Solid(pos)]),
            None => Done::Solid(pos),
        },
    }
}

/// Dig a solid at `pos`, done once nothing solid is there, `guard` holds, or it is obsidian.
fn dig_op(pos: Cell, stand: Cell, guard: Option<Done>, step: Step) -> Op {
    let mut done = vec![Done::Block(pos, BlockKind::Obsidian), Done::NotSolid(pos)];
    done.extend(guard);
    Op {
        kind: OpKind::Dig,
        pos: Some(pos),
        stand,
        item: None,
        aim: None,
        into: None,
        cell: None,
        face: None,
        step,
        done: Done::Or(done),
    }
}

/// A firm wall at `pos` where the rock is missing: a falling block dug out, then a block placed;
/// both done once `pos` is firm or `guard` holds. A portal block there is a wall already.
fn firm_ops(pos: Cell, stand: Cell, guard: Done, step: Step) -> [Op; 2] {
    let walled = Done::Or(vec![
        guard.clone(),
        Done::Block(pos, BlockKind::NetherPortal),
    ]);
    [
        Op {
            kind: OpKind::Dig,
            pos: Some(pos),
            stand,
            item: None,
            aim: None,
            into: None,
            cell: None,
            face: None,
            step: Step::FallClear,
            done: Done::Or(vec![guard, Done::Firm(pos), Done::NotSolid(pos)]),
        },
        place_op(pos, stand, Some(walled), step),
    ]
}

/// The x offset inside a target cell for a ray from a stand `side` away (`side-aim`).
fn side_aim(side: i32) -> f64 {
    0.5 - 0.3 * f64::from(side.signum())
}

/// Cast frame cell `c` (`cell-ops`).
fn cell_ops(f: Frame, (dx, dy): (i32, i32)) -> Vec<Op> {
    let s = stand_of((dx, dy));
    let stand = f.at(s);
    let side = s.0 - dx;
    let hole = f.at((dx, dy, 0));
    let over = f.at((dx, dy + 1, 0));
    let done = cast(f, (dx, dy));
    let walls = [
        (dx, dy, -1),
        (dx, dy - 1, 0),
        (dx - 1, dy, 0),
        (dx + 1, dy, 0),
        (dx, dy + 1, -1),
    ];
    let mut out: Vec<Op> = walls
        .iter()
        .flat_map(|w| firm_ops(f.at(*w), stand, done.clone(), Step::Wall))
        .collect();
    out.push(dig_op(hole, stand, Some(done.clone()), Step::Hole));
    out.push(dig_op(over, stand, Some(done.clone()), Step::Over));
    let pour = |item, aim, into, step, d| Op {
        kind: OpKind::Pour,
        pos: None,
        stand,
        item: Some(item),
        aim: Some(aim),
        into: Some(into),
        cell: Some((dx, dy)),
        face: None,
        step,
        done: d,
    };
    out.push(pour(
        ItemKind::LavaBucket,
        f.aim((dx, dy, 0), [side_aim(side), 0.3, -0.02]),
        hole,
        Step::Lava,
        Done::Or(vec![done.clone(), Done::LavaSource(hole)]),
    ));
    out.push(pour(
        ItemKind::WaterBucket,
        f.aim((dx, dy + 1, 0), [side_aim(side), 0.5, -0.02]),
        over,
        Step::Water,
        done,
    ));
    out.push(Op {
        kind: OpKind::Scoop,
        pos: Some(over),
        stand,
        item: Some(ItemKind::Bucket),
        aim: Some(f.aim((dx, dy + 1, 0), [side_aim(side), 0.4, 0.5])),
        into: None,
        cell: Some((dx, dy)),
        face: None,
        step: Step::Scoop,
        done: Done::NoWaterSource(over),
    });
    out
}

/// The steps from the top of the stair up to `h` over the room's floor (`access`).
pub fn access(h: i32) -> Vec<(i32, i32, i32)> {
    (1..=(h - 3)).map(|i| (2, 3 + i, 3 + i)).collect()
}

/// The cell of the room at height `h` from which a level way leads to the lava (`landing`).
pub fn landing(h: i32) -> (i32, i32, i32) {
    if h > 3 {
        access(h).last().copied().unwrap_or((2, 0, 2))
    } else if h > 0 {
        STAIR
            .iter()
            .find(|(_, feet)| *feet == h)
            .map_or((2, 0, 2), |(x, feet)| (*x, *feet, 3))
    } else {
        (2, 0, 2)
    }
}

/// The cells a stand needs open: the feet, the head, and the room a jump takes (`body`).
fn body((x, feet, dz): (i32, i32, i32)) -> [(i32, i32, i32); 3] {
    [(x, feet, dz), (x, feet + 1, dz), (x, feet + 2, dz)]
}

/// Every cell the full cast opens in front of the frame, nearest the landing first (`room-cells`).
fn room_cells(h: i32) -> Vec<(i32, i32, i32)> {
    let from = landing(h);
    let mut cells: Vec<(i32, i32, i32)> = Vec::new();
    let mut push = |c: (i32, i32, i32)| {
        if !cells.contains(&c) {
            cells.push(c);
        }
    };
    for x in 0..5 {
        for y in 0..6 {
            push((x, y, 1));
        }
    }
    for x in 0..5 {
        for y in 0..6 {
            push((x, y, 2));
        }
    }
    for (x, feet) in STAIR {
        body((x, feet, 3)).into_iter().for_each(&mut push);
    }
    for a in access(h) {
        body(a).into_iter().for_each(&mut push);
    }
    let dist = |c: (i32, i32, i32)| {
        let d = |p: i32, q: i32| f64::from(p - q);
        (d(c.0, from.0).powi(2) + d(c.1, from.1).powi(2) + d(c.2, from.2).powi(2)).sqrt()
    };
    cells.sort_by(|a, b| dist(*a).total_cmp(&dist(*b)).then(a.cmp(b)));
    cells
}

/// The cells under every stand and step that must be firm (`floors`).
fn floors(h: i32) -> Vec<(i32, i32, i32)> {
    let mut out: Vec<(i32, i32, i32)> = (0..5).map(|x| (x, -1, 1)).collect();
    out.extend((0..5).map(|x| (x, -1, 2)));
    out.extend(
        STAIR
            .iter()
            .map(|(x, feet)| (*x, *feet, 3))
            .chain(access(h))
            .map(|(x, feet, dz)| (x, feet - 1, dz)),
    );
    out
}

/// The room the cast needs, dug out of the rock (`room-ops`).
fn room_ops(f: Frame, guard: &Done, h: i32) -> Vec<Op> {
    let from = landing(h);
    let mut out: Vec<Op> = body(from)
        .iter()
        .map(|c| dig_op(f.at(*c), f.at(from), None, Step::Landing))
        .collect();
    out.extend(
        room_cells(h)
            .iter()
            .map(|c| dig_op(f.at(*c), f.at(*c), None, Step::Room)),
    );
    out.extend(floors(h).iter().flat_map(|c| {
        firm_ops(
            f.at(*c),
            f.at((c.0, c.1 + 1, c.2)),
            guard.clone(),
            Step::Floor,
        )
    }));
    out
}

/// After the frame, the 2x3 inside dug out from the floor two in front (`interior-ops`).
fn interior_ops(f: Frame) -> Vec<Op> {
    let mut inside: Vec<(i32, i32)> = [1, 2]
        .iter()
        .flat_map(|dx| [1, 2, 3].map(|dy| (*dx, dy)))
        .collect();
    inside.sort_by_key(|(_, dy)| -dy);
    inside
        .into_iter()
        .map(|(dx, dy)| dig_op(f.at((dx, dy, 0)), f.at((1, 0, 2)), None, Step::Interior))
        .collect()
}

/// Flint and steel on the top of the frame's bottom cell, from two blocks in front (`light-op`).
fn light_op(f: Frame) -> Op {
    Op {
        kind: OpKind::UseOn,
        pos: Some(f.at((1, 0, 0))),
        stand: f.at((1, 0, 2)),
        item: Some(ItemKind::FlintAndSteel),
        aim: None,
        into: None,
        cell: None,
        face: Some(1),
        step: Step::Light,
        done: Done::Block(f.at((1, 1, 0)), BlockKind::NetherPortal),
    }
}

/// Stand in the portal until the dimension is the Nether (`enter-op`).
fn enter_op(f: Frame) -> Op {
    let p = f.at((1, 1, 0));
    Op {
        kind: OpKind::StandIn,
        pos: Some(p),
        stand: p,
        item: None,
        aim: None,
        into: None,
        cell: None,
        face: None,
        step: Step::Enter,
        done: Done::Dimension("minecraft:the_nether".to_owned()),
    }
}

/// The first cell cast and checked (`cast-one-plan`).
pub fn cast_one_plan(f: Frame, h: i32) -> Vec<Op> {
    let first = FRAME_CELLS[0];
    let done = cast(f, first);
    let mut out = room_ops(f, &done, h);
    out.extend(cell_ops(f, first));
    out.push(Op {
        kind: OpKind::Verify,
        pos: None,
        stand: f.at(stand_of(first)),
        item: None,
        aim: None,
        into: None,
        cell: None,
        face: None,
        step: Step::Verify,
        done: Done::And(vec![
            done,
            Done::NoWaterSource(f.at((first.0, first.1 + 1, 0))),
        ]),
    });
    out
}

/// The whole frame lit, and with `enter` stepped through (`frame-plan`, `portal-plan`).
pub fn frame_plan(f: Frame, enter: bool, h: i32) -> Vec<Op> {
    let mut out = room_ops(f, &layer_cast(f, &FRAME_CELLS), h);
    out.extend(FRAME_CELLS.iter().flat_map(|c| cell_ops(f, *c)));
    out.extend(interior_ops(f));
    out.push(light_op(f));
    if enter {
        out.push(enter_op(f));
    }
    out
}

// ---------------------------------------------------------------- reading the world

fn kind_at(w: &World, (x, y, z): Cell) -> Option<BlockKind> {
    w.block_at(BlockPos::new(x, y, z))
}

/// `blocks/solid?`: a full collision cube.
fn solid(k: BlockKind) -> bool {
    data::generated::shapes::collision(k) == Some(data::types::Collision::Full)
}

/// `blocks/falls?`.
fn falls(k: BlockKind) -> bool {
    matches!(
        k,
        BlockKind::Gravel
            | BlockKind::Sand
            | BlockKind::RedSand
            | BlockKind::SuspiciousGravel
            | BlockKind::SuspiciousSand
    ) || k.name().is_some_and(|n| n.ends_with("concrete_powder"))
}

fn source(w: &World, (x, y, z): Cell, kind: BlockKind) -> bool {
    let p = BlockPos::new(x, y, z);
    w.block_at(p) == Some(kind) && w.state_at(p).is_some_and(memory::fluid_source)
}

/// Does `d` hold in the world (`holds?`)? An unloaded cell holds nothing.
pub fn holds(w: &World, d: &Done) -> bool {
    match d {
        Done::Solid(p) => kind_at(w, *p).is_some_and(solid),
        Done::NotSolid(p) => kind_at(w, *p).is_some_and(|k| !solid(k)),
        Done::Firm(p) => kind_at(w, *p).is_some_and(|k| solid(k) && !falls(k)),
        Done::Block(p, k) => kind_at(w, *p) == Some(*k),
        Done::LavaSource(p) => source(w, *p, BlockKind::Lava),
        Done::NoWaterSource(p) => !source(w, *p, BlockKind::Water),
        Done::Dimension(name) => w.dimension.as_ref().is_some_and(|d| &d.name == name),
        Done::Or(ds) => ds.iter().any(|d| holds(w, d)),
        Done::And(ds) => ds.iter().all(|d| holds(w, d)),
    }
}

/// The first op not done: the one to work on.
pub fn next_op<'a>(w: &World, plan: &'a [Op]) -> Option<&'a Op> {
    plan.iter().find(|o| !holds(w, &o.done))
}

// ---------------------------------------------------------------- the differential's text

fn cell_text((x, y, z): Cell) -> String {
    format!("[{x} {y} {z}]")
}

/// A predicate in the oracle's text: `[:block [x y z] :obsidian]`.
pub fn done_text(d: &Done) -> String {
    let name = |k: BlockKind| k.name().unwrap_or("unknown").to_owned();
    match d {
        Done::Solid(p) => format!("[:solid {}]", cell_text(*p)),
        Done::NotSolid(p) => format!("[:not-solid {}]", cell_text(*p)),
        Done::Firm(p) => format!("[:firm {}]", cell_text(*p)),
        Done::Block(p, k) => format!("[:block {} :{}]", cell_text(*p), name(*k)),
        Done::LavaSource(p) => format!("[:lava-source {}]", cell_text(*p)),
        Done::NoWaterSource(p) => format!("[:no-water-source {}]", cell_text(*p)),
        Done::Dimension(n) => format!("[:dimension \"{n}\"]"),
        Done::Or(ds) => format!(
            "[:or{}]",
            ds.iter()
                .map(|d| format!(" {}", done_text(d)))
                .collect::<String>()
        ),
        Done::And(ds) => format!(
            "[:and{}]",
            ds.iter()
                .map(|d| format!(" {}", done_text(d)))
                .collect::<String>()
        ),
    }
}

/// An op in the oracle's text, one line: kind, step, pos, stand, item, aim (6 places), into,
/// cell, face, done.
pub fn op_text(o: &Op) -> String {
    let kind = match o.kind {
        OpKind::Place => "place",
        OpKind::Dig => "dig",
        OpKind::Pour => "pour",
        OpKind::Scoop => "scoop",
        OpKind::UseOn => "use-on",
        OpKind::StandIn => "stand-in",
        OpKind::Verify => "verify",
    };
    let opt = |c: Option<Cell>| c.map_or_else(|| "-".to_owned(), cell_text);
    format!(
        ":{kind} :{} {} {} {} {} {} {} {} {}",
        o.step.name(),
        opt(o.pos),
        cell_text(o.stand),
        o.item
            .and_then(ItemKind::name)
            .map_or("-".to_owned(), |n| format!(":{n}")),
        o.aim.map_or("-".to_owned(), |[x, y, z]| format!(
            "[{x:.6} {y:.6} {z:.6}]"
        )),
        opt(o.into),
        o.cell.map_or("-".to_owned(), |(x, y)| format!("[{x} {y}]")),
        o.face.map_or("-".to_owned(), |_| ":up".to_owned()),
        done_text(&o.done),
    )
}
