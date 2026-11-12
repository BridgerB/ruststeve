//! The planner (brain/ruststeve/design/planner.md): what the bot does next, as a pure function of
//! the world and its own held task. Issue 09 begins it with one goal row, "reach a log", the
//! pathfinder (`path`, a port of clojurecraft's `path.clj`) and the walk (`walk`) with its progress
//! invariant. Every task it starts or ends is an event with its reason. Issue 10 adds the goal
//! table and the needs planner (`make`), and the tasks that change the world: `dig` and
//! `craft`; they act through `Act`s the shell takes with `take_acts`.

#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

pub mod craft;
pub mod dig;
pub mod make;
pub mod path;
pub mod walk;

use std::collections::BTreeMap;
use std::sync::Arc;

use data::generated::tags::block_tag;
use physics::Controls;
use world::World;

use crate::craft::{Craft, CraftStatus};
use crate::dig::{Dig, DigStatus};
use crate::path::{Cell, Goal};
use crate::walk::{Arrive, TaskStatus, Walk};

/// A failed target is refused for this long (clojurecraft's `failure-memory-ms`, 30 s).
pub const FAILURE_MEMORY_TICKS: u64 = 600;
/// After this many failures a target is refused for good (`blocked-after`).
pub const BLOCKED_AFTER: u32 = 3;
/// An explore walks this far on its heading (`explore-distance`).
pub const EXPLORE_DISTANCE: i32 = 48;
/// Columns out from the bot a target search looks (a ring query on memory's column index,
/// never a scan of the world).
pub const SEARCH_COLUMNS: i32 = 8;
/// After a failed task the planner waits this long before choosing again: a choice that fails at
/// once must not be made every tick (the churn invariant).
pub const RETRY_TICKS: u64 = 20;
/// Failed walks in a row, with nothing done between, after which the planner stops: stuck,
/// said so, and the run ends there (clojurecraft's `max-unwalkable`, 12).
pub const MAX_UNWALKABLE: u32 = 12;
/// A target that failed `NoPath` refuses every target within this of it: one unreachable tree
/// is not tried log by log (clojurecraft's `area-radius`).
pub const AREA_RADIUS: f64 = 6.0;
/// Targets the planner routes to in one tick before it waits: a choice is checked for a way
/// before it is made, and the check is bounded (brain/ruststeve/design/progress-invariants.md).
pub const CANDIDATES_PER_TICK: usize = 2;
/// Events kept for `why` and the case file.
const EVENTS_KEPT: usize = 64;

/// What the planner is for: issue 09's "reach a log at least `min_distance` blocks from where
/// the planner began", or since issue 10 a goal row of the table put in play by its name
/// (`table`: the `kit` row, four sticks and a crafting table).
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum Aim {
    ReachLog { min_distance: f64 },
    Table,
}

impl Aim {
    /// `log` or `log:<min>`.
    pub fn parse(s: &str) -> Option<Aim> {
        match s.split_once(':') {
            None if s == "log" => Some(Aim::ReachLog { min_distance: 0.0 }),
            None if s == "table" => Some(Aim::Table),
            Some(("log", m)) => m
                .parse()
                .ok()
                .map(|min_distance| Aim::ReachLog { min_distance }),
            _ => None,
        }
    }

    pub fn name(&self) -> String {
        match self {
            Aim::ReachLog { min_distance } => format!("log:{min_distance}"),
            Aim::Table => "table".to_owned(),
        }
    }
}

/// The task the planner holds.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum Task {
    Idle,
    /// Walking to a log seen with an exposed face.
    ToLog(Walk),
    /// No log in view: walking out on a held heading to see more.
    Explore(Walk),
    /// Digging a block in reach and collecting its drop.
    Dig(Dig),
    /// One run of a 2x2 recipe in window 0.
    Craft(Craft),
}

