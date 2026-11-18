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
/// Blocks from the feet a remembered pool is walked toward from (`pool-radius`).
pub const POOL_RADIUS: f64 = 96.0;
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
    /// Whether the exact line to the aim was clear when the last use was sent.
    #[serde(default)]
    sent_exact: Option<bool>,
    /// The bucket's uses by how they ended and what the exact line said when each was sent:
    /// filled and exact clear, filled and exact blocked, empty and exact clear, empty and exact
    /// blocked. Every use had a clear sampled line. The second counts the exact line refusing
    /// what the server took; the third, the exact line passing what the server refused.
    #[serde(default)]
    pub uses: [u32; 4],
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
    sources_within(w, SEARCH_RADIUS)
}

/// The surface water memory kept within `POOL_RADIUS`: walked toward when none is within
/// `SEARCH_RADIUS` (clojurecraft's `pool-radius`).
pub fn far_sources(w: &World) -> Vec<Cell> {
    sources_within(w, POOL_RADIUS)
}

fn sources_within(w: &World, radius: f64) -> Vec<Cell> {
    let (Some(body), Some(dim)) = (w.body, w.dimension.as_ref()) else {
        return Vec::new();
    };
    let feet = crate::path::feet_cell(body.pos);
    let chunks = (radius as i32) / 16 + 1;
    let mut near: Vec<(f64, Cell)> = w
        .memory
        .sources_near(
            &dim.name,
            memory::Fluid::Water,
            feet.0 >> 4,
            feet.2 >> 4,
            chunks,
        )
        .into_iter()
        .map(|c| (distance(feet, c), c))
        .filter(|(d, _)| *d <= radius)
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

/// Where the look goes from `eye`: the first of the source's faces (the surface, the four sides,
/// the underside, each a little inside) within `reach` with a clear ray. A bucket's ray takes a
/// source through any face: a refill under a pool two blocks up found no stand aiming at the
/// surface alone (docs/problems/portal.md wall 16).
pub fn aim_for(w: &World, eye: [f64; 3], target: Cell, reach: f64) -> Option<[f64; 3]> {
    aim_with(w, eye, target, reach, clear_ray)
}

/// `aim_for` by the exact line only: the face the bucket is used on. The exact line agreed with
/// the server in every use counted (race 44165259286: 603 of 604; kitlava race 44236062785: no
/// use filled with it blocked, and 342 uses with it blocked filled nothing, 200 ticks and a try
/// each, wall 34). The sampled line still finds the stands and routes (letting the exact one
/// decide those too lit 32 of 224 against 56, wall 27).
pub fn aim_at_use(w: &World, eye: [f64; 3], target: Cell, reach: f64) -> Option<[f64; 3]> {
    aim_with(w, eye, target, reach, exact_ray)
}

fn aim_with(
    w: &World,
    eye: [f64; 3],
    (x, y, z): Cell,
    reach: f64,
    clear: fn(&World, [f64; 3], [f64; 3], Cell) -> bool,
) -> Option<[f64; 3]> {
    const FACES: [[f64; 3]; 6] = [
        [0.5, SURFACE_HEIGHT, 0.5],
        [0.1, 0.45, 0.5],
        [0.9, 0.45, 0.5],
        [0.5, 0.45, 0.1],
        [0.5, 0.45, 0.9],
        [0.5, 0.1, 0.5],
    ];
    FACES
        .iter()
        .map(|o| {
            [
                f64::from(x) + o[0],
                f64::from(y) + o[1],
                f64::from(z) + o[2],
            ]
        })
        .filter(|p| (0..3).map(|i| (eye[i] - p[i]).powi(2)).sum::<f64>().sqrt() <= reach)
        .find(|p| clear(w, eye, *p, (x, y, z)))
}

fn eye(body: &physics::Body) -> [f64; 3] {
    [body.pos[0], body.pos[1] + physics::EYE_HEIGHT, body.pos[2]]
}

fn source_of(w: &World, c: Cell, fluid: memory::Fluid) -> bool {
    w.state_at(world::BlockPos::new(c.0, c.1, c.2))
        .and_then(memory::Memory::source_fluid)
        == Some(fluid)
}

/// A stand's eye, from its cell's middle, within this of the surface: `REACH` less the body's
/// drift off the middle when a walk ends (0.3 drift put one at 4.6 past the game's 4.5).
const STAND_REACH: f64 = 4.0;
/// Stands tried for a route, nearest the feet first.
const STAND_TRIES: usize = 4;

/// The cells a scoop of `target` can be sent from: standable, the eye within `REACH` of the
/// surface with a clear ray to it; nearest the feet first.
pub fn stands(w: &World, target: Cell) -> Vec<Cell> {
    let Some(body) = w.body else {
        return Vec::new();
    };
    let feet = crate::path::feet_cell(body.pos);
    let mut cells = crate::path::Cells::new(w);
    let mut out: Vec<(f64, Cell)> = Vec::new();
    for dx in -4..=4 {
        for dz in -4..=4 {
            for dy in -4..=3 {
                let c = (target.0 + dx, target.1 + dy, target.2 + dz);
                let eye = [
                    f64::from(c.0) + 0.5,
                    f64::from(c.1) + physics::EYE_HEIGHT,
                    f64::from(c.2) + 0.5,
                ];
                if c == target || !cells.standable(c) {
                    continue;
                }
                if aim_for(w, eye, target, STAND_REACH).is_some() {
                    out.push((distance(feet, c), c));
                }
            }
        }
    }
    out.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
    out.into_iter().map(|(_, c)| c).collect()
}

/// Whether the eye's ray to `to` crosses nothing solid before the cell `target` (cells read from
/// the loaded terrain, which the eye can see along).
///
/// Sampled, forty points along the line. The exact traversal (`exact_ray`) decided this for one
/// pair of races (wall 27, 610fd87) and the pair lit 32 of 224 against 56: until a race's own
/// counts say where the two disagree with the server (`Scoop::uses`), the sampled line decides
/// and the exact one is measured beside it.
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
        c == target || open(w, c)
    })
}

