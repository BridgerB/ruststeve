//! Stairs down (brain/ruststeve/design/stairs-down.md): a port of the core of clojurecraft's
//! `stairs.clj`. A staircase is cut one stair at a time along a heading: a stair opens three cells
//! one block ahead (head-up, head, and the new feet a block down) and is walked onto, so **the
//! block under the feet is never dug** (blind drops into lava killed the siblings).
//!
//! Before a stair is cut, every cell it opens is checked on the world: water, lava or an unloaded
//! chunk in any of them, lava beside any of them, or no full block under the new feet (a drop)
//! refuses it, and the heading turns a quarter. With all four refused the staircase takes a level
//! step instead (head and feet one block along, a floor under them) and tries again from there;
//! after `MAX_LEVEL_STEPS` such steps it fails `Boxed`. Done after `STAIR_DEPTH` stairs: the
//! planner looks again for what it wants. Each cell is dug by the dig task (its drop collected),
//! and the descent is judged by the feet having gone down, never by a packet having been sent.

use data::BlockKind;
use world::{Act, BlockPos, World};

use crate::dig::{Dig, DigStatus};
use crate::path::{self, Cell, Goal};
use crate::walk::{Arrive, TaskStatus, Walk};

/// Cells a stair's landing may lie under its missing floor (a fall of at most 3 blocks).
pub const MAX_DROP: i32 = 3;

