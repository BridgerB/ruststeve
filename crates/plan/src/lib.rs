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
pub mod hold;
pub mod make;
pub mod path;
pub mod place;
pub mod stairs;
pub mod walk;

use std::collections::BTreeMap;
use std::sync::Arc;

use data::generated::tags::block_tag;
use physics::Controls;
use world::World;

use crate::craft::{Craft, CraftStatus};
use crate::dig::{Dig, DigStatus};
use crate::path::{Cell, Goal};
use crate::place::{Place, PlaceStatus};
use crate::stairs::{Stairs, StairsStatus};
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
/// How far a remembered table counts as near (clojurecraft's `table-search`).
pub const TABLE_SEARCH: f64 = 24.0;
/// Events kept for `why` and the case file.
const EVENTS_KEPT: usize = 64;

/// What the planner is for: issue 09's "reach a log at least `min_distance` blocks from where
/// the planner began", or since issue 10 a goal row of the table put in play by its name
/// (`table`: the `kit` row, four sticks and a crafting table).
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum Aim {
    ReachLog {
        min_distance: f64,
    },
    Table,
    /// Issue 11's rows: a wooden pickaxe, three cobblestone, a stone pickaxe, and a stone
    /// pickaxe then a sword.
    Pickaxe,
    Cobblestone,
    StonePickaxe,
    StoneTools,
}

impl Aim {
    /// `log` or `log:<min>`.
    pub fn parse(s: &str) -> Option<Aim> {
        match s.split_once(':') {
            None if s == "log" => Some(Aim::ReachLog { min_distance: 0.0 }),
            None if s == "table" => Some(Aim::Table),
            None if s == "pickaxe" => Some(Aim::Pickaxe),
            None if s == "cobblestone" => Some(Aim::Cobblestone),
            None if s == "stone-pickaxe" => Some(Aim::StonePickaxe),
            None if s == "stone-tools" => Some(Aim::StoneTools),
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
            Aim::Pickaxe => "pickaxe".to_owned(),
            Aim::Cobblestone => "cobblestone".to_owned(),
            Aim::StonePickaxe => "stone-pickaxe".to_owned(),
            Aim::StoneTools => "stone-tools".to_owned(),
        }
    }