/// A cell a bucket's ray passes: no collision shape.
fn open(w: &World, c: Cell) -> bool {
    w.block_at(world::BlockPos::new(c.0, c.1, c.2))
        .and_then(data::generated::shapes::collision)
        .is_some_and(|k| k == data::types::Collision::Empty)
}

/// `clear_ray` by exact traversal: every cell the line enters before `target` is open.
pub fn exact_ray(w: &World, from: [f64; 3], to: [f64; 3], target: Cell) -> bool {
    ray_cells(from, to)
        .into_iter()
        .all(|c| c == target || open(w, c))
}

/// Every cell the segment `from` to `to` enters, in order, the first excluded (exact voxel
/// traversal, Amanatides and Woo). The server's ray takes the first block it touches; forty
/// samples along the line stepped past a block's corner the ray grazed, and a lava scoop aimed
/// past deepslate filled nothing eight times (docs/problems/portal.md wall 27; clojurecraft
/// issue #225).
pub fn ray_cells(from: [f64; 3], to: [f64; 3]) -> Vec<Cell> {
    let d = [to[0] - from[0], to[1] - from[1], to[2] - from[2]];
    let mut c = [
        from[0].floor() as i32,
        from[1].floor() as i32,
        from[2].floor() as i32,
    ];
    let last = [
        to[0].floor() as i32,
        to[1].floor() as i32,
        to[2].floor() as i32,
    ];
    let step = d.map(|v| {
        if v > 0.0 {
            1
        } else if v < 0.0 {
            -1
        } else {
            0
        }
    });
    let mut t_max = [0.0f64; 3];
    let mut t_delta = [f64::INFINITY; 3];
    for i in 0..3 {
        if step[i] == 0 {
            t_max[i] = f64::INFINITY;
        } else {
            let edge = if step[i] > 0 {
                f64::from(c[i]) + 1.0
            } else {
                f64::from(c[i])
            };
            t_max[i] = (edge - from[i]) / d[i];
            t_delta[i] = (1.0 / d[i]).abs();
        }
    }
    let mut out = Vec::new();
    // At most the cells a segment of reach crosses, and a guard against a degenerate loop.
    for _ in 0..64 {
        if c == last {
            break;
        }
        let axis = if t_max[0] <= t_max[1] && t_max[0] <= t_max[2] {
            0
        } else if t_max[1] <= t_max[2] {
            1
        } else {
            2
        };
        if t_max[axis] > 1.0 {
            break;
        }
        c[axis] += step[axis];
        t_max[axis] += t_delta[axis];
        out.push((c[0], c[1], c[2]));
    }
    out
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
            sent_exact: None,
            uses: [0; 4],
        }
    }

    /// The uses, the one sent last counted as filled: a cast drops its refill the tick its
    /// bucket shows full, before the scoop's own step would see it (race 44159281696 counted no
    /// filled use in 21 lit casts).
    pub fn uses_filled(&mut self) -> [u32; 4] {
        if let Some(exact) = self.sent_exact.take() {
            self.uses[usize::from(!exact)] += 1;
        }
        self.uses
    }

    /// Whether a walk reaches a stand for it now (the churn invariant: never start a scoop with no
    /// way): a standable cell within reach of the surface with a clear look at it (`stands`),
    /// nearest the feet first. Being within reach is not enough: a refill aimed from the cast's
    /// own stand through a block beside it failed `NoSight` every try (docs/problems/portal.md
    /// wall 12).
    pub fn route_now(&mut self, w: &World, tick: u64) -> bool {
        if let Stage::Approach(_) = self.stage {
            // A stand's sight is judged from its cell's middle; a bot already in that cell sees
            // from where its eye is. That stand counts only if the real eye sees the source: the
            // empty route to it reached Aim with no face in sight, and the scoop failed NoSight
            // four times in four ticks on one source (83 casts in races 44184615869 and
            // 44184621358, wall 32).
            let here = w.body.map(|b| (crate::path::feet_cell(b.pos), eye(&b)));
            let target = self.target;
            let standing_blind = |st: Cell| {
                here.is_some_and(|(f, e)| f == st && aim_at_use(w, e, target, REACH).is_none())
            };
            for st in stands(w, self.target)
                .into_iter()
                .filter(|st| !standing_blind(*st))
                .take(STAND_TRIES)
            {
                let mut walk = Walk::new(st, Goal::Block { pos: st }, Arrive::Goal, tick);
                if walk.route_now(w, tick) && walk.route == Some(crate::path::Status::Found) {
                    self.stage = Stage::Approach(walk);
                    return true;
                }
            }
        }
        match &mut self.stage {
            // A found route only: a partial one toward a lake's middle failed every walk, source
            // after source along a shore (docs/problems/portal.md wall 6).
            // A partial route counts when it ends within reach of the surface: the walker keeps
            // off cells beside lava, so a pool's bank is reached only that way.
            // Found or partial, the route's end must see the source: a bot already within the
            // walk's range had an empty route Found, and the scoop failed `NoSight` four times in
            // four ticks on one source (docs/problems/portal.md wall 26).
            Stage::Approach(walk) => {
                let target = self.target;
                let here = w.body.map(|b| eye(&b));
                walk.route_now(w, tick)
                    && match walk.waypoints.as_ref().and_then(|wp| wp.last()) {
                        Some(end) => {
                            let e = [
                                f64::from(end.0) + 0.5,
                                f64::from(end.1) + physics::EYE_HEIGHT,
                                f64::from(end.2) + 0.5,
                            ];
                            aim_at_use(w, e, target, REACH).is_some()
                        }
                        None => here.is_some_and(|e| aim_at_use(w, e, target, REACH).is_some()),
                    }
            }
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
            if let Some(exact) = self.sent_exact.take() {
                self.uses[usize::from(!exact)] += 1;
            }
            return (still, None, ScoopStatus::Done);
        }
        // Gone only before the use is sent: a scooped source's block update arrives a tick before
        // the bucket's slot (and water forms again between two sources).
        if !matches!(self.stage, Stage::Sent { .. }) && !source_of(w, self.target, self.fluid) {
            return (still, None, ScoopStatus::Failed(ScoopFailure::Gone));
        }
        let from = eye(&body);
        let aim = aim_at_use(w, from, self.target, REACH);
        let point = aim.unwrap_or_else(|| surface(self.target));
        let in_reach = (0..3)
            .map(|i| (point[i] - from[i]).powi(2))
            .sum::<f64>()
            .sqrt()
            <= REACH;
        match &mut self.stage {
            Stage::Approach(walk) => {
                if aim.is_some() && body.on_ground {
                    self.stage = Stage::Aim { since: tick };
                    return (still, None, ScoopStatus::Running);
                }
                let (c, status) = walk.step(w, tick);
                match status {
                    TaskStatus::Running => (c, None, ScoopStatus::Running),
                    // At the stand chosen for it (within reach, a clear look): aim from there,
                    // the body a little off the cell's middle notwithstanding. A walk to within
                    // range of the source (no stand the walker takes: a pool's sources further in,
                    // beside flowing lava) aims only when it ended within reach.
                    TaskStatus::Done if in_reach || matches!(walk.goal, Goal::Block { .. }) => {
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
                // Only a face the exact line reaches is used on (wall 34): a use along a line it
                // calls blocked never filled, in every race counted.
                if aim.is_none() {
                    return (still, None, ScoopStatus::Failed(ScoopFailure::NoSight));
                }
                if tick.saturating_sub(since) < SETTLE_TICKS {
                    return (look, None, ScoopStatus::Running);
                }
                self.tries += 1;
                self.sent_exact = Some(exact_ray(w, from, point, self.target));
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
                // No answer: the use filled nothing.
                if let Some(exact) = self.sent_exact.take() {
                    self.uses[2 + usize::from(!exact)] += 1;
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