impl Task {
    pub fn name(&self) -> &'static str {
        match self {
            Task::Idle => "idle",
            Task::ToLog(_) => "walk-to-log",
            Task::Explore(_) => "explore",
            Task::Dig(_) => "dig",
            Task::Craft(_) => "craft",
        }
    }

    pub fn walk(&self) -> Option<&Walk> {
        match self {
            Task::Idle | Task::Dig(_) | Task::Craft(_) => None,
            Task::ToLog(w) | Task::Explore(w) => Some(w),
        }
    }
}

/// What happened, with why: the planner's decisions as data (brain/ruststeve/design/brain-events.md).
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct PlanEvent {
    pub tick: u64,
    pub what: String,
    pub why: String,
}

/// The planner's state.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Plan {
    pub aim: Aim,
    pub task: Task,
    /// Where the planner began: distances for the aim count from here.
    pub origin: Option<[f64; 3]>,
    /// The log reached, once the aim is met.
    pub reached: Option<Cell>,
    /// Failed targets: until when they are refused, and how often they failed.
    pub refused: BTreeMap<Cell, (u64, u32)>,
    /// The explore heading, held across explores and turned a quarter on each failure.
    pub heading: f64,
    pub events: Vec<PlanEvent>,
    /// For the batch: walks started, how many failed and why, the longest stall of any walk,
    /// searches' expansions.
    pub walks: u64,
    pub failures: BTreeMap<String, u64>,
    pub longest_stall: u64,
    pub expanded: u64,
    /// The planner chooses again only from this tick (`RETRY_TICKS` after a failure).
    pub retry_at: u64,
    /// Failed walks since the last one done, and whether the planner has stopped.
    pub failed_in_row: u32,
    pub stuck: bool,
    /// Areas refused after a `NoPath`: their centre and until when.
    pub refused_areas: Vec<(Cell, u64)>,
    /// Events ever made (`events` keeps the last few): the shell records the new ones.
    pub event_count: u64,
    /// The goal table (issue 10), for a goal aim.
    #[serde(skip)]
    pub table: Option<Arc<make::Table>>,
    /// A goal aim is met.
    #[serde(default)]
    pub met: bool,
    /// Digs and crafts done, and their failures by reason (in `failures`).
    #[serde(default)]
    pub digs: u64,
    #[serde(default)]
    pub crafts: u64,
    /// Acts made this tick, for the shell (`take_acts`).
    #[serde(skip)]
    acts: Vec<world::Act>,
}

impl Plan {
    pub fn new(aim: Aim) -> Self {
        Self {
            aim,
            task: Task::Idle,
            origin: None,
            reached: None,
            refused: BTreeMap::new(),
            heading: 0.0,
            events: Vec::new(),
            walks: 0,
            failures: BTreeMap::new(),
            longest_stall: 0,
            expanded: 0,
            retry_at: 0,
            failed_in_row: 0,
            stuck: false,
            refused_areas: Vec::new(),
            event_count: 0,
            table: None,
            met: false,
            digs: 0,
            crafts: 0,
            acts: Vec::new(),
        }
    }

    /// A planner for a goal aim, with the goal table.
    pub fn with_table(aim: Aim, table: Arc<make::Table>) -> Self {
        let mut p = Self::new(aim);
        p.table = Some(table);
        p
    }

    /// The aim is met, or the planner has stopped.
    pub fn finished(&self) -> bool {
        self.reached.is_some() || self.met || self.stuck
    }

    /// The acts made since the last call, in order: the shell sends them.
    pub fn take_acts(&mut self) -> Vec<world::Act> {
        std::mem::take(&mut self.acts)
    }

    fn note(&mut self, tick: u64, what: String, why: String) {
        self.events.push(PlanEvent { tick, what, why });
        self.event_count += 1;
        if self.events.len() > EVENTS_KEPT {
            self.events.remove(0);
        }
    }

    /// The events made since `seen` of them were taken, oldest first.
    pub fn events_since(&self, seen: u64) -> &[PlanEvent] {
        let new = usize::try_from(self.event_count.saturating_sub(seen)).unwrap_or(usize::MAX);
        let from = self.events.len().saturating_sub(new);
        self.events.get(from..).unwrap_or_default()
    }