    /// Whether the aim is a goal row of the table (decided by the needs planner).
    pub fn is_goal(&self) -> bool {
        match self {
            Aim::ReachLog { .. } => false,
            Aim::Table | Aim::Pickaxe | Aim::Cobblestone | Aim::StonePickaxe | Aim::StoneTools => {
                true
            }
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
    /// One run of a recipe: in window 0, or at a crafting table.
    Craft(Craft),
    /// Putting a held block down beside the feet (issue 11b).
    Place(Place),
    /// Cutting stairs down (issue 11b).
    Stairs(Box<Stairs>),
}

impl Task {
    pub fn name(&self) -> &'static str {
        match self {
            Task::Idle => "idle",
            Task::ToLog(_) => "walk-to-log",
            Task::Explore(_) => "explore",
            Task::Dig(_) => "dig",
            Task::Craft(_) => "craft",
            Task::Place(_) => "place",
            Task::Stairs(_) => "stairs",
        }
    }

    pub fn walk(&self) -> Option<&Walk> {
        match self {
            Task::Idle | Task::Dig(_) | Task::Craft(_) | Task::Place(_) | Task::Stairs(_) => None,
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
                Aim::Table
                | Aim::Pickaxe
                | Aim::Cobblestone
                | Aim::StonePickaxe
                | Aim::StoneTools => self.decide(w, origin, tick),
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
                let (c, act, status) = cr.step(w, tick);
                self.acts.extend(act);
                self.after_craft(status, tick);
                return c;
            }
            Task::Place(p) => {
                let (act, status) = p.step(w, tick);
                self.acts.extend(act);
                return self.after_place(status, tick);
            }
            Task::Stairs(st) => {
                let (c, act, status) = st.step(w, tick);
                self.acts.extend(act);
                return self.after_stairs(c, status, tick);
            }
        };
        match status {
            TaskStatus::Running => controls,
            TaskStatus::Done => {
                self.finish_walk();
                self.failed_in_row = 0;
                match std::mem::replace(&mut self.task, Task::Idle) {
                    Task::ToLog(walk) if self.aim.is_goal() => {
                        self.note(
                            tick,
                            format!("dig {:?}", walk.target),
                            "beside the block the needs planner gathers or mines".to_owned(),
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
                    Task::Idle
                    | Task::Dig(_)
                    | Task::Craft(_)
                    | Task::Place(_)
                    | Task::Stairs(_) => {}
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
                            format!("walk to {:?} failed: {f:?}", walk.target),
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
                    Task::Idle
                    | Task::Dig(_)
                    | Task::Craft(_)
                    | Task::Place(_)
                    | Task::Stairs(_) => {}
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
            Aim::Table | Aim::Pickaxe | Aim::Cobblestone | Aim::StonePickaxe | Aim::StoneTools => {
                0.0
            }
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
                Aim::Table
                | Aim::Pickaxe
                | Aim::Cobblestone
                | Aim::StonePickaxe
                | Aim::StoneTools => Walk::to_dig(log, tick),
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
        // Taken up front: the only block need so far is a crafting table.
        let table_near = self
            .near_block(w, data::BlockKind::CraftingTable, tick)
            .is_some();
        let near = move |b: data::BlockKind| b == data::BlockKind::CraftingTable && table_near;
        match make::make(&table, &held(w), &near, goal) {
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
                Some(make::Act::Craft(r)) if !make::fits_grid(r) => {
                    let Some(t) = self.near_block(w, data::BlockKind::CraftingTable, tick) else {
                        self.retry_at = tick + RETRY_TICKS;
                        self.note(
                            tick,
                            "wait".to_owned(),
                            format!("craft {} wants a table near and none is remembered", r.id),
                        );
                        return Task::Idle;
                    };
                    self.note(
                        tick,
                        format!("craft {} at the table {t:?}", r.id),
                        format!("the needs planner's next step toward {}", goal.id),
                    );
                    Task::Craft(Craft::at_table(r, t, tick))
                }
                Some(make::Act::Place(b)) => match Place::new(b, tick) {
                    Some(p) => {
                        self.note(
                            tick,
                            format!("place {b:?}"),
                            format!("{} needs one near", goal.id),
                        );
                        Task::Place(p)
                    }
                    None => {
                        self.stuck = true;
                        self.note(
                            tick,
                            "stuck".to_owned(),
                            format!("{b:?} has no item to place"),
                        );
                        Task::Idle
                    }
                },
                Some(make::Act::Mine(blocks)) => {
                    if let Some(t) = self.carry_table(w, tick) {
                        return t;
                    }
                    self.mine(w, blocks, tick)
                }
                Some(make::Act::Gather) | None if self.carry_table(w, tick).is_some() => {
                    self.carry_table(w, tick).unwrap_or(Task::Idle)
                }
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

    /// clojurecraft's `take-table`: before going out for something, a remembered table in reach
    /// with none held is dug and taken along (a table left behind costs a log).
    fn carry_table(&mut self, w: &World, tick: u64) -> Option<Task> {
        if held(w)
            .get(&data::ItemKind::CraftingTable)
            .copied()
            .unwrap_or(0)
            > 0
        {
            return None;
        }
        let t = self.near_block(w, data::BlockKind::CraftingTable, tick)?;
        let body = w.body?;
        let eye = [body.pos[0], body.pos[1] + physics::EYE_HEIGHT, body.pos[2]];
        let c = [
            f64::from(t.0) + 0.5,
            f64::from(t.1) + 0.5,
            f64::from(t.2) + 0.5,
        ];
        let d = (0..3).map(|i| (eye[i] - c[i]).powi(2)).sum::<f64>().sqrt();
        if d > walk::REACH {
            return None;
        }
        self.note(
            tick,
            format!("dig the table {t:?}"),
            "going out for something: take the table along".to_owned(),
        );
        Some(Task::Dig(Dig::new(w, t, tick)))
    }

    /// The mine act: the nearest exposed block of the kinds the bot can stand beside, routed
    /// first (at most `CANDIDATES_PER_TICK` + 1 tried); with none, stairs down on the held heading.
    fn mine(&mut self, w: &World, blocks: &[data::BlockKind], tick: u64) -> Task {
        let candidates: Vec<Cell> = exposed_near(w, blocks)
            .into_iter()
            .filter(|c| !self.refused_now(*c, tick))
            .take(3)
            .collect();
        for c in candidates {
            let mut walk = Walk::to_dig(c, tick);
            if walk.route_now(w, tick) {
                self.walks += 1;
                self.note(
                    tick,
                    format!("walk to {:?} at {c:?}", blocks.first()),
                    "the nearest exposed block to mine, with a route".to_owned(),
                );
                return Task::ToLog(walk);
            }
            self.expanded += walk.expanded;
            self.refuse(c, tick);
        }
        let dir = match (self.heading as i32).rem_euclid(360) {
            90 => (-1, 0),
            180 => (0, -1),
            270 => (1, 0),
            _ => (0, 1),
        };
        self.note(
            tick,
            "stairs down".to_owned(),
            format!("no exposed {:?} in sight with a route", blocks.first()),
        );
        Task::Stairs(Box::new(Stairs::new(dir, tick)))
    }

    fn after_place(&mut self, status: PlaceStatus, tick: u64) -> Controls {
        match status {
            PlaceStatus::Running => {}
            PlaceStatus::Done(c) => {
                self.failed_in_row = 0;
                self.task = Task::Idle;
                self.note(
                    tick,
                    format!("placed at {c:?}"),
                    "the block update shows it".to_owned(),
                );
            }
            PlaceStatus::Failed(f) => {
                *self.failures.entry(format!("{f:?}")).or_default() += 1;
                self.retry_at = tick + RETRY_TICKS;
                self.failed_in_row += 1;
                self.task = Task::Idle;
                self.note(
                    tick,
                    format!("place failed: {f:?}"),
                    "choose again".to_owned(),
                );
                self.check_stuck(tick);
            }
        }
        Controls::default()
    }

    fn after_stairs(&mut self, c: Controls, status: StairsStatus, tick: u64) -> Controls {
        match status {
            StairsStatus::Running => c,
            StairsStatus::Done => {
                self.failed_in_row = 0;
                if let Task::Stairs(st) = std::mem::replace(&mut self.task, Task::Idle) {
                    self.note(
                        tick,
                        format!("cut {} stairs", st.stairs),
                        "look again for what to mine".to_owned(),
                    );
                }
                Controls::default()
            }
            StairsStatus::Failed(f) => {
                *self.failures.entry(format!("{f:?}")).or_default() += 1;
                self.retry_at = tick + RETRY_TICKS;
                self.failed_in_row += 1;
                self.heading = (self.heading + 90.0) % 360.0;
                if let Task::Stairs(st) = std::mem::replace(&mut self.task, Task::Idle) {
                    let last = st.refusals.last().cloned().unwrap_or_default();
                    self.note(
                        tick,
                        format!("stairs failed: {f:?} after {} stairs", st.stairs),
                        format!("last refusal: {last}; turn the heading"),
                    );
                }
                self.check_stuck(tick);
                Controls::default()
            }
        }
    }

    fn check_stuck(&mut self, tick: u64) {
        if self.failed_in_row >= MAX_UNWALKABLE {
            self.stuck = true;
            self.note(
                tick,
                "stuck".to_owned(),
                format!("{MAX_UNWALKABLE} tasks failed in a row with none done"),
            );
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
                    // A table that will not open (no way to it, another bot's up a cliff) is refused
                    // like any failed target, so the next choice places one instead (issue 11b).
                    if let (craft::CraftFailure::Unopened, Some(t)) = (f, c.table) {
                        self.refuse(t, tick);
                    }
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

    /// A remembered block of a kind within `TABLE_SEARCH` of the eye (clojurecraft's `make/near`),
    /// nearest first; one refused (a walk to it failed) does not count, so an unreachable table is
    /// replaced instead of walked at.
    pub fn near_block(&self, w: &World, kind: data::BlockKind, tick: u64) -> Option<Cell> {
        let body = w.body?;
        let dim = w.dimension.as_ref()?.name.clone();
        let eye = [body.pos[0], body.pos[1] + physics::EYE_HEIGHT, body.pos[2]];
        let (cx, cz) = (
            (body.pos[0].floor() as i32) >> 4,
            (body.pos[2].floor() as i32) >> 4,
        );
        let r = (TABLE_SEARCH as i32 >> 4) + 1;
        let mut best: Option<(f64, Cell)> = None;
        for dx in -r..=r {
            for dz in -r..=r {
                for s in w.memory.in_column(&dim, cx + dx, cz + dz) {
                    // Only the block's own failures count: an area refused around an unreachable
                    // log must not hide the table the bot just placed (issue 11b).
                    let refused = self
                        .refused
                        .get(&s.pos)
                        .is_some_and(|(until, _)| tick < *until);
                    if data::block_of_state(s.state) != Some(kind) || refused {
                        continue;
                    }
                    let (x, y, z) = s.pos;
                    let c = [f64::from(x) + 0.5, f64::from(y) + 0.5, f64::from(z) + 0.5];
                    let d = (0..3).map(|i| (eye[i] - c[i]).powi(2)).sum::<f64>().sqrt();
                    if d <= TABLE_SEARCH
                        && best.is_none_or(|(bd, bc)| d < bd || (d == bd && s.pos < bc))
                    {
                        best = Some((d, s.pos));
                    }
                }
            }
        }
        best.map(|(_, c)| c)
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
                        if !is_log
                            || self.refused_now(s.pos, tick)
                            || flat(origin, s.pos) < min
                            || crowded(w, s.pos)
                        {
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

/// Another player stands within this of a log's column (horizontally, and within 4 blocks of
/// height): a trunk someone is working. Its drops go to whoever touches them first.
pub const CROWD_RADIUS: f64 = 2.0;

/// Whether another player is at a log (docs/problems/wood.md): fourteen bots joining at one
/// spawn chose the same trunk and lost their drops to each other.
pub fn crowded(w: &World, (x, y, z): Cell) -> bool {
    let c = [f64::from(x) + 0.5, f64::from(z) + 0.5];
    w.entities.values().any(|e| {
        e.kind == data::EntityKind::Player
            && ((e.pos[0] - c[0]).powi(2) + (e.pos[2] - c[1]).powi(2)).sqrt() <= CROWD_RADIUS
            && (e.pos[1] - f64::from(y)).abs() <= 4.0
    })
}

/// How far around the feet exposed blocks are looked for (clojurecraft's `search-radius` and
/// `search-height`).
pub const SEARCH_RADIUS: i32 = 24;
pub const SEARCH_HEIGHT: i32 = 12;
/// Exposed blocks found before the search stops (clojurecraft routes the nearest 3).
pub const EXPOSED_WANTED: usize = 8;

/// Blocks of the kinds within `SEARCH_RADIUS` (and `SEARCH_HEIGHT` up or down) that show a face
/// to the air, nearest first: read from the loaded terrain (stone is most of the world; only what
/// is exposed now is worth a route), never a block with no open face (no X-ray).
pub fn exposed_near(w: &World, kinds: &[data::BlockKind]) -> Vec<Cell> {
    let Some(body) = w.body else {
        return Vec::new();
    };
    let (fx, fy, fz) = path::feet_cell(body.pos);
    let at = |x: i32, y: i32, z: i32| w.block_at(world::BlockPos::new(x, y, z));
    let air = |k: Option<data::BlockKind>| {
        matches!(k, Some(data::BlockKind::Air | data::BlockKind::CaveAir))
    };
    let mut found: Vec<(i32, Cell)> = Vec::new();
    // Ring by ring outward (Chebyshev on the plane), stopping once the nearest few are in hand:
    // a whole-radius scan cost 0.4 s in one tick (issue 11b's sim measure).
    for r in 0..=SEARCH_RADIUS {
        for dx in -r..=r {
            for dz in -r..=r {
                if dx.abs().max(dz.abs()) != r {
                    continue;
                }
                for dy in -SEARCH_HEIGHT..=SEARCH_HEIGHT {
                    let (x, y, z) = (fx + dx, fy + dy, fz + dz);
                    if !at(x, y, z).is_some_and(|k| kinds.contains(&k)) {
                        continue;
                    }
                    let open = [
                        (1, 0, 0),
                        (-1, 0, 0),
                        (0, 1, 0),
                        (0, -1, 0),
                        (0, 0, 1),
                        (0, 0, -1),
                    ]
                    .iter()
                    .any(|(ox, oy, oz)| air(at(x + ox, y + oy, z + oz)));
                    if open {
                        found.push((dx * dx + dy * dy + dz * dz, (x, y, z)));
                    }
                }
            }
        }
        if found.len() >= EXPOSED_WANTED {
            break;
        }
    }
    found.sort();
    found.into_iter().map(|(_, c)| c).collect()
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
