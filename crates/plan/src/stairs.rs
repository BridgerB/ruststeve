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
    None
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
        }
    }

    /// The next stair from the feet, turning on refusals; a level step when all four refuse.
    fn plan(&mut self, feet: Cell, w: &World) -> Result<StairStage, StairsFailure> {
        let (x, y, z) = feet;
        for _ in 0..4 {
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
        if self.level_steps >= MAX_LEVEL_STEPS {
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
                        self.dig = Some(Dig::new(w, c, tick));
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
                    if !*level {
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
