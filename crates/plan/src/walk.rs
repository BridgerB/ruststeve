//! The walk (brain/ruststeve/design/walk-and-watchdog.md): clojurecraft's walk intent, following a
//! route's waypoints and planning again as chunks arrive, with a progress invariant: a walk that
//! stops getting closer fails fast (`NoProgress`) instead of pacing.

use physics::Controls;
use world::World;

use crate::path::{self, Budget, Cell, Goal, Status};

/// Horizontal distance to a waypoint's centre that reaches it (clojurecraft's `waypoint-reach`).
pub const WAYPOINT_REACH: f64 = 0.35;
/// A waypoint above the feet is jumped for from this close (`jump-near`).
pub const JUMP_NEAR: f64 = 1.3;
/// No waypoint reached for this many ticks: plan again (`stuck-ticks`, 2 s).
pub const STUCK_TICKS: u64 = 40;
/// Plans one walk may make before it fails `NoPath` (`max-replans`).
pub const MAX_REPLANS: u32 = 6;
/// The ticks a walk may take, and more for each waypoint (`walk-timeout-ticks`,
/// `waypoint-ticks`).
pub const WALK_TIMEOUT_TICKS: u64 = 1200;
pub const WAYPOINT_TICKS: u64 = 10;
/// A route to a block ends within this of it (`goal-range`).
pub const GOAL_RANGE: f64 = 3.0;
/// Eye to block centre within which a block is reached (`reach`; the server allows about 4.5).
pub const REACH: f64 = 4.0;
/// A log is reached only from no more than this far below it (`pickup-rise`).
pub const PICKUP_RISE: i32 = 2;
/// The progress invariant: no gain toward the target for this many ticks fails the walk
/// (brain/ruststeve/design/progress-invariants.md). Ten game seconds, well under the issue's
/// "no walk 60 game seconds without progress".
pub const PATIENCE_TICKS: u64 = 200;
/// A gain smaller than this is not progress (a body jittering in place is not getting closer).
pub const PROGRESS_STEP: f64 = 0.25;

/// Why a task failed: data, for recovery and for the case file.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Failure {
    /// No route, or the replans ran out.
    NoPath,
    /// The progress invariant: no gain for `PATIENCE_TICKS`.
    NoProgress,
    /// Over its deadline.
    Deadline,
    /// The body is gone (a dimension change, a death).
    TargetLost,
}

/// How a task stands after a tick.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum TaskStatus {
    Running,
    Done,
    Failed(Failure),
}

/// What ends a walk: within reach of a block (a log), or at its route's goal (exploring).
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum Arrive {
    Reach,
    Goal,
}

/// A walk in progress. The target is held until the walk ends, never re-chosen per tick.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Walk {
    pub target: Cell,
    pub goal: Goal,
    pub arrive: Arrive,
    pub waypoints: Option<Vec<Cell>>,
    pub at: usize,
    pub route: Option<Status>,
    pub replans: u32,
    pub started: u64,
    /// The last tick a waypoint was reached (or the route planned).
    pub best_tick: u64,
    /// The progress invariant's state: the nearest the walk has come, when, and the longest it
    /// went without gaining.
    pub best_distance: f64,
    pub best_distance_at: u64,
    pub longest_stall: u64,
    /// What its searches spent.
    pub expanded: u64,
}

