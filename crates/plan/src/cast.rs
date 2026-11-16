//! Working a portal plan (issue 14c, brain/ruststeve/design/portal-cast.md): the executor of
//! `plan::build`'s plans, the site they are cut at, and the lava refills, after clojurecraft's
//! `build.clj` executors (`work-op`, `choose-site`, `refill`).
//!
//! The op worked is the first one not done; nothing about progress is stored, so a cast resumes
//! from the world. A dig is worked from wherever the bot can reach it; a place clicks the face of
//! a solid neighbour; a pour, a scoop and the light are worked from the op's own stand with the
//! look on its aim. Strict no X-ray (Bridger, 2026-11-15): a site is chosen from lava memory
//! saw exposed and cells the eye can see, and lava a dig exposes in the room gives the site up.

use data::{BlockKind, ItemKind};
use physics::Controls;
use world::{Act, BlockPos, World};

use crate::Cell;
use crate::build::{Done, Facing, Frame, Op, OpKind, frame_plan, holds, next_op};
use crate::dig::{Dig, DigStatus};
use crate::hold::{self, Hold};
use crate::path::Goal;
use crate::scoop::{Scoop, ScoopStatus};
use crate::walk::{Arrive, TaskStatus, Walk, look_at};

/// Ticks the look is held still on an aim before the use.
pub const SETTLE_TICKS: u64 = 6;
/// Ticks an act waits for the world to show it before trying again (clojurecraft's 3 s answer).
pub const ANSWER_TICKS: u64 = 60;
/// Tries at one op before the cast fails it (each a walk, dig, place or use begun).
pub const OP_TRIES: u32 = 3;
/// Ticks with no op done before the cast gives its site up (the progress invariant).
pub const STALL_TICKS: u64 = 2400;
/// Eye to a block's centre a dig or a place is worked from (the game's 4.5 less a margin).
pub const REACH: f64 = 4.0;
/// Blocks from the feet a lava source is looked at for a site (clojurecraft's `hunt-radius`).
pub const LAVA_RADIUS: f64 = 48.0;
/// Blocks from the feet lava seen is headed for when none is near (`far-radius`).
pub const FAR_RADIUS: f64 = 160.0;
/// A site's landing: this near the lava and no nearer, on the plane.
pub const LANDING_NEAR: i32 = 4;
pub const LANDING_FAR: i32 = 9;
/// The nearest lava sources a site is looked for beside (`max-sources`).
pub const MAX_SOURCES: usize = 3;
/// Blocks round a given-up site's origin no new site is chosen in (`give-up-radius`).
pub const GIVE_UP_RADIUS: f64 = 16.0;
/// Air cells under a placement a foundation is built up through (`foundation-depth`).
pub const FOUNDATION_DEPTH: i32 = 4;

/// The cell to place at for `pos` (`foundation`): `pos` when a solid neighbour holds a block
/// there, else the lowest air cell under it, at most `FOUNDATION_DEPTH` down, with a solid block
/// under it; built up from there one block a place.
fn foundation(w: &World, pos: Cell) -> Option<Cell> {
    let supported = |c: Cell| memory::neighbours(c).into_iter().any(|n| full(kind(w, n)));
    if supported(pos) {
        return Some(pos);
    }
    (1..=FOUNDATION_DEPTH)
        .map(|k| (pos.0, pos.1 - k, pos.2))
        .take_while(|c| kind(w, *c).is_some_and(|k| !full(Some(k))))
        .find(|c| full(kind(w, (c.0, c.1 - 1, c.2))))
}

/// Tunnels a refill cuts toward its pool before it fails.
pub const MAX_TUNNELS: u32 = 4;
/// Sources a refill tries a walk to, nearest first.
pub const REFILL_TRIES: usize = 4;
/// Blocks from a site's lava its pool is counted within (clojurecraft's `pool-radius`).
pub const POOL_RADIUS: f64 = 16.0;

/// Whether the pool by `lava` holds fewer sources than the frame has cells left to cast
/// (`spent?`): each cell spends one, and lava does not form again.
pub fn spent(w: &World, lava: Cell, frame: Frame) -> bool {
    let left = crate::build::FRAME_CELLS
        .iter()
        .filter(|c| kind(w, frame.at((c.0, c.1, 0))) != Some(BlockKind::Obsidian))
        .count();
    let pool = lava_sources(w)
        .into_iter()
        .filter(|c| dist(centre(*c), centre(lava)) <= POOL_RADIUS)
        .count();
    pool < left
}