    /// Why the current task runs, for the case file and the live stream.
    pub fn why(&self) -> String {
        self.events
            .last()
            .map_or_else(|| "no task yet".to_owned(), |e| e.why.clone())
    }

    /// One tick: the controls to press. The world is read, never changed.
    pub fn step(&mut self, w: &World) -> Controls {
        let tick = w.ticks;
        let Some(body) = w.body else {
            return Controls::default();
        };
        if self.finished() {
            return Controls::default();
        }
        let origin = *self.origin.get_or_insert(body.pos);
        if matches!(self.task, Task::Idle) {
            if tick < self.retry_at {
                return Controls::default();
            }
            self.task = match self.aim {
                Aim::ReachLog { .. } => self.choose(w, origin, tick),
                Aim::Table => self.decide(w, origin, tick),
            };
        }
        let (controls, status) = match &mut self.task {
            Task::Idle => (Controls::default(), TaskStatus::Running),
            Task::ToLog(walk) | Task::Explore(walk) => walk.step(w, tick),
            Task::Dig(d) => {
                let (c, act, status) = d.step(w, tick);
                self.acts.extend(act);
                return self.after_dig(c, status, tick);
            }
            Task::Craft(cr) => {
                let (act, status) = cr.step(w, tick);
                self.acts.extend(act);
                return self.after_craft(status, tick);
            }
        };
        match status {
            TaskStatus::Running => controls,
            TaskStatus::Done => {
                self.finish_walk();
                self.failed_in_row = 0;
                match std::mem::replace(&mut self.task, Task::Idle) {
                    Task::ToLog(walk) if self.aim == Aim::Table => {
                        self.note(
                            tick,
                            format!("dig log {:?}", walk.target),
                            "in reach of the log the needs planner gathers".to_owned(),
                        );
                        self.task = Task::Dig(Dig::new(w, walk.target, tick));
                    }
                    Task::ToLog(walk) => {
                        self.reached = Some(walk.target);
                        self.note(
                            tick,
                            format!("reached log {:?}", walk.target),
                            "the aim is met".to_owned(),
                        );
                    }
                    Task::Explore(walk) => {
                        self.note(
                            tick,
                            format!("explored to {:?}", walk.target),
                            "look again for a log".to_owned(),
                        );
                    }
                    Task::Idle | Task::Dig(_) | Task::Craft(_) => {}
                }
                Controls::default()
            }
            TaskStatus::Failed(f) => {
                self.finish_walk();
                *self.failures.entry(format!("{f:?}")).or_default() += 1;
                self.retry_at = tick + RETRY_TICKS;
                self.failed_in_row += 1;
                if self.failed_in_row >= MAX_UNWALKABLE {
                    self.stuck = true;
                }
                match std::mem::replace(&mut self.task, Task::Idle) {
                    Task::ToLog(walk) => {
                        self.refuse(walk.target, tick);
                        if f == walk::Failure::NoPath {
                            self.refused_areas
                                .push((walk.target, tick + FAILURE_MEMORY_TICKS));
                        }
                        self.note(
                            tick,
                            format!("walk to log {:?} failed: {f:?}", walk.target),
                            "refused for a while; choose again".to_owned(),
                        );
                    }
                    Task::Explore(walk) => {
                        self.heading = (self.heading + 90.0) % 360.0;
                        self.note(
                            tick,
                            format!("explore to {:?} failed: {f:?}", walk.target),
                            "turn the heading a quarter".to_owned(),
                        );
                    }
                    Task::Idle | Task::Dig(_) | Task::Craft(_) => {}
                }
                if self.stuck {
                    self.note(
                        tick,
                        "stuck".to_owned(),
                        format!("{MAX_UNWALKABLE} walks failed in a row with none done"),
                    );
                }
                Controls::default()
            }
        }
    }