fn horizontal(a: [f64; 3], b: [f64; 3]) -> f64 {
    ((a[0] - b[0]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
}

fn centre((x, y, z): Cell) -> [f64; 3] {
    [f64::from(x) + 0.5, f64::from(y) + 0.5, f64::from(z) + 0.5]
}

/// Yaw toward a point, Notchian (south 0, west +90): clojurecraft's `physics/look-at`.
pub fn yaw_to(from: [f64; 3], to: [f64; 3]) -> f64 {
    let (dx, dz) = (to[0] - from[0], to[2] - from[2]);
    (-dx).atan2(dz).to_degrees()
}

impl Walk {
    pub fn new(target: Cell, goal: Goal, arrive: Arrive, tick: u64) -> Self {
        Self {
            target,
            goal,
            arrive,
            waypoints: None,
            at: 0,
            route: None,
            replans: 0,
            started: tick,
            best_tick: tick,
            best_distance: f64::MAX,
            best_distance_at: tick,
            longest_stall: 0,
            expanded: 0,
        }
    }

    /// A walk to within reach of a block (a log): its route ends within `GOAL_RANGE`.
    pub fn to_block(target: Cell, tick: u64) -> Self {
        Self::new(
            target,
            Goal::Near {
                pos: target,
                range: GOAL_RANGE,
            },
            Arrive::Reach,
            tick,
        )
    }

    fn arrived(&self, pos: [f64; 3]) -> bool {
        match self.arrive {
            Arrive::Reach => {
                let eye = [pos[0], pos[1] + physics::EYE_HEIGHT, pos[2]];
                let c = centre(self.target);
                let d =
                    ((eye[0] - c[0]).powi(2) + (eye[1] - c[1]).powi(2) + (eye[2] - c[2]).powi(2))
                        .sqrt();
                d <= REACH && self.target.1 - pos[1].floor() as i32 <= PICKUP_RISE
            }
            Arrive::Goal => self.goal.done(path::feet_cell(pos)),
        }
    }

    fn deadline(&self) -> u64 {
        WALK_TIMEOUT_TICKS + WAYPOINT_TICKS * self.waypoints.as_ref().map_or(0, |w| w.len() as u64)
    }

    /// Plan the route now and say whether the target is walkable (clojurecraft's `walkable?`):
    /// the route is found, or it ends short only because the target's column is not loaded yet
    /// (the walk plans again as chunks arrive). A partial route toward a loaded target is not a
    /// way: a log seen down a cave or up a cliff is not walked at. A walk with no way is never
    /// started (the churn invariant).
    pub fn route_now(&mut self, w: &World, tick: u64) -> bool {
        let Some(body) = w.body else {
            return false;
        };
        self.route(w, body.pos, tick);
        let loaded = w.columns.contains_key(
            &world::BlockPos::new(self.target.0, self.target.1, self.target.2).chunk(),
        );
        match self.route {
            Some(Status::Found) => true,
            // An explore only needs ground to cover: any route with waypoints, the progress
            // invariant failing it if it paces.
            Some(Status::Partial | Status::None) => {
                (!loaded || self.arrive == Arrive::Goal)
                    && self.waypoints.as_ref().is_some_and(|w| !w.is_empty())
            }
            None => false,
        }
    }

    fn route(&mut self, w: &World, pos: [f64; 3], tick: u64) {
        let r = path::plan(
            w,
            path::feet_cell(pos),
            &self.goal,
            Budget::Far.expansions(),
        );
        self.expanded += u64::from(r.expanded);
        self.waypoints = Some(r.waypoints);
        self.route = Some(r.status);
        self.at = 0;
        self.replans += 1;
        self.best_tick = tick;
    }

    /// clojurecraft's `route-stale?`: a waypoint ahead can no longer be stood on, no waypoint
    /// reached for `STUCK_TICKS`, or a short route walked out.
    fn stale(&self, w: &World, tick: u64) -> bool {
        let Some(wps) = &self.waypoints else {
            return true;
        };
        let ahead = wps.get(self.at..).unwrap_or_default();
        let mut cells = path::Cells::new(w);
        ahead.iter().any(|c| !cells.standable(*c))
            || tick.saturating_sub(self.best_tick) > STUCK_TICKS
            || (ahead.is_empty() && self.route != Some(Status::Found))
    }

    /// One tick: the controls to press and how the walk stands.
    pub fn step(&mut self, w: &World, tick: u64) -> (Controls, TaskStatus) {
        let Some(body) = w.body else {
            return (Controls::default(), TaskStatus::Failed(Failure::TargetLost));
        };
        let pos = body.pos;
        // The progress invariant, every tick.
        let d = horizontal(pos, centre(self.target));
        if d < self.best_distance - PROGRESS_STEP {
            self.best_distance = d;
            self.best_distance_at = tick;
        }
        let stall = tick.saturating_sub(self.best_distance_at);
        self.longest_stall = self.longest_stall.max(stall);
        if self.arrived(pos) {
            return (Controls::default(), TaskStatus::Done);
        }
        if stall > PATIENCE_TICKS {
            return (Controls::default(), TaskStatus::Failed(Failure::NoProgress));
        }
        if tick.saturating_sub(self.started) > self.deadline() {
            return (Controls::default(), TaskStatus::Failed(Failure::Deadline));
        }
        if self.waypoints.is_none() {
            self.route(w, pos, tick);
            let none = self.route == Some(Status::None)
                && self.waypoints.as_ref().is_some_and(Vec::is_empty);
            return if none {
                (Controls::default(), TaskStatus::Failed(Failure::NoPath))
            } else {
                (Controls::default(), TaskStatus::Running)
            };
        }
        if self.stale(w, tick) {
            if self.replans >= MAX_REPLANS {
                return (Controls::default(), TaskStatus::Failed(Failure::NoPath));
            }
            self.route(w, pos, tick);
            return (Controls::default(), TaskStatus::Running);
        }
        let Some(next) = self
            .waypoints
            .as_ref()
            .and_then(|w| w.get(self.at))
            .copied()
        else {
            // The route is walked out and the target still out of reach: straight at it.
            return (toward(body, centre(self.target)), TaskStatus::Running);
        };
        let (wx, wy, wz) = next;
        let wc = [f64::from(wx) + 0.5, f64::from(wy), f64::from(wz) + 0.5];
        if horizontal(pos, wc) <= WAYPOINT_REACH && (pos[1] - f64::from(wy)).abs() <= 0.5 {
            self.at += 1;
            self.best_tick = tick;
            return (Controls::default(), TaskStatus::Running);
        }
        (follow(body, next), TaskStatus::Running)
    }
}

/// Toward a point, jumping when blocked (clojurecraft's `toward`).
fn toward(body: physics::Body, point: [f64; 3]) -> Controls {
    Controls {
        forward: true,
        jump: body.horizontal_collision,
        sneak: false,
        yaw: yaw_to(body.pos, point),
    }
}

/// To a waypoint (clojurecraft's `follow`): face its centre, forward, jump only when it is above
/// and near, or when blocked; stop once wholly over a descending waypoint.
fn follow(body: physics::Body, (wx, wy, wz): Cell) -> Controls {
    let [px, py, pz] = body.pos;
    let centre = [
        f64::from(wx) + 0.5,
        f64::from(wy) + 0.5,
        f64::from(wz) + 0.5,
    ];
    let flat = horizontal(body.pos, [f64::from(wx) + 0.5, 0.0, f64::from(wz) + 0.5]);
    if py > f64::from(wy) && flat <= 0.5 - physics::HALF_WIDTH {
        return Controls::default();
    }
    let near = horizontal([px, 0.0, pz], centre) < JUMP_NEAR;
    let up = f64::from(wy) > py.floor() + 0.5;
    let mut c = toward(body, centre);
    c.jump = c.jump || (up && near);
    c
}
