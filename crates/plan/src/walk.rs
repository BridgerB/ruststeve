//! The walk (brain/ruststeve/design/walk-and-watchdog.md): clojurecraft's walk intent, following a
//! route's waypoints and planning again as chunks arrive, with a progress invariant: a walk that
//! stops getting closer fails fast (`NoProgress`) instead of pacing.

use physics::Controls;
use world::{Act, BlockPos, World};

use crate::path::{self, Budget, Cell, Goal, MoveKind, Status};

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

/// A far walk's search (docs/problems/nether-walk.md): the expansions it may spend and the weight
/// on its heuristic. Measured on the recorded Nether of race 45194253932 lane 5 at tick 25,140
/// (probe 45336893578): from the portal to the bricks 110 blocks off, weight 2 finds a route in
/// 147,900 expansions (1.8 s), 3 in 35,525, 4 in 12,664 (0.1 s; 222 cells, 163 bridged), 10 in
/// 8,906; with the chunks half loaded a search spends all it is given and finds none.
pub const FAR_NODES: u32 = 40_000;
pub const FAR_WEIGHT: f64 = 4.0;
/// After a far search that found no route the planner runs no other for this long: ten seconds
/// (a bot just through a portal has half its chunks, and a search then finds none).
pub const FAR_RETRY_TICKS: u64 = 200;
/// After a far route was found, the least ticks before the planner searches the next.
pub const FAR_AGAIN_TICKS: u64 = 20;
/// Waypoints ahead a far walk watches for a mob to keep clear of.
pub const SHUN_AHEAD: usize = 12;
/// Ticks between two clicks laying one bridge block: the block update arrives in a tick or two.
pub const PLACE_EVERY_TICKS: u64 = 6;
/// Clicks for one bridge block before the walk fails.
pub const MAX_PLACE_TRIES: u32 = 8;
/// The ticks a walk may take more for each cell it bridges (hold the block, click, step).
pub const BRIDGE_TICKS: u64 = 40;
/// Laying a block, the body stays this near the centre of the block it stands on.
pub const BRIDGE_STAND: f64 = 0.45;

/// Brick columns within the nine 16-block squares round a brick that make it part of a fortress's
/// body and not a lone pillar: a corridor five wide crossing three squares is 240; the pillar in
/// the lava sea the first far walks ended at ((58 to 66, 95 to 104) of race 45337580375) is 60.
pub const BODY_COLUMNS: usize = 200;

/// The first of `bricks` (nearest first) that stands in a fortress's body: `BODY_COLUMNS` brick
/// columns in the nine 16-block squares round it. A far walk prefers it to a nearer lone pillar,
/// which is `fortress` met on a spot with lava all round (docs/problems/nether-walk.md).
pub fn brick_in_the_body(bricks: &[Cell]) -> Option<Cell> {
    use std::collections::{BTreeMap, BTreeSet};
    let mut squares: BTreeMap<(i32, i32), BTreeSet<(i32, i32)>> = BTreeMap::new();
    for (x, _, z) in bricks {
        squares
            .entry((x >> 4, z >> 4))
            .or_default()
            .insert((*x, *z));
    }
    let mut counted: BTreeMap<(i32, i32), usize> = BTreeMap::new();
    bricks.iter().copied().find(|(x, _, z)| {
        let at = (x >> 4, z >> 4);
        *counted.entry(at).or_insert_with(|| {
            (-1..=1)
                .flat_map(|dx| (-1..=1).map(move |dz| (at.0 + dx, at.1 + dz)))
                .map(|k| squares.get(&k).map_or(0, BTreeSet::len))
                .sum()
        }) >= BODY_COLUMNS
    })
}

/// Is there rock under the feet to tunnel into: a full block under them with a full block under
/// that (a causeway is one block thick over lava or air), and no lava at the door. Where there
/// is not, the fortress leg takes the far walk before the tunnel.
pub fn rock_to_tunnel(w: &World, (x, y, z): Cell) -> bool {
    let full = |c: Cell| {
        w.block_at(BlockPos::new(c.0, c.1, c.2))
            .and_then(data::generated::shapes::collision)
            == Some(data::types::Collision::Full)
    };
    full((x, y - 1, z))
        && full((x, y - 2, z))
        && crate::escape::lava_door_at(w, (x, y, z)).is_none()
}

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
    /// The route's moves and the bridge in hand (boxed: a walk sits inside other tasks' enums).
    #[serde(default)]
    pub more: Box<More>,
}