    fn finish_walk(&mut self) {
        if let Some(w) = self.task.walk() {
            self.longest_stall = self.longest_stall.max(w.longest_stall);
            self.expanded += w.expanded;
        }
    }

    fn refuse(&mut self, c: Cell, tick: u64) {
        let e = self.refused.entry(c).or_insert((0, 0));
        e.1 += 1;
        e.0 = if e.1 >= BLOCKED_AFTER {
            u64::MAX
        } else {
            tick + FAILURE_MEMORY_TICKS
        };
    }

    fn refused_now(&self, c: Cell, tick: u64) -> bool {
        let near = |(a, b): (Cell, Cell)| {
            let (dx, dy, dz) = (
                f64::from(a.0 - b.0),
                f64::from(a.1 - b.1),
                f64::from(a.2 - b.2),
            );
            (dx * dx + dy * dy + dz * dz).sqrt() <= AREA_RADIUS
        };
        self.refused.get(&c).is_some_and(|(until, _)| tick < *until)
            || self
                .refused_areas
                .iter()
                .any(|(centre, until)| tick < *until && near((*centre, c)))
    }

    /// The next task: the nearest log in view far enough from the origin, else explore.
    /// The next task: the nearest log in view far enough from the origin, else explore. Each
    /// candidate is routed before it is chosen, so a chosen walk always has a way (the churn
    /// invariant: the chosen task is not over after one tick); at most `CANDIDATES_PER_TICK`
    /// are tried, and with none the planner waits and counts a failed choice.
    fn choose(&mut self, w: &World, origin: [f64; 3], tick: u64) -> Task {
        let min_distance = match self.aim {
            Aim::ReachLog { min_distance } => min_distance,
            Aim::Table => 0.0,
        };
        let Some(body) = w.body else {
            return Task::Idle;
        };
        for _ in 0..CANDIDATES_PER_TICK {
            let Some(log) = self.nearest_log(w, origin, min_distance, tick) else {
                break;
            };
            let mut walk = match self.aim {
                Aim::ReachLog { .. } => Walk::to_block(log, tick),
                // To dig it: beside it, where its drop falls in the pickup box.
                Aim::Table => Walk::to_dig(log, tick),
            };
            if walk.route_now(w, tick) {
                self.walks += 1;
                self.note(
                    tick,
                    format!("walk to log {log:?}"),
                    format!("the nearest log in view at least {min_distance} blocks from the start, with a route"),
                );
                return Task::ToLog(walk);
            }
            self.expanded += walk.expanded;
            self.refuse(log, tick);
            self.refused_areas.push((log, tick + FAILURE_MEMORY_TICKS));
        }
        // No log in view with a way (none seen, or each down a cave, up a cliff, across water):
        // explore, which finds others, before counting a failed choice.
        for _ in 0..CANDIDATES_PER_TICK {
            let (s, c) = self.heading.to_radians().sin_cos();
            let d = f64::from(EXPLORE_DISTANCE);
            let p = [body.pos[0] - s * d, body.pos[1], body.pos[2] + c * d];
            let target = path::feet_cell(p);
            let mut walk = Walk::new(
                target,
                Goal::Afar {
                    pos: path::feet_cell(body.pos),
                    range: d - 4.0,
                },
                Arrive::Goal,
                tick,
            );
            if walk.route_now(w, tick) {
                self.walks += 1;
                self.note(
                    tick,
                    format!("explore toward {target:?}"),
                    format!(
                        "no log in view at least {min_distance} blocks from the start with a way; heading {}",
                        self.heading
                    ),
                );
                return Task::Explore(walk);
            }
            self.expanded += walk.expanded;
            self.heading = (self.heading + 90.0) % 360.0;
        }
        // Nothing with a way this tick: wait, and count it toward giving up.
        self.retry_at = tick + RETRY_TICKS;
        self.failed_in_row += 1;
        *self.failures.entry("NoWay".to_owned()).or_default() += 1;
        if self.failed_in_row >= MAX_UNWALKABLE {
            self.stuck = true;
            self.note(
                tick,
                "stuck".to_owned(),
                format!("{MAX_UNWALKABLE} choices in a row had no way"),
            );
        } else {
            self.note(
                tick,
                "wait".to_owned(),
                "no candidate had a way; look again shortly".to_owned(),
            );
        }
        Task::Idle
    }