/// Stairs cut before the planner looks again (clojurecraft's `stair-depth`).
pub const STAIR_DEPTH: u32 = 6;
/// Level steps before a staircase fails `Boxed` (clojurecraft's `max-level-steps`).
pub const MAX_LEVEL_STEPS: u32 = 3;
/// Ticks a step onto a stair may take.
pub const STEP_TICKS: u64 = 100;

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum StairsFailure {
    /// Every heading refused, and the level steps ran out.
    Boxed,
    /// A cell would not dig.
    DigFailed,
    /// The step onto a stair did not take the feet there.
    StepFailed,
    Lost,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StairsStatus {
    Running,
    Done,
    Failed(StairsFailure),
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum StairStage {
    Plan,
    /// Opening `cells` (top first), then stepping onto `to`.
    Cut {
        cells: Vec<Cell>,
        to: Cell,
        level: bool,
    },
    Step {
        to: Cell,
        walk: Walk,
        since: u64,
        level: bool,
        /// A travel tunnel's step whose walk failed (the route has no start beside lava or on
        /// soul sand): the feet are steered straight at the cell just cut, one block away.
        #[serde(default)]
        direct: bool,
    },
    /// A travel tunnel's stair up with no block to step onto (a cavern's floor, the air under a
    /// roof): held blocks are laid first, each `(cell, the block clicked, its face)`, then the
    /// stair is cut as any other.
    Lay {
        lays: Vec<(Cell, Cell, i8)>,
        cells: Vec<Cell>,
        to: Cell,
        sent: Option<u64>,
    },
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Stairs {
    /// The heading, a unit step on the plane.
    pub dir: (i32, i32),
    pub turns: u32,
    pub level_steps: u32,
    pub stairs: u32,
    pub stage: StairStage,
    pub dig: Option<Dig>,
    /// Stairs refused, by reason, for the case file.
    pub refusals: Vec<String>,
    pub started: u64,
    /// Level steps only (issue 13b): a mine row at its depth tunnels along instead of down, each
    /// step counted as a stair.
    #[serde(default)]
    pub level_only: bool,
    /// Stairs up (issue 16b): a step up toward the heading, its floor the block ahead at the
    /// feet's height. Seven bots of race 44781414716 stood 21 under a spawner they had seen, the
    /// tunnel toward it level or down only (docs/problems/blaze.md wall 1).
    #[serde(default)]
    pub up: bool,
    /// A travel tunnel's stretch (docs/problems/nether-tunnel.md): the feet cells to cut to, in
    /// order, the first where the feet are. Each step is the level step, stair up or stair down
    /// that leads to the next cell; a refusal ends the stretch (the planner blocks the cell and
    /// searches its route again), no heading is turned.
    #[serde(default)]
    pub route: Vec<Cell>,
    /// Cells dug, and where the feet were when the task began: a staircase that ends having dug
    /// nothing and gone nowhere has failed (docs/problems/nether-testbed.md: "cut 6 stairs" in
    /// 40 ticks over cells already open, again and again, read as progress).
    #[serde(default)]
    pub dug: u32,
    #[serde(default)]
    pub from: Option<Cell>,
    /// Steps of a travel tunnel steered straight after their walk failed, and blocks laid: said
    /// in the task's end note.
    #[serde(default)]
    pub steered: u32,
    #[serde(default)]
    pub laid: u32,
    /// Digs begun for the stair being cut, and the feet's cell with the tick it was last seen to
    /// change: a staircase that neither opens its cells nor moves ends, with the reason
    /// (docs/problems/nether-tunnel.md wall 6: one stairs task ran 19,900 ticks).
    #[serde(default)]
    pub cut_tries: u32,
    #[serde(default)]
    pub stood: Option<(Cell, u64)>,
}

/// Digs one stair's cells may take before the staircase fails: three cells, each dug once, with
/// room for a block that falls in.
pub const MAX_CUT_TRIES: u32 = 8;
/// Ticks the feet may stay in one cell before the staircase fails: a stair is three digs and a
/// step, under 200 ticks by hand in netherrack and 900 in deepslate.
pub const STAND_TICKS: u64 = 1200;

/// Blocks a stair is laid with, the first held: what a tunnel leaves in the pack.
pub const LAY_ITEMS: [data::ItemKind; 3] = [
    data::ItemKind::Netherrack,
    data::ItemKind::Cobblestone,
    data::ItemKind::CobbledDeepslate,
];
/// A laid stair has ground this near under it (open cells only between). Race 45340444298: a
/// route laid stairs up from a cliff's edge out over a cavern, and the bot ended its run there.
pub const LAY_OVER: i32 = 6;
/// Ticks a laid block may take to show before the step fails.
pub const LAY_TICKS: u64 = 40;

/// The first of `LAY_ITEMS` in the inventory.
pub fn lay_item(w: &World) -> Option<data::ItemKind> {
    LAY_ITEMS.into_iter().find(|i| w.inventory.count(*i) > 0)
}

/// What a stair up from `from` to `to` must lay first, when the block it steps onto is missing:
/// the cell under that block (clicked onto the side of the block the feet stand on) when it is
/// open too, then the block itself (clicked onto the top of the one under it). Empty when the
/// step has its floor, is not a stair up, or has no ground within `LAY_OVER` under it: stairs
/// are laid over a cavern's floor, never out over a drop or a lava sea.
pub fn lays(w: &World, from: Cell, to: Cell) -> Vec<(Cell, Cell, i8)> {
    let (dx, dz) = (to.0 - from.0, to.2 - from.2);
    let floor = (to.0, to.1 - 1, to.2);
    if to.1 != from.1 + 1 || !open(kind(w, floor)) {
        return Vec::new();
    }
    let ground = (1..=LAY_OVER)
        .map(|d| kind(w, (floor.0, floor.1 - d, floor.2)))
        .find(|k| !open(*k));
    if !full(ground.flatten()) {
        return Vec::new();
    }
    let under = (floor.0, floor.1 - 1, floor.2);
    let mut out = Vec::new();
    if !full(kind(w, under)) {
        let face = match (dx, dz) {
            (1, _) => 5,
            (-1, _) => 4,
            (_, 1) => 3,
            _ => 2,
        };
        out.push((under, (from.0, from.1 - 1, from.2), face));
    }
    out.push((floor, under, 1));
    out
}

fn kind(w: &World, (x, y, z): Cell) -> Option<BlockKind> {
    w.block_at(BlockPos::new(x, y, z))
}

fn fluid(k: Option<BlockKind>) -> Option<&'static str> {
    match k {
        Some(
            BlockKind::Water
            | BlockKind::BubbleColumn
            | BlockKind::Kelp
            | BlockKind::KelpPlant
            | BlockKind::Seagrass
            | BlockKind::TallSeagrass,
        ) => Some("water"),
        Some(BlockKind::Lava) => Some("lava"),
        _ => None,
    }
}

fn open(k: Option<BlockKind>) -> bool {
    matches!(
        k,
        Some(BlockKind::Air | BlockKind::CaveAir | BlockKind::VoidAir)
    )
}

fn full(k: Option<BlockKind>) -> bool {
    k.and_then(data::generated::shapes::collision) == Some(data::types::Collision::Full)
}

const SIDES: [(i32, i32, i32); 6] = [
    (1, 0, 0),
    (-1, 0, 0),
    (0, 1, 0),
    (0, -1, 0),
    (0, 0, 1),
    (0, 0, -1),
];

/// Why a stair (or level step) opening `cells` onto a floor at `floor` is refused, if it is.
pub fn refusal(w: &World, cells: &[Cell], floor: Cell) -> Option<String> {
    for c in cells.iter().chain(std::iter::once(&floor)) {
        if !w
            .columns
            .contains_key(&BlockPos::new(c.0, c.1, c.2).chunk())
        {
            return Some(format!("unloaded at {c:?}"));
        }
    }
    for c in cells {
        if let Some(f) = fluid(kind(w, *c)) {
            return Some(format!("{f} at {c:?}"));
        }
        for (dx, dy, dz) in SIDES {
            let n = (c.0 + dx, c.1 + dy, c.2 + dz);
            if fluid(kind(w, n)) == Some("lava") {
                return Some(format!("lava beside {c:?}"));
            }
        }
    }
    // A floor the walk takes: magma is a full block the route never stands on (blaze.md wall 23:
    // stairs planned onto magma failed their step at once, for ever, with nothing refused).
    if hot(kind(w, floor)) {
        return Some(format!("magma under {floor:?}"));
    }
    // What the walk stands on is a floor here too, full block or not (soul sand, once the route
    // takes it): the stair and the walk agree by construction (docs/problems/nether-tunnel.md:
    // 19 of a tunnel's 22 refusals were soul sand floors on one cavern face).
    let stood_on =
        path::classify(w.state_at(BlockPos::new(floor.0, floor.1, floor.2))) == path::Class::Solid;
    if !full(kind(w, floor)) && !stood_on && !short_drop(w, floor) {
        return Some(format!("no floor at {floor:?}"));
    }
    // Nothing that falls over what is opened: gravel or sand over a dug cell drops into it and
    // buries the head (five bots suffocated tunnelling: docs/problems/iron-kit.md wall 10).
    if let Some(top) = cells.iter().max_by_key(|c| c.1) {
        let over = (top.0, top.1 + 1, top.2);
        if falls(kind(w, over)) {
            return Some(format!("falling block over {top:?}"));
        }
    }
    None
}

/// Under a missing floor, a landing a short fall down: open air (no fluid) to a full block at most
/// `MAX_DROP` under the floor's cell, no lava beside the landing. A fall of three blocks does no
/// damage (`fall_distance` over 3 hurts); 17 of 56 bots ended boxed in by cave floors and ledges
/// a block or two down (docs/problems/iron-kit.md wall 17).
fn short_drop(w: &World, floor: Cell) -> bool {
    for j in 0..MAX_DROP {
        let c = (floor.0, floor.1 - j, floor.2);
        let k = kind(w, c);
        if full(k) {
            return j > 0
                && !hot(k)
                && SIDES.iter().all(|(dx, dy, dz)| {
                    fluid(kind(w, (c.0 + dx, c.1 + 1 + dy, c.2 + dz))) != Some("lava")
                });
        }
        if !open(k) {
            return false;
        }
    }
    false
}

/// A full block that burns what stands on it: `path::classify` never takes it as a floor.
fn hot(k: Option<BlockKind>) -> bool {
    k == Some(BlockKind::MagmaBlock)
}

/// Blocks that fall when what is under them opens: the blocks report's falling definitions
/// (sand, gravel, concrete powder, the suspicious blocks).
pub fn falls(k: Option<BlockKind>) -> bool {
    k.and_then(|k| k.info()).is_some_and(|i| {
        matches!(
            i.definition,
            "sand" | "colored_falling" | "concrete_powder" | "brushable" | "falling"
        )
    })
}

/// One step up from `feet` along `dir`: where the feet go, and the cells opened for it, top
/// first: the room over the head (to jump), then the step's head and feet. Its floor is the block
/// ahead at the feet's height (issue 16b).
pub fn up_step((x, y, z): Cell, (dx, dz): (i32, i32)) -> (Cell, Vec<Cell>) {
    let to = (x + dx, y + 1, z + dz);
    (to, vec![(x, y + 2, z), (x + dx, y + 2, z + dz), to])
}

fn turn((dx, dz): (i32, i32)) -> (i32, i32) {
    (-dz, dx)
}

impl Stairs {
    pub fn new(dir: (i32, i32), tick: u64) -> Self {
        Self {
            dir,
            turns: 0,
            level_steps: 0,
            stairs: 0,
            stage: StairStage::Plan,
            dig: None,
            refusals: Vec::new(),
            started: tick,
            level_only: false,
            up: false,
            route: Vec::new(),
            dug: 0,
            from: None,
            steered: 0,
            laid: 0,
            cut_tries: 0,
            stood: None,
        }
    }

    /// A stretch of a travel tunnel: the steps that lead along `route`, as far as it goes.
    pub fn along(route: Vec<Cell>, tick: u64) -> Self {
        let dir = match route.as_slice() {
            [a, b, ..] => (b.0 - a.0, b.2 - a.2),
            _ => (1, 0),
        };
        Self {
            route,
            ..Self::new(dir, tick)
        }
    }

    /// The next step along the route from the feet; `Ok(None)` at its end.
    fn plan_route(&mut self, feet: Cell, w: &World) -> Result<Option<StairStage>, StairsFailure> {
        let Some(i) = self.route.iter().position(|c| *c == feet) else {
            self.refusals.push(format!("off the route at {feet:?}"));
            return Err(StairsFailure::Boxed);
        };
        let Some(to) = self.route.get(i + 1).copied() else {
            return Ok(None);
        };
        let Some((cells, floor)) = crate::tunnel::opened(feet, to) else {
            self.refusals
                .push(format!("no step from {feet:?} to {to:?}"));
            return Err(StairsFailure::Boxed);
        };
        self.dir = (to.0 - feet.0, to.2 - feet.2);
        let lays = lays(w, feet, to);
        if !lays.is_empty() {
            // Nothing laid into or beside a fluid, and only onto something clickable.
            let wet = lays.iter().any(|(c, _, _)| {
                !open(kind(w, *c))
                    || SIDES.iter().any(|(dx, dy, dz)| {
                        fluid(kind(w, (c.0 + dx, c.1 + dy, c.2 + dz))).is_some()
                    })
            });
            let why = if wet {
                Some(format!(
                    "no floor at {floor:?} and fluid where one would be laid"
                ))
            } else if lay_item(w).is_none() {
                Some(format!("no floor at {floor:?} and no block to lay"))
            } else {
                None
            };
            if let Some(why) = why {
                self.refusals.push(why);
                return Err(StairsFailure::Boxed);
            }
            return Ok(Some(StairStage::Lay {
                lays,
                cells,
                to,
                sent: None,
            }));
        }
        if let Some(why) = refusal(w, &cells, floor) {
            self.refusals.push(why);
            return Err(StairsFailure::Boxed);
        }
        Ok(Some(StairStage::Cut {
            cells,
            to,
            level: to.1 == feet.1,
        }))
    }

    /// Stairs up: `STAIR_DEPTH` steps up along the heading (a level step where all four headings
    /// refuse), each opening the room over the head, then the next column's head and feet one up.
    pub fn up(dir: (i32, i32), tick: u64) -> Self {
        Self {
            up: true,
            ..Self::new(dir, tick)
        }
    }

    /// A tunnel at the feet's level: level steps only, `STAIR_DEPTH` of them.
    pub fn level(dir: (i32, i32), tick: u64) -> Self {
        Self {
            level_only: true,
            ..Self::new(dir, tick)
        }
    }

    /// The next stair from the feet, turning on refusals; a level step when all four refuse.
    fn plan(&mut self, feet: Cell, w: &World) -> Result<StairStage, StairsFailure> {
        let (x, y, z) = feet;
        for _ in 0..if self.level_only { 0 } else { 4 } {
            let (dx, dz) = self.dir;
            let (to, cells) = if self.up {
                up_step(feet, self.dir)
            } else {
                let to = (x + dx, y - 1, z + dz);
                (to, vec![(x + dx, y + 1, z + dz), (x + dx, y, z + dz), to])
            };
            match refusal(w, &cells, (to.0, to.1 - 1, to.2)) {
                None => {
                    return Ok(StairStage::Cut {
                        cells,
                        to,
                        level: false,
                    });
                }
                Some(why) => {
                    self.refusals.push(why);
                    self.dir = turn(self.dir);
                    self.turns += 1;
                }
            }
        }
        if self.level_steps >= MAX_LEVEL_STEPS && !self.level_only {
            return Err(StairsFailure::Boxed);
        }
        for _ in 0..4 {
            let (dx, dz) = self.dir;
            let to = (x + dx, y, z + dz);
            let cells = vec![(x + dx, y + 1, z + dz), to];
            match refusal(w, &cells, (to.0, to.1 - 1, to.2)) {
                None => {
                    self.level_steps += 1;
                    return Ok(StairStage::Cut {
                        cells,
                        to,
                        level: true,
                    });
                }
                Some(why) => {
                    self.refusals.push(why);
                    self.dir = turn(self.dir);
                }
            }
        }
        Err(StairsFailure::Boxed)
    }

    /// One tick: the controls, the act, how the staircase stands.
    pub fn step(&mut self, w: &World, tick: u64) -> (physics::Controls, Option<Act>, StairsStatus) {
        let still = physics::Controls::default();
        let Some(body) = w.body else {
            return (still, None, StairsStatus::Failed(StairsFailure::Lost));
        };
        let feet = path::feet_cell(body.pos);
        let from = *self.from.get_or_insert(feet);
        match self.stood {
            Some((c, since)) if c == feet => {
                if tick.saturating_sub(since) > STAND_TICKS {
                    self.refusals.push(format!(
                        "the feet stayed at {feet:?} for {STAND_TICKS} ticks ({} digs begun for this stair)",
                        self.cut_tries
                    ));
                    return (still, None, StairsStatus::Failed(StairsFailure::StepFailed));
                }
            }
            Some(_) | None => self.stood = Some((feet, tick)),
        }
        match &mut self.stage {
            StairStage::Lay {
                lays,
                cells,
                to,
                sent,
            } => {
                // A block that has shown is done, and the next gets its own click: with two to
                // lay, the wait for the first was held against the second, which was never
                // clicked ("a block laid at … did not show", with the first block spent: races
                // 45337739022 and 45344386490).
                let before = lays.len();
                lays.retain(|(c, _, _)| open(kind(w, *c)));
                if lays.len() != before {
                    *sent = None;
                }
                let Some((c, against, face)) = lays.first().copied() else {
                    let (cells, to) = (cells.clone(), *to);
                    return match refusal(w, &cells, (to.0, to.1 - 1, to.2)) {
                        None => {
                            self.stage = StairStage::Cut {
                                cells,
                                to,
                                level: false,
                            };
                            (still, None, StairsStatus::Running)
                        }
                        Some(why) => {
                            self.refusals.push(why);
                            (still, None, StairsStatus::Failed(StairsFailure::Boxed))
                        }
                    };
                };
                if let Some(at) = *sent {
                    if tick.saturating_sub(at) <= LAY_TICKS {
                        return (still, None, StairsStatus::Running);
                    }
                    self.refusals
                        .push(format!("a block laid at {c:?} did not show"));
                    return (still, None, StairsStatus::Failed(StairsFailure::StepFailed));
                }
                // Never into the body's own box: a server refuses a block that would overlap the
                // player, and a step is done the moment the feet cross into its cell, 0.3 of the
                // body still in the cell behind. 53 laid blocks "did not show" in race
                // 45337739022, each where the route turned back over the cell just left. Walk to
                // the middle of the cell first.
                let b = physics::aabb(body.pos);
                let cell = [
                    f64::from(c.0),
                    f64::from(c.1),
                    f64::from(c.2),
                    f64::from(c.0) + 1.0,
                    f64::from(c.1) + 1.0,
                    f64::from(c.2) + 1.0,
                ];
                if (0..3).all(|a| physics::overlaps(&b, &cell, a)) {
                    let middle = [
                        f64::from(feet.0) + 0.5,
                        body.pos[1],
                        f64::from(feet.2) + 0.5,
                    ];
                    let c = physics::Controls {
                        forward: true,
                        yaw: crate::walk::yaw_to(body.pos, middle),
                        ..still
                    };
                    return (c, None, StairsStatus::Running);
                }
                let Some(item) = lay_item(w) else {
                    self.refusals.push(format!("no block to lay at {c:?}"));
                    return (still, None, StairsStatus::Failed(StairsFailure::Boxed));
                };
                match crate::hold::toward(w, item) {
                    crate::hold::Hold::Held => {
                        *sent = Some(tick);
                        self.laid += 1;
                        let act = Act::UseOn {
                            pos: BlockPos::new(against.0, against.1, against.2),
                            face,
                        };
                        (still, Some(act), StairsStatus::Running)
                    }
                    crate::hold::Hold::Act(a) => (still, Some(a), StairsStatus::Running),
                    crate::hold::Hold::Wait => (still, None, StairsStatus::Running),
                    crate::hold::Hold::Missing => {
                        (still, None, StairsStatus::Failed(StairsFailure::Boxed))
                    }
                }
            }
            StairStage::Plan => {
                if self.stairs >= STAIR_DEPTH {
                    // Six steps over cells already open that came back to where they began:
                    // nothing was done, and saying "done" hides it.
                    let gone =
                        (feet.0 - from.0).abs() + (feet.1 - from.1).abs() + (feet.2 - from.2).abs();
                    if self.dug == 0 && gone <= 1 {
                        self.refusals
                            .push("nothing cut and nowhere gone".to_owned());
                        return (still, None, StairsStatus::Failed(StairsFailure::Boxed));
                    }
                    return (still, None, StairsStatus::Done);
                }
                if !self.route.is_empty() {
                    // A stretch that cut a step is done where it stops: the planner reads the
                    // feet's place on the route and knows the next step failed.
                    return match self.plan_route(feet, w) {
                        Ok(Some(s)) => {
                            self.stage = s;
                            (still, None, StairsStatus::Running)
                        }
                        Ok(None) => (still, None, StairsStatus::Done),
                        Err(_) if self.stairs > 0 => (still, None, StairsStatus::Done),
                        Err(f) => (still, None, StairsStatus::Failed(f)),
                    };
                }
                let before = self.refusals.len();
                match self.plan(feet, w) {
                    Ok(s) => {
                        self.stage = s;
                        (still, None, StairsStatus::Running)
                    }
                    Err(f) => {
                        // Boxed: every heading's refusal in one line, the last the ledger prints.
                        let all = self.refusals.get(before..).unwrap_or_default().join("; ");
                        self.refusals.push(format!(
                            "boxed at {feet:?} after {} level steps: {all}",
                            self.level_steps
                        ));
                        (still, None, StairsStatus::Failed(f))
                    }
                }
            }
            StairStage::Cut { cells, to, level } => {
                let (to, level) = (*to, *level);
                if let Some(d) = self.dig.as_mut() {
                    let (c, act, status) = d.step(w, tick);
                    return match status {
                        DigStatus::Running => (c, act, StairsStatus::Running),
                        DigStatus::Done => {
                            self.dig = None;
                            (still, act, StairsStatus::Running)
                        }
                        // A drop not collected is no reason to stop a staircase.
                        DigStatus::Failed(
                            crate::dig::DigFailure::Uncollected
                            | crate::dig::DigFailure::Taken
                            | crate::dig::DigFailure::DropUnreachable
                            | crate::dig::DigFailure::DropWalkFailed
                            | crate::dig::DigFailure::Gone,
                        ) => {
                            self.dig = None;
                            (still, act, StairsStatus::Running)
                        }
                        DigStatus::Failed(f) => {
                            self.refusals
                                .push(format!("a dig for the stair to {to:?} failed {f:?}"));
                            (still, act, StairsStatus::Failed(StairsFailure::DigFailed))
                        }
                    };
                }
                match cells.iter().find(|c| !open(kind(w, **c))).copied() {
                    Some(c) if self.cut_tries >= MAX_CUT_TRIES => {
                        self.refusals.push(format!(
                            "{:?} at {c:?} is still there after {MAX_CUT_TRIES} digs for one stair",
                            kind(w, c)
                        ));
                        (still, None, StairsStatus::Failed(StairsFailure::DigFailed))
                    }
                    Some(c) => {
                        self.cut_tries += 1;
                        // No collect: the drop falls into the stair the bot steps onto next and
                        // is picked up there. Waiting for it cost the full collect time (300
                        // ticks) a stair: docs/problems/iron-kit.md wall 2.
                        let mut d = Dig::new(w, c, tick);
                        d.drop = None;
                        d.others.clear();
                        self.dig = Some(d);
                        self.dug += 1;
                        (still, None, StairsStatus::Running)
                    }
                    None => {
                        let walk = Walk::new(to, Goal::Block { pos: to }, Arrive::Goal, tick);
                        self.stage = StairStage::Step {
                            to,
                            walk,
                            since: tick,
                            level,
                            direct: false,
                        };
                        (still, None, StairsStatus::Running)
                    }
                }
            }
            StairStage::Step {
                to,
                walk,
                since,
                level,
                direct,
            } => {
                // On the stair, or down a short drop under it.
                if feet == *to
                    || (feet.0 == to.0
                        && feet.2 == to.2
                        && feet.1 < to.1
                        && feet.1 >= to.1 - MAX_DROP)
                {
                    if !*level || self.level_only || !self.route.is_empty() {
                        self.stairs += 1;
                        self.turns = 0;
                    }
                    self.cut_tries = 0;
                    self.stage = StairStage::Plan;
                    return (still, None, StairsStatus::Running);
                }
                // Every failure names its reason (644 "last refusal: ;" in one ledger said
                // nothing: race 45204423376 lane 3).
                let to = *to;
                if tick.saturating_sub(*since) > STEP_TICKS {
                    self.refusals.push(format!(
                        "the step from {feet:?} onto {to:?} took over {STEP_TICKS} ticks"
                    ));
                    return (still, None, StairsStatus::Failed(StairsFailure::StepFailed));
                }
                if *direct {
                    let middle = [f64::from(to.0) + 0.5, body.pos[1], f64::from(to.2) + 0.5];
                    let c = physics::Controls {
                        forward: true,
                        jump: to.1 > feet.1,
                        yaw: crate::walk::yaw_to(body.pos, middle),
                        ..still
                    };
                    return (c, None, StairsStatus::Running);
                }
                match walk.step(w, tick) {
                    (c, TaskStatus::Running) => (c, None, StairsStatus::Running),
                    (_, TaskStatus::Done) => (still, None, StairsStatus::Running),
                    (_, TaskStatus::Failed(_)) if !self.route.is_empty() => {
                        *direct = true;
                        *since = tick;
                        self.steered += 1;
                        (still, None, StairsStatus::Running)
                    }
                    (_, TaskStatus::Failed(f)) => {
                        let under = kind(w, (to.0, to.1 - 1, to.2));
                        self.refusals.push(format!(
                            "the walk from {feet:?} onto {to:?} failed {f:?} (its floor {under:?})"
                        ));
                        (still, None, StairsStatus::Failed(StairsFailure::StepFailed))
                    }
                }
            }
        }
    }
}