/// What a walk keeps beyond clojurecraft's: its route's moves, and its bridging.
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct More {
    /// The move that reaches each waypoint (`MoveKind::Bridge` lays its floor first), and the
    /// cell the route was searched from.
    pub kinds: Vec<MoveKind>,
    pub from: Option<Cell>,
    /// A far walk: its searches spend `FAR_NODES` at `FAR_WEIGHT`.
    pub far: bool,
    /// Bridge blocks laid, the last click and the clicks sent for the block in hand.
    pub placed: u32,
    pub place_tick: u64,
    pub place_tries: u32,
    /// The floor cell being laid.
    pub laying: Option<Cell>,
    /// Acts to send and notes to write, taken by the planner each tick (`take_out`).
    pub acts: Vec<Act>,
    pub said: Vec<(String, String)>,
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

/// The look from an eye to a point (issue 14a: a bucket is used along it): the yaw, and the
/// pitch in Notchian degrees, down positive (`calculateViewVector`'s inverse).
pub fn look_at(eye: [f64; 3], to: [f64; 3]) -> (f64, f32) {
    let (dx, dy, dz) = (to[0] - eye[0], to[1] - eye[1], to[2] - eye[2]);
    let flat = (dx * dx + dz * dz).sqrt();
    #[allow(clippy::cast_possible_truncation)]
    let pitch = (-dy.atan2(flat).to_degrees()) as f32;
    (yaw_to(eye, to), pitch)
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
            more: Box::default(),
        }
    }

    /// A walk to a place far off (a fortress seen 110 blocks away): to within `range` of
    /// `target`, its route searched with `FAR_NODES` at `FAR_WEIGHT`, bridging where the search
    /// may. Arrives at the goal.
    pub fn far(target: Cell, range: f64, tick: u64) -> Self {
        let mut walk = Self::new(
            target,
            Goal::Near { pos: target, range },
            Arrive::Goal,
            tick,
        );
        walk.more.far = true;
        walk
    }

    /// A far walk to any goal (an explore's `Goal::Afar`, a spawner's `Goal::Near`): as `far`,
    /// for the planner's other legs. Start it only when `route_now` answers a found route, and
    /// run it as `Task::Explore`: the planner sends its acts and writes its notes there.
    pub fn far_to(target: Cell, goal: Goal, tick: u64) -> Self {
        let mut walk = Self::new(target, goal, Arrive::Goal, tick);
        walk.more.far = true;
        walk
    }

    /// The acts and notes this tick left, for the planner to send and write.
    pub fn take_out(&mut self) -> (Vec<Act>, Vec<(String, String)>) {
        (
            std::mem::take(&mut self.more.acts),
            std::mem::take(&mut self.more.said),
        )
    }

    /// The bridge cells the route still has ahead.
    pub fn bridges_ahead(&self) -> usize {
        self.more
            .kinds
            .iter()
            .skip(self.at)
            .filter(|k| **k == MoveKind::Bridge)
            .count()
    }

    /// A walk to beside a block to dig (issue 10): where its drop falls within the pickup box,
    /// a neighbouring column (diagonals too) with the block at the feet's height or up to
    /// `PICKUP_RISE` above (clojurecraft's `:pickup` goal). Arrives at the goal.
    pub fn to_dig(target: Cell, tick: u64) -> Self {
        Self::new(
            target,
            Goal::Pickup {
                pos: target,
                range: path::DIAGONAL_COST,
                rise: PICKUP_RISE,
            },
            Arrive::Goal,
            tick,
        )
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
        WALK_TIMEOUT_TICKS
            + WAYPOINT_TICKS * self.waypoints.as_ref().map_or(0, |w| w.len() as u64)
            + BRIDGE_TICKS
                * self
                    .more
                    .kinds
                    .iter()
                    .filter(|k| **k == MoveKind::Bridge)
                    .count() as u64
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
        let (nodes, weight) = if self.more.far {
            (FAR_NODES, FAR_WEIGHT)
        } else {
            (Budget::Far.expansions(), 1.0)
        };
        let r = if self.more.far {
            path::plan_far(w, path::feet_cell(pos), &self.goal, nodes, weight)
        } else {
            path::plan_weighted(w, path::feet_cell(pos), &self.goal, nodes, weight)
        };
        self.expanded += u64::from(r.expanded);
        self.waypoints = Some(r.waypoints);
        self.more.kinds = r.kinds;
        self.more.from = r.from;
        self.more.laying = None;
        self.more.place_tries = 0;
        // A far search is dear: each one after the first is written down with what it spent.
        if self.more.far && self.replans > 0 {
            self.more.said.push((
                "far route planned again".to_owned(),
                format!(
                    "{:?}, {} cells, {} expansions; plan {} of {MAX_REPLANS}",
                    r.status,
                    self.waypoints.as_ref().map_or(0, Vec::len),
                    r.expanded,
                    self.replans + 1
                ),
            ));
        }
        self.route = Some(r.status);
        self.at = 0;
        self.replans += 1;
        self.best_tick = tick;
    }

    /// A far walk's own reason to plan again: a mob its route keeps clear of (`path::SHUN_KINDS`)
    /// has come within its radius of the next `SHUN_AHEAD` waypoints.
    fn foe_ahead(&self, w: &World) -> bool {
        if !self.more.far {
            return false;
        }
        let cells = path::Cells::far(w);
        let from = self.more.from.unwrap_or(self.target);
        self.waypoints
            .as_ref()
            .and_then(|wps| wps.get(self.at..))
            .unwrap_or_default()
            .iter()
            .take(SHUN_AHEAD)
            .any(|c| cells.shunned(*c, from))
    }

    /// clojurecraft's `route-stale?`: a waypoint ahead can no longer be stood on, no waypoint
    /// reached for `STUCK_TICKS`, or a short route walked out.
    fn stale(&self, w: &World, tick: u64) -> bool {
        let Some(wps) = &self.waypoints else {
            return true;
        };
        let ahead = wps.get(self.at..).unwrap_or_default();
        let mut cells = if self.more.far {
            path::Cells::far(w)
        } else {
            path::Cells::new(w)
        };
        self.foe_ahead(w)
            || ahead.iter().enumerate().any(|(i, c)| {
                let kind = self
                    .more
                    .kinds
                    .get(self.at + i)
                    .copied()
                    .unwrap_or(MoveKind::Walk);
                !cells.usable(*c, kind)
            })
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
            if self.foe_ahead(w) {
                self.more.said.push((
                    "a mob to keep clear of is near the way ahead".to_owned(),
                    format!(
                        "within {} of the next {SHUN_AHEAD} cells; plan again round it",
                        path::SHUN_RADIUS
                    ),
                ));
            }
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
            // On a found route a waypoint reached is progress, though the route leads round
            // (a way over a lava sea goes away from its goal before it comes back).
            if self.route == Some(Status::Found) {
                self.best_distance_at = tick;
            }
            return (Controls::default(), TaskStatus::Running);
        }
        if self.more.kinds.get(self.at) == Some(&MoveKind::Bridge)
            && let Some(out) = self.bridge(w, body, next, tick)
        {
            return out;
        }
        (follow(body, next), TaskStatus::Running)
    }

    /// The bridge move's executor: with no floor under `next`, stand on the block before it and
    /// lay a held block against that block's side (the server asks only that the block clicked is
    /// in reach, not that its face is in view). `None` once the floor is there: walk on. Until
    /// then the body never leaves the block it stands on.
    fn bridge(
        &mut self,
        w: &World,
        body: physics::Body,
        next: Cell,
        tick: u64,
    ) -> Option<(Controls, TaskStatus)> {
        let still = Controls::default();
        let floor = (next.0, next.1 - 1, next.2);
        let mut cells = path::Cells::new(w);
        if cells.floor(floor) {
            if self.more.laying.take() == Some(floor) {
                self.more.placed += 1;
                self.more.place_tries = 0;
                self.more.said.push((
                    format!("bridge block laid at {floor:?}"),
                    format!(
                        "the route's bridge move, {} laid on this walk, {} bridge cells ahead",
                        self.more.placed,
                        self.bridges_ahead().saturating_sub(1)
                    ),
                ));
            }
            return None;
        }
        // The block stood on: the waypoint before, or the cell the route was searched from. With
        // no such block to click the walk fails here: it never steps out over a floor not laid.
        let lost = Some((still, TaskStatus::Failed(Failure::NoPath)));
        let prev = match self.at.checked_sub(1) {
            Some(i) => self.waypoints.as_ref().and_then(|w| w.get(i)).copied(),
            None => self.more.from,
        };
        let Some(prev) = prev else {
            return lost;
        };
        let face = match (next.0 - prev.0, next.2 - prev.2) {
            (0, -1) => 2,
            (0, 1) => 3,
            (-1, 0) => 4,
            (1, 0) => 5,
            _ => return lost,
        };
        let under = (prev.0, prev.1 - 1, prev.2);
        if prev.1 != next.1 || !cells.floor(under) {
            return lost;
        }
        // Progress here is a block laid, not ground gained.
        self.best_tick = tick;
        let stand = [
            f64::from(prev.0) + 0.5,
            body.pos[1],
            f64::from(prev.2) + 0.5,
        ];
        if horizontal(body.pos, stand) > BRIDGE_STAND || !body.on_ground {
            let mut c = toward(body, stand);
            c.jump = false;
            c.forward = body.on_ground;
            return Some((c, TaskStatus::Running));
        }
        let Some(item) = path::bridge_item(w) else {
            self.more.said.push((
                format!("no bridge block for {floor:?}"),
                "none of the bridge blocks is held".to_owned(),
            ));
            return Some((still, TaskStatus::Failed(Failure::NoPath)));
        };
        match crate::hold::toward(w, item) {
            crate::hold::Hold::Act(a) => self.more.acts.push(a),
            crate::hold::Hold::Wait => {}
            crate::hold::Hold::Missing => {
                return Some((still, TaskStatus::Failed(Failure::NoPath)));
            }
            crate::hold::Hold::Held => {
                if tick.saturating_sub(self.more.place_tick) >= PLACE_EVERY_TICKS {
                    if self.more.place_tries >= MAX_PLACE_TRIES {
                        self.more.said.push((
                            format!("bridge block at {floor:?} not laid"),
                            format!("{MAX_PLACE_TRIES} clicks on {under:?} face {face}"),
                        ));
                        return Some((still, TaskStatus::Failed(Failure::NoPath)));
                    }
                    self.more.place_tries += 1;
                    self.more.place_tick = tick;
                    self.more.laying = Some(floor);
                    self.more.acts.push(Act::UseOn {
                        pos: BlockPos::new(under.0, under.1, under.2),
                        face,
                    });
                }
            }
        }
        Some((still, TaskStatus::Running))
    }
}