    /// A goal aim's next task, from the needs planner over what is held (re-derived each time
    /// the planner is idle; nothing stored): met, a craft, or a gather (a walk to the nearest
    /// log in view with a way, or an explore).
    fn decide(&mut self, w: &World, origin: [f64; 3], tick: u64) -> Task {
        let Some(table) = self.table.clone() else {
            self.stuck = true;
            self.note(tick, "stuck".to_owned(), "no goal table".to_owned());
            return Task::Idle;
        };
        let Some(goal) = table.until(&self.aim.name()) else {
            self.stuck = true;
            self.note(
                tick,
                "stuck".to_owned(),
                format!("no goal row for {}", self.aim.name()),
            );
            return Task::Idle;
        };
        match make::make(&table, &held(w), goal) {
            make::Choice::Done => {
                self.met = true;
                self.note(
                    tick,
                    format!("{} met", goal.id),
                    "its provides are held".to_owned(),
                );
                Task::Idle
            }
            make::Choice::Stuck => {
                self.stuck = true;
                self.note(
                    tick,
                    "stuck".to_owned(),
                    format!("no producer for {} can be done", goal.id),
                );
                Task::Idle
            }
            make::Choice::Act(row) => match row.act {
                Some(make::Act::Craft(r)) => {
                    self.note(
                        tick,
                        format!("craft {}", r.id),
                        format!("the needs planner's next step toward {}", goal.id),
                    );
                    Task::Craft(Craft::new(r, tick))
                }
                Some(make::Act::Gather) | None => self.choose(w, origin, tick),
            },
        }
    }

    fn after_dig(&mut self, c: Controls, status: DigStatus, tick: u64) -> Controls {
        match status {
            DigStatus::Running => c,
            DigStatus::Done => {
                self.digs += 1;
                self.failed_in_row = 0;
                if let Task::Dig(d) = std::mem::replace(&mut self.task, Task::Idle) {
                    self.note(
                        tick,
                        format!("dug {:?}", d.target),
                        "the block broke and its drop is held".to_owned(),
                    );
                }
                Controls::default()
            }
            DigStatus::Failed(f) => {
                *self.failures.entry(format!("{f:?}")).or_default() += 1;
                self.retry_at = tick + RETRY_TICKS;
                self.failed_in_row += 1;
                if let Task::Dig(d) = std::mem::replace(&mut self.task, Task::Idle) {
                    self.refuse(d.target, tick);
                    self.note(
                        tick,
                        format!("dig {:?} failed: {f:?}", d.target),
                        format!(
                            "drop last seen at {:?}, feet at {:?}; refused for a while; choose again",
                            d.drop_seen, d.feet
                        ),
                    );
                }
                if self.failed_in_row >= MAX_UNWALKABLE {
                    self.stuck = true;
                    self.note(
                        tick,
                        "stuck".to_owned(),
                        format!("{MAX_UNWALKABLE} tasks failed in a row with none done"),
                    );
                }
                Controls::default()
            }
        }
    }

