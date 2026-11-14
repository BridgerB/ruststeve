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

use data::{BlockKind, ItemKind};
use world::{Act, BlockPos, World};

use crate::dig::{Dig, DigStatus};
use crate::path::{self, Cell, Goal};
use crate::walk::{Arrive, TaskStatus, Walk};

/// Cells a staircase fills to get out of being boxed in before it fails `Boxed`.
pub const MAX_FILLS: u32 = 4;
/// Ticks to wait for a fill's block update before trying again; tries before it fails.
pub const FILL_TICKS: u64 = 40;
pub const FILL_TRIES: u32 = 3;
/// Blocks a staircase fills with, from what a miner carries (the first held is used).
const FILLERS: &[ItemKind] = &[
    ItemKind::Cobblestone,
    ItemKind::CobbledDeepslate,
    ItemKind::Dirt,
    ItemKind::Andesite,
    ItemKind::Diorite,
    ItemKind::Granite,
    ItemKind::Netherrack,
];

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
    /// Boxed in: a held filler block put into `cell` (water a stair would open, or the missing
    /// floor under one), then plan again (issue 13b).
    Fill {
        cell: Cell,
        at: Option<u64>,
        tries: u32,
    },
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
    /// Cells filled to get out of being boxed in (issue 13b).
    #[serde(default)]
    pub fills: u32,
    /// Level steps only (issue 13b): a mine row at its depth tunnels along instead of down, each
    /// step counted as a stair.
    #[serde(default)]
    pub level_only: bool,
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
    if !full(kind(w, floor)) {
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
            fills: 0,
            level_only: false,
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
            let to = (x + dx, y - 1, z + dz);
            let cells = vec![(x + dx, y + 1, z + dz), (x + dx, y, z + dz), to];
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
        // Boxed in by water or a drop: fill the water a stair would open, or the floor it lacks,
        // with a held block, and plan again (a bot in a pit by a lake used all its relocations on
        // stairs refused for water on every side: docs/problems/iron-kit.md wall 15).
        if self.fills < MAX_FILLS {
            for _ in 0..4 {
                let (dx, dz) = self.dir;
                let to = (x + dx, y - 1, z + dz);
                let cells = [(x + dx, y + 1, z + dz), (x + dx, y, z + dz), to];
                let lava_near = cells.iter().any(|c| {
                    fluid(kind(w, *c)) == Some("lava")
                        || SIDES.iter().any(|(ox, oy, oz)| {
                            fluid(kind(w, (c.0 + ox, c.1 + oy, c.2 + oz))) == Some("lava")
                        })
                });
                let water = cells.iter().find(|c| fluid(kind(w, **c)) == Some("water"));
                let floor = (to.0, to.1 - 1, to.2);
                let target = water.copied().or_else(|| {
                    (open(kind(w, floor)) && fluid(kind(w, floor)).is_none()).then_some(floor)
                });
                if let Some(cell) = target.filter(|_| !lava_near) {
                    self.fills += 1;
                    return Ok(StairStage::Fill {
                        cell,
                        at: None,
                        tries: 0,
                    });
                }
                self.dir = turn(self.dir);
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
        match &mut self.stage {
            StairStage::Plan => {
                if self.stairs >= STAIR_DEPTH {
                    return (still, None, StairsStatus::Done);
                }
                match self.plan(feet, w) {
                    Ok(s) => {
                        self.stage = s;
                        (still, None, StairsStatus::Running)
                    }
                    Err(f) => (still, None, StairsStatus::Failed(f)),
                }
            }
            StairStage::Fill { cell, at, tries } => {
                let cell = *cell;
                if full(kind(w, cell)) {
                    self.stage = StairStage::Plan;
                    return (still, None, StairsStatus::Running);
                }
                if let Some(sent) = *at {
                    if tick.saturating_sub(sent) < FILL_TICKS {
                        return (still, None, StairsStatus::Running);
                    }
                    if *tries >= FILL_TRIES {
                        return (still, None, StairsStatus::Failed(StairsFailure::Boxed));
                    }
                }
                let Some(item) = FILLERS
                    .iter()
                    .copied()
                    .find(|i| crate::craft::count(w, *i) > 0)
                else {
                    return (still, None, StairsStatus::Failed(StairsFailure::Boxed));
                };
                match crate::hold::toward(w, item) {
                    crate::hold::Hold::Held => {}
                    crate::hold::Hold::Act(a) => return (still, Some(a), StairsStatus::Running),
                    crate::hold::Hold::Wait => return (still, None, StairsStatus::Running),
                    crate::hold::Hold::Missing => {
                        return (still, None, StairsStatus::Failed(StairsFailure::Boxed));
                    }
                }
                // Against a full neighbour's face toward the cell.
                let faces = [
                    ((0, -1, 0), 1),
                    ((0, 1, 0), 0),
                    ((0, 0, -1), 3),
                    ((0, 0, 1), 2),
                    ((-1, 0, 0), 5),
                    ((1, 0, 0), 4),
                ];
                let Some((n, face)) = faces.iter().find_map(|((dx, dy, dz), f)| {
                    let n = (cell.0 + dx, cell.1 + dy, cell.2 + dz);
                    full(kind(w, n)).then_some((n, *f))
                }) else {
                    return (still, None, StairsStatus::Failed(StairsFailure::Boxed));
                };
                if let StairStage::Fill { at, tries, .. } = &mut self.stage {
                    *at = Some(tick);
                    *tries += 1;
                }
                (
                    still,
                    Some(Act::UseOn {
                        pos: BlockPos::new(n.0, n.1, n.2),
                        face,
                    }),
                    StairsStatus::Running,
                )
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
                        DigStatus::Failed(_) => {
                            (still, act, StairsStatus::Failed(StairsFailure::DigFailed))
                        }
                    };
                }
                match cells.iter().find(|c| !open(kind(w, **c))).copied() {
                    Some(c) => {
                        // No collect: the drop falls into the stair the bot steps onto next and
                        // is picked up there. Waiting for it cost the full collect time (300
                        // ticks) a stair: docs/problems/iron-kit.md wall 2.
                        let mut d = Dig::new(w, c, tick);
                        d.drop = None;
                        d.others.clear();
                        self.dig = Some(d);
                        (still, None, StairsStatus::Running)
                    }
                    None => {
                        let walk = Walk::new(to, Goal::Block { pos: to }, Arrive::Goal, tick);
                        self.stage = StairStage::Step {
                            to,
                            walk,
                            since: tick,
                            level,
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
            } => {
                if feet == *to {
                    if !*level || self.level_only {
                        self.stairs += 1;
                        self.turns = 0;
                    }
                    self.stage = StairStage::Plan;
                    return (still, None, StairsStatus::Running);
                }
                if tick.saturating_sub(*since) > STEP_TICKS {
                    return (still, None, StairsStatus::Failed(StairsFailure::StepFailed));
                }
                match walk.step(w, tick) {
                    (c, TaskStatus::Running) => (c, None, StairsStatus::Running),
                    (_, TaskStatus::Done) => (still, None, StairsStatus::Running),
                    (_, TaskStatus::Failed(_)) => {
                        (still, None, StairsStatus::Failed(StairsFailure::StepFailed))
                    }
                }
            }
        }
    }
}
