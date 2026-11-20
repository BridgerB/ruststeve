//! The planner (brain/ruststeve/design/planner.md): what the bot does next, as a pure function of
//! the world and its own held task. Issue 09 begins it with one goal row, "reach a log", the
//! pathfinder (`path`, a port of clojurecraft's `path.clj`) and the walk (`walk`) with its progress
//! invariant. Every task it starts or ends is an event with its reason. Issue 10 adds the goal
//! table and the needs planner (`make`), and the tasks that change the world: `dig` and
//! `craft`; they act through `Act`s the shell takes with `take_acts`.

#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

pub mod build;
pub mod cast;
pub mod craft;
pub mod defend;
pub mod dig;
pub mod eat;
pub mod escape;
pub mod hold;
pub mod hunt;
pub mod make;
pub mod path;
pub mod place;
pub mod redig;
pub mod scoop;
pub mod smelt;
pub mod stairs;
pub mod station;
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
/// Times a cast out of water keeps its site to fetch water (wall 43): enough for a cast's spare
/// water lost twice, never a site worked forever.
pub const WATER_TRIPS: u32 = 2;
/// Chunk columns round the feet a portal is looked for in (issue 15c).
const PORTAL_CHUNKS: i32 = 8;
/// Ticks stood in a portal before the walk into it counts as failed: a survival player crosses
/// after 80 (`Player.getPortalWaitTime`), and the dimension's load takes a while more.
const PORTAL_WAIT_TICKS: u64 = 400;

/// Walking into a portal cell, then standing in it (issue 15c).
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Enter {
    walk: Walk,
    cell: Cell,
    /// The tick the feet were first in a portal block.
    inside_since: Option<u64>,
}

impl Enter {
    /// Done once the world is in another dimension than the portal's; while the feet are in a
    /// portal block, stand still (a step out resets the game's count); else walk on.
    fn step(&mut self, w: &World, tick: u64) -> (Controls, TaskStatus) {
        if w.dimension
            .as_ref()
            .is_some_and(|d| d.name != "minecraft:overworld")
        {
            return (Controls::default(), TaskStatus::Done);
        }
        // Between dimensions the world holds no body: wait for it.
        let Some(body) = w.body else {
            return (Controls::default(), TaskStatus::Running);
        };
        let f = path::feet_cell(body.pos);
        let in_portal = w
            .block_at(world::BlockPos::new(f.0, f.1, f.2))
            .is_some_and(|k| k == data::BlockKind::NetherPortal);
        if in_portal {
            let since = *self.inside_since.get_or_insert(tick);
            if tick.saturating_sub(since) > PORTAL_WAIT_TICKS {
                return (
                    Controls::default(),
                    TaskStatus::Failed(walk::Failure::Deadline),
                );
            }
            return (Controls::default(), TaskStatus::Running);
        }
        let (c, status) = self.walk.step(w, tick);
        match status {
            // At the cell but not in a portal block (the frame went out): the portal is gone.
            TaskStatus::Done => (c, TaskStatus::Failed(walk::Failure::TargetLost)),
            TaskStatus::Running | TaskStatus::Failed(_) => (c, status),
        }
    }
}
/// Chunk columns round the feet a fortress walk looks for bricks in (16: a view's 256 blocks).
const FORTRESS_CHUNKS: i32 = 16;
/// Times a cast out of lava keeps its site to fetch lava from further off (wall 44).
pub const LAVA_TRIPS: u32 = 2;
/// Lava sources a fetch asks a scoop route of, nearest first.
const LAVA_FETCH_TRIES: usize = 8;
/// Walks toward lava a fetch takes for one site when no scoop routes (wall 47).
const LAVA_FETCH_WALKS: u32 = 4;
/// Failed walks in a row, with nothing done between, after which the planner stops: stuck,
/// said so, and the run ends there (clojurecraft's `max-unwalkable`, 12).
pub const MAX_UNWALKABLE: u32 = 12;
/// A goal aim that fails `MAX_UNWALKABLE` tasks in a row moves elsewhere (refusals cleared, an
/// explore walk on a turned heading) this many times before it stops (issue 13b: 13 of 56 bots
/// stopped for good on a ledge, a lake or out of reach of the trees, and idled out the cap).
pub const MAX_RELOCATIONS: u32 = 6;
/// After its relocations, a goal aim rests this long and starts again.
pub const REST_TICKS: u64 = 600;
/// A task with no progress this long (the body not a block from where it was, the items held
/// unchanged, no planner event) is cut: one hung a bot for 33,000 ticks after a stair's digs
/// (docs/problems/iron-kit.md wall 14).
pub const STALL_TICKS: u64 = 1200;
/// The trail: a mark this often, when the body has moved `TRAIL_STEP` since the last; this many
/// kept; a relocation walks back to one at least `TRAIL_BACK` blocks away.
pub const TRAIL_EVERY: u64 = 200;
pub const TRAIL_STEP: f64 = 2.0;
pub const TRAIL_KEPT: usize = 60;
pub const TRAIL_BACK: f64 = 10.0;
/// The feet height the lava hunt goes down to: a block over the lava sea (clojurecraft's
/// `lava-depth`; below y -54 the game fills its aquifers with lava).
pub const LAVA_DEPTH: i32 = -50;
/// Explore targets reached that are kept, and how near a new one may aim to one of them.
pub const EXPLORED_KEPT: usize = 24;
pub const EXPLORED_NEAR: f64 = 12.0;
/// The most a swim off an island takes.
pub const SWIM_TICKS: u64 = 1200;
/// A target that failed `NoPath` refuses every target within this of it: one unreachable tree
/// is not tried log by log (clojurecraft's `area-radius`).
pub const AREA_RADIUS: f64 = 6.0;
/// Targets the planner routes to in one tick before it waits: a choice is checked for a way
/// before it is made, and the check is bounded (brain/ruststeve/design/progress-invariants.md).
pub const CANDIDATES_PER_TICK: usize = 2;
/// How far a remembered table or furnace counts as near: 64, where clojurecraft's `table-search`
/// is 24. A miner 30 blocks from its table and with no wood left planned a new one from logs it
/// could not reach; one too far to walk to fails `Unopened` and is refused, and a new one is
/// placed (docs/problems/iron-kit.md wall 22).
pub const TABLE_SEARCH: f64 = 64.0;
/// Events kept for `why` and the case file.
const EVENTS_KEPT: usize = 64;