    fn after_craft(&mut self, status: CraftStatus, tick: u64) -> Controls {
        match status {
            CraftStatus::Running => {}
            CraftStatus::Done => {
                self.crafts += 1;
                self.failed_in_row = 0;
                if let Task::Craft(c) = std::mem::replace(&mut self.task, Task::Idle) {
                    self.note(
                        tick,
                        format!("crafted {}", c.recipe),
                        format!("in {} clicks", c.clicks),
                    );
                }
            }
            CraftStatus::Failed(f) => {
                *self.failures.entry(format!("{f:?}")).or_default() += 1;
                self.retry_at = tick + RETRY_TICKS;
                self.failed_in_row += 1;
                if let Task::Craft(c) = std::mem::replace(&mut self.task, Task::Idle) {
                    self.note(
                        tick,
                        format!("craft {} failed: {f:?}", c.recipe),
                        "settle the window and choose again".to_owned(),
                    );
                }
                if self.failed_in_row >= MAX_UNWALKABLE {
                    self.stuck = true;
                    self.note(
                        tick,
                        "stuck".to_owned(),
                        format!("{MAX_UNWALKABLE} tasks failed in a row with none done"),
                    );
                }
            }
        }
        Controls::default()
    }

    /// A ring query on memory's column index around the bot, nearest first: no scan of the
    /// world, no X-ray (only sightings, made from an exposed face, count).
    fn nearest_log(&self, w: &World, origin: [f64; 3], min: f64, tick: u64) -> Option<Cell> {
        let body = w.body?;
        let dim = w.dimension.as_ref()?.name.clone();
        let logs = block_tag("logs")?;
        let (cx, cz) = (
            (body.pos[0].floor() as i32) >> 4,
            (body.pos[2].floor() as i32) >> 4,
        );
        let flat = |a: [f64; 3], (x, _, z): Cell| {
            ((a[0] - (f64::from(x) + 0.5)).powi(2) + (a[2] - (f64::from(z) + 0.5)).powi(2)).sqrt()
        };
        let mut best: Option<(f64, Cell)> = None;
        for r in 0..=SEARCH_COLUMNS {
            // Every column of ring r; once a log is held nearer than the ring's inner edge, stop.
            if let Some((d, _)) = best
                && d < f64::from((r - 1).max(0) * 16)
            {
                break;
            }
            for dx in -r..=r {
                for dz in -r..=r {
                    if dx.abs().max(dz.abs()) != r {
                        continue;
                    }
                    for s in w.memory.in_column(&dim, cx + dx, cz + dz) {
                        let is_log =
                            data::block_of_state(s.state).is_some_and(|k| logs.contains(&k));
                        if !is_log || self.refused_now(s.pos, tick) || flat(origin, s.pos) < min {
                            continue;
                        }
                        let d = flat(body.pos, s.pos);
                        if best.is_none_or(|(bd, bc)| d < bd || (d == bd && s.pos < bc)) {
                            best = Some((d, s.pos));
                        }
                    }
                }
            }
        }
        best.map(|(_, c)| c)
    }
}

/// What the player holds, by item: window 0's slots from 1 (the grid, armour, inventory,
/// hotbar, offhand) and the cursor; the result slot is not held until taken.
pub fn held(w: &World) -> make::Counts {
    let mut c = make::Counts::new();
    for (slot, (k, n)) in &w.inventory.slots {
        if *slot >= 1 {
            *c.entry(*k).or_default() += *n;
        }
    }
    if let Some((k, n)) = w.inventory.carried {
        *c.entry(k).or_default() += n;
    }
    c
}

/// Is a log in view (a sighting, so seen with an exposed face) at least `min` blocks from
/// `origin` on the plane, reachable or not: whether a landing qualifies for the "reach a log"
/// measure, decided from what the bot could see, never from the world's hidden blocks.
pub fn log_in_view(w: &World, origin: [f64; 3], min: f64) -> bool {
    let (Some(dim), Some(logs)) = (w.dimension.as_ref(), block_tag("logs")) else {
        return false;
    };
    w.memory.log().iter().any(|s| {
        s.dimension == dim.name
            && w.memory
                .latest(&dim.name, s.pos)
                .is_some_and(|l| data::block_of_state(l.state).is_some_and(|k| logs.contains(&k)))
            && ((origin[0] - (f64::from(s.pos.0) + 0.5)).powi(2)
                + (origin[2] - (f64::from(s.pos.2) + 0.5)).powi(2))
            .sqrt()
                >= min
    })
}
