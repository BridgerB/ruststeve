//! Filling a bucket with water (issue 14b, brain/ruststeve/design/buckets-and-fluids.md), a port
//! of the core of clojurecraft's `scoop.clj`: a surface water source memory kept (seen with air
//! over it: never cave water, the aquifer trap the siblings drowned in), walked to within reach,
//! the bucket held, the look set on the source's surface, and `use_item` sent along it. Done
//! only when the inventory holds one more water bucket than when it began; an unanswered scoop
//! is tried again, and the last try fails `NoScoop`, which refuses the source.

use data::ItemKind;
use physics::Controls;
use world::{Act, World};

use crate::Cell;
use crate::hold::{self, Hold};
use crate::path::Goal;
use crate::walk::{Arrive, TaskStatus, Walk, look_at};

/// Blocks from the feet a remembered source is scooped from (clojurecraft's `search-radius`).
pub const SEARCH_RADIUS: f64 = 24.0;
/// Where a source's surface is aimed at: a source stands 8/9 of a block (`surface-height`).
pub const SURFACE_HEIGHT: f64 = 0.85;
/// Ticks the look is held still on the source before the use (clojurecraft's 300 ms).
pub const SETTLE_TICKS: u64 = 6;
/// Ticks a scoop waits for the water bucket before it is tried again (its 3 s).
pub const ANSWER_TICKS: u64 = 60;
/// Scoops sent at one source before it fails (`max-tries`).
pub const MAX_TRIES: u32 = 3;
/// Blocks from the eye to the surface point a scoop is sent from: the game's 4.5 less a margin.
/// A pool's sources further in are reached across the flowing fluid at its edge (a source-only
/// ray passes over it): a cast's lava refills spent the edge first (issue 14c).
pub const REACH: f64 = 4.4;
/// Blocks from the source's cell a walk to it stops within.
const WALK_RANGE: f64 = 3.5;

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ScoopFailure {
    /// No walk reaches it.
    NoWay,
    /// Gone (scooped, frozen, flowed away) before the scoop.
    Gone,
    /// The look to it is blocked from where the walk ended.
    NoSight,
    /// `MAX_TRIES` scoops went unanswered.
    NoScoop,
    /// No bucket held.
    NoBucket,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScoopStatus {
    Running,
    Done,
    Failed(ScoopFailure),
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
enum Stage {
    Approach(Walk),
    Aim { since: u64 },
    Sent { at: u64, before: i32 },
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Scoop {
    pub target: Cell,
    stage: Stage,
    pub tries: u32,
    /// Water (a scoop), or lava (a cast's refill at its pool, issue 14c).
    #[serde(default = "water")]
    pub fluid: memory::Fluid,
}

fn water() -> memory::Fluid {
    memory::Fluid::Water
}

fn bucket_of(fluid: memory::Fluid) -> ItemKind {
    match fluid {
        memory::Fluid::Water => ItemKind::WaterBucket,
        memory::Fluid::Lava => ItemKind::LavaBucket,
    }
}

/// The surface water sources memory kept within `SEARCH_RADIUS` of the feet, nearest first.
pub fn sources(w: &World) -> Vec<Cell> {
    let (Some(body), Some(dim)) = (w.body, w.dimension.as_ref()) else {
        return Vec::new();
    };
    let feet = crate::path::feet_cell(body.pos);
    let mut near: Vec<(f64, Cell)> = w
        .memory
        .sources_near(&dim.name, memory::Fluid::Water, feet.0 >> 4, feet.2 >> 4, 2)
        .into_iter()
        .map(|c| (distance(feet, c), c))
        .filter(|(d, _)| *d <= SEARCH_RADIUS)
        .collect();
    near.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
    near.into_iter().map(|(_, c)| c).collect()
}

fn distance(a: Cell, b: Cell) -> f64 {
    let d = |p: i32, q: i32| f64::from(p - q);
    (d(a.0, b.0).powi(2) + d(a.1, b.1).powi(2) + d(a.2, b.2).powi(2)).sqrt()
}

fn surface((x, y, z): Cell) -> [f64; 3] {
    [
        f64::from(x) + 0.5,
        f64::from(y) + SURFACE_HEIGHT,
        f64::from(z) + 0.5,
    ]
}

fn eye(body: &physics::Body) -> [f64; 3] {
    [body.pos[0], body.pos[1] + physics::EYE_HEIGHT, body.pos[2]]
}

fn source_of(w: &World, c: Cell, fluid: memory::Fluid) -> bool {
    w.state_at(world::BlockPos::new(c.0, c.1, c.2))
        .and_then(memory::Memory::source_fluid)
        == Some(fluid)
}

/// Whether the eye's ray to `to` crosses nothing solid before the cell `target` (cells read from
/// the loaded terrain, which the eye can see along).
pub fn clear_ray(w: &World, from: [f64; 3], to: [f64; 3], target: Cell) -> bool {
    let steps = 40;
    (1..steps).all(|i| {
        let t = f64::from(i) / f64::from(steps);
        let p = [
            from[0] + (to[0] - from[0]) * t,
            from[1] + (to[1] - from[1]) * t,
            from[2] + (to[2] - from[2]) * t,
        ];
        let c = (
            p[0].floor() as i32,
            p[1].floor() as i32,
            p[2].floor() as i32,
        );
        c == target
            || w.block_at(world::BlockPos::new(c.0, c.1, c.2))
                .and_then(data::generated::shapes::collision)
                .is_some_and(|k| k == data::types::Collision::Empty)
    })
}

impl Scoop {
    pub fn new(target: Cell, tick: u64) -> Self {
        Self::of(target, memory::Fluid::Water, tick)
    }

    /// A bucket filled at `target`, a source of `fluid`.
    pub fn of(target: Cell, fluid: memory::Fluid, tick: u64) -> Self {
        Self {
            fluid,
            target,
            stage: Stage::Approach(Walk::new(
                target,
                Goal::Near {
                    pos: target,
                    range: WALK_RANGE,
                },
                Arrive::Goal,
                tick,
            )),
            tries: 0,
        }
    }

    /// Whether a walk reaches it now (the churn invariant: never start a scoop with no way).
    pub fn route_now(&mut self, w: &World, tick: u64) -> bool {
        match &mut self.stage {
            Stage::Approach(walk) => walk.route_now(w, tick),
            Stage::Aim { .. } | Stage::Sent { .. } => true,
        }
    }

    fn held_filled(&self, w: &World) -> i32 {
        crate::held(w)
            .get(&bucket_of(self.fluid))
            .copied()
            .unwrap_or(0)
    }

    pub fn step(&mut self, w: &World, tick: u64) -> (Controls, Option<Act>, ScoopStatus) {
        let still = Controls::default();
        let Some(body) = w.body else {
            return (still, None, ScoopStatus::Running);
        };
        if let Stage::Sent { before, .. } = self.stage
            && self.held_filled(w) > before
        {
            return (still, None, ScoopStatus::Done);
        }
        // Gone only before the use is sent: a scooped source's block update arrives a tick before
        // the bucket's slot (and water forms again between two sources).
        if !matches!(self.stage, Stage::Sent { .. }) && !source_of(w, self.target, self.fluid) {
            return (still, None, ScoopStatus::Failed(ScoopFailure::Gone));
        }
        let point = surface(self.target);
        let from = eye(&body);
        let in_reach = (0..3)
            .map(|i| (point[i] - from[i]).powi(2))
            .sum::<f64>()
            .sqrt()
            <= REACH;
        match &mut self.stage {
            Stage::Approach(walk) => {
                if in_reach && body.on_ground {
                    self.stage = Stage::Aim { since: tick };
                    return (still, None, ScoopStatus::Running);
                }
                let (c, status) = walk.step(w, tick);
                match status {
                    TaskStatus::Running => (c, None, ScoopStatus::Running),
                    TaskStatus::Done if in_reach => {
                        self.stage = Stage::Aim { since: tick };
                        (still, None, ScoopStatus::Running)
                    }
                    TaskStatus::Done | TaskStatus::Failed(_) => {
                        (still, None, ScoopStatus::Failed(ScoopFailure::NoWay))
                    }
                }
            }
            Stage::Aim { since } => {
                let since = *since;
                match hold::toward(w, ItemKind::Bucket) {
                    Hold::Held => {}
                    Hold::Act(a) => return (still, Some(a), ScoopStatus::Running),
                    Hold::Wait => return (still, None, ScoopStatus::Running),
                    Hold::Missing => {
                        return (still, None, ScoopStatus::Failed(ScoopFailure::NoBucket));
                    }
                }
                let (yaw, pitch) = look_at(from, point);
                let look = Controls {
                    yaw,
                    pitch: Some(pitch),
                    ..still
                };
                if !clear_ray(w, from, point, self.target) {
                    return (still, None, ScoopStatus::Failed(ScoopFailure::NoSight));
                }
                if tick.saturating_sub(since) < SETTLE_TICKS {
                    return (look, None, ScoopStatus::Running);
                }
                self.tries += 1;
                self.stage = Stage::Sent {
                    at: tick,
                    before: self.held_filled(w),
                };
                (look, Some(Act::UseItem), ScoopStatus::Running)
            }
            Stage::Sent { at, .. } => {
                let (yaw, pitch) = look_at(from, point);
                let look = Controls {
                    yaw,
                    pitch: Some(pitch),
                    ..still
                };
                if tick.saturating_sub(*at) < ANSWER_TICKS {
                    return (look, None, ScoopStatus::Running);
                }
                if self.tries >= MAX_TRIES {
                    return (look, None, ScoopStatus::Failed(ScoopFailure::NoScoop));
                }
                self.stage = Stage::Aim { since: tick };
                (look, None, ScoopStatus::Running)
            }
        }
    }
}