/// Sites that pass that are collected before the best is routed to.
pub const SITE_CANDIDATES: usize = 12;
/// Candidate sites a route to the landing is looked for, best first.
pub const SITE_ROUTES: usize = 4;
/// Blocks a site's cast may place at most (`site-blocks`): a handful.
pub const SITE_BLOCKS: usize = 16;

/// The blocks a plan places where the eye sees the rock missing (`blocks-needed`, strict no
/// X-ray: a cell inside rock counts as rock). Cells the plan digs (room, holes) are not counted.
pub fn blocks_needed(w: &World, plan: &[Op]) -> usize {
    let dug: Vec<Cell> = plan
        .iter()
        .filter(|o| o.kind == OpKind::Dig && o.step != crate::build::Step::FallClear)
        .filter_map(|o| o.pos)
        .collect();
    let mut cells: Vec<Cell> = plan
        .iter()
        .filter(|o| o.kind == OpKind::Place)
        .filter_map(|o| o.pos)
        .filter(|c| !dug.contains(c))
        .filter(|c| seen(w, *c).is_some_and(|k| !full(Some(k))))
        // and the foundation under it, through the air seen there
        .flat_map(|c| {
            (0..=FOUNDATION_DEPTH)
                .map(move |k| (c.0, c.1 - k, c.2))
                .take_while(|b| seen(w, *b).is_some_and(|k| !full(Some(k))))
                .collect::<Vec<_>>()
        })
        .collect();
    cells.sort_unstable();
    cells.dedup();
    cells.len()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum CastFailure {
    /// An op failed `OP_TRIES` times.
    Op,
    /// No op done in `STALL_TICKS`.
    Stalled,
    /// Lava showed in the room (exposed by a dig).
    Exposed,
    /// A pour's bucket could not be filled: no lava or water source reached.
    NoRefill,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CastStatus {
    Running,
    Done,
    Failed(CastFailure),
}

/// Where a cast is cut: its frame, and the lava it refills from.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Site {
    pub frame: Frame,
    pub lava: Cell,
    /// The lava's height over the room's floor, which the room's access steps climb to
    /// (clojurecraft's `h`): a pool above the room is refilled from level with it (wall 16).
    #[serde(default)]
    pub h: i32,
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
enum Work {
    Idle,
    Walk(Walk),
    Dig(Box<Dig>),
    Placed {
        at: u64,
    },
    Aim {
        since: u64,
    },
    Sent {
        at: u64,
    },
    Refill(Box<Scoop>),
    /// A level tunnel toward a pool no walk reaches, before the refill's scoop (wall 13).
    Tunnel(Box<crate::stairs::Stairs>),
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Cast {
    pub site: Site,
    plan: Vec<Op>,
    /// The op worked: its index in the plan.
    op: Option<usize>,
    work: Work,
    tries: u32,
    last_done: u64,
    /// Ops done since the cast began, for the notes.
    pub done_ops: u32,
    /// Tunnels cut toward the pool for the refill in hand.
    #[serde(default)]
    tunnels: u32,
}

fn eye(b: &physics::Body) -> [f64; 3] {
    [b.pos[0], b.pos[1] + physics::EYE_HEIGHT, b.pos[2]]
}

fn centre((x, y, z): Cell) -> [f64; 3] {
    [f64::from(x) + 0.5, f64::from(y) + 0.5, f64::from(z) + 0.5]
}

fn dist(a: [f64; 3], b: [f64; 3]) -> f64 {
    (0..3).map(|i| (a[i] - b[i]).powi(2)).sum::<f64>().sqrt()
}

fn kind(w: &World, (x, y, z): Cell) -> Option<BlockKind> {
    w.block_at(BlockPos::new(x, y, z))
}

fn full(k: Option<BlockKind>) -> bool {
    k.and_then(data::generated::shapes::collision) == Some(data::types::Collision::Full)
}

/// The kind at a cell when the eye could see it: a neighbour lets a face show (memory's rule).
/// Strict no X-ray: a cell inside rock reads as `None`.
pub fn seen(w: &World, c: Cell) -> Option<BlockKind> {
    let k = kind(w, c)?;
    let open = memory::neighbours(c)
        .into_iter()
        .any(|n| kind(w, n).is_some() && !full(kind(w, n)));
    open.then_some(k)
}

fn feet(b: &physics::Body) -> Cell {
    crate::path::feet_cell(b.pos)
}

/// The cells of a plan's room and frame (its footprint).
pub fn footprint(plan: &[Op]) -> Vec<Cell> {
    let mut out: Vec<Cell> = plan
        .iter()
        .filter_map(|o| o.pos)
        .chain(plan.iter().map(|o| o.stand))
        .collect();
    out.sort_unstable();
    out.dedup();
    out
}

/// The holes and the cells over them: fluid there is the cast's own.
fn cast_cells(plan: &[Op]) -> Vec<Cell> {
    plan.iter()
        .filter(|o| o.kind == OpKind::Pour || o.kind == OpKind::Scoop)
        .filter_map(|o| o.into.or(o.pos))
        .collect()
}

impl Cast {
    pub fn new(site: Site, tick: u64) -> Self {
        Self {
            site,
            plan: frame_plan(site.frame, false, site.h),
            op: None,
            work: Work::Idle,
            tries: 0,
            last_done: tick,
            done_ops: 0,
            tunnels: 0,
        }
    }

    /// The op being worked, for the notes.
    pub fn current(&self) -> Option<&Op> {
        self.op.and_then(|i| self.plan.get(i))
    }

    /// Lava seen in the room or the frame's walls that is not the cast's own.
    fn exposed(&self, w: &World) -> bool {
        let own = cast_cells(&self.plan);
        footprint(&self.plan)
            .into_iter()
            .filter(|c| !own.contains(c))
            .any(|c| seen(w, c) == Some(BlockKind::Lava))
    }

    pub fn step(&mut self, w: &World, tick: u64) -> (Controls, Option<Act>, CastStatus) {
        let still = Controls::default();
        let Some(body) = w.body else {
            return (still, None, CastStatus::Running);
        };
        let Some((i, op)) = next_op(w, &self.plan).and_then(|o| {
            self.plan
                .iter()
                .position(|p| std::ptr::eq(p, o))
                .map(|i| (i, o.clone()))
        }) else {
            return (still, None, CastStatus::Done);
        };
        if self.op != Some(i) {
            if self.op.is_some() {
                self.done_ops += 1;
                self.last_done = tick;
            }
            self.op = Some(i);
            self.work = Work::Idle;
            self.tries = 0;
        }
        if tick.saturating_sub(self.last_done) > STALL_TICKS {
            return (still, None, CastStatus::Failed(CastFailure::Stalled));
        }
        if self.exposed(w) {
            return (still, None, CastStatus::Failed(CastFailure::Exposed));
        }
        match op.kind {
            OpKind::Dig => self.dig(w, &body, &op, tick),
            OpKind::Place => self.place(w, &body, &op, tick),
            OpKind::Pour | OpKind::Scoop | OpKind::UseOn => self.use_at(w, &body, &op, tick),
            OpKind::Verify | OpKind::StandIn => self.go_to(w, &body, op.stand, tick),
        }
    }

    fn fail(&mut self) -> (Controls, Option<Act>, CastStatus) {
        (
            Controls::default(),
            None,
            CastStatus::Failed(CastFailure::Op),
        )
    }

    /// A new try at the op: counted; over `OP_TRIES` the cast fails.
    fn try_again(&mut self) -> bool {
        self.tries += 1;
        self.work = Work::Idle;
        self.tries <= OP_TRIES
    }

    /// Walk to within reach of `target` (or onto `stand` when given), then `Ok`.
    fn approach(
        &mut self,
        w: &World,
        body: &physics::Body,
        target: Cell,
        stand: Option<Cell>,
        tick: u64,
    ) -> Result<(), (Controls, Option<Act>, CastStatus)> {
        let there = match stand {
            Some(s) => feet(body) == s && body.on_ground,
            None => dist(eye(body), centre(target)) <= REACH && body.on_ground,
        };
        if there {
            if let Work::Walk(_) = self.work {
                self.work = Work::Idle;
            }
            return Ok(());
        }
        if !matches!(self.work, Work::Walk(_)) {
            let goal = match stand {
                Some(s) => Goal::Block { pos: s },
                None => Goal::Near {
                    pos: target,
                    range: 2.5,
                },
            };
            let mut walk = Walk::new(stand.unwrap_or(target), goal, Arrive::Goal, tick);
            if !walk.route_now(w, tick) {
                return Err(if self.try_again() {
                    (Controls::default(), None, CastStatus::Running)
                } else {
                    self.fail()
                });
            }
            self.work = Work::Walk(walk);
        }
        let Work::Walk(walk) = &mut self.work else {
            return Err((Controls::default(), None, CastStatus::Running));
        };
        let (c, status) = walk.step(w, tick);
        match status {
            TaskStatus::Running => Err((c, None, CastStatus::Running)),
            TaskStatus::Done => {
                self.work = Work::Idle;
                Err((Controls::default(), None, CastStatus::Running))
            }
            TaskStatus::Failed(_) => Err(if self.try_again() {
                (Controls::default(), None, CastStatus::Running)
            } else {
                self.fail()
            }),
        }
    }

    fn go_to(
        &mut self,
        w: &World,
        body: &physics::Body,
        stand: Cell,
        tick: u64,
    ) -> (Controls, Option<Act>, CastStatus) {
        match self.approach(w, body, stand, Some(stand), tick) {
            Ok(()) => (Controls::default(), None, CastStatus::Running),
            Err(r) => r,
        }
    }

    fn dig(
        &mut self,
        w: &World,
        body: &physics::Body,
        op: &Op,
        tick: u64,
    ) -> (Controls, Option<Act>, CastStatus) {
        let Some(pos) = op.pos else {
            return self.fail();
        };
        if !matches!(self.work, Work::Dig(_))
            && let Err(r) = self.approach(w, body, pos, None, tick)
        {
            return r;
        }
        if matches!(self.work, Work::Idle) {
            // A dig never takes obsidian: its op is done then, and never worked.
            self.work = Work::Dig(Box::new(Dig::new(w, pos, tick)));
        }
        let Work::Dig(d) = &mut self.work else {
            return (Controls::default(), None, CastStatus::Running);
        };
        let (c, act, status) = d.step(w, tick);
        match status {
            DigStatus::Running => (c, act, CastStatus::Running),
            // Done: the op's predicate holds next tick, or the dig is tried again.
            DigStatus::Done => {
                self.work = Work::Idle;
                (c, act, CastStatus::Running)
            }
            DigStatus::Failed(_) => {
                if holds(w, &op.done) {
                    self.work = Work::Idle;
                    return (c, act, CastStatus::Running);
                }
                if self.try_again() {
                    (c, act, CastStatus::Running)
                } else {
                    self.fail()
                }
            }
        }
    }

    fn place(
        &mut self,
        w: &World,
        body: &physics::Body,
        op: &Op,
        tick: u64,
    ) -> (Controls, Option<Act>, CastStatus) {
        let Some(want) = op.pos else {
            return self.fail();
        };
        let Some(pos) = foundation(w, want) else {
            return self.fail();
        };
        if let Work::Placed { at } = self.work {
            if tick.saturating_sub(at) < ANSWER_TICKS / 3 {
                return (Controls::default(), None, CastStatus::Running);
            }
            if !self.try_again() {
                return self.fail();
            }
        }
        // Never place into the body: the server refuses a block where the player stands, so a
        // place whose cell the feet or the head are in steps out of it first.
        let f = feet(body);
        if f == pos || (f.0, f.1 + 1, f.2) == pos {
            if !matches!(self.work, Work::Walk(_)) {
                let mut walk = Walk::new(pos, Goal::Away { pos, range: 1.5 }, Arrive::Goal, tick);
                if !walk.route_now(w, tick) {
                    return if self.try_again() {
                        (Controls::default(), None, CastStatus::Running)
                    } else {
                        self.fail()
                    };
                }
                self.work = Work::Walk(walk);
            }
            if let Work::Walk(walk) = &mut self.work {
                let (c, status) = walk.step(w, tick);
                if status != TaskStatus::Running {
                    self.work = Work::Idle;
                }
                return (c, None, CastStatus::Running);
            }
        }
        if let Err(r) = self.approach(w, body, pos, None, tick) {
            return r;
        }
        match hold::toward(w, op.item.unwrap_or(ItemKind::Cobblestone)) {
            Hold::Held => {}
            Hold::Act(a) => return (Controls::default(), Some(a), CastStatus::Running),
            Hold::Wait => return (Controls::default(), None, CastStatus::Running),
            Hold::Missing => return self.fail(),
        }
        // A solid neighbour and its face toward the cell (0 down, 1 up, 2 north, 3 south, 4 west,
        // 5 east): the block under first, as a player places.
        const NEIGHBOURS: [((i32, i32, i32), i8); 6] = [
            ((0, -1, 0), 1),
            ((0, 1, 0), 0),
            ((0, 0, -1), 3),
            ((0, 0, 1), 2),
            ((-1, 0, 0), 5),
            ((1, 0, 0), 4),
        ];
        let Some((n, face)) = NEIGHBOURS.iter().find_map(|((dx, dy, dz), face)| {
            let n = (pos.0 + dx, pos.1 + dy, pos.2 + dz);
            full(kind(w, n)).then_some((n, *face))
        }) else {
            return if self.try_again() {
                (Controls::default(), None, CastStatus::Running)
            } else {
                self.fail()
            };
        };
        self.work = Work::Placed { at: tick };
        (
            Controls::default(),
            Some(Act::UseOn {
                pos: BlockPos::new(n.0, n.1, n.2),
                face,
            }),
            CastStatus::Running,
        )
    }

    /// A pour, a scoop or the light, from the op's stand with the look on its aim.
    fn use_at(
        &mut self,
        w: &World,
        body: &physics::Body,
        op: &Op,
        tick: u64,
    ) -> (Controls, Option<Act>, CastStatus) {
        let item = op.item.unwrap_or(ItemKind::Bucket);
        // A bucket to pour that is not held: fill it first (lava at the site's pool, water at a
        // remembered surface source).
        if op.kind == OpKind::Pour && crate::craft::count(w, item) == 0 {
            return self.refill(w, item, tick);
        }
        if let Work::Refill(_) = self.work {
            self.work = Work::Idle;
        }
        if let Err(r) = self.approach(w, body, op.stand, Some(op.stand), tick) {
            return r;
        }
        match hold::toward(w, item) {
            Hold::Held => {}
            Hold::Act(a) => return (Controls::default(), Some(a), CastStatus::Running),
            Hold::Wait => return (Controls::default(), None, CastStatus::Running),
            Hold::Missing => return self.fail(),
        }
        let target = op
            .aim
            .or_else(|| {
                op.pos.map(|p| {
                    [
                        f64::from(p.0) + 0.5,
                        f64::from(p.1) + 1.0,
                        f64::from(p.2) + 0.5,
                    ]
                })
            })
            .unwrap_or_else(|| centre(op.stand));
        let (yaw, pitch) = look_at(eye(body), target);
        let look = Controls {
            yaw,
            pitch: Some(pitch),
            ..Controls::default()
        };
        match self.work {
            Work::Idle => {
                self.work = Work::Aim { since: tick };
                (look, None, CastStatus::Running)
            }
            Work::Aim { since } if tick.saturating_sub(since) < SETTLE_TICKS => {
                (look, None, CastStatus::Running)
            }
            Work::Aim { .. } => {
                self.work = Work::Sent { at: tick };
                let act = match (op.kind, op.pos) {
                    (OpKind::UseOn, Some(p)) => Act::UseOn {
                        pos: BlockPos::new(p.0, p.1, p.2),
                        face: op.face.unwrap_or(1),
                    },
                    _ => Act::UseItem,
                };
                (look, Some(act), CastStatus::Running)
            }
            Work::Sent { at } if tick.saturating_sub(at) < ANSWER_TICKS => {
                (look, None, CastStatus::Running)
            }
            _ => {
                if self.try_again() {
                    (look, None, CastStatus::Running)
                } else {
                    self.fail()
                }
            }
        }
    }

    /// Fill a bucket for a pour: lava at the site's pool, water at a remembered surface source.
    fn refill(
        &mut self,
        w: &World,
        item: ItemKind,
        tick: u64,
    ) -> (Controls, Option<Act>, CastStatus) {
        if let Work::Tunnel(st) = &mut self.work {
            let (c, act, status) = st.step(w, tick);
            if status == crate::stairs::StairsStatus::Running {
                return (c, act, CastStatus::Running);
            }
            self.work = Work::Idle;
            self.tunnels += 1;
            self.last_done = tick;
            return (c, act, CastStatus::Running);
        }
        if !matches!(self.work, Work::Refill(_)) {
            let fluid = if item == ItemKind::LavaBucket {
                memory::Fluid::Lava
            } else {
                memory::Fluid::Water
            };
            // The nearest few sources first, the first a walk reaches: a pool's edge sources go
            // first, and those further in can be out of reach from its bank.
            let own = cast_cells(&self.plan);
            let mut targets: Vec<Cell> = match fluid {
                memory::Fluid::Lava => lava_sources(w)
                    .into_iter()
                    .filter(|c| !own.contains(c))
                    .filter(|c| dist(centre(*c), centre(self.site.lava)) <= POOL_RADIUS)
                    .collect(),
                memory::Fluid::Water => crate::scoop::sources(w),
            };
            targets.truncate(REFILL_TRIES);
            let nearest = targets.first().copied();
            let Some(s) = targets.into_iter().find_map(|t| {
                let mut s = Scoop::of(t, fluid, tick);
                s.route_now(w, tick).then_some(s)
            }) else {
                // No walk to the pool: tunnel toward its nearest source (clojurecraft digs a way to
                // its station); a tunnel refuses cells with lava in or beside them, so it stops
                // short of it, within a bucket's reach (wall 13).
                if let (Some(t), Some(body), true) = (nearest, w.body, self.tunnels < MAX_TUNNELS) {
                    let f = feet(&body);
                    let (dx, dz) = (t.0 - f.0, t.2 - f.2);
                    let dir = if dx.abs() >= dz.abs() {
                        (dx.signum(), 0)
                    } else {
                        (0, dz.signum())
                    };
                    if dir != (0, 0) {
                        self.work = Work::Tunnel(Box::new(crate::stairs::Stairs::level(dir, tick)));
                        self.last_done = tick;
                        return (Controls::default(), None, CastStatus::Running);
                    }
                }
                return (
                    Controls::default(),
                    None,
                    CastStatus::Failed(CastFailure::NoRefill),
                );
            };
            self.tunnels = 0;
            self.work = Work::Refill(Box::new(s));
            self.last_done = tick;
        }
        let Work::Refill(s) = &mut self.work else {
            return (Controls::default(), None, CastStatus::Running);
        };
        let (c, act, status) = s.step(w, tick);
        match status {
            ScoopStatus::Running => (c, act, CastStatus::Running),
            ScoopStatus::Done => {
                self.work = Work::Idle;
                (c, act, CastStatus::Running)
            }
            ScoopStatus::Failed(_) => {
                self.work = Work::Idle;
                if self.try_again() {
                    (c, act, CastStatus::Running)
                } else {
                    (c, act, CastStatus::Failed(CastFailure::NoRefill))
                }
            }
        }
    }
}

/// The lava sources memory saw exposed within `LAVA_RADIUS` of the feet, nearest first.
pub fn lava_sources(w: &World) -> Vec<Cell> {
    lava_within(w, LAVA_RADIUS)
}

/// The nearest lava source memory saw exposed within `FAR_RADIUS`: what a hunt heads for
/// (clojurecraft's `far-radius`).
pub fn far_lava(w: &World, given_up: &[Cell]) -> Option<Cell> {
    let here = w.body.map_or([0.0; 3], |b| centre(feet(&b)));
    lava_within(w, FAR_RADIUS).into_iter().find(|c| {
        // Lava within `LAVA_RADIUS` has been through the site choice already: a hunt toward it
        // walked three bots back to a three-source pool for the whole race (wall 15).
        dist(here, centre(*c)) > LAVA_RADIUS
            && !given_up
                .iter()
                .any(|g| dist(centre(*g), centre(*c)) <= GIVE_UP_RADIUS)
    })
}

fn lava_within(w: &World, radius: f64) -> Vec<Cell> {
    let (Some(body), Some(dim)) = (w.body, w.dimension.as_ref()) else {
        return Vec::new();
    };
    let f = feet(&body);
    let r = (radius as i32) / 16 + 1;
    let fc = centre(f);
    let mut near: Vec<(f64, Cell)> = w
        .memory
        .sources_near(&dim.name, memory::Fluid::Lava, f.0 >> 4, f.2 >> 4, r)
        .into_iter()
        .map(|c| (dist(fc, centre(c)), c))
        .filter(|(d, _)| *d <= radius)
        .collect();
    near.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
    near.into_iter().map(|(_, c)| c).collect()
}

/// A site beside one of the nearest lava sources (`choose-site`): the room's landing an open
/// standable cell `LANDING_NEAR` to `LANDING_FAR` from the lava on the plane, at its height to
/// three over; every footprint cell loaded, none seen holding fluid and none within a block of a
/// remembered lava source; the frame facing so the lava is in front of it. The nearest landing to
/// the feet wins. `given_up`: origins whose radius is refused.
pub fn choose_site(w: &World, given_up: &[Cell]) -> Option<Site> {
    site_choice(w, given_up).and_then(|c| c.routed.then_some(c.site))
}

/// The best site found and whether a walk reaches its landing: with none routed, the best
/// unrouted one and its landing, for the planner to dig toward (wall 9).
pub struct SiteChoice {
    pub site: Site,
    pub landing: Cell,
    pub routed: bool,
}

/// Why sites were passed over, counted over one choice: the hunt's note carries it, so a race's
/// case files say which rule kept a bot beside lava from casting (wall 13).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SiteReport {
    pub sources: usize,
    pub landings: usize,
    pub refused_or_spent: usize,
    pub lava_behind: usize,
    pub footprint: usize,
    pub blocks: usize,
    pub no_station: usize,
    pub passed: usize,
}

impl SiteReport {
    pub fn text(&self) -> String {
        format!(
            "sources {} landings {} refused/spent {} behind {} footprint {} blocks {} station {} passed {}",
            self.sources,
            self.landings,
            self.refused_or_spent,
            self.lava_behind,
            self.footprint,
            self.blocks,
            self.no_station,
            self.passed
        )
    }
}

pub fn site_choice(w: &World, given_up: &[Cell]) -> Option<SiteChoice> {
    site_choice_report(w, given_up).0
}

pub fn site_choice_report(w: &World, given_up: &[Cell]) -> (Option<SiteChoice>, SiteReport) {
    let mut report = SiteReport::default();
    let Some(body) = w.body else {
        return (None, report);
    };
    let here = centre(feet(&body));
    let lava = lava_sources(w);
    let refused = |c: Cell| {
        given_up
            .iter()
            .any(|g| dist(centre(*g), centre(c)) <= GIVE_UP_RADIUS)
    };
    let mut cells = crate::path::Cells::new(w);
    // The standable landings first, nearest the feet first: a plan is built only for those, and
    // the search stops at `SITE_CANDIDATES` sites that pass (an open cave has thousands).
    let mut landings: Vec<(f64, Cell, Cell)> = Vec::new();
    for src in lava.iter().filter(|c| !refused(**c)).take(MAX_SOURCES) {
        report.sources += 1;
        for dy in -3..=3 {
            for dx in -LANDING_FAR..=LANDING_FAR {
                for dz in -LANDING_FAR..=LANDING_FAR {
                    let plane = dx.abs().max(dz.abs());
                    if !(LANDING_NEAR..=LANDING_FAR).contains(&plane) {
                        continue;
                    }
                    let landing = (src.0 + dx, src.1 + dy, src.2 + dz);
                    if cells.standable(landing) {
                        landings.push((dist(here, centre(landing)), landing, *src));
                    }
                }
            }
        }
    }
    landings.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
    landings.dedup_by_key(|l| l.1);
    report.landings = landings.len();
    let mut found: Vec<((usize, f64), Site, Cell)> = Vec::new();
    for (d, landing, src) in landings {
        if found.len() >= SITE_CANDIDATES {
            break;
        }
        let src = &src;
        for facing in Facing::ALL {
            let probe = Frame {
                origin: (0, 0, 0),
                facing,
            };
            let l0 = probe.at(crate::build::landing(0));
            let frame = Frame {
                origin: (landing.0 - l0.0, landing.1 - l0.1, landing.2 - l0.2),
                facing,
            };
            if refused(frame.origin) || spent(w, *src, frame) {
                report.refused_or_spent += 1;
                continue;
            }
            // The lava in front: farther from the frame's cells than the landing.
            let back = frame.at((1, 0, 0));
            if dist(centre(*src), centre(back)) <= dist(centre(landing), centre(back)) {
                report.lava_behind += 1;
                continue;
            }
            // Access steps up to a pool above the room: its surface one over the step's feet.
            let h = (src.1 + 1 - landing.1).clamp(0, crate::build::MAX_ACCESS);
            let plan = frame_plan(frame, false, h);
            let foot = footprint(&plan);
            let ok = foot.iter().all(|c| {
                kind(w, *c).is_some()
                    && !matches!(seen(w, *c), Some(BlockKind::Water | BlockKind::Lava))
                    // Lava flows into a cell through a face only: a source sharing one, not a
                    // diagonal (wall 13: the diagonal rule refused 74 of 140 sites round one pool).
                    && !lava.iter().any(|l| {
                        (l.0 - c.0).abs() + (l.1 - c.1).abs() + (l.2 - c.2).abs() <= 1
                    })
            });
            if !ok {
                report.footprint += 1;
                continue;
            }
            let n = blocks_needed(w, &plan);
            if n > SITE_BLOCKS {
                report.blocks += 1;
                continue;
            }
            // A station (clojurecraft's): a walk from the landing to within a bucket's reach of
            // the lava, or every refill fails (casts that cut their whole room failed their first
            // pour: wall 10).
            if !station(w, landing, *src) {
                report.no_station += 1;
                continue;
            }
            report.passed += 1;
            found.push((
                (n, d),
                Site {
                    frame,
                    lava: *src,
                    h,
                },
                landing,
            ));
            break;
        }
    }
    // The churn invariant: a site is taken only when a walk to its landing is found (one bot at
    // y -32 chose a landing at y -52 in another cave and failed its first dig: wall 4).
    found.sort_by(|a, b| a.0.0.cmp(&b.0.0).then(a.0.1.total_cmp(&b.0.1)));
    let from = feet(&body);
    let best = found.first().map(|(_, s, l)| SiteChoice {
        site: *s,
        landing: *l,
        routed: false,
    });
    let choice = found
        .into_iter()
        .take(SITE_ROUTES)
        .find(|(_, _, landing)| {
            crate::path::plan(
                w,
                from,
                &Goal::Block { pos: *landing },
                crate::path::Budget::Far.expansions(),
            )
            .status
                == crate::path::Status::Found
        })
        .map(|(_, s, l)| SiteChoice {
            site: s,
            landing: l,
            routed: true,
        })
        .or(best);
    (choice, report)
}

/// Whether a walk from `landing` ends within a bucket's reach of the lava source `src`.
pub fn station(w: &World, landing: Cell, src: Cell) -> bool {
    let route = crate::path::plan(
        w,
        landing,
        &Goal::Near {
            pos: src,
            range: 3.5,
        },
        crate::path::Budget::Far.expansions(),
    );
    let surface = [
        f64::from(src.0) + 0.5,
        f64::from(src.1) + crate::scoop::SURFACE_HEIGHT,
        f64::from(src.2) + 0.5,
    ];
    let end = route.waypoints.last().copied().unwrap_or(landing);
    let eye = [
        f64::from(end.0) + 0.5,
        f64::from(end.1) + physics::EYE_HEIGHT,
        f64::from(end.2) + 0.5,
    ];
    // Or a way to dig: lava no more than three below the landing nor one above it, which a
    // refill's tunnel reaches (the walker keeps off cells beside lava, so a pool in rock has no
    // walk to its bank: wall 13).
    dist(eye, surface) <= crate::scoop::REACH || (-3..=1).contains(&(src.1 - landing.1))
}

/// Whether the plan's light op holds: a portal block in the frame.
pub fn lit(w: &World, site: &Site) -> bool {
    holds(
        w,
        &Done::Block(site.frame.at((1, 1, 0)), BlockKind::NetherPortal),
    )
}