/// A cast's account when it ends, for its note: the ops done, where it stood (the op, its
/// regressions, the buckets held, the refill's last steps) and its bucket uses.
struct CastAccount {
    ops: u32,
    at: String,
    uses: String,
    /// Water and lava buckets held when the cast ended.
    water: i32,
    lava: i32,
}

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
    /// Issue 13's rows: a furnace placed, four ingots, an iron pickaxe, a bucket, flint and
    /// steel, and the kit (all three).
    Furnace,
    Ingots,
    IronPickaxe,
    Bucket,
    FlintAndSteel,
    IronKit,
    /// Issue 14b's rows: a water bucket filled, and what a portal cast spends (a water bucket
    /// and a bucket).
    WaterBucket,
    PortalKit,
    /// Issue 14c's: a portal cast and lit.
    Portal,
    /// Issue 15's: a fortress reached, nether bricks seen with an exposed face (strict no X-ray).
    Fortress,
    /// Issue 15c's: in the Nether through a portal: the nearest seen walked into, else one cast.
    Nether,
    /// Every stage built so far, in order (the marathon's aim): a portal cast and entered, then a
    /// fortress reached. Each issue that adds a stage extends it.
    Race,
    /// No goal: only the always-rows run (issue 12b's escape rows, 12e's hold after the aim).
    Hold,
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
            None if s == "furnace" => Some(Aim::Furnace),
            None if s == "ingots" => Some(Aim::Ingots),
            None if s == "iron-pickaxe" => Some(Aim::IronPickaxe),
            None if s == "bucket" => Some(Aim::Bucket),
            None if s == "flint-and-steel" => Some(Aim::FlintAndSteel),
            None if s == "iron-kit" => Some(Aim::IronKit),
            None if s == "water-bucket" => Some(Aim::WaterBucket),
            None if s == "portal-kit" => Some(Aim::PortalKit),
            None if s == "portal" => Some(Aim::Portal),
            None if s == "fortress" => Some(Aim::Fortress),
            None if s == "nether" => Some(Aim::Nether),
            None if s == "race" => Some(Aim::Race),
            None if s == "hold" => Some(Aim::Hold),
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
            Aim::Furnace => "furnace".to_owned(),
            Aim::Ingots => "ingots".to_owned(),
            Aim::IronPickaxe => "iron-pickaxe".to_owned(),
            Aim::Bucket => "bucket".to_owned(),
            Aim::FlintAndSteel => "flint-and-steel".to_owned(),
            Aim::IronKit => "iron-kit".to_owned(),
            Aim::WaterBucket => "water-bucket".to_owned(),
            Aim::PortalKit => "portal-kit".to_owned(),
            Aim::Portal => "portal".to_owned(),
            Aim::Fortress => "fortress".to_owned(),
            Aim::Nether => "nether".to_owned(),
            Aim::Race => "race".to_owned(),
            Aim::Hold => "hold".to_owned(),
        }
    }

    /// Whether the aim is a goal row of the table (decided by the needs planner).
    pub fn is_goal(&self) -> bool {
        match self {
            Aim::ReachLog { .. } => false,
            Aim::Table
            | Aim::Pickaxe
            | Aim::Cobblestone
            | Aim::StonePickaxe
            | Aim::StoneTools
            | Aim::Furnace
            | Aim::Ingots
            | Aim::IronPickaxe
            | Aim::Bucket
            | Aim::FlintAndSteel
            | Aim::IronKit
            | Aim::WaterBucket
            | Aim::PortalKit
            | Aim::Portal
            | Aim::Fortress
            | Aim::Nether
            | Aim::Race
            | Aim::Hold => true,
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
    /// A batch in a furnace, or the results of one left loaded (issue 13b).
    Smelt(Box<smelt::Smelt>),
    /// Place a block and dig it again for its chance drop (flint, issue 13b).
    Redig(Box<redig::Redig>),
    /// A bucket filled at a surface water source (issue 14b).
    Scoop(Box<scoop::Scoop>),
    /// A portal cast at a site (issue 14c).
    Cast(Box<cast::Cast>),
    /// Walking into a lit portal and standing in it until the dimension changes (issue 15c).
    Enter(Box<Enter>),
    /// Out of water, lava or fire: an always-row in force (issue 12b).
    Escape(Box<escape::Escape>),
    /// Across water to a far shore: a relocation off an island (issue 13b), and its start.
    Swim(Box<escape::Escape>, u64),
    /// A hostile mob near: fight it or back away (issue 12d).
    Defend(Box<defend::Defend>),
    /// Eating something held (issue 12c).
    Eat(eat::Eat),
    /// Killing an animal for its meat (issue 12c).
    Hunt(Box<hunt::Hunt>),
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
            Task::Smelt(_) => "smelt",
            Task::Redig(_) => "redig",
            Task::Scoop(_) => "scoop",
            Task::Cast(_) => "cast",
            Task::Enter(_) => "enter-portal",
            Task::Escape(_) => "escape",
            Task::Swim(..) => "swim",
            Task::Defend(_) => "defend",
            Task::Eat(_) => "eat",
            Task::Hunt(_) => "hunt",
        }
    }

    pub fn walk(&self) -> Option<&Walk> {
        match self {
            Task::Idle
            | Task::Dig(_)
            | Task::Craft(_)
            | Task::Place(_)
            | Task::Stairs(_)
            | Task::Smelt(_)
            | Task::Redig(_)
            | Task::Scoop(_)
            | Task::Cast(_)
            | Task::Enter(_)
            | Task::Escape(_)
            | Task::Swim(..)
            | Task::Defend(_)
            | Task::Eat(_)
            | Task::Hunt(_) => None,
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
    /// Targets whose walks stopped getting closer (`NoProgress`), cleared on a relocation.
    pub stalled: Vec<Cell>,
    /// Where a `never_fight` mob was fled from, until when, and how far round it work waits.
    pub dangers: Vec<(Cell, u64, f64)>,
    /// The cast's site, kept while it is worked (issue 14c).
    #[serde(default)]
    pub site: Option<cast::Site>,
    /// Water fetched for the site in hand after a cast ran out of it (wall 43); at most
    /// `WATER_TRIPS`, then the site is given up as before.
    #[serde(default)]
    pub water_trips: u32,
    /// Lava fetched for the site in hand after its refill failed (wall 44), and whether a fetch
    /// is due before the cast resumes.
    #[serde(default)]
    pub lava_trips: u32,
    #[serde(default)]
    pub fetch_lava: bool,
    /// Walks toward lava taken for the site in hand (wall 47).
    #[serde(default)]
    pub fetch_walks: u32,
    /// Origins of sites given up: no new site within `cast::GIVE_UP_RADIUS`.
    #[serde(default)]
    pub given_up_sites: Vec<Cell>,
    /// Explore targets reached, the newest `EXPLORED_KEPT`: a new explore never aims within
    /// `EXPLORED_NEAR` of one (clojurecraft's hunt never walks back over its ground; two bots
    /// cycled six targets a whole race looking for water: docs/problems/portal.md wall 3).
    #[serde(default)]
    pub explored: Vec<Cell>,
    /// The lit portal's inside cell, once seen (issue 14c; issue 15 walks back to it).
    #[serde(default)]
    pub portal: Option<Cell>,
    /// Ranged foes a fight landed nothing on, and until when they are left alone (wall 30).
    pub ignored_foes: Vec<(i32, u64)>,
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
    /// The trap counters the always-rows read, and escapes started (issue 12b).
    #[serde(default)]
    pub traps: escape::Traps,
    #[serde(default)]
    pub escapes: u64,
    /// Meals eaten and hunts that ended in a kill (issue 12c); an eat or hunt row is not
    /// chosen again before `always_retry_at` after it failed, and animals a hunt failed on are
    /// refused until then too.
    #[serde(default)]
    pub meals: u64,
    #[serde(default)]
    pub kills: u64,
    #[serde(default)]
    pub always_retry_at: u64,
    /// A batch left cooking (issue 13b): the furnace, its result and how many. The planner goes
    /// back for it before anything else, and never mines for what is already in it.
    #[serde(default)]
    pub batch: Option<(Cell, data::ItemKind, i32)>,
    #[serde(default)]
    pub smelts: u64,
    /// Relocations made, and whether the next choice is one (issue 13b).
    #[serde(default)]
    pub relocations: u32,
    /// The block a dig-toward heads for, the nearest it has come, and cuts made without coming
    /// nearer (a progress invariant, issue 13b).
    #[serde(default)]
    pub toward: Option<(Cell, f64, u32)>,
    /// The watchdog's mark: where the body was, how many items were held, the events made, and
    /// when (issue 13b).
    #[serde(default)]
    pub mark: Option<([f64; 3], i64, u64, u64)>,
    #[serde(default)]
    pub stalls: u64,
    /// Where the body has stood, every `TRAIL_EVERY` ticks, newest last: the way back out of a
    /// dead end (issue 13b).
    #[serde(default)]
    pub trail: Vec<[f64; 3]>,
    #[serde(default)]
    pub relocate: bool,
    /// Walks failed in a row with water beside the feet: the next relocation swims (wall 24).
    pub maroon: bool,
    #[serde(default)]
    pub refused_prey: Vec<i32>,
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
            stalled: Vec::new(),
            dangers: Vec::new(),
            site: None,
            water_trips: 0,
            lava_trips: 0,
            fetch_lava: false,
            fetch_walks: 0,
            given_up_sites: Vec::new(),
            explored: Vec::new(),
            portal: None,
            ignored_foes: Vec::new(),
            event_count: 0,
            table: None,
            met: false,
            digs: 0,
            crafts: 0,
            traps: escape::Traps::default(),
            escapes: 0,
            meals: 0,
            kills: 0,
            always_retry_at: 0,
            batch: None,
            smelts: 0,
            relocations: 0,
            toward: None,
            mark: None,
            stalls: 0,
            trail: Vec::new(),
            relocate: false,
            maroon: false,
            refused_prey: Vec::new(),
            acts: Vec::new(),
        }
    }

    /// A planner for a goal aim, with the goal table.
    pub fn with_table(aim: Aim, table: Arc<make::Table>) -> Self {
        let mut p = Self::new(aim);
        p.table = Some(table);
        p
    }

    /// After a death and a respawn (the marathon's revive): drop the task in hand, whose targets
    /// were where the body was, and say so; the aim and what memory holds carry on.
    pub fn revived(&mut self, tick: u64, killer: &str, deaths: u32) {
        self.task = Task::Idle;
        self.retry_at = tick;
        self.failed_in_row = 0;
        self.note(
            tick,
            format!("died ({killer}) and respawned"),
            format!("death {deaths}; items kept, the aim carries on"),
        );
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
        // The trail, the way back out of a dead end.
        if tick.is_multiple_of(TRAIL_EVERY) {
            let far = self.trail.last().is_none_or(|t| {
                (0..3)
                    .map(|i| (t[i] - body.pos[i]).powi(2))
                    .sum::<f64>()
                    .sqrt()
                    >= TRAIL_STEP
            });
            if far {
                self.trail.push(body.pos);
                if self.trail.len() > TRAIL_KEPT {
                    self.trail.remove(0);
                }
            }
        }
        // The watchdog: a task with no progress for STALL_TICKS is cut.
        let items: i64 = w.inventory.slots.values().map(|(_, n)| i64::from(*n)).sum();
        let moved = |m: [f64; 3]| (0..3).map(|i| (m[i] - body.pos[i]).powi(2)).sum::<f64>() > 1.0;
        match self.mark {
            Some((at, n, ev, since)) if !moved(at) && n == items && ev == self.event_count => {
                if !matches!(self.task, Task::Idle) && tick.saturating_sub(since) > STALL_TICKS {
                    // A cast cut here gives its site up, or the next choice resumes it stuck
                    // (wall 6).
                    if let Task::Cast(k) = &self.task {
                        self.given_up_sites.push(k.site.frame.origin);
                        self.site = None;
                    }
                    let cut = self.task.name();
                    self.stalls += 1;
                    self.task = Task::Idle;
                    self.retry_at = tick;
                    self.failed_in_row += 1;
                    *self.failures.entry("Stalled".to_owned()).or_default() += 1;
                    self.note(
                        tick,
                        format!("{cut} stalled"),
                        format!("no progress for {STALL_TICKS} ticks; choose again"),
                    );
                    self.mark = Some((body.pos, items, self.event_count, tick));
                }
            }
            Some(_) | None => self.mark = Some((body.pos, items, self.event_count, tick)),
        }
        // The always-rows come first, ahead of every goal and after the aim is met.
        if let Some(c) = self.always(w, &body, tick) {
            return c;
        }
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
                | Aim::Furnace
                | Aim::Ingots
                | Aim::IronPickaxe
                | Aim::Bucket
                | Aim::FlintAndSteel
                | Aim::IronKit
                | Aim::WaterBucket
                | Aim::PortalKit
                | Aim::Portal
                | Aim::StoneTools => self.decide(w, origin, tick),
                Aim::Fortress => self.fortress_task(w, tick),
                Aim::Nether | Aim::Race => self.nether_task(w, origin, tick),
                Aim::Hold => Task::Idle,
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
            Task::Smelt(sm) => {
                let (c, act, status) = sm.step(w, tick);
                self.acts.extend(act);
                return self.after_smelt(c, status, tick);
            }
            Task::Redig(r) => {
                let (c, act, status) = r.step(w, tick);
                self.acts.extend(act);
                return self.after_redig(c, status, tick);
            }
            Task::Scoop(s) => {
                let (c, act, status) = s.step(w, tick);
                self.acts.extend(act);
                let target = s.target;
                return self.after_scoop(c, status, target, tick);
            }
            Task::Enter(e) => {
                let (c, status) = e.step(w, tick);
                let cell = e.cell;
                return self.after_enter(c, status, cell, tick);
            }
            Task::Cast(k) => {
                let (c, act, status) = k.step(w, tick);
                self.acts.extend(act);
                let site = k.site;
                let ops = k.done_ops;
                let uses = k.uses_text();
                // The buckets held when the cast ended: a cast that lost its water says so here.
                let held = |i| craft::count(w, i);
                let at = format!(
                    "{}; {}; buckets water {} lava {} empty {}; {uses}; {}",
                    k.current().map(build::op_text).unwrap_or_default(),
                    k.regression_text(),
                    held(data::ItemKind::WaterBucket),
                    held(data::ItemKind::LavaBucket),
                    held(data::ItemKind::Bucket),
                    k.refill_text()
                );
                let water = held(data::ItemKind::WaterBucket);
                let lava = held(data::ItemKind::LavaBucket);
                let account = CastAccount {
                    ops,
                    at,
                    uses,
                    water,
                    lava,
                };
                return self.after_cast(c, status, site, account, tick);
            }
            Task::Swim(e, since) => {
                let (c, act) = e.step(w, tick);
                self.acts.extend(act);
                let target = e.target;
                let since = *since;
                // Done on dry land at the far shore; over after SWIM_TICKS whatever.
                let landed = target.zip(w.body).is_some_and(|(t, b)| {
                    escape::dry_land(w, &b)
                        && (f64::from(t.0) + 0.5 - b.pos[0]).abs() < 2.0
                        && (f64::from(t.2) + 0.5 - b.pos[2]).abs() < 2.0
                });
                if landed || tick.saturating_sub(since) > SWIM_TICKS {
                    self.task = Task::Idle;
                    self.retry_at = tick;
                    self.note(
                        tick,
                        if landed { "swum across" } else { "swim over" }.to_owned(),
                        "plan again from here".to_owned(),
                    );
                    return Controls::default();
                }
                return c;
            }
            // `always` runs these (and ends them).
            Task::Escape(_) | Task::Defend(_) | Task::Eat(_) | Task::Hunt(_) => {
                return Controls::default();
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
                        self.explored.push(walk.target);
                        if self.explored.len() > EXPLORED_KEPT {
                            self.explored.remove(0);
                        }
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
                    | Task::Stairs(_)
                    | Task::Smelt(_)
                    | Task::Redig(_)
                    | Task::Scoop(_)
                    | Task::Cast(_)
                    | Task::Enter(_)
                    | Task::Escape(_)
                    | Task::Swim(..)
                    | Task::Defend(_)
                    | Task::Eat(_)
                    | Task::Hunt(_) => {}
                }
                Controls::default()
            }
            TaskStatus::Failed(f) => {
                self.finish_walk();
                *self.failures.entry(format!("{f:?}")).or_default() += 1;
                self.retry_at = tick + RETRY_TICKS;
                self.failed_in_row += 1;
                let give_up = self.failed_in_row >= MAX_UNWALKABLE;
                match std::mem::replace(&mut self.task, Task::Idle) {
                    Task::ToLog(walk) => {
                        self.refuse(walk.target, tick);
                        match f {
                            walk::Failure::NoPath => self
                                .refused_areas
                                .push((walk.target, tick + FAILURE_MEMORY_TICKS)),
                            // A way found and not followed is found again: the area, escalating
                            // (a coal vein tried cell by cell held a bot 8,000 ticks: wall 25).
                            walk::Failure::NoProgress => self.stalled_area(walk.target, tick),
                            _ => {}
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
                    | Task::Stairs(_)
                    | Task::Smelt(_)
                    | Task::Redig(_)
                    | Task::Scoop(_)
                    | Task::Cast(_)
                    | Task::Enter(_)
                    | Task::Escape(_)
                    | Task::Swim(..)
                    | Task::Defend(_)
                    | Task::Eat(_)
                    | Task::Hunt(_) => {}
                }
                if give_up {
                    self.maroon = w.body.is_some_and(|b| wet(w, &b));
                    self.give_up(
                        tick,
                        format!("{MAX_UNWALKABLE} walks failed in a row with none done"),
                    );
                }
                Controls::default()
            }
        }
    }

    /// The always-rows (issues 12b, 12c), in the table's order: an escape answers a trap of its
    /// kind and holds until it clears; `eat` answers hunger with food held, `hunt` hunger with
    /// none and an animal a route reaches, each running to its own end. A row in force cuts the
    /// task in hand (nothing of it is kept: the needs planner re-derives what to do from what is
    /// held); an escape cuts an eat or a hunt too.
    fn always(&mut self, w: &World, body: &physics::Body, tick: u64) -> Option<Controls> {
        let table = self.table.clone()?;
        if table.always.is_empty() {
            return None;
        }
        let trap = self.traps.tick(w, body);
        let rules = &table.survival;
        let ignored: Vec<i32> = self
            .ignored_foes
            .iter()
            .filter(|(_, until)| tick < *until)
            .map(|(id, _)| *id)
            .collect();
        // A swim off an island is in water on purpose: only another trap (lava, fire, a wall)
        // cuts it, and a foe or hunger waits for the far shore (wall 24).
        if matches!(self.task, Task::Swim(..))
            && matches!(trap, None | Some(escape::TrapKind::Water))
        {
            return None;
        }
        // What is running: an escape whose trap holds, an eat or a hunt, else nothing.
        let running = match &self.task {
            Task::Escape(e) => Some(make::Always::Escape(e.kind)),
            Task::Defend(_) => Some(make::Always::Defend),
            Task::Eat(_) => Some(make::Always::Eat),
            Task::Hunt(_) => Some(make::Always::Hunt),
            Task::Idle
            | Task::ToLog(_)
            | Task::Explore(_)
            | Task::Dig(_)
            | Task::Craft(_)
            | Task::Place(_)
            | Task::Stairs(_)
            | Task::Smelt(_)
            | Task::Redig(_)
            | Task::Scoop(_)
            | Task::Cast(_)
            | Task::Enter(_)
            | Task::Swim(..) => None,
        };
        // The first row in force, in order; a running eat or hunt stays in force until it ends.
        let mut chosen: Option<(String, make::Always)> = None;
        for (id, row) in &table.always {
            let in_force = match row {
                make::Always::Escape(k) => trap == Some(*k),
                make::Always::Defend => {
                    running == Some(make::Always::Defend)
                        || defend::foe(w, rules, &ignored).is_some()
                }
                make::Always::Eat | make::Always::Hunt if running == Some(*row) => true,
                // A hunt picks up all its drops before the meal: only an escape cuts it.
                make::Always::Eat => {
                    running != Some(make::Always::Hunt)
                        && tick >= self.always_retry_at
                        && eat::hungry(w, rules)
                        && eat::best_food(w, rules).is_some()
                }
                make::Always::Hunt => {
                    tick >= self.always_retry_at
                        && ((eat::hungry(w, rules) && eat::best_food(w, rules).is_none())
                            || self.provisioning(w, rules))
                        && !hunt::prey(w, rules, self.hunt_radius(w, rules), &self.refused_prey)
                            .is_empty()
                }
            };
            if in_force {
                chosen = Some((id.clone(), *row));
                break;
            }
        }
        let Some((id, row)) = chosen else {
            if let Some(make::Always::Escape(k)) = running {
                self.task = Task::Idle;
                self.retry_at = tick;
                self.note(
                    tick,
                    format!("out of the {}", k.name()),
                    "dry land held; back to the goal".to_owned(),
                );
            }
            return None;
        };
        if running != Some(row) {
            // Starting a row: what it cuts.
            if running == Some(make::Always::Eat) {
                self.acts.push(world::Act::ReleaseUse);
            }
            self.finish_walk();
            self.keep_batch();
            let cut = self.task.name();
            let task = match row {
                make::Always::Escape(k) => {
                    self.escapes += 1;
                    self.note(tick, id, format!("trapped in {}; cut {cut}", k.name()));
                    Some(Task::Escape(Box::new(escape::Escape::new(k, w, tick))))
                }
                make::Always::Defend => defend::foe(w, rules, &ignored).map(|(foe, kind)| {
                    let d = defend::Defend::new(w, foe, kind, rules, tick);
                    self.note(
                        tick,
                        format!("defend {kind:?} {foe} ({:?})", d.stance),
                        format!(
                            "a hostile within {}; health {:?}; cut {cut}",
                            rules.defend_radius, w.player.health
                        ),
                    );
                    Task::Defend(Box::new(d))
                }),
                make::Always::Eat => eat::best_food(w, rules).map(|f| {
                    self.note(
                        tick,
                        format!("eat {}", f.name().unwrap_or_default()),
                        format!(
                            "food {:?} under {}; cut {cut}",
                            w.player.food, rules.hungry_below
                        ),
                    );
                    Task::Eat(eat::Eat::new(w, f, tick))
                }),
                make::Always::Hunt => self.choose_prey(w, rules, tick).map(|h| {
                    self.note(
                        tick,
                        format!("hunt {:?} {}", h.kind, h.prey),
                        format!(
                            "food {:?} and nothing edible held; cut {cut}",
                            w.player.food
                        ),
                    );
                    Task::Hunt(Box::new(h))
                }),
            };
            let Some(task) = task else {
                self.always_retry_at = tick + RETRY_TICKS;
                return None;
            };
            self.task = task;
        }
        let (c, act, ended) = match &mut self.task {
            Task::Escape(e) => {
                let (c, act) = e.step(w, tick);
                (c, act, None)
            }
            Task::Defend(d) => {
                let (c, act, status) = d.step(w, rules, tick);
                let ended = match status {
                    defend::DefendStatus::Running => None,
                    defend::DefendStatus::Done => {
                        // Fled a creeper: its ground waits, or the next walk goes straight back
                        // to it (wall 29).
                        // A ranged foe no strike reached: left alone a while, or it is chosen
                        // again at once (one bot fought one skeleton 51 times: wall 30).
                        if rules.ranged.contains(&d.kind) && d.last_hit_at == d.started {
                            self.ignored_foes.retain(|(_, until)| tick < *until);
                            self.ignored_foes.push((d.foe, tick + FAILURE_MEMORY_TICKS));
                        }
                        if rules.never_fight.contains(&d.kind) {
                            self.dangers.retain(|(_, until, _)| tick < *until);
                            self.dangers.push((
                                path::feet_cell(d.last_seen),
                                tick + FAILURE_MEMORY_TICKS,
                                rules.flee_clear,
                            ));
                        }
                        Some(Ok(format!(
                            "defended: {} swings, {:?}, {} steps held back",
                            d.swings, d.stance, d.held_back
                        )))
                    }
                };
                (c, act, ended)
            }
            Task::Eat(e) => {
                let (act, status) = e.step(w, tick);
                let ended = match status {
                    eat::EatStatus::Running => None,
                    eat::EatStatus::Done => Some(Ok("ate".to_owned())),
                    eat::EatStatus::Failed(f) => Some(Err(format!("eat failed: {f:?}"))),
                };
                (Controls::default(), act, ended)
            }
            Task::Hunt(h) => {
                let (c, act, status) = h.step(w, rules, tick);
                let ended = match status {
                    hunt::HuntStatus::Running => None,
                    hunt::HuntStatus::Done => Some(Ok(format!("hunt done: {} swings", h.swings))),
                    hunt::HuntStatus::Failed(f) => {
                        self.refused_prey.push(h.prey);
                        Some(Err(format!("hunt failed: {f:?}")))
                    }
                };
                (c, act, ended)
            }
            Task::Idle
            | Task::ToLog(_)
            | Task::Explore(_)
            | Task::Dig(_)
            | Task::Craft(_)
            | Task::Place(_)
            | Task::Stairs(_)
            | Task::Smelt(_)
            | Task::Redig(_)
            | Task::Scoop(_)
            | Task::Cast(_)
            | Task::Enter(_)
            | Task::Swim(..) => return None,
        };
        self.acts.extend(act);
        if let Some(end) = ended {
            match (&self.task, &end) {
                (Task::Eat(_), Ok(_)) => self.meals += 1,
                (Task::Hunt(_), Ok(_)) => self.kills += 1,
                _ => {}
            }
            match end {
                Ok(what) => self.note(tick, what, "back to the goal".to_owned()),
                Err(what) => {
                    *self.failures.entry(what.clone()).or_default() += 1;
                    self.always_retry_at = tick + RETRY_TICKS;
                    self.note(tick, what, "not again for a while".to_owned());
                }
            }
            self.task = Task::Idle;
            self.retry_at = tick;
        }
        Some(c)
    }

    /// Starving hunts farther (clojurecraft's `starving-radius`).
    fn hunt_radius(&self, w: &World, rules: &make::SurvivalRules) -> f64 {
        if w.player.food.is_some_and(|f| f <= rules.starving_below) {
            rules.starving_radius
        } else {
            rules.hunt_radius
        }
    }

    /// The nearest animal a route reaches, of the first `CANDIDATES` (the churn invariant: a
    /// hunt with no way is never started); the others tried are refused for a while.
    fn choose_prey(
        &mut self,
        w: &World,
        rules: &make::SurvivalRules,
        tick: u64,
    ) -> Option<hunt::Hunt> {
        let radius = self.hunt_radius(w, rules);
        for (id, kind) in hunt::prey(w, rules, radius, &self.refused_prey)
            .into_iter()
            .take(hunt::CANDIDATES)
        {
            let mut h = hunt::Hunt::new(w, id, kind, tick);
            if h.reachable(w, tick) {
                return Some(h);
            }
            self.refused_prey.push(id);
        }
        None
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

    /// A walk into the area around `c` stopped getting closer: every target within
    /// `AREA_RADIUS` refused a while, and for good (until a relocation clears it) once walks
    /// there have stalled `BLOCKED_AFTER` times, as a cell is.
    fn stalled_area(&mut self, c: Cell, tick: u64) {
        let near = |a: Cell| {
            let (dx, dy, dz) = (
                f64::from(a.0 - c.0),
                f64::from(a.1 - c.1),
                f64::from(a.2 - c.2),
            );
            (dx * dx + dy * dy + dz * dz).sqrt() <= AREA_RADIUS
        };
        self.stalled.push(c);
        let n = self.stalled.iter().filter(|a| near(**a)).count();
        let until = if n >= BLOCKED_AFTER as usize {
            u64::MAX
        } else {
            tick + FAILURE_MEMORY_TICKS
        };
        self.refused_areas.push((c, until));
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
            || self.dangers.iter().any(|(centre, until, r)| {
                tick < *until && {
                    let (dx, dy, dz) = (
                        f64::from(centre.0 - c.0),
                        f64::from(centre.1 - c.1),
                        f64::from(centre.2 - c.2),
                    );
                    (dx * dx + dy * dy + dz * dz).sqrt() <= *r
                }
            })
            || self
                .refused_areas
                .iter()
                .any(|(centre, until)| tick < *until && near((*centre, c)))
    }

    /// Short of the food reserve in the overworld on the way to the Nether (issue 16a): animals in
    /// view are hunted then, not only when hungry.
    fn provisioning(&self, w: &World, rules: &make::SurvivalRules) -> bool {
        matches!(self.aim, Aim::Nether | Aim::Race)
            && w.dimension
                .as_ref()
                .is_some_and(|d| d.name == "minecraft:overworld")
            && eat::food_points(w, rules) < rules.food_reserve
    }

    /// Issue 15c's aim: met in the Nether. A portal seen (or the one this bot lit) is walked into
    /// and stood in until the dimension changes; with none, the portal row casts one.
    fn nether_task(&mut self, w: &World, origin: [f64; 3], tick: u64) -> Task {
        if w.dimension
            .as_ref()
            .is_some_and(|d| d.name == "minecraft:the_nether")
        {
            // The race goes on to the fortress from here.
            if self.aim == Aim::Race {
                return self.fortress_task(w, tick);
            }
            self.met = true;
            self.note(tick, "nether met".to_owned(), "through a portal".to_owned());
            return Task::Idle;
        }
        let seen = world::stage::kind_near(w, data::BlockKind::NetherPortal, PORTAL_CHUNKS);
        let cells: Vec<Cell> = self
            .portal
            .into_iter()
            .chain(seen)
            .filter(|c| !self.refused_now(*c, tick))
            .take(CANDIDATES_PER_TICK)
            .collect();
        // Food first, then the Nether (Bridger, 2026-11-20): with a portal to enter and the reserve
        // short, look for animals (the hunt row takes those in view) before stepping in.
        let short = self
            .table
            .clone()
            .filter(|t| !cells.is_empty() && self.provisioning(w, &t.survival))
            .map(|t| (eat::food_points(w, &t.survival), t.survival.food_reserve));
        if let Some((have, want)) = short {
            self.note(
                tick,
                format!("provisions {have} of {want}"),
                "short of the food reserve before the portal".to_owned(),
            );
            if let Some(t) = self.explore_walk(w, tick, "look for animals before the Nether") {
                return t;
            }
        }
        for c in cells {
            let mut walk = Walk::new(c, Goal::Block { pos: c }, Arrive::Goal, tick);
            if walk.route_now(w, tick) && walk.route == Some(path::Status::Found) {
                self.walks += 1;
                self.note(
                    tick,
                    format!("enter the portal at {c:?}"),
                    "a lit portal seen".to_owned(),
                );
                return Task::Enter(Box::new(Enter {
                    walk,
                    cell: c,
                    inside_since: None,
                }));
            }
            self.refuse(c, tick);
        }
        self.decide(w, origin, tick)
    }

    /// Issue 15's aim: met once nether bricks have been seen with an exposed face; until then,
    /// explore (15a's baseline; the held-heading search is 15b).
    fn fortress_task(&mut self, w: &World, tick: u64) -> Task {
        if world::stage::at_fortress(w) {
            self.met = true;
            self.note(
                tick,
                "fortress met".to_owned(),
                format!(
                    "within {} of nether bricks seen exposed",
                    world::stage::FORTRESS_NEAR
                ),
            );
            return Task::Idle;
        }
        // Bricks seen: walk toward the nearest a route gets nearer to, refusing each once tried.
        let bricks: Vec<Cell> = world::stage::bricks_near(w, FORTRESS_CHUNKS)
            .into_iter()
            .filter(|c| !self.refused_now(*c, tick))
            .take(CANDIDATES_PER_TICK)
            .collect();
        for c in bricks {
            let mut walk = Walk::new(
                c,
                Goal::Near {
                    pos: c,
                    range: world::stage::FORTRESS_NEAR - 1.0,
                },
                Arrive::Goal,
                tick,
            );
            // A found route only: partial ones ended in the open over lava, among ghasts and
            // piglins, and 113 of 224 bots died (docs/problems/fortress.md wall 3).
            if walk.route_now(w, tick) && walk.route == Some(path::Status::Found) {
                self.refuse(c, tick);
                self.walks += 1;
                self.note(
                    tick,
                    format!("walk toward the fortress at {c:?}"),
                    "nether bricks seen exposed".to_owned(),
                );
                return Task::Explore(walk);
            }
        }
        // No walk: tunnel toward the nearest brick (stairs down to it when it is below), the
        // tunnel hiding the bot from ghasts; three without coming nearer refuse it.
        if let Some(c) = world::stage::bricks_near(w, FORTRESS_CHUNKS)
            .into_iter()
            .find(|c| !self.refused_now(*c, tick))
            && let Some(t) = self.dig_toward(
                w,
                c,
                true,
                ("the fortress", "nether bricks seen, no walk to them"),
                tick,
            )
        {
            return t;
        }
        if let Some(t) = self.explore_walk(w, tick, "no fortress in view") {
            return t;
        }
        self.failed_in_row += 1;
        self.retry_at = tick + RETRY_TICKS;
        self.note(
            tick,
            "no explore walk".to_owned(),
            "no fortress in view and no walk out".to_owned(),
        );
        self.check_stuck(tick);
        Task::Idle
    }

    /// The next task: the nearest log in view far enough from the origin, else explore.
    /// The next task: the nearest log in view far enough from the origin, else explore. Each
    /// candidate is routed before it is chosen, so a chosen walk always has a way (the churn
    /// invariant: the chosen task is not over after one tick); at most `CANDIDATES_PER_TICK`
    /// are tried, and with none the planner waits and counts a failed choice.
    fn choose(&mut self, w: &World, origin: [f64; 3], tick: u64) -> Task {
        let min_distance = match self.aim {
            Aim::ReachLog { min_distance } => min_distance,
            Aim::Table
            | Aim::Pickaxe
            | Aim::Cobblestone
            | Aim::StonePickaxe
            | Aim::StoneTools
            | Aim::Furnace
            | Aim::Ingots
            | Aim::IronPickaxe
            | Aim::Bucket
            | Aim::FlintAndSteel
            | Aim::IronKit
            | Aim::WaterBucket
            | Aim::PortalKit
            | Aim::Portal
            | Aim::Fortress
            | Aim::Nether
            | Aim::Race
            | Aim::Hold => 0.0,
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
                | Aim::StoneTools
                | Aim::Furnace
                | Aim::Ingots
                | Aim::IronPickaxe
                | Aim::Bucket
                | Aim::FlintAndSteel
                | Aim::IronKit
                | Aim::WaterBucket
                | Aim::PortalKit
                | Aim::Portal
                | Aim::Fortress
                | Aim::Nether
                | Aim::Race
                | Aim::Hold => Walk::to_dig(log, tick),
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
            self.give_up(
                tick,
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
        // The nether aim casts with the portal row until a portal is seen (issue 15c).
        let row = match self.aim {
            Aim::Nether | Aim::Race => "portal".to_owned(),
            _ => self.aim.name(),
        };
        let Some(goal) = table.until(&row) else {
            self.stuck = true;
            self.note(
                tick,
                "stuck".to_owned(),
                format!("no goal row for {}", self.aim.name()),
            );
            return Task::Idle;
        };
        // A relocation first: walk off on the turned heading, then plan from there (issue 13b).
        if std::mem::take(&mut self.relocate)
            && let Some(t) = self.explore_walk(w, tick, "relocate: plan again from elsewhere")
        {
            return t;
        }
        // A batch left cooking is collected before anything else (issue 13b).
        if let Some((furnace, result, count)) = self.batch {
            self.note(
                tick,
                format!("collect {count} {result:?} at {furnace:?}"),
                "a batch was left cooking".to_owned(),
            );
            return Task::Smelt(Box::new(smelt::Smelt::collect(
                w, furnace, result, count, tick,
            )));
        }
        // Taken up front: the block needs are a crafting table and a furnace.
        let table_near = self
            .near_block(w, data::BlockKind::CraftingTable, tick)
            .is_some();
        let furnace_near = self.near_block(w, data::BlockKind::Furnace, tick).is_some();
        let near = move |b: data::BlockKind| match b {
            data::BlockKind::CraftingTable => table_near,
            data::BlockKind::Furnace => furnace_near,
            _ => false,
        };
        // A spare is made by a gather only when a log can be started now.
        // A spare is made by a gather only when a log can be started now: one remembered near
        // the feet's height. A bot down a mine cannot walk to the trees it saw on the surface (it
        // has no stairs up), and walks to them failed until the planner stopped
        // (docs/problems/iron-kit.md wall 3).
        let gather_now = self
            .nearest_log(w, origin, 0.0, tick)
            .zip(w.body)
            .is_some_and(|((_, y, _), b)| (f64::from(y) - b.pos[1]).abs() <= GATHER_NOW_RISE);
        // A lava bucket is the cast's bucket: one held after a cast was cut is no bucket short
        // (a bot tunnelled for coal to smelt iron for one: docs/problems/portal.md wall 6).
        let mut counts = held(w);
        if matches!(self.aim, Aim::Portal | Aim::Nether | Aim::Race)
            && let Some(n) = counts.get(&data::ItemKind::LavaBucket).copied()
        {
            *counts.entry(data::ItemKind::Bucket).or_default() += n;
        }
        let choice = make::make_with(&table, &counts, &near, gather_now, goal);
        if matches!(choice, make::Choice::Act(_))
            && let Some(t) = self.opportunity(w, &table, goal, &near, gather_now, tick)
        {
            return t;
        }
        // The portal (issue 14c): its kit held, the cast; met once a portal is lit.
        if matches!(self.aim, Aim::Portal | Aim::Nether | Aim::Race) {
            if let Some(site) = self.site.filter(|s| cast::lit(w, s)) {
                self.portal = Some(site.frame.at((1, 1, 0)));
            }
            // Lit: the nether aim walks into it next (`nether_task`).
            if self.portal.is_some() && matches!(self.aim, Aim::Nether | Aim::Race) {
                return Task::Idle;
            }
            if self.portal.is_some() {
                self.met = true;
                self.note(tick, "portal met".to_owned(), "a portal is lit".to_owned());
                return Task::Idle;
            }
            if choice == make::Choice::Done {
                return self.cast_task(w, tick);
            }
        }
        match choice {
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
                Some(make::Act::Mine { blocks, depth }) => {
                    if let Some(t) = self.carry_table(w, tick) {
                        return t;
                    }
                    self.mine(w, blocks, depth, tick)
                }
                Some(make::Act::Smelt(r)) => {
                    let Some(f) = self.near_block(w, data::BlockKind::Furnace, tick) else {
                        self.retry_at = tick + RETRY_TICKS;
                        self.note(
                            tick,
                            "wait".to_owned(),
                            format!("smelt {} wants a furnace near and none is remembered", r.id),
                        );
                        return Task::Idle;
                    };
                    // The whole batch of the input held: one trip to the furnace for all the ore.
                    let count = r
                        .input
                        .iter()
                        .map(|i| craft::count(w, *i))
                        .sum::<i32>()
                        .min(64);
                    match smelt::Smelt::new(w, f, r, count, tick) {
                        Some(sm) => {
                            self.note(
                                tick,
                                format!("smelt {count} for {} at {f:?}", r.id),
                                format!("the needs planner's next step toward {}", goal.id),
                            );
                            Task::Smelt(Box::new(sm))
                        }
                        None => {
                            self.retry_at = tick + RETRY_TICKS;
                            Task::Idle
                        }
                    }
                }
                Some(make::Act::Scoop) => self.scoop(w, goal.id.clone(), tick),
                Some(make::Act::Redig(b)) => match redig::Redig::new(w, b, tick) {
                    Some(r) => {
                        self.note(
                            tick,
                            format!("redig {b:?}"),
                            format!("its chance drop for {}", goal.id),
                        );
                        Task::Redig(Box::new(r))
                    }
                    None => {
                        self.stuck = true;
                        Task::Idle
                    }
                },
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
                    self.give_up(
                        tick,
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
    fn mine(
        &mut self,
        w: &World,
        blocks: &[data::BlockKind],
        depth: Option<i32>,
        tick: u64,
    ) -> Task {
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
        // An exposed block seen with no walk to it (in a cave behind rock: 1,108 walks failed
        // NoPath in one race) is dug toward: stairs if it is below the feet, else a level
        // tunnel, six steps, then the planner looks again from nearer
        // (docs/problems/iron-kit.md wall 11).
        let aim = exposed_near(w, blocks)
            .into_iter()
            .find(|c| !self.refused_now(*c, tick));
        if let (Some(aim), Some(body)) = (aim, w.body) {
            let (fx, fy, fz) = path::feet_cell(body.pos);
            // Three cuts toward it without coming nearer: refuse it (a bot cut toward one coal
            // seam every 51 ticks for the last stretch of its run: wall 12).
            let d =
                f64::from((aim.0 - fx).pow(2) + (aim.1 - fy).pow(2) + (aim.2 - fz).pow(2)).sqrt();
            let (best, stalls) = match self.toward {
                Some((c, b, n)) if c == aim => {
                    if d < b - 0.5 {
                        (d, 0)
                    } else {
                        (b, n + 1)
                    }
                }
                Some(_) | None => (d, 0),
            };
            if stalls >= 3 {
                self.toward = None;
                self.refuse(aim, tick);
                self.refused_areas.push((aim, tick + FAILURE_MEMORY_TICKS));
                return self.mine(w, blocks, depth, tick);
            }
            self.toward = Some((aim, best, stalls));
            let (dx, dz) = (aim.0 - fx, aim.2 - fz);
            let dir = if dx.abs() >= dz.abs() {
                (dx.signum(), 0)
            } else {
                (0, dz.signum())
            };
            if dir != (0, 0) {
                let below = aim.1 < fy;
                self.note(
                    tick,
                    format!("dig toward {:?} at {aim:?}", blocks.first()),
                    "seen exposed, with no walk to it".to_owned(),
                );
                return Task::Stairs(Box::new(if below {
                    Stairs::new(dir, tick)
                } else {
                    Stairs::level(dir, tick)
                }));
            }
        }
        // The nearest cardinal to the heading: only exact quarters counted before, so the lava
        // hunt's bearing (172 degrees, say) cut stairs south whatever it aimed at
        // (docs/problems/portal.md wall 8).
        let dir = match ((self.heading / 90.0).round() as i32).rem_euclid(4) {
            1 => (-1, 0),
            2 => (0, -1),
            3 => (1, 0),
            _ => (0, 1),
        };
        // At the row's depth, the lowest feet height its stairs go to (clojurecraft's
        // `:goal/depth`), tunnel along instead (issue 13b; docs/problems/iron-kit.md wall 1).
        let deep = depth.is_some_and(|d| w.body.is_some_and(|b| b.pos[1] <= f64::from(d)));
        if deep {
            self.note(
                tick,
                "tunnel".to_owned(),
                format!("no exposed {:?} in sight, at its depth", blocks.first()),
            );
            return Task::Stairs(Box::new(Stairs::level(dir, tick)));
        }
        self.note(
            tick,
            "stairs down".to_owned(),
            format!("no exposed {:?} in sight with a route", blocks.first()),
        );
        Task::Stairs(Box::new(Stairs::new(dir, tick)))
    }

    /// A smelt cut with its batch loaded leaves it to be collected (issue 13b).
    fn keep_batch(&mut self) {
        if let Task::Smelt(sm) = &self.task
            && sm.loaded()
        {
            self.batch = Some((sm.furnace, sm.result, sm.count));
        }
    }

    fn after_smelt(&mut self, c: Controls, status: smelt::SmeltStatus, tick: u64) -> Controls {
        match status {
            smelt::SmeltStatus::Running => return c,
            smelt::SmeltStatus::Done => {
                self.smelts += 1;
                self.failed_in_row = 0;
                self.batch = None;
                self.task = Task::Idle;
                self.note(
                    tick,
                    "smelted".to_owned(),
                    "the batch's results are held".to_owned(),
                );
            }
            smelt::SmeltStatus::Failed(f) => {
                *self.failures.entry(format!("{f:?}")).or_default() += 1;
                self.retry_at = tick + RETRY_TICKS;
                self.failed_in_row += 1;
                self.batch = None;
                if let Task::Smelt(sm) = std::mem::replace(&mut self.task, Task::Idle)
                    && f == smelt::SmeltFailure::Unopened
                {
                    self.refuse(sm.furnace, tick);
                }
                self.note(
                    tick,
                    format!("smelt failed: {f:?}"),
                    "choose again".to_owned(),
                );
                self.check_stuck(tick);
            }
        }
        Controls::default()
    }

    /// Stairs down, or a level tunnel, toward the bank over a source seen with no walk to it
    /// (clojurecraft's `to-bank`): the water-bucket race 44263024772 left 28 of 112 bots 6 to 12
    /// blocks over a pool they could see, a lake roofed with stone at sea level with no way down
    /// (docs/problems/portal.md wall 40). Three stairs without coming nearer refuse the bank.
    fn stairs_to_bank(&mut self, w: &World, source: Cell, tick: u64) -> Option<Task> {
        let bank = (source.0, source.1 + 1, source.2);
        // Water above the feet is not reached by stairs down; a level tunnel reaches its own
        // height only.
        self.dig_toward(
            w,
            bank,
            false,
            ("the bank", "water seen with no walk to a bank"),
            tick,
        )
    }

    /// Stairs down toward `target` when it is below the feet, else a level tunnel (only when
    /// `above_ok`, for a target over the feet); three digs without coming nearer refuse it. The
    /// note says `what` and `why`: "stairs down toward the bank at …".
    fn dig_toward(
        &mut self,
        w: &World,
        bank: Cell,
        above_ok: bool,
        (what, why): (&str, &str),
        tick: u64,
    ) -> Option<Task> {
        let body = w.body?;
        let (fx, fy, fz) = path::feet_cell(body.pos);
        if (bank.1 > fy && !above_ok) || self.refused_now(bank, tick) {
            return None;
        }
        let d =
            f64::from((bank.0 - fx).pow(2) + (bank.1 - fy).pow(2) + (bank.2 - fz).pow(2)).sqrt();
        let (best, stalls) = match self.toward {
            Some((c, b, n)) if c == bank => {
                if d < b - 0.5 {
                    (d, 0)
                } else {
                    (b, n + 1)
                }
            }
            Some(_) | None => (d, 0),
        };
        if stalls >= 3 {
            self.toward = None;
            self.refuse(bank, tick);
            return None;
        }
        self.toward = Some((bank, best, stalls));
        let (dx, dz) = (bank.0 - fx, bank.2 - fz);
        let down = bank.1 < fy;
        let dir = match (dx.abs() >= dz.abs(), dx, dz) {
            // Right over the bank: stairs need a heading; any one comes back down beside it.
            (_, 0, 0) if down => (1, 0),
            (_, 0, 0) => return None,
            (true, _, _) => (dx.signum(), 0),
            (false, _, _) => (0, dz.signum()),
        };
        self.note(
            tick,
            format!(
                "{} toward {what} at {bank:?}",
                if down { "stairs down" } else { "tunnel" }
            ),
            why.to_owned(),
        );
        Some(Task::Stairs(Box::new(if down {
            Stairs::new(dir, tick)
        } else {
            Stairs::level(dir, tick)
        })))
    }

    /// The nearest remembered surface source a walk reaches (issue 14b); none: explore for one.
    fn scoop(&mut self, w: &World, goal: String, tick: u64) -> Task {
        let candidates: Vec<Cell> = scoop::sources(w)
            .into_iter()
            .filter(|c| !self.refused_now(*c, tick))
            .take(CANDIDATES_PER_TICK)
            .collect();
        // Sources within reach of a search but no scoop routed to any: said, with the first, so
        // a race tells "beside water and no way to scoop it" from "no water" (wall 39: 20 of 56
        // kit bots walked from pool cell to pool cell a block apart and never scooped).
        let near = candidates.first().copied();
        let tried = candidates.len();
        for c in candidates {
            let mut s = scoop::Scoop::new(c, tick);
            if s.route_now(w, tick) {
                self.note(
                    tick,
                    format!("scoop water at {c:?}"),
                    format!(
                        "a surface source within {} blocks, for {goal}",
                        scoop::SEARCH_RADIUS
                    ),
                );
                return Task::Scoop(Box::new(s));
            }
            self.refuse(c, tick);
        }
        if let Some(c) = near {
            self.note(
                tick,
                format!("no scoop routes: {tried} sources within 24, first {c:?}"),
                "the stands round them unreachable or blind".to_owned(),
            );
            if let Some(t) = self.stairs_to_bank(w, c, tick) {
                return t;
            }
        }
        // A remembered pool further off is walked toward first (clojurecraft's `pool-radius`):
        // bots on a plateau explored it whole and never went down to the water they had seen
        // (docs/problems/portal.md wall 11).
        let pools: Vec<Cell> = scoop::far_sources(w)
            .into_iter()
            .filter(|c| !self.refused_now(*c, tick))
            .take(CANDIDATES_PER_TICK)
            .collect();
        for c in pools {
            let mut walk = Walk::new(
                c,
                Goal::Near {
                    pos: c,
                    range: scoop::SEARCH_RADIUS / 2.0,
                },
                Arrive::Goal,
                tick,
            );
            self.refuse(c, tick);
            if walk.route_now(w, tick) {
                self.walks += 1;
                self.note(
                    tick,
                    format!("walk toward water at {c:?}"),
                    format!(
                        "a pool within {}, none within {}",
                        scoop::POOL_RADIUS,
                        scoop::SEARCH_RADIUS
                    ),
                );
                return Task::Explore(walk);
            }
        }
        if let Some(t) = self.explore_walk(w, tick, "no surface water in view with a way") {
            return t;
        }
        // No source and no walk out: a failed choice, counted, so a dead end relocates (one bot
        // idled 13,000 ticks without a note: docs/problems/portal.md wall 2).
        self.failed_in_row += 1;
        self.retry_at = tick + RETRY_TICKS;
        self.note(
            tick,
            "no water to scoop".to_owned(),
            "no surface source with a way and no explore walk".to_owned(),
        );
        self.check_stuck(tick);
        Task::Idle
    }

    /// A scoop of lava for the site in hand from the nearest source within `LAVA_RADIUS` a route
    /// reaches (wall 44), never a cell of the frame or beside one: lava poured into a hole is the
    /// cast's own.
    fn lava_fetch(&mut self, w: &World, site: cast::Site, tick: u64) -> Option<Task> {
        let frame: Vec<Cell> = build::FRAME_CELLS
            .iter()
            .map(|c| site.frame.at((c.0, c.1, 0)))
            .collect();
        let ours = |c: Cell| {
            frame
                .iter()
                .any(|f| (f.0 - c.0).abs() + (f.1 - c.1).abs() + (f.2 - c.2).abs() <= 1)
        };
        let candidates: Vec<Cell> = cast::lava_within(w, cast::LAVA_RADIUS)
            .into_iter()
            .filter(|c| !ours(*c) && !self.refused_now(*c, tick))
            .take(LAVA_FETCH_TRIES)
            .collect();
        // Taken before the scoops refuse what they try: a source no scoop routes to now may be
        // walked toward, where more of its pool shows.
        let far: Vec<Cell> = cast::lava_within(w, cast::FAR_RADIUS)
            .into_iter()
            .filter(|c| !ours(*c) && !self.refused_now(*c, tick))
            .take(LAVA_FETCH_TRIES)
            .collect();
        for c in candidates {
            let mut s = scoop::Scoop::of(c, memory::Fluid::Lava, tick);
            if s.route_now(w, tick) {
                self.note(
                    tick,
                    format!("fetch lava at {c:?}"),
                    format!("for the site at {:?}", site.frame.origin),
                );
                return Some(Task::Scoop(Box::new(s)));
            }
            self.refuse(c, tick);
        }
        // No scoop routes: walk toward the nearest source a walk gets nearer to, the site kept and
        // the fetch due again on arrival (wall 47: casts 71 to 86 ops in gave their site up with
        // "no lava to fetch", 47 a pair).
        if self.fetch_walks >= LAVA_FETCH_WALKS {
            return None;
        }
        for c in far {
            let mut walk = Walk::new(
                c,
                Goal::Near {
                    pos: c,
                    range: scoop::REACH,
                },
                Arrive::Goal,
                tick,
            );
            self.refuse(c, tick);
            if walk.route_now(w, tick) {
                self.fetch_walks += 1;
                self.fetch_lava = true;
                self.walks += 1;
                self.note(
                    tick,
                    format!("walk toward lava to fetch at {c:?}"),
                    format!(
                        "no scoop routes; walk {} of {LAVA_FETCH_WALKS}",
                        self.fetch_walks
                    ),
                );
                return Some(Task::Explore(walk));
            }
        }
        None
    }

    /// A cast at the site in hand, or one chosen now; with none, go down for lava (clojurecraft's
    /// `lava-hunt`: stairs toward the lava sea, the lava they expose kept in memory).
    fn cast_task(&mut self, w: &World, tick: u64) -> Task {
        if self.fetch_lava
            && let Some(site) = self.site
        {
            self.fetch_lava = false;
            if let Some(t) = self.lava_fetch(w, site, tick) {
                return t;
            }
            self.lava_trips = 0;
            self.fetch_walks = 0;
            self.given_up_sites.push(site.frame.origin);
            self.site = None;
            self.note(
                tick,
                "no lava to fetch".to_owned(),
                format!(
                    "no scoop routes to a source within {}; site given up",
                    cast::LAVA_RADIUS
                ),
            );
        }
        let (choice, report) = if self.site.is_some() {
            (None, cast::SiteReport::default())
        } else {
            cast::site_choice_report(w, &self.given_up_sites)
        };
        // A site whose landing no walk reaches yet: dig toward the landing (stairs when it is
        // below, a level tunnel else), three cuts without coming nearer refusing it, as the mine
        // task digs toward ore (docs/problems/portal.md wall 9).
        if let (Some(c), Some(body)) = (choice.as_ref().filter(|c| !c.routed), w.body)
            && !self.refused_now(c.landing, tick)
        {
            let aim = c.landing;
            let (fx, fy, fz) = path::feet_cell(body.pos);
            let d =
                f64::from((aim.0 - fx).pow(2) + (aim.1 - fy).pow(2) + (aim.2 - fz).pow(2)).sqrt();
            let (best, stalls) = match self.toward {
                Some((t, b, n)) if t == aim => {
                    if d < b - 0.5 {
                        (d, 0)
                    } else {
                        (b, n + 1)
                    }
                }
                Some(_) | None => (d, 0),
            };
            if stalls >= 3 {
                self.toward = None;
                self.refuse(aim, tick);
                self.given_up_sites.push(c.site.frame.origin);
            } else {
                self.toward = Some((aim, best, stalls));
                let (dx, dz) = (aim.0 - fx, aim.2 - fz);
                let dir = if dx.abs() >= dz.abs() {
                    (dx.signum(), 0)
                } else {
                    (0, dz.signum())
                };
                if dir != (0, 0) {
                    self.note(
                        tick,
                        format!("dig toward the landing {aim:?}"),
                        format!(
                            "site at {:?} facing {}, no walk to it",
                            c.site.frame.origin,
                            c.site.frame.facing.name()
                        ),
                    );
                    return Task::Stairs(Box::new(if aim.1 < fy {
                        Stairs::new(dir, tick)
                    } else {
                        Stairs::level(dir, tick)
                    }));
                }
            }
        }
        let site = self
            .site
            .or_else(|| choice.filter(|c| c.routed).map(|c| c.site));
        if let Some(site) = site {
            self.site = Some(site);
            self.note(
                tick,
                format!(
                    "cast at {:?} facing {} (lava {:?})",
                    site.frame.origin,
                    site.frame.facing.name(),
                    site.lava
                ),
                "a site beside lava seen exposed".to_owned(),
            );
            return Task::Cast(Box::new(cast::Cast::new(site, tick)));
        }
        // No site: head for the nearest lava seen within `FAR_RADIUS`, down to its height
        // (clojurecraft's lava hunt goes toward far sources); with none seen, down toward the lava
        // sea. Stairs down blind boxed in 25,652 times in one race (docs/problems/portal.md wall 5).
        if let (Some(far), Some(body)) = (cast::far_lava(w, &self.given_up_sites), w.body) {
            let feet = path::feet_cell(body.pos);
            let to = [f64::from(far.0) + 0.5, body.pos[1], f64::from(far.2) + 0.5];
            self.heading = walk::yaw_to(body.pos, to).rem_euclid(360.0);
            // A walk toward it first, where a route gets nearer (clojurecraft's hunt walks before
            // it digs); stairs only where none does. A bot tunnelled in place 55,000 ticks toward
            // lava 58 blocks off, its stairs turned by a diagonal heading (wall 7).
            if !self.refused_now(far, tick) {
                let mut walk = Walk::new(
                    far,
                    Goal::Near {
                        pos: far,
                        range: f64::from(cast::LANDING_FAR),
                    },
                    Arrive::Goal,
                    tick,
                );
                if walk.route_now(w, tick) {
                    self.refuse(far, tick);
                    self.walks += 1;
                    self.note(
                        tick,
                        format!("walk toward lava at {far:?}"),
                        format!(
                            "seen from {feet:?}, no site within {}: {}",
                            cast::LAVA_RADIUS,
                            report.text()
                        ),
                    );
                    return Task::Explore(walk);
                }
            }
            self.note(
                tick,
                format!("hunt lava at {far:?}"),
                format!(
                    "seen from {feet:?}, no site within {}: {}",
                    cast::LAVA_RADIUS,
                    report.text()
                ),
            );
            return self.mine(w, &[], Some(far.1 + 2), tick);
        }
        self.mine(w, &[], Some(LAVA_DEPTH), tick)
    }

    fn after_cast(
        &mut self,
        c: Controls,
        status: cast::CastStatus,
        site: cast::Site,
        account: CastAccount,
        tick: u64,
    ) -> Controls {
        let CastAccount {
            ops,
            at,
            uses,
            water,
            lava,
        } = account;
        match status {
            cast::CastStatus::Running => return c,
            cast::CastStatus::Done => {
                self.water_trips = 0;
                self.lava_trips = 0;
                self.fetch_walks = 0;
                self.portal = Some(site.frame.at((1, 1, 0)));
                self.failed_in_row = 0;
                self.task = Task::Idle;
                self.note(
                    tick,
                    "portal lit".to_owned(),
                    format!("{ops} ops done; {uses}"),
                );
            }
            // Out of water: the site is kept and the water search (the scoop ladder, 109 of 112
            // from `kit`) fills a bucket; the cast resumes at the first op not done. The cast's
            // own refill tries only sources near the site, and gave up casts 108 ops in
            // (wall 43: 27 casts a pair ended NoRefill at :pour :water, holding no water).
            cast::CastStatus::Failed(cast::CastFailure::NoRefill)
                if water == 0 && self.water_trips < WATER_TRIPS =>
            {
                self.water_trips += 1;
                self.retry_at = tick + RETRY_TICKS;
                self.task = Task::Idle;
                self.note(
                    tick,
                    format!("cast out of water after {ops} ops, at {at}"),
                    format!(
                        "site kept; fetch water ({} of {WATER_TRIPS})",
                        self.water_trips
                    ),
                );
            }
            // Out of lava with water in hand: the site is kept and lava fetched from any source
            // within `LAVA_RADIUS` a scoop routes to; the cast's own refill tries four sources
            // within `POOL_RADIUS` of its pool, and stations with one source in reach ran dry
            // (wall 44: 34 casts a pair ended NoRefill at :pour :lava, "no route … tunnels").
            cast::CastStatus::Failed(cast::CastFailure::NoRefill)
                if water > 0 && lava == 0 && self.lava_trips < LAVA_TRIPS =>
            {
                self.lava_trips += 1;
                self.fetch_lava = true;
                self.retry_at = tick + RETRY_TICKS;
                self.task = Task::Idle;
                self.note(
                    tick,
                    format!("cast out of lava after {ops} ops, at {at}"),
                    format!(
                        "site kept; fetch lava ({} of {LAVA_TRIPS})",
                        self.lava_trips
                    ),
                );
            }
            cast::CastStatus::Failed(f) => {
                *self.failures.entry(format!("Cast{f:?}")).or_default() += 1;
                self.water_trips = 0;
                self.lava_trips = 0;
                self.fetch_walks = 0;
                self.fetch_lava = false;
                self.given_up_sites.push(site.frame.origin);
                self.site = None;
                self.retry_at = tick + RETRY_TICKS;
                self.failed_in_row += 1;
                self.task = Task::Idle;
                self.note(
                    tick,
                    format!("cast failed: {f:?} after {ops} ops, at {at}"),
                    format!("site given up; none within {} of it", cast::GIVE_UP_RADIUS),
                );
                self.check_stuck(tick);
            }
        }
        Controls::default()
    }

    fn after_enter(&mut self, c: Controls, status: TaskStatus, cell: Cell, tick: u64) -> Controls {
        match status {
            TaskStatus::Running => return c,
            TaskStatus::Done => {
                self.failed_in_row = 0;
                self.task = Task::Idle;
                self.note(
                    tick,
                    "through the portal".to_owned(),
                    "the dimension changed".to_owned(),
                );
            }
            TaskStatus::Failed(f) => {
                *self.failures.entry(format!("Enter{f:?}")).or_default() += 1;
                self.refuse(cell, tick);
                self.retry_at = tick + RETRY_TICKS;
                self.failed_in_row += 1;
                self.task = Task::Idle;
                self.note(
                    tick,
                    format!("enter failed: {f:?}"),
                    "the portal cell refused a while; choose again".to_owned(),
                );
                self.check_stuck(tick);
            }
        }
        Controls::default()
    }

    fn after_scoop(
        &mut self,
        c: Controls,
        status: scoop::ScoopStatus,
        target: Cell,
        tick: u64,
    ) -> Controls {
        match status {
            scoop::ScoopStatus::Running => return c,
            scoop::ScoopStatus::Done => {
                self.failed_in_row = 0;
                self.task = Task::Idle;
                self.note(tick, "scooped".to_owned(), "a bucket filled".to_owned());
            }
            scoop::ScoopStatus::Failed(f) => {
                *self.failures.entry(format!("Scoop{f:?}")).or_default() += 1;
                self.refuse(target, tick);
                self.retry_at = tick + RETRY_TICKS;
                self.failed_in_row += 1;
                self.task = Task::Idle;
                self.note(
                    tick,
                    format!("scoop failed: {f:?}"),
                    "the source refused a while; choose again".to_owned(),
                );
                self.check_stuck(tick);
            }
        }
        Controls::default()
    }

    fn after_redig(&mut self, c: Controls, status: redig::RedigStatus, tick: u64) -> Controls {
        match status {
            redig::RedigStatus::Running => return c,
            redig::RedigStatus::Done => {
                self.digs += 1;
                self.failed_in_row = 0;
                self.task = Task::Idle;
                self.note(
                    tick,
                    "redug".to_owned(),
                    "the needs planner looks again".to_owned(),
                );
            }
            redig::RedigStatus::Failed(f) => {
                *self.failures.entry(format!("Redig{f:?}")).or_default() += 1;
                self.retry_at = tick + RETRY_TICKS;
                self.failed_in_row += 1;
                self.task = Task::Idle;
                self.note(
                    tick,
                    format!("redig failed: {f:?}"),
                    "choose again".to_owned(),
                );
                self.check_stuck(tick);
            }
        }
        Controls::default()
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
            self.give_up(
                tick,
                format!("{MAX_UNWALKABLE} tasks failed in a row with none done"),
            );
        }
    }

    /// Too many failures in a row: a goal aim moves elsewhere (refusals forgotten, the heading
    /// turned, the next choice an explore walk) up to `MAX_RELOCATIONS` times; then, or for issue
    /// 09's log aim, the planner stops.
    fn give_up(&mut self, tick: u64, why: String) {
        if self.aim.is_goal() && self.relocations < MAX_RELOCATIONS {
            self.relocations += 1;
            self.relocate = true;
            self.failed_in_row = 0;
            self.refused.clear();
            self.refused_areas.clear();
            self.stalled.clear();
            self.heading = (self.heading + 90.0) % 360.0;
            self.note(
                tick,
                format!("relocate ({} of {MAX_RELOCATIONS})", self.relocations),
                why,
            );
            return;
        }
        // A goal aim never stops for good: it rests, then starts relocating again (27 of 112 bots
        // stopped at tick 4,000 to 8,000 and idled out the rest: docs/problems/iron-kit.md wall
        // 19).
        if self.aim.is_goal() {
            self.relocations = 0;
            self.failed_in_row = 0;
            self.retry_at = tick + REST_TICKS;
            self.note(tick, format!("rest {REST_TICKS} ticks"), why);
            return;
        }
        self.stuck = true;
        self.note(tick, "stuck".to_owned(), why);
    }

    /// An explore walk on the heading (issue 09's, also a relocation's).
    /// A block one of the goal's provides will need dug, exposed within `OPPORTUNITY_RADIUS`
    /// with a route: dug now rather than walked back for (issue 13b: bots passed gravel on the
    /// way down for a pickaxe, then tunnelled for gravel at their depth). Each provide is planned
    /// on its own; one whose next act is a mine row with such a block near wins.
    fn opportunity(
        &mut self,
        w: &World,
        table: &make::Table,
        goal: &make::Row,
        near: &dyn Fn(data::BlockKind) -> bool,
        gather_now: bool,
        tick: u64,
    ) -> Option<Task> {
        let body = w.body?;
        let counts = held(w);
        // The goal's provides, then what it wants up to their counts (less what is held).
        let wanted = goal.provides.iter().cloned().chain(
            goal.wants
                .iter()
                .filter(|(k, n)| k.have(&counts) < *n)
                .cloned(),
        );
        for p in wanted.collect::<Vec<_>>().iter() {
            let one = make::Row {
                id: goal.id.clone(),
                needs: Vec::new(),
                provides: vec![p.clone()],
                holds: Vec::new(),
                spares: Vec::new(),
                wants: Vec::new(),
                act: None,
            };
            let make::Choice::Act(r) = make::make_with(table, &counts, near, gather_now, &one)
            else {
                continue;
            };
            let Some(make::Act::Mine { blocks, .. }) = r.act else {
                continue;
            };
            let close = exposed_near(w, blocks).into_iter().find(|c| {
                let d = [
                    f64::from(c.0) + 0.5 - body.pos[0],
                    f64::from(c.1) + 0.5 - body.pos[1],
                    f64::from(c.2) + 0.5 - body.pos[2],
                ];
                (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt() <= OPPORTUNITY_RADIUS
                    && !self.refused_now(*c, tick)
            });
            let Some(c) = close else { continue };
            let mut walk = Walk::to_dig(c, tick);
            if walk.route_now(w, tick) {
                self.walks += 1;
                self.note(
                    tick,
                    format!("walk to {:?} at {c:?}", blocks.first()),
                    format!("{} will need it, and it is exposed near", r.id),
                );
                return Some(Task::ToLog(walk));
            }
            self.refuse(c, tick);
        }
        None
    }

    fn explore_walk(&mut self, w: &World, tick: u64, why: &str) -> Option<Task> {
        let body = w.body?;
        // Marooned (every walk failed, water beside the feet): swim to a far shore
        // (docs/problems/iron-kit.md wall 24).
        if std::mem::take(&mut self.maroon)
            && let Some(e) = escape::Escape::swim(w, tick)
        {
            self.note(tick, format!("swim to {:?}", e.target), why.to_owned());
            return Some(Task::Swim(Box::new(e), tick));
        }
        // The full distance first, then shorter: a bot on a shore or a ledge may have only a
        // short way off it (docs/problems/iron-kit.md wall 7).
        for d in [f64::from(EXPLORE_DISTANCE), 16.0, 8.0]
            .into_iter()
            .flat_map(|d| std::iter::repeat_n(d, 4))
        {
            let (s, c) = self.heading.to_radians().sin_cos();
            let p = [body.pos[0] - s * d, body.pos[1], body.pos[2] + c * d];
            let target = path::feet_cell(p);
            if self.explored.iter().any(|e| {
                f64::from((e.0 - target.0).pow(2) + (e.2 - target.2).pow(2)).sqrt() < EXPLORED_NEAR
            }) {
                self.heading = (self.heading + 90.0) % 360.0;
                continue;
            }
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
                self.note(tick, format!("explore toward {target:?}"), why.to_owned());
                return Some(Task::Explore(walk));
            }
            self.expanded += walk.expanded;
            self.heading = (self.heading + 90.0) % 360.0;
        }
        // No way out ahead: back along the trail, the newest mark far enough away that a route
        // reaches (bots dug into dead ends by drops and water, every way ahead refused:
        // docs/problems/iron-kit.md wall 18).
        let marks: Vec<[f64; 3]> = self.trail.iter().rev().copied().collect();
        for m in marks {
            let d = (0..3)
                .map(|i| (m[i] - body.pos[i]).powi(2))
                .sum::<f64>()
                .sqrt();
            if d < TRAIL_BACK {
                continue;
            }
            let target = path::feet_cell(m);
            let mut walk = Walk::new(
                target,
                Goal::Near {
                    pos: target,
                    range: 1.5,
                },
                Arrive::Goal,
                tick,
            );
            if walk.route_now(w, tick) {
                self.walks += 1;
                self.note(tick, format!("back to {target:?}"), why.to_owned());
                return Some(Task::Explore(walk));
            }
            self.expanded += walk.expanded;
        }
        // No walk off at all with water beside the feet: swim (a relocation after boxed stairs
        // on a lake shore never armed `maroon`, which walk failures set: wall 26).
        if wet(w, &body)
            && let Some(e) = escape::Escape::swim(w, tick)
        {
            self.note(tick, format!("swim to {:?}", e.target), why.to_owned());
            return Some(Task::Swim(Box::new(e), tick));
        }
        None
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
/// A remembered log counts as one a gather can start on now only this near the feet's height.
pub const GATHER_NOW_RISE: f64 = 6.0;
/// A block a provide of the goal will need is dug now when exposed this near (blocks).
pub const OPPORTUNITY_RADIUS: f64 = 6.0;
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

/// Water beside the feet or under a cell beside them: a shore, or an island.
fn wet(w: &World, body: &physics::Body) -> bool {
    let (x, y, z) = path::feet_cell(body.pos);
    [(1, 0), (-1, 0), (0, 1), (0, -1)].iter().any(|(dx, dz)| {
        [y, y - 1].iter().any(|y| {
            matches!(
                w.block_at(world::BlockPos::new(x + dx, *y, z + dz)),
                Some(data::BlockKind::Water)
            )
        })
    })
}