/// Toward a point, jumping when blocked (clojurecraft's `toward`).
fn toward(body: physics::Body, point: [f64; 3]) -> Controls {
    Controls {
        forward: true,
        jump: body.horizontal_collision,
        sneak: false,
        yaw: yaw_to(body.pos, point),
        pitch: None,
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
    // A cell up, read from the cell the feet stand in (`FEET_LIFT`): on soul sand, feet at
    // y + 0.875, the next cell's floor line is an eighth up, a step and no jump.
    let up = f64::from(wy) > (py + path::FEET_LIFT).floor() + 0.5;
    // That eighth is stepped, never jumped: under a two-high roof a jump hits the lintel, comes
    // down on the soul sand still blocked by the eighth, and jumps again (the model's kit bot
    // hopped at (61, 46.88, 79.7) for 8,000 ticks: docs/problems/nether-walk.md).
    let rise = f64::from(wy) - py;
    let low_step = body.on_ground && rise > 0.0 && rise <= physics::STEP_HEIGHT;
    let mut c = toward(body, centre);
    c.jump = (c.jump && !low_step) || (up && near);
    c
}

#[cfg(test)]
mod body_tests {
    use super::*;

    /// A lone pillar near and a corridor far: the corridor's nearest brick is the body's.
    #[test]
    fn a_corridor_is_preferred_to_a_nearer_pillar() {
        let pillar: Vec<Cell> = (58..=66)
            .flat_map(|x| (95..=104).flat_map(move |z| (28..=40).map(move |y| (x, y, z))))
            .collect();
        let corridor: Vec<Cell> = (100..=160)
            .flat_map(|x| (150..=154).map(move |z| (x, 64, z)))
            .collect();
        let mut bricks = pillar.clone();
        bricks.extend(corridor);
        assert_eq!(brick_in_the_body(&bricks), Some((100, 64, 150)));
        assert_eq!(brick_in_the_body(&pillar), None);
    }
}
