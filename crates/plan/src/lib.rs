//! The planner (brain/ruststeve/design/planner.md): what the bot does next, as a pure function of
//! the world and its own held task. Issue 09 begins it with one goal row, "reach a log", the
//! pathfinder (`path`, a port of clojurecraft's `path.clj`) and the walk (`walk`) with its progress
//! invariant. Every task it starts or ends is an event with its reason. Issue 10 adds the goal
//! table and the needs planner (`make`), and the tasks that change the world: `dig` and
//! `craft`; they act through `Act`s the shell takes with `take_acts`.

#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

// >>> Q7 (issue 19, the overworld arrival)
pub mod arrival;
// <<< Q7
// >>> base (P5, issue 18d)
pub mod base;
// <<< base
pub mod build;
// >>> pearls (P3, issue 18e/f)
pub mod burrow;
// <<< pearls
pub mod cast;
pub mod cover;
pub mod craft;
pub mod defend;
pub mod dig;
pub mod eat;
pub mod endermen;
pub mod escape;
// >>> eyes (Q3, issue 19c)
pub mod eyes;
// <<< eyes
// >>> heading (E2, issue 17d)
pub mod heading;
// >>> home (Q2, issue 19d)
pub mod home;
// <<< home
// <<< heading
// >>> Q11 (issue 19): the way home's fortress stretch.
pub mod fortress_way;
// <<< Q11
pub mod hold;
pub mod hunt;
pub mod ledger;
pub mod look;
pub mod make;
// >>> overland (Q4, issue 19e)
pub mod overland;
// <<< overland
pub mod path;
// >>> pearls (P3, issue 18f)
pub mod pearls;
// <<< pearls
pub mod place;
// >>> Q6 (issue 19): the last leg to the portal's front.
pub mod portal_way;
// <<< Q6
pub mod redig;
pub mod scoop;
pub mod shield;
pub mod smelt;
pub mod stairs;
pub mod station;
// >>> stronghold (Q4, issue 19f)
pub mod stronghold;
// <<< stronghold
pub mod tunnel;
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
/// In the Nether, a dig toward a target farther than this on the plane is a travel tunnel
/// (`tunnel::route`); nearer, the plain cut: a route's reach is 5.
pub const TUNNEL_FROM: i32 = 8;
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
    // >>> home (Q2, issue 19d): the dimension the walk began in; done once it differs. A task
    // from before the field (none: tasks are not in a snapshot) reads as the overworld's.
    #[serde(default)]
    from: Option<String>,
    // <<< home
}

impl Enter {
    /// Done once the world is in another dimension than the portal's; while the feet are in a
    /// portal block, stand still (a step out resets the game's count); else walk on.
    fn step(&mut self, w: &World, tick: u64) -> (Controls, TaskStatus) {
        // >>> home (Q2, issue 19d): done in any dimension other than the one it began in (the
        // way home enters from the Nether).
        let from = self.from.as_deref().unwrap_or("minecraft:overworld");
        if w.dimension.as_ref().is_some_and(|d| d.name != from) {
            return (Controls::default(), TaskStatus::Done);
        }
        // <<< home
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
/// Blaze rods the race wants (clojurecraft's `rods-wanted`: six for twelve eyes, one spare).
pub const RODS_WANTED: i32 = 7;
/// With the rods held, the bot walks this far from the spawner by the route's measure (a blaze
/// shoots from 48), at most `LEAVE_TRIES` walks, before the aim is met.
pub const LEAVE_RANGE: f64 = 50.0;
pub const LEAVE_TRIES: u32 = 12;
/// Ticks a fruitless search for a cover cell is not made again.
pub const COVER_SEARCH_TICKS: u64 = 20;
/// Ticks between putting on one piece of armour and the next.
const EQUIP_TICKS: u64 = 20;
/// An escape this long writes its trail to the ledger, and again each time as long (16e).
const ESCAPE_REPORT_TICKS: u64 = 2400;
/// Blocks from the feet a lying blaze rod is walked to (issue 16d).
const ROD_REACH: f64 = 8.0;
/// A brick within this of a trail mark has been visited (issue 16b, the spawner search).
const VISITED_NEAR: f64 = 8.0;
/// What a shelter is made of, the first held: cobblestone does not burn on; netherrack burns for
/// ever once a fireball lights it, so it is the last.
const SHELTER_BLOCKS: &[data::ItemKind] = &[
    data::ItemKind::Cobblestone,
    data::ItemKind::CobbledDeepslate,
    data::ItemKind::NetherBricks,
    data::ItemKind::Netherrack,
];
/// Ticks between two shelter blocks: the block update arrives in a tick or two (the escape's
/// `SEAL_EVERY_TICKS`).
const SHELTER_EVERY_TICKS: u64 = 10;
/// Ticks a stack may sit on the cursor with no window and no click before it is put down: a swap
/// through the cursor is a few ticks (a click and its answer), five seconds is stuck.
const CURSOR_STUCK_TICKS: u64 = 100;
/// Shelter blocks in one run before the bot stops building: a closed box round one cell is 10
/// (eight wall cells, one more for the roof to hang on, the roof); eight boxes.
const SHELTER_MAX: u32 = 80;
/// Blocks a cook's hut may take: ten round a body in one column (eight walls and a roof, the
/// table and furnace among the walls), more where the body spans two columns or a floor is
/// missing (overworld-leg wall 11).
const HUT_MAX: u32 = 24;
/// Ticks before dusk at which the surface walks stop: a walk back of up to 48 blocks and a hut
/// of ten blocks, with room (overworld-leg wall 16).
const DUSK_MARGIN: i64 = 1200;
/// Ticks after a junk drop in which its kind coming back counts as the drop picked up again: the
/// 40 ticks of a thrown stack's pick-up delay and the walk back over it.
const REPICKUP_TICKS: u64 = 200;
/// Stone over the junk keep at which the carried table is put down to press it: a stack, a slot
/// (wall 12).
const PRESS_AT: i32 = 64;
/// Looks at one far lava target that come no nearer than the best before it is given up: a leg
/// of stairs or a walk each (overworld-leg wall 13).
const LAVA_AIM_STALLS: u32 = 4;
/// Items of the dropped kind coming back at once that count as the stack picked up again: a dig
/// brings one.
const REPICKUP_JUMP: i32 = 4;
/// Ticks a full pack holds tasks back while its junk is dropped: three drops at `EQUIP_TICKS`
/// and their resyncs, with room (overworld-leg wall 10).
const FULL_WAIT_TICKS: u64 = 200;
// >>> overland and stronghold (Q4)
/// A throw's walk point (`eyes_way`, read from the feet each call) that moved more than this is
/// a new target for the overland: a quarter of the shortest leg (`[overland] leg_min` 64), so a
/// leg is not begun again for the point's drift as the bot walks.
const WAY_MOVED: f64 = 16.0;
/// A throw waiting this long at its spot (`eyes_task` blocked: a canopy or a cliff on the
/// bearing) moves the spot: half a minute.
const WAY_WAIT: u64 = 600;
/// The spot moved this far, east, south, west, north in turn: past a tree's crown (the canopy
/// Q3 reads is 10 to 14 out) and within the baseline's slack.
const WAY_NUDGE: f64 = 20.0;
/// Twelve moves: three rings of four, each ring `WAY_NUDGE` farther out (race 45747459284 lane 0
/// used its four of 20 under one wide canopy).
const WAY_NUDGES: u32 = 12;
// <<< overland and stronghold
/// A trail mark counts as toward the spawner when it is at least this much nearer to it than the
/// feet are (two trail steps: a mark beside the bot is no way on).
pub const TRAIL_TOWARD: f64 = 4.0;
/// Ticks between looks while a spawner on record is refused and no trail mark is nearer (a
/// refusal lasts `FAILURE_MEMORY_TICKS` 600: six looks).
const SPAWNER_WAIT_TICKS: u64 = 100;

/// Whether a rest can bring health back: food to eat, or a food bar natural healing runs on
/// (`eat::HEALS_FROM`).
pub fn can_heal(food: Option<i32>, food_held: bool) -> bool {
    food_held || food.is_none_or(|f| f >= eat::HEALS_FROM)
}

/// Blocks behind the nearest it came to a dig's target at which the bot counts as pushed back:
/// a stairs leg is six steps, so from six back no one leg comes nearer than the mark.
pub const PUSHED_BACK: f64 = 6.0;

/// Whether feet at `pos` are `PUSHED_BACK` or more farther from `target` than `best`, the
/// nearest a dig toward it has come (cell to cell, as `dig_toward` measures).
pub fn pushed_back(pos: [f64; 3], target: Cell, best: f64) -> bool {
    let f = path::feet_cell(pos);
    let d = f64::from((target.0 - f.0).pow(2) + (target.1 - f.1).pow(2) + (target.2 - f.2).pow(2))
        .sqrt();
    d >= best + PUSHED_BACK
}

/// The trail marks at least `TRAIL_TOWARD` nearer to `target` than `feet`, the nearest to the
/// target first: where the bot itself has stood, so no cell it has not seen.
pub fn trail_toward(trail: &[[f64; 3]], feet: [f64; 3], target: Cell) -> Vec<[f64; 3]> {
    let to = |p: [f64; 3]| {
        let d = [
            f64::from(target.0) + 0.5 - p[0],
            f64::from(target.1) + 0.5 - p[1],
            f64::from(target.2) + 0.5 - p[2],
        ];
        (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt()
    };
    let here = to(feet);
    let mut marks: Vec<(f64, [f64; 3])> = trail
        .iter()
        .map(|m| (to(*m), *m))
        .filter(|(d, _)| *d + TRAIL_TOWARD <= here)
        .collect();
    marks.sort_by(|a, b| a.0.total_cmp(&b.0));
    marks.into_iter().map(|(_, m)| m).collect()
}
/// Chunk columns a spawner search looks for bricks in: 48 blocks, farther is another fortress
/// (clojurecraft's `corridor-reach`).
const CORRIDOR_CHUNKS: i32 = 3;
/// Times a cast out of lava keeps its site to fetch lava from further off (wall 44).
pub const LAVA_TRIPS: u32 = 2;
/// Lava sources a fetch asks a scoop route of, nearest first.
const LAVA_FETCH_TRIES: usize = 8;
/// Eye to a source's centre within which a fetch walks toward it no more: a walk to within
/// `scoop::REACH` of it ends at once (cast.md wall 2). A block over the reach, for the walk's
/// own range measured from the feet.
const FETCH_NEAR: f64 = scoop::REACH + 1.0;
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
    // >>> base (P5, issue 18d): a development aim, the forest base where the bot stands.
    Base,
    // <<< base
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
            None if s == "base" => Some(Aim::Base),
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
            Aim::Base => "base".to_owned(),
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
            | Aim::Base
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
    // >>> eyes (Q3, issue 19c)
    /// An eye of ender thrown, followed, read and picked up again (`eyes`).
    Throw(Box<eyes::Throw>),
    // <<< eyes
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
            // >>> eyes
            Task::Throw(_) => "throw-eye",
            // <<< eyes
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
            // >>> eyes
            Task::Throw(_) => None,
            // <<< eyes
            Task::ToLog(w) | Task::Explore(w) => Some(w),
        }
    }
}

pub use ledger::{Open, TaskCounts, TaskRecord};

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
    /// Swings sent and swings that took health off a foe, over every fight (issue 16: the case
    /// file wrote 0 for both, so blaze wall 3's "0 swings" was no reading).
    #[serde(default)]
    pub swings_sent: u64,
    #[serde(default)]
    pub swings_landed: u64,
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
    /// When armour may next be put on (issue 16).
    #[serde(default)]
    pub equip_at: u64,
    /// When the next shelter block may be placed, and how many have been (spawner wall 3).
    #[serde(default)]
    pub shelter_at: u64,
    #[serde(default)]
    pub shelter_blocks: u32,
    /// When "no rest: nothing to heal with" may next be noted.
    #[serde(default)]
    pub unhealed_at: u64,
    /// When "no fight post: hurt, no cover found" may next be noted.
    #[serde(default)]
    pub unposted_at: u64,
    /// No cover cell with a route was found: the search waits until this tick.
    #[serde(default)]
    pub cover_none_until: u64,
    /// Walks away from the spawner begun with 7 rods held.
    #[serde(default)]
    pub leave_tries: u32,
    /// Issue 17: after the rods, on to an enderman (`endermen`; E3's state, one field).
    #[serde(default)]
    pub ender: endermen::EnderState,
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
    /// The task ledger (issue 16e): the serial `begin` gives each task, the one open, the records
    /// not yet taken by the shell, and the counts coverage reads.
    #[serde(default)]
    pub task_serial: u64,
    #[serde(default)]
    pub open: Option<Open>,
    #[serde(skip)]
    pub records: Vec<TaskRecord>,
    #[serde(default)]
    pub task_counts: TaskCounts,
    /// The "low on digs" note was made (once a run).
    #[serde(default)]
    pub low_digs_noted: bool,
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
    /// The clean flow (issue 16, Bridger 2026-11-22): wood and food stocked on the surface before
    /// the first descent; then, the kit made, back up under the open sky and stocked again before
    /// the cast (`world::stage::Stage::Ready`). Each is done once; the clocks bound the food hunt.
    #[serde(default)]
    pub stocked: bool,
    #[serde(default)]
    pub readied: bool,
    #[serde(default)]
    pub provision_since: Option<u64>,
    /// The cell the climb to the surface heads for, and how many headings were refused.
    #[serde(default)]
    pub surface_target: Option<Cell>,
    #[serde(default)]
    pub surface_tries: u8,
    /// The Nether kit (the `nether-kit` row: spare pickaxes and a table) was made, or could not
    /// be, before the portal was entered (blaze.md wall 18); `kit_row` is set while the needs
    /// planner is asked for that row.
    #[serde(default)]
    pub kit_done: bool,
    #[serde(skip)]
    kit_row: bool,
    /// Tasks begun for the kit: past `KIT_TASKS` it is given up.
    #[serde(default)]
    pub kit_tasks: u32,
    /// Spare pickaxes are being made (overworld-leg wall 2): the `nether-kit` row is worked
    /// until its pickaxes are held; not asked again before `spares_retry_at` once it could not be.
    #[serde(default)]
    pub sparing: bool,
    #[serde(default)]
    pub spares_retry_at: u64,
    /// Legs of the lava hunt's own stairs cut so far: the spiral's next heading (wall 1).
    #[serde(default)]
    pub descent_legs: u32,
    /// The cook phase (overworld-leg wall 4): tasks begun for it, whether it is over (done or
    /// given up), and whether its end was noted.
    #[serde(default)]
    pub cook_tasks: u32,
    #[serde(default)]
    pub cook_over: bool,
    /// The way home's cook (issue 19d): set only around one `decide` call from Home's night in
    /// the hut, which then takes the cook row as the clean flow's cook phase does.
    #[serde(default)]
    pub home_cook: bool,
    /// A stack on the cursor since (no window, no click), and the stacks put down from it.
    #[serde(default)]
    pub cursor_since: Option<u64>,
    #[serde(default)]
    pub cursor_puts: u32,
    /// The cook walled in (wall 11): blocks placed for its hut, and whether the hut is done
    /// (closed, or none could be built).
    #[serde(default)]
    pub hut_blocks: u32,
    #[serde(default)]
    pub hut_done: bool,
    /// Furnaces pressed from junk stone (wall 12), and junk stacks picked up again after a drop:
    /// the kind, the count left after the drop, and when.
    #[serde(default)]
    pub pressed: u32,
    /// Far lava targets of the hunt with the nearest the feet came and the looks since that came
    /// no nearer (wall 13).
    #[serde(default)]
    pub lava_aims: Vec<(Cell, f64, u32)>,
    /// A table put down to press stone at, to be taken along after (wall 12).
    #[serde(default)]
    pub press_table: bool,
    #[serde(default)]
    pub repickups: u32,
    #[serde(default)]
    pub dropped: Option<(data::ItemKind, i32, u64)>,
    /// Whether dusk has stopped the surface walks, noted (wall 16).
    #[serde(default)]
    pub dusk_noted: bool,
    /// When the pack was first seen with no main slot free (wall 10).
    #[serde(default)]
    pub full_since: Option<u64>,
    /// The second stocking's look for animals is over: its walks failed `HUNT_WALK_FAILS` times
    /// running (overworld-leg wall 4: twelve failed walks cost a relocation, and six end the run).
    #[serde(default)]
    pub hunt_over: bool,
    /// The feet cell the last leg of own stairs began from, and legs begun from it since: a leg
    /// that leaves the feet where they were made no way down (wall 1).
    #[serde(default)]
    pub own_from: Option<(Cell, u32)>,
    /// The last leg of own stairs begun (its feet cell and heading), the leg that led to the
    /// cell the feet are on, and a cell walked back to with the heading it must not take again
    /// (wall 9: a leg cut onto a ledge over a cave, every heading's floor air).
    #[serde(default)]
    pub own_leg: Option<(Cell, (i32, i32))>,
    #[serde(default)]
    pub own_into: Option<(Cell, (i32, i32))>,
    #[serde(default)]
    pub own_back: Option<(Cell, (i32, i32))>,
    /// When the portal's food check first found the bot short, and whether it has said the bot
    /// goes in short (wall 3).
    #[serde(default)]
    pub portal_food_since: Option<u64>,
    #[serde(default)]
    pub portal_food_noted: bool,
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
    /// The travel tunnel in hand: its target, route and the cells found bad
    /// (docs/problems/nether-tunnel.md).
    #[serde(default)]
    pub travel: tunnel::Travel,
    /// The same count for each cast landing looked at, the newest last (blaze.md wall 19: the
    /// site choice flipped between two landings and each flip forgot the other).
    #[serde(default)]
    pub landings: Vec<(Cell, f64, u32)>,
    /// The tick the next far search may run (A2, nether-walk.md): one costs tens of thousands
    /// of expansions, so it is run once, its route held by the walk, and not again for
    /// `walk::FAR_RETRY_TICKS`.
    #[serde(default)]
    pub far_search_at: u64,
    /// Far walks begun on a found route that failed, in a row: each doubles the wait before the
    /// next far search, so a route found and not walkable gives way to the tunnel (the model's
    /// lake: the same 50-cell route found and failed every 80 ticks for a whole run).
    #[serde(default)]
    pub far_fails: u32,
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
    /// The shield's raise and lower, and what it met (docs/problems/shield.md).
    #[serde(default)]
    pub shield: shield::Shield,
    /// The shield made in the Nether from what is held: over (made, skipped or given up), the
    /// tasks begun for it, and set while the needs planner is asked for it alone.
    #[serde(default)]
    pub shield_done: bool,
    #[serde(default)]
    pub shield_tasks: u32,
    /// After the shield: cook rounds and table digs begun, and the whole stop over.
    #[serde(default)]
    pub shield_cooks: u32,
    #[serde(default)]
    pub shield_carries: u32,
    #[serde(default)]
    pub shield_over: bool,
    /// When "shield waits for safe ground" may next be noted.
    #[serde(default)]
    pub shield_wait_at: u64,
    #[serde(skip)]
    shield_only: bool,
    // >>> heading (E2, issue 17d): the heading held (docs/problems/heading.md).
    #[serde(default)]
    pub headed: Option<heading::Heading>,
    // <<< heading
    // >>> pearls (P3, issue 18e/f): the burrow and the hunt (docs/problems/pearls-hunt.md).
    #[serde(default)]
    pub pearls: pearls::PearlState,
    // <<< pearls
    // >>> base (P5, issue 18d): the forest base's steps.
    #[serde(default)]
    pub base: base::BaseState,
    // <<< base
    // >>> overland and stronghold (Q4, issue 19e/f, docs/problems/eyes-stronghold.md).
    #[serde(default)]
    pub overland: overland::OverlandState,
    #[serde(default)]
    pub stronghold: stronghold::StrongholdState,
    // <<< overland and stronghold
    // >>> eyes (Q3, issue 19c): the lines read, their fix, and the throws' record
    // (docs/problems/eyes-throw.md).
    #[serde(default)]
    pub fix: Option<eyes::Fix>,
    #[serde(default)]
    pub throws: Vec<eyes::Line>,
    #[serde(default)]
    pub eyes: eyes::EyeState,
    // <<< eyes
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
            swings_sent: 0,
            swings_landed: 0,
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
            equip_at: 0,
            shelter_at: 0,
            shelter_blocks: 0,
            unhealed_at: 0,
            unposted_at: 0,
            cover_none_until: 0,
            leave_tries: 0,
            ender: endermen::EnderState::default(),
            given_up_sites: Vec::new(),
            explored: Vec::new(),
            portal: None,
            ignored_foes: Vec::new(),
            event_count: 0,
            task_serial: 0,
            open: None,
            records: Vec::new(),
            task_counts: TaskCounts::default(),
            low_digs_noted: false,
            table: None,
            met: false,
            digs: 0,
            crafts: 0,
            traps: escape::Traps::default(),
            escapes: 0,
            stocked: false,
            readied: false,
            kit_done: false,
            kit_row: false,
            kit_tasks: 0,
            sparing: false,
            spares_retry_at: 0,
            descent_legs: 0,
            portal_food_since: None,
            portal_food_noted: false,
            cook_tasks: 0,
            cook_over: false,
            home_cook: false,
            cursor_since: None,
            cursor_puts: 0,
            hut_blocks: 0,
            hut_done: false,
            full_since: None,
            dusk_noted: false,
            pressed: 0,
            lava_aims: Vec::new(),
            press_table: false,
            repickups: 0,
            dropped: None,
            hunt_over: false,
            own_from: None,
            own_leg: None,
            own_into: None,
            own_back: None,
            provision_since: None,
            surface_target: None,
            surface_tries: 0,
            meals: 0,
            kills: 0,
            always_retry_at: 0,
            batch: None,
            smelts: 0,
            relocations: 0,
            toward: None,
            travel: tunnel::Travel::default(),
            landings: Vec::new(),
            far_search_at: 0,
            far_fails: 0,
            mark: None,
            stalls: 0,
            trail: Vec::new(),
            relocate: false,
            maroon: false,
            refused_prey: Vec::new(),
            shield: shield::Shield::default(),
            shield_done: false,
            shield_tasks: 0,
            shield_cooks: 0,
            shield_carries: 0,
            shield_over: false,
            shield_wait_at: 0,
            shield_only: false,
            // >>> heading (E2)
            headed: None,
            // <<< heading
            // >>> pearls
            pearls: pearls::PearlState::default(),
            // <<< pearls
            // >>> base
            base: base::BaseState::default(),
            // <<< base
            // >>> overland and stronghold (Q4)
            overland: overland::OverlandState::default(),
            stronghold: stronghold::StrongholdState::default(),
            // <<< overland and stronghold
            // >>> eyes
            fix: None,
            throws: Vec::new(),
            eyes: eyes::EyeState::default(),
            // <<< eyes
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
        // >>> home: the way home keeps the bridge blocks it lays, to dig back when stranded.
        if self.ender.phase == endermen::EnderPhase::Home
            && let Some(c) = what
                .strip_prefix("bridge block laid at ")
                .and_then(home::parse_cell)
        {
            self.ender.home.laid.push(c);
            if self.ender.home.laid.len() > home::LAID_KEPT {
                self.ender.home.laid.remove(0);
            }
        }
        // <<< home
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

    /// One tick: the controls to press. The world is read, never changed. Every task begun or
    /// dropped in the tick goes into the task ledger (issue 16e).
    pub fn step(&mut self, w: &World) -> Controls {
        let notes_before = self.event_count;
        // >>> shield (docs/problems/shield.md): the one call site of `plan::shield`. The defend's
        // swing count before the tick: a strike the raised shield drops is given back.
        let swings_before = self.swing_count();
        let c = self.step_inner(w);
        self.cursor_watch(w);
        self.count_repickup(w);
        let c = self.shield_step(w, c, swings_before);
        // <<< shield
        // Never stare (issue 17e): every look the planner sends leaves here, kept off endermen.
        let c = self.keep_look(w, c);
        self.track(w.session.server_ticks, notes_before);
        c
    }

    /// A stack left on the cursor with no window and no click pending for `CURSOR_STUCK_TICKS` is
    /// put down in a free main slot (else the window is asked for again), noted: every task
    /// begins with an empty cursor (race 45715674818 lane 0: netherrack 33 on the cursor, a
    /// tunnel's stairs at (418, 40, 158) stood 1,200 ticks twice with no dig begun).
    fn cursor_watch(&mut self, w: &World) {
        let tick = w.session.server_ticks;
        let Some((kind, n)) = w.inventory.carried else {
            self.cursor_since = None;
            return;
        };
        if w.window.is_some() || w.click.is_some() {
            return;
        }
        let since = *self.cursor_since.get_or_insert(tick);
        if tick.saturating_sub(since) < CURSOR_STUCK_TICKS || !self.acts.is_empty() {
            return;
        }
        self.cursor_since = None;
        self.cursor_puts += 1;
        let act = match hold::put_down_slot(w) {
            Some(slot) => world::Act::Click {
                slot,
                button: 0,
                mode: 0,
            },
            None => world::Act::Resync,
        };
        self.note(
            tick,
            format!("cursor put down ({kind:?} {n})"),
            format!(
                "on the cursor {} ticks with no window and no click; {act:?}; {} in the run",
                tick.saturating_sub(since),
                self.cursor_puts
            ),
        );
        self.acts.push(act);
    }

    /// A junk stack dropped and its kind back by `REPICKUP_JUMP` or more at once within
    /// `REPICKUP_TICKS`: a
    /// pick-up of the drop, noted and counted (overworld-leg wall 12; the run's count is to be 0).
    fn count_repickup(&mut self, w: &World) {
        let Some((kind, left, at)) = self.dropped else {
            return;
        };
        let tick = w.session.server_ticks;
        if tick.saturating_sub(at) > REPICKUP_TICKS {
            self.dropped = None;
            return;
        }
        // `left` is negative until the drop shows (minus one less the count it must fall to); then the
        // lowest count seen since. A stack back in one look, not a block or two dug since (a
        // stair's digs come one by one), is the drop picked up again.
        let now = craft::count(w, kind);
        if left < 0 {
            if now < -left {
                self.dropped = Some((kind, now, at));
            }
            return;
        }
        if now < left + REPICKUP_JUMP {
            self.dropped = Some((kind, now.min(left), at));
            return;
        }
        {
            self.dropped = None;
            self.repickups += 1;
            self.note(
                tick,
                format!("junk picked up again: {kind:?}"),
                format!(
                    "{now} held, {left} after the drop {} ticks before; {} in the run",
                    tick.saturating_sub(at),
                    self.repickups
                ),
            );
        }
    }

    /// A shield made in the Nether from what the bot holds (docs/problems/shield.md): a real bot
    /// that came through its portal before the kit made shields holds raw iron, a log or two and
    /// a table, which is one shield. The needs planner is asked for the shield row alone, work
    /// where the bot stands only (`decide` ends it on anything else), at most `SHIELD_TASKS`
    /// tasks; then the table is taken along again.
    fn nether_shield(&mut self, w: &World, origin: [f64; 3], tick: u64) -> Option<Task> {
        if self.shield_over {
            return None;
        }
        // Never beside a spawner or in a shooter's sight: the shield waits for safe ground (the
        // travel tunnel, a rest out of sight beyond the spawner's range), and the leg goes on.
        if !self.shield_done
            && let Some(why) = cover::unsafe_to_work(w)
        {
            if tick >= self.shield_wait_at {
                self.shield_wait_at = tick + FAILURE_MEMORY_TICKS;
                self.note(
                    tick,
                    "shield waits for safe ground".to_owned(),
                    format!("{why}; made where neither holds"),
                );
            }
            return None;
        }
        if !self.shield_done {
            // Only a bot holding no shield begins: one made at the surface is never made again.
            let row = if self.shield_tasks == 0 {
                shield::nether_row(&held(w))
            } else {
                shield::kit_row(&held(w))
            };
            if row.is_some() && self.shield_tasks < SHIELD_TASKS {
                self.shield_tasks += 1;
                if self.shield_tasks == 1 {
                    self.note(
                        tick,
                        format!("make a shield here ({})", row.unwrap_or_default()),
                        "iron and wood for one are held; none of the kit's is".to_owned(),
                    );
                }
                self.kit_row = true;
                self.shield_only = true;
                let t = self.decide(w, origin, tick);
                self.kit_row = false;
                self.shield_only = false;
                if !self.shield_done {
                    return Some(t);
                }
            }
            self.shield_done = true;
            if self.shield_tasks > 0 {
                self.note(
                    tick,
                    format!(
                        "shield making over: {} held after {} tasks",
                        craft::count(w, data::ItemKind::Shield),
                        self.shield_tasks
                    ),
                    "cook what the fuel left allows, then on to the fortress".to_owned(),
                );
            }
        }
        // Nothing was made here: no furnace or table of this bot's stands near.
        if self.shield_tasks == 0 {
            self.shield_over = true;
            return None;
        }
        // The furnace stands: the held raw meat cooked with the fuel left, a round or two (a
        // round ends at the first fuel kind's end only when the smelt cannot refuel).
        if self.shield_cooks < SHIELD_COOKS {
            self.shield_cooks += 1;
            if let Some(t) = self.cook_held(w, tick) {
                return Some(t);
            }
            self.shield_cooks = SHIELD_COOKS;
        }
        // Then the table is taken along again, with a bound on the digs for it.
        self.shield_carries += 1;
        if self.shield_carries <= SHIELD_CARRIES
            && let Some(t) = self.carry_table(w, tick)
        {
            return Some(t);
        }
        self.shield_over = true;
        self.note(
            tick,
            format!(
                "shield stop over: {} shield, {} cooked food, {} tables held",
                craft::count(w, data::ItemKind::Shield),
                shield::COOKED
                    .iter()
                    .map(|k| craft::count(w, *k))
                    .sum::<i32>(),
                craft::count(w, data::ItemKind::CraftingTable)
            ),
            "on to the fortress".to_owned(),
        );
        None
    }

    /// A smelt of the raw meat held at the furnace near, as far as the fuel held burns
    /// (`shield::cook_choice`), or `None`.
    fn cook_held(&mut self, w: &World, tick: u64) -> Option<Task> {
        let counts = held(w);
        let (r, count) = shield::cook_choice(&counts)?;
        let Some(f) = self.near_block(w, data::BlockKind::Furnace, tick) else {
            self.note(
                tick,
                "no cooking: no furnace near".to_owned(),
                format!("{count} {} would cook", r.id),
            );
            return None;
        };
        let sm = smelt::Smelt::new(w, f, r, count, tick)?;
        self.note(
            tick,
            format!("cook {count} for {} at {f:?}", r.id),
            format!(
                "raw meat held and {} ticks of fuel left after the shield",
                shield::fuel_ticks_held(&counts)
            ),
        );
        Some(Task::Smelt(Box::new(sm)))
    }

    /// The shield over whatever the tick chose: a held shield goes to the off hand; with a
    /// fireball coming or a blaze about to shoot it is raised, and while it is up the bot strikes
    /// nothing and walks slowed, toward the threat only (`shield::controls`: what a client using
    /// an item can do). An escape, a meal or any work of the hands comes first.
    fn shield_step(
        &mut self,
        w: &World,
        c: Controls,
        swings_before: Option<shield::SwingCount>,
    ) -> Controls {
        let tick = w.ticks;
        if self.acts.is_empty()
            && tick >= self.equip_at
            && let Some((slot, act)) = shield::to_off_hand(w)
        {
            self.equip_at = tick + EQUIP_TICKS;
            self.acts.push(act);
            self.note(
                tick,
                format!("shield to the off hand from slot {slot}"),
                "a shield held, the off hand without one".to_owned(),
            );
            return c;
        }
        let task_free = match &self.task {
            Task::Defend(_) | Task::Idle | Task::ToLog(_) | Task::Explore(_) => true,
            Task::Escape(_)
            | Task::Eat(_)
            | Task::Hunt(_)
            | Task::Dig(_)
            | Task::Craft(_)
            | Task::Place(_)
            | Task::Stairs(_)
            | Task::Smelt(_)
            | Task::Redig(_)
            | Task::Scoop(_)
            | Task::Cast(_)
            | Task::Enter(_)
            // >>> eyes
            | Task::Throw(_)
            // <<< eyes
            | Task::Swim(..) => false,
        };
        // A strike, a change of the held slot or a click to place a block is weighed by the
        // shield; any other act is work.
        let hands_free = self.acts.iter().all(|a| {
            matches!(
                a,
                world::Act::Attack { .. }
                    | world::Act::Swing
                    | world::Act::Hold { .. }
                    | world::Act::UseOn { .. }
            )
        });
        let placing = self
            .acts
            .iter()
            .any(|a| matches!(a, world::Act::UseOn { .. }));
        let duty = shield::Duty {
            free: task_free && hands_free,
            placing,
            striking: match &self.task {
                Task::Defend(d) => shield::striking(
                    w,
                    d,
                    self.table
                        .as_ref()
                        .map_or(&[][..], |t| &t.survival.ranged[..]),
                ),
                _ => None,
            },
        };
        let step = self.shield.step(w, tick, &duty);
        for what in step.notes {
            self.note(tick, what, "shield".to_owned());
        }
        // A placing click held for the shield is not sent: its try is given back to the task
        // that made it (the shelter's count, the bridge's tries), which clicks again.
        if step.hold_place {
            self.acts.retain(|a| !matches!(a, world::Act::UseOn { .. }));
            match &mut self.task {
                Task::Idle => self.shelter_blocks = self.shelter_blocks.saturating_sub(1),
                Task::Explore(walk) if walk.more.place_tick == tick => {
                    walk.more.place_tries = walk.more.place_tries.saturating_sub(1);
                }
                _ => {}
            }
        }
        if step.yaw.is_some() {
            let dropped = self
                .acts
                .iter()
                .filter(|a| matches!(a, world::Act::Attack { .. }))
                .count();
            self.acts
                .retain(|a| !matches!(a, world::Act::Attack { .. } | world::Act::Swing));
            // The defend counted the swing and set its cooldown when it chose it: the strike
            // was never sent, so both are put back (the fight's swing evidence, and the next
            // swing at once when the shield lowers, not a full cooldown later).
            if dropped > 0
                && let Task::Defend(d) = &mut self.task
            {
                let back = swings_before.unwrap_or_default().restore(d);
                self.swings_sent = self.swings_sent.saturating_sub(u64::from(back));
                self.shield.strikes_held += back;
            }
        }
        // First: a lowering goes ahead of a meal's own `use_item` in the same tick.
        if let Some(act) = step.act {
            self.acts.insert(0, act);
        }
        match step.yaw {
            Some(yaw) => shield::controls(c, yaw, tick),
            None => c,
        }
    }

    /// The defend's swing count, for `shield_step` to give a dropped strike back.
    fn swing_count(&self) -> Option<shield::SwingCount> {
        match &self.task {
            Task::Defend(d) => Some(shield::SwingCount::of(d)),
            _ => None,
        }
    }

    /// The task records made since the last call, oldest first: the shell writes them.
    pub fn take_records(&mut self) -> Vec<TaskRecord> {
        std::mem::take(&mut self.records)
    }

    /// The makings of a stone pickaxe held: two sticks' worth of wood, three stone, and a table
    /// held or remembered near (clojurecraft's `build/pickaxe-at-hand?`; blaze wall 8).
    fn pickaxe_at_hand(&self, w: &World, tick: u64) -> bool {
        let table_near = self
            .near_block(w, data::BlockKind::CraftingTable, tick)
            .is_some();
        pickaxe_makeable(w, table_near)
    }

    /// Ends the open task with the run (the ledger then holds an end for every start).
    pub fn close(&mut self, at: u64, why: &str) {
        // An escape cut by the run's end says what it tried (nether-walk.md: three bots died in
        // lava with the escape's seals and targets in no file).
        if let Task::Escape(e) = &self.task {
            let said = e.report();
            self.note(
                at,
                format!("escape-{} ended with the run", e.kind.name()),
                said,
            );
        }
        if let Some(o) = self.open.take() {
            self.task_counts.ended += 1;
            self.records.push(TaskRecord::Ended {
                id: o.id,
                at,
                task: o.task,
                ticks: at.saturating_sub(o.at),
                outcome: Some("the run ended".to_owned()),
                why: Some(why.to_owned()),
            });
        }
    }

    /// The held tools' wear for the needs planner: the worn tools (fewer than `spare_uses` left)
    /// and whether the digs held are under the travel reserve (blaze wall 7).
    fn wear(&self, w: &World, rules: &make::SurvivalRules, table_near: bool) -> make::Wear {
        let mut worn = make::Counts::new();
        for (slot, (k, _)) in &w.inventory.slots {
            if *slot >= 1
                && let Some(u) = w.inventory.uses_left(*slot)
                && u < rules.spare_uses
            {
                *worn.entry(*k).or_default() += 1;
            }
        }
        make::Wear {
            worn,
            low_on_digs: dig_budget(w, table_near) < rules.travel_reserve_digs,
        }
    }

    /// Sets a task the planner has chosen, numbering it for the ledger. `Idle` is no task.
    fn begin(&mut self, task: Task) {
        if !matches!(task, Task::Idle) {
            self.task_serial += 1;
        }
        self.task = task;
    }

    /// Closes the open task if it was dropped or replaced this tick, and opens the one begun. The
    /// notes made this tick give the outcome (the one before a start, when both happened) and
    /// the reason (the last).
    fn track(&mut self, at: u64, notes_before: u64) {
        let made = usize::try_from(self.event_count.saturating_sub(notes_before)).unwrap_or(0);
        let notes: Vec<PlanEvent> = self.events.iter().rev().take(made).rev().cloned().collect();
        let idle = matches!(self.task, Task::Idle);
        let begun = !idle && self.open.as_ref().is_none_or(|o| o.id != self.task_serial);
        let ended = self.open.is_some() && (idle || begun);
        if ended && let Some(o) = self.open.take() {
            let note = if begun {
                notes.len().checked_sub(2).and_then(|i| notes.get(i))
            } else {
                notes.last()
            };
            self.task_counts.ended += 1;
            self.records.push(TaskRecord::Ended {
                id: o.id,
                at,
                task: o.task,
                ticks: at.saturating_sub(o.at),
                outcome: note.map(|n| n.what.clone()),
                why: note.map(|n| n.why.clone()),
            });
        }
        if begun {
            let note = notes.last();
            let name = self.task.name().to_owned();
            self.task_counts.started += 1;
            if note.is_some() {
                self.task_counts.with_reason += 1;
            }
            self.records.push(TaskRecord::Started {
                id: self.task_serial,
                at,
                task: name.clone(),
                what: note.map(|n| n.what.clone()),
                why: note.map(|n| n.why.clone()),
            });
            self.open = Some(Open {
                id: self.task_serial,
                task: name,
                at,
            });
        }
    }

    fn step_inner(&mut self, w: &World) -> Controls {
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
                    // An escape says what it tried (16e: an escape held 25,000 ticks beside a
                    // dry cell with nothing in the ledger to say why).
                    let trail = if let Task::Escape(e) = &self.task {
                        format!("; trail: {}", e.trail.join(" / "))
                    } else {
                        String::new()
                    };
                    self.stalls += 1;
                    self.task = Task::Idle;
                    self.retry_at = tick;
                    self.failed_in_row += 1;
                    *self.failures.entry("Stalled".to_owned()).or_default() += 1;
                    self.note(
                        tick,
                        format!("{cut} stalled"),
                        format!("no progress for {STALL_TICKS} ticks; choose again{trail}"),
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
        // >>> pearls (P3, issue 18f): the hunt's rule table, in the warped forest
        // (docs/problems/pearls-hunt.md). `None` lets the task in hand run.
        if let Some(c) = self.pearls_step(w, tick) {
            return c;
        }
        // <<< pearls
        // A provoked enderman (issue 17e): sealed in where the bot stands, never a swing.
        if let Some(c) = self.ender_threat(w, tick) {
            return c;
        }
        // >>> Q7: the arrival's shaft up, a tick at a time while no task runs.
        if let Some(c) = self.arrival_step(w, tick) {
            return c;
        }
        // <<< Q7
        if self.finished() {
            return Controls::default();
        }
        let origin = *self.origin.get_or_insert(body.pos);
        if matches!(self.task, Task::Idle) {
            if tick < self.retry_at {
                return Controls::default();
            }
            let chosen = match self.aim {
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
                Aim::Base => self.base_aim(w, tick),
                Aim::Hold => Task::Idle,
            };
            self.begin(chosen);
        }
        // Read before the task is borrowed: a cast with no pickaxe and the makings of one pauses.
        let remake = matches!(self.task, Task::Cast(_))
            && !holds_pickaxe(w)
            && self.pickaxe_at_hand(w, tick);
        // >>> Q8 (issue 19): idle at a lava edge (a rail laid, a hold), sneak: the server keeps a
        // sneaking body on its block, a knock-back too (`walk::lava_edge`).
        let idle_sneak = matches!(self.task, Task::Idle) && crate::home::at_lava_edge(w);
        // <<< Q8
        let (controls, status) = match &mut self.task {
            Task::Idle => (
                Controls {
                    sneak: idle_sneak,
                    ..Controls::default()
                },
                TaskStatus::Running,
            ),
            Task::ToLog(walk) | Task::Explore(walk) => {
                let out = walk.step(w, tick);
                // A walk's bridge move lays blocks: its acts are sent and each block laid is
                // written to the ledger (docs/problems/nether-walk.md).
                let (acts, said) = walk.take_out();
                self.acts.extend(acts);
                for (what, why) in said {
                    self.note(tick, what, why);
                }
                out
            }
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
            // No pickaxe left mid-cast but the makings of one held: pause it, the site kept, so the
            // planner crafts one and the cast resumes there (blaze wall 8: one cast ran 104,600
            // ticks by hand with 24 sticks and 49 stone held).
            Task::Cast(_) if remake => {
                self.task = Task::Idle;
                self.note(
                    tick,
                    "cast paused: no pickaxe".to_owned(),
                    "the site is kept; make a pickaxe, then cast again".to_owned(),
                );
                return Controls::default();
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
                    "{}; {}; {}; buckets water {} lava {} empty {}; {uses}; {}; {}",
                    k.current().map(build::op_text).unwrap_or_default(),
                    k.tries_text(),
                    k.regression_text(),
                    held(data::ItemKind::WaterBucket),
                    held(data::ItemKind::LavaBucket),
                    held(data::ItemKind::Bucket),
                    k.refused_text(),
                    k.refill_text()
                );
                let refused = k.refused_sources().to_vec();
                let water = held(data::ItemKind::WaterBucket);
                let lava = held(data::ItemKind::LavaBucket);
                // A source the cast's scoop failed at is refused for the fetch too: the fetch
                // took the very source the refill had failed at four times (cast.md wall 1).
                if !matches!(status, cast::CastStatus::Running) {
                    for c in refused {
                        self.refuse(c, tick);
                    }
                }
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
            // >>> eyes (Q3, issue 19c): the one call site of `eyes::Plan::throw_step`.
            Task::Throw(_) => return self.throw_step(w, tick),
            // <<< eyes
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
                        self.begin(Task::Dig(Dig::new(w, walk.target, tick)));
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
                        if walk.more.far {
                            self.far_fails = 0;
                        }
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
                    // >>> eyes
                    Task::Throw(_) => {} // <<< eyes
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
                        // A2: a far walk found and not walked is not searched again at once.
                        if walk.more.far {
                            self.far_fails += 1;
                            let wait = walk::FAR_RETRY_TICKS << self.far_fails.min(5);
                            self.far_search_at = tick + wait;
                            self.note(
                                tick,
                                format!("far walk refused for {wait} ticks"),
                                format!(
                                    "{} far walks failed in a row, this one at cell {} of {} with {} blocks laid",
                                    self.far_fails,
                                    walk.at,
                                    walk.waypoints.as_ref().map_or(0, Vec::len),
                                    walk.more.placed
                                ),
                            );
                        }
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
                    // >>> eyes
                    Task::Throw(_) => {} // <<< eyes
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
        // Armour held and not worn goes on here in the Nether, ahead of the rows: `equip` runs
        // only when a task is chosen, and a defend cuts in before that (blaze-fight wall 1: three
        // bots fought their first blazes bare and put the armour on 220 ticks later).
        if tick >= self.equip_at
            && w.window.is_none()
            && w.click.is_none()
            && w.inventory.carried.is_none()
            && w.dimension
                .as_ref()
                .is_some_and(|d| d.name == "minecraft:the_nether")
            && let Some(slot) = unworn_armour(&w.inventory.slots)
        {
            self.equip_at = tick + EQUIP_TICKS;
            self.acts.push(world::Act::Click {
                slot,
                button: 0,
                mode: 1,
            });
            self.note(
                tick,
                format!("armour on from slot {slot}"),
                "armour held, not worn; ahead of the always-rows".to_owned(),
            );
        }
        // A cast and a scoop stand beside lava on purpose; anything else answers lava at the door.
        let door = !matches!(self.task, Task::Cast(_) | Task::Scoop(_));
        let trap = self.traps.tick(w, body, door);
        let rules = &table.survival;
        let mut ignored: Vec<i32> = self
            .ignored_foes
            .iter()
            .filter(|(_, until)| tick < *until)
            .map(|(id, _)| *id)
            .collect();
        // Seven rods held: no blaze is gone after any more (the spawner task walks away).
        if craft::count(w, data::ItemKind::BlazeRod) >= RODS_WANTED {
            ignored.extend(
                w.entities
                    .iter()
                    .filter(|(_, e)| e.kind == data::EntityKind::Blaze)
                    .map(|(id, _)| *id),
            );
        }
        // >>> pearls (P3, the orchestrator 2026-11-25): in the hunt a magma cube is never fought
        // by choice (a no-heal bot loses every trade, and they split): the hunt seals a pocket.
        // The way home (issue 19d) is not the hunt, though the top-up started it: on the trip a
        // magma cube that strikes is fought (race 45698618366 lane 3 walked on through five
        // magma cube hits, 19.8 to 1.8 health in 140 ticks, its defence ignoring every cube).
        let home_trip =
            self.ender.phase == endermen::EnderPhase::Home && self.ender.home.top_up_over;
        let hunt_pocket =
            (self.pearls.started && !home_trip) || self.ender.phase == endermen::EnderPhase::Hunt;
        // >>> Q8: on the trip a cube the rules say not to fight is never fought, even one that
        // struck (`defend::foe_except` lets a provoked kind through `ignored`: race 45758211666
        // lane 1 began and dropped a cube fight every tick for 420 ticks on the west span, still,
        // until a hit knocked it off).
        let mut home_never_cubes = false;
        // <<< Q8
        if hunt_pocket {
            ignored.extend(
                w.entities
                    .iter()
                    .filter(|(_, e)| e.kind == data::EntityKind::MagmaCube)
                    .map(|(id, _)| *id),
            );
        } else if home_trip {
            // On the trip a cube is fought only when it is on the bot (within
            // `home::CUBE_FIGHT`) and the health is over the danger line: a large one splits and
            // a no-heal bot loses a long fight; further off the legs shun it, and under the line
            // or with two near the trip seals a pocket (`home_recover`).
            // Two within `CUBE_NEAR`, or a cube pocket in hand: the pocket, no fight.
            let crowd = w
                .entities
                .values()
                .filter(|e| {
                    !e.dead
                        && e.kind == data::EntityKind::MagmaCube
                        && crate::home::dist(e.pos, body.pos) <= crate::home::CUBE_NEAR
                })
                .count();
            // After a pocket given up (`home::CUBE_HOLD_MAX`) the near one is fought though
            // two are near: the pocket waits `CUBE_RESUME_TICKS`.
            let resting = self.ender.home.cube_resume_at.is_some_and(|t| tick < t);
            // A fight begun on one cube that split (race 45722775712 lane 2: a large one struck
            // at (418, 40, 154) became three, the defence switched among them for 280 ticks, 16.2
            // to dead) ends once two are near: the pocket takes over.
            if crowd >= crate::home::CUBE_CROWD && !resting && matches!(self.task, Task::Defend(_))
            {
                self.task = Task::Idle;
                self.note(
                    tick,
                    "home: the cube fight dropped for the pocket".to_owned(),
                    format!("{crowd} magma cubes within {}", crate::home::CUBE_NEAR),
                );
            }
            let over = w.player.health.is_some_and(|h| h > rules.danger_health)
                && (crowd < crate::home::CUBE_CROWD || resting)
                && self.ender.home.cube_pocket.is_none()
                && self.ender.home.cube_tunnel.is_none();
            // >>> Q8: by a lava edge a large or medium cube is never struck: a split puts two to
            // four cubes by the edge (race 45749508716 lane 3 split one on the west span, knocked
            // into lava 40 ticks later). The rails and the sneak hold the bot; it goes on.
            let edge = crate::home::near_lava_edge(w);
            let big: Vec<i32> = w
                .entities
                .iter()
                .filter(|(_, e)| {
                    edge && !e.dead
                        && e.kind == data::EntityKind::MagmaCube
                        && crate::home::cube_size(e) > 1
                        && crate::home::dist(e.pos, body.pos) <= crate::home::CUBE_FIGHT
                })
                .map(|(id, _)| *id)
                .collect();
            if !big.is_empty()
                && self
                    .ender
                    .home
                    .big_cube_noted_at
                    .is_none_or(|t| tick >= t + 200)
            {
                self.ender.home.big_cube_noted_at = Some(tick);
                self.ender.home.big_cubes_spared += 1;
                self.note(
                    tick,
                    "home: a large cube not struck by a lava edge".to_owned(),
                    format!(
                        "{} within {} (sizes {:?}); spared {}",
                        big.len(),
                        crate::home::CUBE_FIGHT,
                        big.iter()
                            .filter_map(|id| w.entities.get(id))
                            .map(crate::home::cube_size)
                            .collect::<Vec<_>>(),
                        self.ender.home.big_cubes_spared
                    ),
                );
            }
            home_never_cubes = !over || !big.is_empty();
            ignored.extend(big);
            // <<< Q8
            ignored.extend(
                w.entities
                    .iter()
                    .filter(|(_, e)| {
                        e.kind == data::EntityKind::MagmaCube
                            && (!over
                                || crate::home::dist(e.pos, body.pos) > crate::home::CUBE_FIGHT)
                    })
                    .map(|(id, _)| *id),
            );
        }
        // On the way home a blaze or a ghast is never fought, at any health (a fireball costs 6 to
        // 9 through iron and nothing heals): the trip's cover takes it (`home_recover`).
        // A rail wanted with a knocker near (`home::rail_wanted`) cuts a walk under way: the
        // planner lays the floor beside the feet before the next step.
        if home_trip
            && matches!(
                self.task,
                Task::Explore(_) | Task::ToLog(_) | Task::Stairs(_)
            )
            && (crate::home::rail_wanted(w).is_some()
                // >>> Q8
                || (crate::home::guard_next(w, self.ender.home.axis, &self.ender.home.guard_refused)
                    .is_some()
                    && self.headed.as_ref().is_none_or(|h| h.restock.is_none())))
                // <<< Q8
            && craft::count(w, data::ItemKind::Netherrack)
                + craft::count(w, data::ItemKind::Basalt)
                + craft::count(w, data::ItemKind::Cobblestone)
                > 0
        {
            let cut = self.task.name();
            self.task = Task::Idle;
            self.retry_at = tick;
            self.note(
                tick,
                format!("home: {cut} cut for a rail"),
                "a mob that knocks back near and no floor beside the feet".to_owned(),
            );
        }
        if home_trip {
            ignored.extend(
                w.entities
                    .iter()
                    .filter(|(_, e)| {
                        matches!(
                            e.kind,
                            data::EntityKind::Blaze
                                | data::EntityKind::Ghast
                                | data::EntityKind::Skeleton
                        )
                    })
                    .map(|(id, _)| *id),
            );
        }
        let never_kinds: &[data::EntityKind] = if hunt_pocket || home_never_cubes {
            &[data::EntityKind::MagmaCube]
        } else {
            &[]
        };
        // <<< pearls
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
            // >>> eyes
            | Task::Throw(_)
            // <<< eyes
            | Task::Swim(..) => None,
        };
        // The first row in force, in order; a running eat or hunt stays in force until it ends.
        let mut chosen: Option<(String, make::Always)> = None;
        for (id, row) in &table.always {
            let in_force = match row {
                make::Always::Escape(k) => trap == Some(*k),
                make::Always::Defend => {
                    running == Some(make::Always::Defend)
                        || defend::foe_except(w, rules, &ignored, never_kinds).is_some()
                }
                make::Always::Eat | make::Always::Hunt if running == Some(*row) => true,
                // A hunt picks up all its drops before the meal: only an escape cuts it.
                make::Always::Eat => {
                    running != Some(make::Always::Hunt)
                        && tick >= self.always_retry_at
                        && (eat::hungry(w, rules) || eat::eat_to_heal(w, rules))
                        && eat::best_food(w, rules).is_some()
                }
                // Never into a cast, a scoop or a portal's entry (a lava bucket in hand, a frame
                // half built): the review of 2026-11-24 found a hunt could cut them.
                make::Always::Hunt => {
                    hunt_may_cut(&self.task)
                        && tick >= self.always_retry_at
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
                let said = match std::mem::replace(&mut self.task, Task::Idle) {
                    Task::Escape(e) => e.report(),
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
                    | Task::Swim(..)
                    | Task::Defend(_)
                    | Task::Eat(_)
                    | Task::Hunt(_) => String::new(),
                    // >>> eyes
                    Task::Throw(_) => String::new(),
                    // <<< eyes
                };
                self.retry_at = tick;
                self.note(
                    tick,
                    format!("out of the {}", k.name()),
                    format!("dry land held; back to the goal; {said}"),
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
                    // Which lava: on the body, or at the door and where (nether-walk.md).
                    let door = (k == escape::TrapKind::Lava && !body.in_lava)
                        .then(|| escape::lava_door_at(w, path::feet_cell(body.pos)))
                        .flatten()
                        .map(|c| format!(" (at the door, {c:?}, the body not in it)"))
                        .unwrap_or_default();
                    self.note(
                        tick,
                        id,
                        format!("trapped in {}{door}; cut {cut}", k.name()),
                    );
                    // Stairs that cut into water: their target waits, or the next choice cuts the
                    // same stairs into the same water (blaze wall 16: 72 escapes in one run, each
                    // after "stairs up toward the log at (36, 88, 7)").
                    if k == escape::TrapKind::Water
                        && matches!(self.task, Task::Stairs(_))
                        && let Some((target, _, _)) = self.toward.take()
                    {
                        self.refuse(target, tick);
                        self.note(
                            tick,
                            format!("refused {target:?}"),
                            "its stairs cut into water".to_owned(),
                        );
                    }
                    Some(Task::Escape(Box::new(escape::Escape::new(k, w, tick))))
                }
                make::Always::Defend => {
                    defend::foe_except(w, rules, &ignored, never_kinds).map(|(foe, kind)| {
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
                    })
                }
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
            self.begin(task);
        }
        // What a defend did this tick, for the ledger (blaze-fight walls 2 and 3).
        let mut said: Vec<String> = Vec::new();
        let (c, act, ended) = match &mut self.task {
            Task::Escape(e) => {
                let (c, act) = e.step(w, tick);
                // A long escape says what it tries, though it never stalls (16e: two ran 23,000
                // ticks, moving, with nothing in the ledger).
                let ran = tick.saturating_sub(e.started);
                let due = ran >= ESCAPE_REPORT_TICKS * (e.reports + 1);
                if due {
                    e.reports += 1;
                }
                let say = due.then(|| format!("{} ticks; trail: {}", ran, e.trail.join(" / ")));
                if let Some(why) = say {
                    self.note(tick, "escape still trapped".to_owned(), why);
                }
                (c, act, None)
            }
            Task::Defend(d) => {
                let (sent, landed) = (d.swings, d.landed);
                let (mut c, act, status) = d.step(w, rules, tick);
                // >>> Q8 (issue 19): a fight at a lava edge sneaks: the server holds a sneaking
                // body on its block, its own steps and a knock-back (race 45758211666 lane 2 was
                // hit into the lava sea fighting small cubes on the island by the join).
                if !c.jump && crate::home::at_lava_edge(w) {
                    c.sneak = true;
                    d.edge_sneak_ticks += 1;
                }
                // <<< Q8
                said = std::mem::take(&mut d.notes);
                self.swings_sent += u64::from(d.swings.saturating_sub(sent));
                self.swings_landed += u64::from(d.landed.saturating_sub(landed));
                let ended = match status {
                    defend::DefendStatus::Running => None,
                    defend::DefendStatus::Done => {
                        // Fled a creeper: its ground waits, or the next walk goes straight back
                        // to it (wall 29).
                        // A ranged foe no strike reached: left alone a while, or it is chosen
                        // again at once (one bot fought one skeleton 51 times: wall 30).
                        // Any foe never struck that never hurt the bot, too: a spider in a pit
                        // under the eye was fought 96 times in one run (blaze wall 15).
                        let unhurt = d
                            .health_start
                            .zip(w.player.health)
                            .is_some_and(|(was, now)| now >= was);
                        if !rules.never_fight.contains(&d.kind)
                            && d.last_hit_at == d.started
                            && (rules.ranged.contains(&d.kind) || unhurt)
                        {
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
                            "defended: {} swings, {} landed, {:?}, {} steps held back; {:?} health {:?} to {:?}, foe up to {:.1} over the eye, charging {} ticks, kept off {} ticks, sweeps held {} ticks, dodged {} fireballs ({} with a velocity), {} swings falling, {} in passing, {} steps held from fire, {} steps to rods on the deck, {} steps off fire on the deck, {} ticks sneaked at a lava edge",
                            d.swings,
                            d.landed,
                            d.stance,
                            d.held_back,
                            d.kind,
                            d.health_start,
                            w.player.health,
                            d.max_dy,
                            d.charged_ticks,
                            d.kept_off,
                            d.sweeps_held,
                            d.dodges,
                            d.dodges_known,
                            d.falling_swings,
                            d.passing_swings,
                            d.fire_held,
                            d.rod_steps,
                            d.fire_steps,
                            d.edge_sneak_ticks
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
            // >>> eyes
            | Task::Throw(_)
            // <<< eyes
            | Task::Swim(..) => return None,
        };
        self.acts.extend(act);
        for what in said {
            self.note(tick, what, "defend".to_owned());
        }
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
        // Walled in for the cook (wall 11): no animal is gone after from the hut.
        let walled = self.hut_done && self.hut_blocks > 0 && !self.readied;
        !walled
            && matches!(self.aim, Aim::Nether | Aim::Race)
            && w.dimension
                .as_ref()
                .is_some_and(|d| d.name == "minecraft:overworld")
            && if rules.meat_stock > 0 {
                eat::meat_pieces(&held(w), rules) < rules.meat_stock
            } else {
                eat::food_points(w, rules) < rules.food_reserve
            }
    }

    /// Armour held but not worn goes on (issue 16): a quick move (shift-click) of the piece from the
    /// inventory takes it to its armour slot. One a second, while window 0 is the open one.
    fn equip(&mut self, w: &World, tick: u64) {
        if tick < self.equip_at || w.window.is_some() {
            return;
        }
        // A nearly full inventory drops a stack of junk first, by the same quick-move clock.
        let mut junk = self
            .table
            .as_ref()
            .map(|t| t.survival.junk.clone())
            .unwrap_or_default();
        // >>> overland and stronghold (Q4): the Nether's leftovers go in the overworld while the
        // stronghold is the aim (race 45695074789 lane 0's full pack took no cobblestone from nine
        // stair legs, and lane 1's crafts failed on a full pack).
        if self.aim == Aim::Race
            && w.dimension
                .as_ref()
                .is_some_and(|d| d.name == stronghold::OVERWORLD)
            && (craft::count(w, data::ItemKind::EnderEye) > 0 || self.stronghold.room.is_some())
        {
            junk.extend(self.stronghold_rules().junk_kinds());
        }
        // >>> Q11 (issue 19): the fortress stretch's tunnel fills the pack (race 45766983995: every
        // wooden pickaxe craft failed on a full pack) (`crate::fortress_way`).
        if self.ender.phase == crate::endermen::EnderPhase::Home
            && self.ender.home.fortress.begun
            && w.dimension
                .as_ref()
                .is_some_and(|d| d.name == crate::home::NETHER)
        {
            junk.extend(self.fortress_rules().junk_kinds());
        }
        // <<< Q11
        // >>> Q8 (issue 19): the same leftovers on the whole trip home in the Nether, the top-up over
        // (the coordinator, 2026-11-26: Q11's race 45766983995 filled every pack before its
        // tunnel), basalt kept to 64 for the guard and the bridges (the deltas' restock digs it).
        if self.ender.phase == crate::endermen::EnderPhase::Home
            && self.ender.home.top_up_over
            && !self.ender.home.fortress.begun
            && w.dimension
                .as_ref()
                .is_some_and(|d| d.name == crate::home::NETHER)
        {
            junk.extend(
                self.fortress_rules()
                    .junk_kinds()
                    .into_iter()
                    .map(|(k, n)| {
                        if k == data::ItemKind::Basalt {
                            (k, crate::home::BASALT_KEPT)
                        } else {
                            (k, n)
                        }
                    }),
            );
        }
        // <<< Q8
        // A kind that came back after its drop is held back (`junk_watch`, `junk_now`).
        self.junk_watch(w, tick);
        let walking = matches!(self.task, Task::Explore(_));
        let junk = stronghold::junk_now(&junk, &self.stronghold.junk_held, tick, walking);
        // In a hut nothing is dropped: a drop lands inside and comes back (Q7's race
        // 45741154601); its blocks are the hut's walls instead (`overland_hut_step`).
        let junk = if self.overland.hut.is_some() {
            Vec::new()
        } else {
            junk
        };
        // <<< overland and stronghold
        // A click no reply came to, with the pack full of junk: window 0 asked for again, so the
        // drop below can go (overworld-leg wall 10: a craft's take into a full pack drew no
        // reply, the click stayed pending, and no junk was dropped for 1,600 ticks until the
        // planner was stuck).
        if let Some(c) = w.click
            && tick.saturating_sub(c.at_tick) > craft::CLICK_PATIENCE_TICKS
            && w.inventory.carried.is_none()
            && junk_slot(&w.inventory.slots, &junk).is_some()
        {
            self.equip_at = tick + EQUIP_TICKS;
            self.acts.push(world::Act::Resync);
            self.note(
                tick,
                "resync a stale click".to_owned(),
                format!(
                    "a click unanswered {} ticks and fewer than {JUNK_FREE_SLOTS} main slots free: the junk drop goes next",
                    tick.saturating_sub(c.at_tick)
                ),
            );
            return;
        }
        // A stack on the cursor and no slot for it: out it goes if it is junk, else onto a junk
        // stack's slot, which comes back on the cursor for the next look (blaze.md wall 21).
        if w.click.is_none()
            && let Some(slot) = cursor_step(&w.inventory.slots, w.inventory.carried, &junk)
        {
            self.equip_at = tick + EQUIP_TICKS;
            self.acts.push(world::Act::Click {
                slot,
                button: 0,
                mode: 0,
            });
            self.note(
                tick,
                format!("cursor stack to slot {slot}"),
                "a stack on the cursor and no main slot free".to_owned(),
            );
            return;
        }
        if w.click.is_none()
            && w.inventory.carried.is_none()
            && let Some((slot, kind)) = junk_slot(&w.inventory.slots, &junk)
        {
            self.equip_at = tick + EQUIP_TICKS;
            // Kept to count a pick-up of it again (wall 12).
            let stack = w.inventory.slots.get(&slot).map_or(0, |(_, n)| *n);
            self.dropped = Some((kind, -(craft::count(w, kind) - stack).max(0) - 1, tick));
            // >>> overland and stronghold (Q4): the drop kept per kind for `junk_watch`.
            self.junk_dropped(kind, (craft::count(w, kind) - stack).max(0), tick);
            // <<< overland and stronghold
            // Mode 4 button 1: drop the whole stack (the client's ctrl+Q over the slot).
            self.acts.push(world::Act::Click {
                slot,
                button: 1,
                mode: 4,
            });
            self.note(
                tick,
                format!("drop {kind:?} from slot {slot}"),
                format!("fewer than {JUNK_FREE_SLOTS} main slots free"),
            );
            return;
        }
        let worn = |slot: i16| w.inventory.slots.contains_key(&slot);
        let piece = w.inventory.slots.iter().find_map(|(slot, (k, _))| {
            let to = match k {
                data::ItemKind::IronHelmet => 5,
                data::ItemKind::IronChestplate => 6,
                data::ItemKind::IronLeggings => 7,
                data::ItemKind::IronBoots => 8,
                _ => return None,
            };
            (*slot >= 9 && !worn(to)).then_some(*slot)
        });
        if let Some(slot) = piece {
            self.equip_at = tick + EQUIP_TICKS;
            self.acts.push(world::Act::Click {
                slot,
                button: 0,
                mode: 1,
            });
            self.note(
                tick,
                format!("put on the armour in slot {slot}"),
                "armour held, not worn".to_owned(),
            );
        }
    }

    /// Issue 15c's aim: met in the Nether. A portal seen (or the one this bot lit) is walked into
    /// and stood in until the dimension changes; with none, the portal row casts one.
    fn nether_task(&mut self, w: &World, origin: [f64; 3], tick: u64) -> Task {
        self.equip(w, tick);
        // No main slot free and junk to drop: nothing begun until a stack is out (wall 10: a
        // craft into a full pack failed twelve times running and the run ended stuck), for
        // `FULL_WAIT_TICKS` at most: a stack thrown at the feet is picked up again 40 ticks later,
        // and a bot that waited on it idled 14,700 ticks (race 45354021804 lane 1).
        let full = (9i16..=44).all(|s| w.inventory.slots.contains_key(&s));
        let since = if full {
            *self.full_since.get_or_insert(tick)
        } else {
            self.full_since = None;
            tick
        };
        if full
            && tick.saturating_sub(since) < FULL_WAIT_TICKS
            && w.window.is_none()
            && w.inventory.carried.is_none()
            && self
                .table
                .as_ref()
                .is_some_and(|t| junk_slot(&w.inventory.slots, &t.survival.junk).is_some())
        {
            return Task::Idle;
        }
        // >>> home (Q2, issue 19d, docs/problems/eyes-home.md): the way home, in either dimension;
        // `None` when not on it or once it is over.
        if let Some(t) = self.home_race_task(w, tick) {
            return t;
        }
        // <<< home
        // >>> overland and stronghold (Q4, issue 19e/f)
        if let Some(t) = self.eyes_stronghold_task(w, tick) {
            return t;
        }
        // <<< overland and stronghold
        if w.dimension
            .as_ref()
            .is_some_and(|d| d.name == "minecraft:the_nether")
        {
            // The race goes on to the fortress, then its spawner and the rods (issue 16).
            if self.aim == Aim::Race {
                // Issue 17: the leave walk is over; on to an enderman.
                if self.ender.left {
                    return self.endermen_task(w, tick);
                }
                // >>> shield (docs/problems/shield.md): a shield first, from what is held.
                if let Some(t) = self.nether_shield(w, origin, tick) {
                    return t;
                }
                // <<< shield
                // Seven rods held is past the fortress whatever the stage clock has read yet
                // (issue 17: the leave walk, then the endermen).
                if w.stage_reached
                    .is_some_and(|s| s >= world::stage::Stage::Fortress)
                    || craft::count(w, data::ItemKind::BlazeRod) >= RODS_WANTED
                {
                    return self.spawner_task(w, tick);
                }
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
        // Proportionate (overworld-leg wall 3): the bot goes in with `portal_food` or more; under
        // it, animals are looked for only under the open sky and for `provision_ticks` at most.
        // Two bots with lit portals explored for animals at y -53 for 24,000 ticks, at 24 and 21
        // of 32.
        let short = self
            .table
            .clone()
            .filter(|t| !cells.is_empty() && self.provisioning(w, &t.survival))
            .map(|t| {
                (
                    eat::food_points(w, &t.survival),
                    portal_food(&t.survival),
                    t.survival.provision_ticks,
                )
            })
            .filter(|(have, want, _)| have < want);
        if let Some((have, want, clock)) = short {
            let since = *self.portal_food_since.get_or_insert(tick);
            let spent = tick.saturating_sub(since);
            if portal_food_look(self.readied, spent, clock, world::stage::open_sky(w)) {
                self.note(
                    tick,
                    format!("provisions {have} of {want}"),
                    "short of the portal's food under the open sky".to_owned(),
                );
                if let Some(t) = self.explore_walk(w, tick, "look for animals before the Nether") {
                    return t;
                }
            } else if !self.portal_food_noted {
                self.portal_food_noted = true;
                self.note(
                    tick,
                    "into the Nether short of food".to_owned(),
                    format!(
                        "provisions {have} of {want}; {}",
                        if spent >= clock {
                            format!("{spent} ticks looked")
                        } else {
                            "no animal lives under rock".to_owned()
                        }
                    ),
                );
            }
        }
        // Tools next (blaze.md wall 18): the Nether has no wood, so the spare pickaxes and a table
        // are made on this side, once, before the first step in.
        if self.aim == Aim::Race && !cells.is_empty() && !self.kit_done {
            self.kit_row = true;
            let t = self.decide(w, origin, tick);
            self.kit_row = false;
            // A bound on the kit (wall 21: its crafts failed until the planner was stuck).
            self.kit_tasks += 1;
            if self.kit_tasks > KIT_TASKS && !self.kit_done {
                self.kit_done = true;
                self.note(
                    tick,
                    "nether kit given up".to_owned(),
                    format!("{KIT_TASKS} tasks begun for it"),
                );
            }
            if !self.kit_done {
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
                    from: w.dimension.as_ref().map(|d| d.name.clone()),
                }));
            }
            self.refuse(c, tick);
        }
        self.decide(w, origin, tick)
    }

    // >>> overland and stronghold (Q4, issue 19e/f, docs/problems/eyes-stronghold.md)
    /// In the overworld with eyes held (or a portal room known): into the room if its frames are
    /// seen, else overland toward the fix (`OverlandState::fix`, set from the eyes' fix), else
    /// `None` (the eyes' throws come first; the orchestrator orders the calls).
    fn eyes_stronghold_task(&mut self, w: &World, tick: u64) -> Option<Task> {
        if self.aim != Aim::Race {
            return None;
        }
        // The eyes made from the pearls and rods held, before anything else (a safety net: Q2's
        // Home phase crafts them in the forest, so this normally finds nothing to do).
        let overworld = w
            .dimension
            .as_ref()
            .is_some_and(|d| d.name == stronghold::OVERWORLD);
        if overworld && let Some(t) = self.eyes_craft_task(w, tick) {
            return Some(t);
        }
        let eyes = craft::count(w, data::ItemKind::EnderEye) > 0;
        if !eyes && self.stronghold.room.is_none() {
            return None;
        }
        // A full pack with only held-back junk: a block of it put down, never dropped.
        if let Some(t) = self.junk_place(w, tick) {
            return Some(t);
        }
        // Fresh stone pickaxes first, between legs and stairs (not in the room: the nook's).
        let in_room = self.stronghold.entered_noted;
        if !in_room && let Some(t) = self.stronghold_pickaxes(w, tick) {
            return Some(t);
        }
        // The eyes' fix is the overland's target from the moment it is crossed.
        if let Some(f) = self.fix {
            let fix = (f.point, f.err_along.max(f.err_across));
            if self.overland.fix != Some(fix) || self.overland.way {
                self.overland.retarget(fix, false);
            }
        }
        // Into the room when its frames are seen, or the search from the fix's arrival.
        if let Some(t) = self.stronghold_task(w, tick) {
            return Some(t);
        }
        // A throw due here, or the eye of the last one followed and picked up (Q3).
        if let Some(t) = self.eyes_task(w, tick) {
            return Some(t);
        }
        // No fix yet: the walk to the spot the next throw is due at (the baseline for throw 2),
        // measured from the spot's own point when a nudge moved the walk off it.
        if self.fix.is_none()
            && let Some(p) = self.eyes_way(w)
        {
            let base = self.overland.way_base.or(self.overland.fix.map(|(q, _)| q));
            if !self.overland.way
                || base.is_none_or(|q| (q[0] - p[0]).hypot(q[1] - p[1]) > WAY_MOVED)
            {
                self.overland.retarget((p, 0.0), true);
                self.overland.way_base = None;
                self.overland.nudges = 0;
            }
            // At the spot with the throw waiting (a canopy or a cliff on the bearing, race
            // 45719263137 lane 1 idled 2,000 ticks): the spot moved `WAY_NUDGE` round the compass,
            // `WAY_NUDGES` times at most.
            if self.overland.way && self.overland.arrived {
                let since = *self.overland.wait_from.get_or_insert(tick);
                if tick.saturating_sub(since) > WAY_WAIT && self.overland.nudges < WAY_NUDGES {
                    let base = self.overland.way_base.unwrap_or(p);
                    let (dx, dz) = [(1.0, 0.0), (0.0, 1.0), (-1.0, 0.0), (0.0, -1.0)]
                        [self.overland.nudges as usize % 4];
                    let ring = f64::from(self.overland.nudges / 4 + 1) * WAY_NUDGE;
                    let to = [base[0] + dx * ring, base[1] + dz * ring];
                    let nudges = self.overland.nudges + 1;
                    self.overland.retarget((to, 0.0), true);
                    self.overland.way_base = Some(base);
                    self.overland.nudges = nudges;
                    self.note(
                        tick,
                        format!("overland throw spot moved ({nudges} of {WAY_NUDGES})"),
                        format!(
                            "the throw waited {} ticks at ({:.0}, {:.0}); to ({:.0}, {:.0})",
                            tick.saturating_sub(since),
                            base[0],
                            base[1],
                            to[0],
                            to[1]
                        ),
                    );
                } else if tick.saturating_sub(since) > WAY_WAIT
                    && self.overland.nudges >= WAY_NUDGES
                    && !self.eyes.force_throw
                {
                    // The moves spent and the throw still waiting: it goes anyway, through the
                    // column (`EyeState::force_throw`); a lost eye costs less than a stalled run.
                    self.eyes.force_throw = true;
                    self.note(
                        tick,
                        "overland throw forced".to_owned(),
                        format!(
                            "{WAY_NUDGES} spot moves spent and the throw waited {} ticks",
                            tick.saturating_sub(since)
                        ),
                    );
                }
            } else {
                self.overland.wait_from = None;
            }
        }
        if let Some(t) = self.overland_task(w, tick) {
            return Some(t);
        }
        // Eyes held in the overworld and nothing to do now (a throw that waits on the night or
        // a hostile): a look again, never on to the Nether task's portal below.
        if w.dimension
            .as_ref()
            .is_some_and(|d| d.name == stronghold::OVERWORLD)
        {
            self.retry_at = tick + RETRY_TICKS;
            return Some(Task::Idle);
        }
        None
    }
    // <<< overland and stronghold

    /// Under the engage health in the Nether: rest where no shooter in range sees the body and
    /// nothing burns; seen or burning, the nearest cell out of sight on a found route; with none,
    /// a held block into each cell a line of sight comes in through, until none does
    /// (`cover::shelter_block`, spawner wall 3). `None` when not hurt, or when none of it can be
    /// done.
    fn take_cover(&mut self, w: &World, tick: u64) -> Option<Task> {
        let rules = self.table.as_ref().map(|t| t.survival.clone())?;
        let body = w.body?;
        if w.player.health.is_none_or(|h| h >= rules.engage_health) {
            return None;
        }
        let burning = escape::touching_fire(w, body.pos);
        // A shelter block just placed: its block update is a tick or two away, and the lines of
        // sight are read again only once it is in.
        if !burning && tick < self.shelter_at {
            self.retry_at = self.shelter_at;
            return Some(Task::Idle);
        }
        // A rest that cannot heal is no rest: with no safe food held and the food bar under 18
        // nothing comes back, and two real bots sat out their last 7,400 and 7,900 ticks at 15.8
        // and 16.8 health, 27 blocks from the spawner, their meat eaten (race 45346418004). Over
        // the danger line the bot goes on with what it has; at or under it, it stays hidden.
        if !can_heal(w.player.food, eat::best_food(w, &rules).is_some())
            && w.player.health.is_some_and(|h| h > rules.danger_health)
        {
            if tick >= self.unhealed_at {
                self.unhealed_at = tick + FAILURE_MEMORY_TICKS;
                self.note(
                    tick,
                    "no rest: nothing to heal with".to_owned(),
                    format!(
                        "health {:?} under {}, food {:?} under {} and no safe food held; on with it",
                        w.player.health,
                        rules.engage_health,
                        w.player.food,
                        eat::HEALS_FROM
                    ),
                );
            }
            return None;
        }
        let eyes = cover::shooters(w);
        // A spawner within 16 blocks of a player is awake: resting there, the shooters multiply.
        let awake = cover::spawners_awake(w, body.pos);
        if !burning && awake.is_empty() && !cover::seen_body(w, &eyes, body.pos) {
            self.retry_at = tick + RETRY_TICKS;
            self.note(
                tick,
                "rest".to_owned(),
                format!(
                    "health {:?} under {}; out of sight of {} shooters within {}",
                    w.player.health,
                    rules.engage_health,
                    eyes.len(),
                    cover::COVER_RANGE
                ),
            );
            return Some(Task::Idle);
        }
        // Not the cell it stands in: seen there by where the body really is, a walk to it is over
        // at once and the shelter never starts.
        let feet = path::feet_cell(body.pos);
        // The search for a cover cell (every cell within 8, two lines from each shooter, and up to
        // two routes) found nothing a moment ago: not again for a second. Between shelter clicks
        // the task runs every tick, and it ran this search each time (review of bba8161: the
        // likely cause of a 128 ms planner step, race 45350933958 lane 1).
        let searched = tick < self.cover_none_until;
        let candidates = if searched {
            Vec::new()
        } else {
            cover::cover_cells(w, &eyes)
        };
        for c in candidates
            .into_iter()
            .filter(|c| *c != feet && !self.refused_now(*c, tick))
            .take(CANDIDATES_PER_TICK)
        {
            let mut walk = Walk::new(c, Goal::Block { pos: c }, Arrive::Goal, tick);
            if walk.route_now(w, tick) && walk.route == Some(path::Status::Found) {
                self.walks += 1;
                // Walked to and still seen there, it is not offered again for a while.
                self.refuse(c, tick);
                self.note(
                    tick,
                    format!("take cover at {c:?}"),
                    format!(
                        "health {:?}; {} shooters in range, burning {burning}",
                        w.player.health,
                        eyes.len()
                    ),
                );
                return Some(Task::Explore(walk));
            }
        }
        if !searched {
            self.cover_none_until = tick + COVER_SEARCH_TICKS;
        }
        // Inside a spawner's range with no cover cell beyond it: away from the spawner first, on
        // a found route, and the rest or the shelter is made out there.
        // The walk's goal is measured as routes are (octile plus height), which runs longer than
        // the straight line `awake_at` uses: from (128, 61, 176) a goal of 20 was already met
        // while the spawner was 17.2 off, and "leave the spawner" ended at once 300 times in 380
        // ticks, the bot standing in sight of three blazes until it died (race 45350452261 lane
        // 0). So the goal is always 2 past where the feet are, and a goal met at the feet is no
        // walk. Searched with the cover cells, at most once a second.
        if !searched && let Some(sp) = awake.first().copied() {
            let range = (cover::SPAWNER_QUIET + 2.0).max(path::flat_distance(feet, sp) + 2.0);
            let goal = Goal::Away { pos: sp, range };
            let mut walk = Walk::new(sp, goal, Arrive::Goal, tick);
            if !goal.done(feet)
                && walk.route_now(w, tick)
                && walk.route == Some(path::Status::Found)
            {
                self.walks += 1;
                self.note(
                    tick,
                    format!("leave the spawner at {sp:?} to heal"),
                    format!(
                        "health {:?}; a spawner within {} is awake and no cover cell beyond it; to {range:.1} by the route",
                        w.player.health,
                        cover::SPAWNER_QUIET
                    ),
                );
                return Some(Task::Explore(walk));
            }
        }
        // No cell out of sight to walk to (an open bridge): make one where the bot stands.
        if burning || !body.on_ground {
            return None;
        }
        let item = SHELTER_BLOCKS
            .iter()
            .copied()
            .find(|k| craft::count(w, *k) > 0)?;
        let (cell, click, face) = cover::shelter_block(w, &eyes, body.pos)?;
        if self.shelter_blocks >= SHELTER_MAX {
            return None;
        }
        // One click a look; a window open or a click not yet answered waits.
        self.retry_at = tick + 1;
        if w.window.is_some() || w.click.is_some() || !self.acts.is_empty() {
            return Some(Task::Idle);
        }
        match hold::toward(w, item) {
            hold::Hold::Held => {
                self.shelter_at = tick + SHELTER_EVERY_TICKS;
                self.shelter_blocks += 1;
                self.acts.push(world::Act::UseOn {
                    pos: world::BlockPos::new(click.0, click.1, click.2),
                    face,
                });
                self.note(
                    tick,
                    format!("shelter block {} at {cell:?}", self.shelter_blocks),
                    format!(
                        "health {:?}; seen by a shooter, no cover cell with a route; on {click:?} face {face}",
                        w.player.health
                    ),
                );
                Some(Task::Idle)
            }
            hold::Hold::Act(a) => {
                self.acts.push(a);
                Some(Task::Idle)
            }
            hold::Hold::Wait => Some(Task::Idle),
            hold::Hold::Missing => None,
        }
    }

    /// The cook's hut (overworld-leg wall 11): before the first smelt on the surface, the bot
    /// walls itself in with held blocks where it stands, beside its table and furnace, and roofs
    /// it over, a block a look; the cook then runs to dusk and after with no mob able to reach it.
    /// `None` once the hut is closed or cannot be built (noted once).
    fn hut_step(&mut self, w: &World, tick: u64) -> Option<Task> {
        if self.hut_done {
            return None;
        }
        let body = w.body?;
        let item = SHELTER_BLOCKS
            .iter()
            .copied()
            .find(|k| craft::count(w, *k) > 0);
        let next = cover::hut_block(w, body.pos);
        let end = match (item, next) {
            _ if self.hut_blocks == 0 && !world::stage::open_sky(w) => {
                Some("not under the open sky: no hut")
            }
            (_, None) => Some("hut closed"),
            (None, Some(_)) => Some("no block held for a hut"),
            _ if self.hut_blocks >= HUT_MAX => Some("hut given up"),
            _ => None,
        };
        if let Some(why) = end {
            self.hut_done = true;
            self.note(
                tick,
                format!("{why} ({} blocks placed)", self.hut_blocks),
                "the cook walled in before its first smelt".to_owned(),
            );
            return None;
        }
        let (item, (cell, click, face)) = item.zip(next)?;
        self.retry_at = tick + 1;
        // A window open under the hut (a furnace or table clicked by mistake): closed first.
        if w.window.is_some() && w.click.is_none() && self.acts.is_empty() {
            self.acts.push(world::Act::CloseWindow);
            return Some(Task::Idle);
        }
        if w.window.is_some()
            || w.click.is_some()
            || !self.acts.is_empty()
            || tick < self.shelter_at
        {
            return Some(Task::Idle);
        }
        match hold::toward(w, item) {
            hold::Hold::Held => {
                self.shelter_at = tick + SHELTER_EVERY_TICKS;
                self.hut_blocks += 1;
                self.acts.push(world::Act::UseOn {
                    pos: world::BlockPos::new(click.0, click.1, click.2),
                    face,
                });
                self.note(
                    tick,
                    format!("hut block {} at {cell:?}", self.hut_blocks),
                    format!("the cook walled in; on {click:?} face {face}"),
                );
                Some(Task::Idle)
            }
            hold::Hold::Act(a) => {
                self.acts.push(a);
                Some(Task::Idle)
            }
            hold::Hold::Wait => Some(Task::Idle),
            hold::Hold::Missing => {
                self.hut_done = true;
                None
            }
        }
    }

    /// Issue 16: from the fortress to its blaze spawner, and there until 7 rods are held. A spawner
    /// seen exposed is walked to on a found route, else tunnelled toward; with none seen, the nearest
    /// nether brick not yet stood near is (clojurecraft's `unvisited-fortress-bricks`), else explore.
    /// At the spawner the bot holds its ground: the `defend` row fights the blazes that come.
    fn spawner_task(&mut self, w: &World, tick: u64) -> Task {
        if craft::count(w, data::ItemKind::BlazeRod) >= RODS_WANTED {
            // Held: out of the blazes' reach before the aim is met, not stood among them (in the
            // whole-stage model two seeds held 7 rods at ticks 2,537 and 2,099 and died at 3,440
            // and 10,540, still at the spawner; the defend row leaves blazes alone from now).
            // The goal is by the route's measure, met at the feet is no walk (spawner wall 8).
            // Past a blaze's sight (48), and walked again when a meal or an escape cuts the walk:
            // with one walk and the aim met after it, the bots of the seed loop stood on the
            // bridge 14 to 30 blocks off, in sight, and burned there (probe 45355765767).
            if let Some(body) = w.body
                && let Some(sp) =
                    world::stage::kind_near(w, data::BlockKind::Spawner, FORTRESS_CHUNKS)
                        .first()
                        .copied()
                && self.leave_tries < LEAVE_TRIES
            {
                let feet = path::feet_cell(body.pos);
                let range = LEAVE_RANGE;
                let goal = Goal::Away { pos: sp, range };
                let mut walk = Walk::new(sp, goal, Arrive::Goal, tick);
                if !goal.done(feet)
                    && walk.route_now(w, tick)
                    && walk.route == Some(path::Status::Found)
                {
                    self.leave_tries += 1;
                    self.walks += 1;
                    self.note(
                        tick,
                        format!("rods held: away from the spawner at {sp:?}"),
                        format!("{RODS_WANTED} blaze rods held; to {range:.0} by the route"),
                    );
                    return Task::Explore(walk);
                }
            }
            // Issue 17: the race no longer ends at the rods; the endermen task takes over.
            self.ender.left = true;
            self.note(
                tick,
                "rods held: on to the endermen".to_owned(),
                format!(
                    "{RODS_WANTED} blaze rods held; the leave walk is over after {} walks",
                    self.leave_tries
                ),
            );
            return self.endermen_task(w, tick);
        }
        // Hurt: out of every blaze's sight, and rest there until the health is back; the eat row
        // feeds the heal (clojurecraft's heal and rest; blaze wall 3: resumed bots at 6 health
        // walked back to the spawner and died to its fireballs, 7 of 7).
        if let Some(t) = self.take_cover(w, tick) {
            return t;
        }
        // A rod lying near is picked up first (a kill drops one half the time).
        if let Some(body) = w.body {
            let rod = w
                .entities
                .values()
                .filter(|e| e.item().is_some_and(|(k, _)| k == data::ItemKind::BlazeRod))
                .map(|e| path::feet_cell(e.pos))
                .filter(|c| {
                    let d = [
                        f64::from(c.0) + 0.5 - body.pos[0],
                        f64::from(c.1) - body.pos[1],
                        f64::from(c.2) + 0.5 - body.pos[2],
                    ];
                    (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt() <= ROD_REACH
                })
                .find(|c| !self.refused_now(*c, tick));
            if let Some(c) = rod {
                let mut walk = Walk::new(c, Goal::Near { pos: c, range: 1.0 }, Arrive::Goal, tick);
                self.refuse(c, tick);
                if walk.route_now(w, tick) && walk.route == Some(path::Status::Found) {
                    self.walks += 1;
                    self.note(
                        tick,
                        format!("pick up a rod at {c:?}"),
                        "a blaze rod lying near".to_owned(),
                    );
                    return Task::Explore(walk);
                }
            }
        }
        // --- the fight post (docs/problems/blaze-fight.md wall 10; defend.rs owns the rule) ---
        // Healthy at the spawner with no fight on, the bot stands on the spawner's deck, where
        // every blaze spawns within a strike, not on the bridge under it.
        // Hurt with a way to heal, the post waits even when the cover found no cell, no way out
        // of the spawner's range and no shelter: at 12.7 health, the leave walk with no route, the
        // bot walked back onto the deck and was at 2.9 160 ticks later (the whole-stage model,
        // probe 45350488562, ticks 4,440 to 4,620). The eat row heals it where it stands.
        let hurt = self.table.as_ref().is_some_and(|t| {
            w.player.health.is_some_and(|h| {
                h < t.survival.engage_health
                    && can_heal(w.player.food, eat::best_food(w, &t.survival).is_some())
            })
        });
        if hurt && defend::spawner_near(w).is_some() {
            if tick >= self.unposted_at {
                self.unposted_at = tick + FAILURE_MEMORY_TICKS;
                self.note(
                    tick,
                    "no fight post: hurt, no cover found".to_owned(),
                    format!(
                        "health {:?}; it can heal, so it waits off the deck",
                        w.player.health
                    ),
                );
            }
            self.retry_at = tick + RETRY_TICKS;
            return Task::Idle;
        }
        if let Some(body) = w.body
            && let Some(sp) = defend::spawner_near(w)
            && !defend::on_post(path::feet_cell(body.pos), sp)
        {
            for c in defend::post_cells(w, sp, body.pos)
                .into_iter()
                .take(CANDIDATES_PER_TICK)
            {
                if self.refused_now(c, tick) {
                    continue;
                }
                let mut walk = Walk::new(c, Goal::Block { pos: c }, Arrive::Goal, tick);
                self.refuse(c, tick);
                if walk.route_now(w, tick) && walk.route == Some(path::Status::Found) {
                    self.walks += 1;
                    self.note(
                        tick,
                        format!("fight post at {c:?}"),
                        format!("the deck of the spawner at {sp:?}; no fight on"),
                    );
                    return Task::Explore(walk);
                }
            }
        }
        // --- end of the fight post ---
        if world::stage::at_spawner(w) {
            self.retry_at = tick + RETRY_TICKS;
            return Task::Idle;
        }
        let known = world::stage::kind_near(w, data::BlockKind::Spawner, FORTRESS_CHUNKS);
        if let Some(sp) = known.iter().copied().find(|c| !self.refused_now(*c, tick)) {
            let mut walk = Walk::new(
                sp,
                Goal::Near {
                    pos: sp,
                    range: world::stage::SPAWNER_NEAR - 2.0,
                },
                Arrive::Goal,
                tick,
            );
            if walk.route_now(w, tick) && walk.route == Some(path::Status::Found) {
                self.walks += 1;
                self.note(
                    tick,
                    format!("walk to the spawner at {sp:?}"),
                    "a blaze spawner seen exposed".to_owned(),
                );
                return Task::Explore(walk);
            }
            // Pushed back (a retreat from a skeleton took the bot 14 blocks down its own
            // tunnel): the dig's stall count is against the nearest it ever came, so three legs
            // back up the tunnel were three stalls and the spawner was refused (spawner wall 2,
            // 3 of 6 lanes). From well behind that mark the count starts again from here.
            if let Some((c, best, _)) = self.toward
                && c == sp
                && let Some(body) = w.body
                && pushed_back(body.pos, sp, best)
            {
                self.toward = None;
                self.note(
                    tick,
                    format!("pushed back from the spawner at {sp:?}"),
                    format!("the nearest was {best:.0} blocks; the dig's stalls count from here"),
                );
            }
            if let Some(t) = self.dig_toward(
                w,
                sp,
                true,
                ("the spawner", "a spawner seen, no walk to it"),
                tick,
            ) {
                return t;
            }
            // No plain route and no dig (no rock: a real bot stood at the foot of a brick pillar
            // over the lava sea, 68 blocks from this spawner, race 45337580375): one far search
            // (`Walk::far`, the Nether walk's: weighed, 40,000 expansions, bridging on the level
            // with held blocks). It comes after the tunnel: ahead of it, it left a tunnel with a
            // route of 150 steps for 47 cells of bridge in the open, laid two and stalled, in
            // three lanes of three (race 45343411709).
            if tick >= self.far_search_at {
                self.far_search_at = tick + walk::FAR_RETRY_TICKS;
                let mut far = Walk::far(sp, world::stage::SPAWNER_NEAR - 2.0, tick);
                let found = far.route_now(w, tick) && far.route == Some(path::Status::Found);
                let steps = far.waypoints.as_ref().map_or(0, Vec::len);
                if found {
                    self.far_search_at = tick + walk::FAR_AGAIN_TICKS;
                    self.walks += 1;
                    self.note(
                        tick,
                        format!("far walk toward the spawner at {sp:?}"),
                        format!(
                            "no plain route; {steps} cells, {} to bridge, found in {} expansions",
                            far.bridges_ahead(),
                            far.expanded
                        ),
                    );
                    return Task::Explore(far);
                }
                self.expanded += far.expanded;
                self.note(
                    tick,
                    format!("no far route to the spawner at {sp:?}"),
                    format!(
                        "{:?} after {} expansions; none again for {} ticks",
                        far.route,
                        far.expanded,
                        walk::FAR_RETRY_TICKS
                    ),
                );
            }
        }
        // A spawner known and no way on from here (spawner wall 2: a bot pushed back down its own
        // tunnel refused the spawner after three stalls, explored 60 blocks the other way and dug
        // two pickaxes away from there): back along the trail to the mark nearest the spawner,
        // and there wait out the refusal. Never an explore away from a spawner on record.
        if let Some((sp, body)) = known.first().copied().zip(w.body) {
            for m in trail_toward(&self.trail, body.pos, sp)
                .into_iter()
                .take(CANDIDATES_PER_TICK)
            {
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
                if walk.route_now(w, tick) && walk.route == Some(path::Status::Found) {
                    self.walks += 1;
                    self.note(
                        tick,
                        format!("back toward the spawner at {sp:?} by the trail mark {target:?}"),
                        "a spawner on record, no walk or dig to it from here".to_owned(),
                    );
                    return Task::Explore(walk);
                }
            }
            let for_good = self
                .refused
                .get(&sp)
                .is_some_and(|(until, _)| *until == u64::MAX);
            if !for_good {
                self.retry_at = tick + SPAWNER_WAIT_TICKS;
                self.note(
                    tick,
                    format!("wait for the spawner at {sp:?}"),
                    "refused a while, no trail mark nearer; not exploring away from it".to_owned(),
                );
                return Task::Idle;
            }
        }
        // No spawner seen: the fortress's corridors, the nearest brick not yet stood near first.
        let visited = |c: Cell| {
            self.trail.iter().any(|t| {
                let d = [
                    f64::from(c.0) + 0.5 - t[0],
                    f64::from(c.1) + 0.5 - t[1],
                    f64::from(c.2) + 0.5 - t[2],
                ];
                (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt() <= VISITED_NEAR
            })
        };
        let bricks: Vec<Cell> = world::stage::bricks_near(w, CORRIDOR_CHUNKS)
            .into_iter()
            .filter(|c| !visited(*c) && !self.refused_now(*c, tick))
            .take(CANDIDATES_PER_TICK)
            .collect();
        for c in &bricks {
            let mut walk = Walk::new(
                *c,
                Goal::Near {
                    pos: *c,
                    range: 2.0,
                },
                Arrive::Goal,
                tick,
            );
            self.refuse(*c, tick);
            if walk.route_now(w, tick) && walk.route == Some(path::Status::Found) {
                self.walks += 1;
                self.note(
                    tick,
                    format!("walk the fortress to {c:?}"),
                    "no spawner seen; a brick not yet stood near".to_owned(),
                );
                return Task::Explore(walk);
            }
        }
        if let Some(c) = bricks.first().copied()
            && let Some(t) = self.dig_toward(
                w,
                c,
                true,
                ("the fortress", "no spawner seen, no walk to its bricks"),
                tick,
            )
        {
            return t;
        }
        if let Some(t) = self.explore_walk(w, tick, "no spawner and no unvisited bricks in view") {
            return t;
        }
        self.failed_in_row += 1;
        self.retry_at = tick + RETRY_TICKS;
        self.check_stuck(tick);
        Task::Idle
    }

    /// Issue 15's aim: met once nether bricks have been seen with an exposed face; until then,
    /// explore (15a's baseline; the held-heading search is 15b).
    fn fortress_task(&mut self, w: &World, tick: u64) -> Task {
        // The race goes on from the fortress: a brick seen between the tick that sets the stage
        // and this choice must not end the run there (met ends a run with no hold).
        if self.aim == Aim::Race && world::stage::at_fortress(w) {
            return self.spawner_task(w, tick);
        }
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
        // --- A2: with no rock under the feet to tunnel into (a causeway over the sea, a ledge
        // with lava at its door) the far walk comes before the tunnel; on rock, after it.
        let rock = w
            .body
            .is_some_and(|b| walk::rock_to_tunnel(w, path::feet_cell(b.pos)));
        if !rock && let Some(t) = self.far_fortress_walk(w, tick) {
            return t;
        }
        // --- the end of A2's lines.
        // No walk: the travel tunnel to the brick held, else to the nearest: up out of the lava
        // sea's level, through rock, and down at the far end (docs/problems/nether-tunnel.md; the
        // stairs down this used to cut ended beside lava, blaze.md walls 22 and 23).
        let held = self
            .travel
            .target
            .filter(|c| !self.travel.exploring && !self.refused_now(*c, tick));
        if let Some(c) = held.or_else(|| {
            world::stage::bricks_near(w, FORTRESS_CHUNKS)
                .into_iter()
                .find(|c| !self.refused_now(*c, tick))
        }) {
            self.travel.exploring = false;
            if let Some(t) = self.travel_tunnel(
                w,
                c,
                ("the fortress", "nether bricks seen, no walk to them"),
                tick,
            ) {
                return t;
            }
        }
        // --- A2: on rock the tunnel did not take, the far walk.
        if rock && let Some(t) = self.far_fortress_walk(w, tick) {
            return t;
        }
        // --- the end of A2's lines.
        // No bricks to go to: in the Nether a search tunnel on a held heading first (bricks with
        // an open face are seen as their columns load, through rock; explore walks turn a quarter
        // a try and cross open ground among lava, ghasts and piglins), then a walk.
        if world::stage::in_nether(w)
            && let Some(t) = self.explore_tunnel(w, tick, "no fortress in view")
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

    /// A2's far walk (docs/problems/nether-walk.md): one weighed search toward the fortress,
    /// bridging where it must; found, its route is held by the walk and followed. Not found, no
    /// far search again for a while. The way over open lava, where there is no rock to tunnel.
    fn far_fortress_walk(&mut self, w: &World, tick: u64) -> Option<Task> {
        // The target: bricks of a fortress's body before a nearer lone pillar (the first far walks
        // met `fortress` at a pillar's foot in the lava sea, the fortress proper 70 blocks on).
        let far_target = (tick >= self.far_search_at)
            .then(|| world::stage::bricks_near(w, FORTRESS_CHUNKS))
            .and_then(|bricks| {
                walk::brick_in_the_body(&bricks).or_else(|| bricks.first().copied())
            });
        if let Some(c) = far_target {
            self.far_search_at = tick + walk::FAR_RETRY_TICKS;
            let mut walk = Walk::far(c, world::stage::FORTRESS_NEAR - 1.0, tick);
            let found = walk.route_now(w, tick) && walk.route == Some(path::Status::Found);
            let steps = walk.waypoints.as_ref().map_or(0, Vec::len);
            if found {
                // Cut (a meal, a foe) or failed, the next one may be searched soon.
                self.far_search_at = tick + walk::FAR_AGAIN_TICKS;
                self.walks += 1;
                self.note(
                    tick,
                    format!("far walk toward the fortress at {c:?}"),
                    format!(
                        "a route of {steps} cells, {} to bridge with {} blocks held, found in {} \
                         expansions",
                        walk.bridges_ahead(),
                        path::bridge_blocks(w),
                        walk.expanded
                    ),
                );
                return Some(Task::Explore(walk));
            }
            self.expanded += walk.expanded;
            self.note(
                tick,
                format!("no far route to the fortress at {c:?}"),
                format!(
                    "{:?} after {} expansions, {steps} cells; none again for {} ticks",
                    walk.route,
                    walk.expanded,
                    walk::FAR_RETRY_TICKS
                ),
            );
        }
        None
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
            | Aim::Base
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
                | Aim::Base
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
            if let Some(t) = self.stairs_to_log(w, log, tick) {
                return t;
            }
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

    /// The race's clean flow (issue 16, Bridger 2026-11-22: fix the flow from further back, then a
    /// clean snapshot). Twice the race stocks up above ground: before its first descent (logs,
    /// then food while animals are found), and once its kit is made (the gear, both buckets, flint
    /// and steel), when it first climbs back under the open sky. Each phase has a clock, so a
    /// treeless or empty place costs minutes, never the run. `Stage::Ready` reads the result.
    fn clean_flow(&mut self, w: &World, rules: &make::SurvivalRules, tick: u64) -> Flow {
        if rules.stock_logs <= 0 || self.readied {
            return Flow::On;
        }
        let n = |k: data::ItemKind| craft::count(w, k);
        let logs = logs_held(w);
        let food = eat::food_points(w, rules);
        let counts = held(w);
        let pieces = eat::meat_pieces(&counts, rules);
        let raw: i32 = eat::raw_held(&counts, rules).iter().map(|r| r.1).sum();
        // Something left for the cook phase: raw meat, or the kit's ingots still to smelt.
        let to_cook = cook_goal(&counts, rules).is_some();
        let kit = geared(w)
            && n(data::ItemKind::WaterBucket) >= 1
            && n(data::ItemKind::Bucket) >= 1
            && n(data::ItemKind::FlintAndSteel) >= 1;
        let first = !self.stocked && !geared(w);
        if !first && !kit {
            return Flow::On;
        }
        if !first {
            // The stage's own bar and the whole reserve (overworld-leg wall 4): a bot resumed
            // from a `ready` snapshot with less than the reserve hunts on the surface again, on
            // the clock below, before it goes down.
            if world::stage::Stage::Ready.holds(w)
                && if rules.meat_stock > 0 {
                    self.cook_over || (pieces >= rules.meat_stock && !to_cook)
                } else {
                    food >= rules.food_reserve
                }
            {
                self.readied = true;
                self.note(
                    tick,
                    "ready".to_owned(),
                    format!("the kit, {logs} logs and {food} food points held under the open sky"),
                );
                return Flow::On;
            }
            // Under its own hut's roof the bot is at the surface: no stairs up through it (wall
            // 11: a bot roofed itself in and cut stairs up onto its hut, race 45357148484).
            if !world::stage::open_sky(w) && self.hut_blocks == 0 {
                if let Some(t) = self.stairs_to_surface(w, tick) {
                    return Flow::Task(Box::new(t));
                }
                self.readied = true;
                self.note(
                    tick,
                    "not ready".to_owned(),
                    "no way up to the surface: on to the cast as it is".to_owned(),
                );
                return Flow::On;
            }
        }
        let since = *self.provision_since.get_or_insert(tick);
        let spent = tick.saturating_sub(since);
        let want = if first {
            rules.stock_logs
        } else {
            world::stage::READY_LOGS as i32
        };
        // The second stocking, with a meat stock set (overworld-leg wall 4): animals first, for
        // one `provision_ticks` (it begins with a dozen pieces held, and a bot resumed at noon
        // that looked for three would still be hunting after dark); then all the raw meat cooked;
        // then the logs the cook burned.
        if !first && rules.meat_stock > 0 {
            if !self.hunt_over && self.failed_in_row >= HUNT_WALK_FAILS {
                self.hunt_over = true;
                self.note(
                    tick,
                    "the look for animals is over".to_owned(),
                    format!(
                        "{} walks failed running; {pieces} of {} pieces held after {spent} ticks",
                        self.failed_in_row, rules.meat_stock
                    ),
                );
            }
            // Dusk near: no more walks out for animals or trees (wall 16: lane 0 of race
            // 45349387912 walked to trees after dark and a creeper killed it).
            let dusk = self.dusk_near(w, tick);
            if pieces < rules.meat_stock
                && spent < rules.provision_ticks
                && !self.hunt_over
                && !dusk
            {
                let why = format!(
                    "provisions {pieces} of {} pieces: look for animals",
                    rules.meat_stock
                );
                if let Some(t) = self.explore_walk(w, tick, &why) {
                    return Flow::Task(Box::new(t));
                }
            }
            if to_cook && !self.cook_over {
                return Flow::Cook;
            }
            if !self.cook_over {
                self.cook_over = true;
                self.note(
                    tick,
                    "cooked".to_owned(),
                    format!(
                        "{pieces} pieces and {food} food points held, {raw} raw, {} iron ingots, {} shields, after {} cook tasks",
                        craft::count(w, data::ItemKind::IronIngot),
                        craft::count(w, data::ItemKind::Shield),
                        self.cook_tasks
                    ),
                );
                // The spare pickaxes first, at the cook's table (wall 15: a bot took its table
                // along, lost it in a dig, had no wood for another after the shields, and dug
                // 10,000 ticks by hand with both pickaxes broken, race 45357148484): the spares'
                // row takes the table along after.
                let spares = fresh_pickaxes(w, rules.spare_uses) >= rules.spare_pickaxes;
                if spares && let Some(t) = self.carry_table(w, tick) {
                    return Flow::Task(Box::new(t));
                }
            }
            // The logs were for the shields' planks: with the shields made at the cook site, none
            // are gathered (wall 10: a bot cooked until dusk, walked to far trees after dark for
            // its four logs, and a creeper killed it there).
            let shields_made = rules.kit_shields > 0
                && craft::count(w, data::ItemKind::Shield) >= rules.kit_shields;
            if logs < want && spent < rules.provision_ticks * 4 && !shields_made && !dusk {
                return Flow::Stock;
            }
            self.provision_since = None;
            self.readied = true;
            self.note(
                tick,
                "stocked again".to_owned(),
                format!(
                    "{logs} logs, {pieces} pieces of meat and {food} food points held after {spent} ticks"
                ),
            );
            return Flow::On;
        }
        if logs < want && spent < rules.provision_ticks * 2 {
            return Flow::Stock;
        }
        // The second stocking looks for one `provision_ticks` only: it begins with most of the
        // reserve held, and a bot resumed at noon that looked for three would still be on the
        // surface after dark.
        let look = rules.provision_ticks * if first { 3 } else { 1 };
        if food < rules.food_reserve && spent < look {
            let why = format!(
                "provisions {food} of {}: look for animals",
                rules.food_reserve
            );
            if let Some(t) = self.explore_walk(w, tick, &why) {
                return Flow::Task(Box::new(t));
            }
        }
        self.provision_since = None;
        if first {
            self.stocked = true;
        } else {
            self.readied = true;
        }
        self.note(
            tick,
            if first { "stocked" } else { "stocked again" }.to_owned(),
            format!("{logs} logs and {food} food points held after {spent} ticks"),
        );
        Flow::On
    }

    /// Whether dusk is within `DUSK_MARGIN` ticks (or night has fallen), by the server's day
    /// clock; noted once a day when it first stops the surface walks.
    fn dusk_near(&mut self, w: &World, tick: u64) -> bool {
        let Some(t) = w.day_time() else {
            return false;
        };
        let left = world::time::ticks_to_dusk(t);
        let near = left < DUSK_MARGIN;
        if near && !self.dusk_noted {
            self.dusk_noted = true;
            self.note(
                tick,
                "dusk near: no walk out".to_owned(),
                format!("day time {t}, {left} ticks to dusk: the surface walks for animals and trees stop"),
            );
        }
        near
    }

    /// Stairs up toward the open sky, 24 blocks at a time along one heading; a heading refused
    /// (three stalls, or its stairs cut into water) gives way to the next, and eight refusals
    /// end the climb.
    fn stairs_to_surface(&mut self, w: &World, tick: u64) -> Option<Task> {
        let (fx, fy, fz) = path::feet_cell(w.body?.pos);
        for _ in 0..2 {
            let live = self
                .surface_target
                .filter(|t| !self.refused_now(*t, tick) && fy < t.1 - 1);
            let target = match live {
                Some(t) => t,
                None => {
                    if self
                        .surface_target
                        .is_some_and(|t| self.refused_now(t, tick))
                    {
                        self.surface_tries += 1;
                    }
                    if self.surface_tries >= SURFACE_TRIES {
                        return None;
                    }
                    let (dx, dz) =
                        [(1, 0), (0, 1), (-1, 0), (0, -1)][usize::from(self.surface_tries % 4)];
                    let t = (
                        fx + dx * SURFACE_RISE,
                        fy + SURFACE_RISE,
                        fz + dz * SURFACE_RISE,
                    );
                    self.surface_target = Some(t);
                    t
                }
            };
            if let Some(t) = self.dig_toward(
                w,
                target,
                true,
                (
                    "the surface",
                    "the kit is made: up under the open sky before the cast",
                ),
                tick,
            ) {
                return Some(t);
            }
            self.refuse(target, tick);
        }
        None
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
        // The clean flow first (issue 16): stock up above ground before the first descent, and
        // come back up, stocked, before the cast.
        let mut stock_row = false;
        let mut cook_row = false;
        if self.home_cook {
            cook_row = true;
        } else if self.aim == Aim::Race && !self.kit_row {
            match self.clean_flow(w, &table.survival, tick) {
                Flow::Task(t) => return *t,
                Flow::Stock => stock_row = true,
                Flow::Cook => cook_row = true,
                Flow::On => {}
            }
        }
        // Spare pickaxes ahead of need (overworld-leg wall 2).
        let spare_row = !stock_row && !cook_row && self.spares_wanted(w, &table.survival, tick);
        // The nether aim casts with the portal row until a portal is seen (issue 15c).
        let row = match self.aim {
            // A portal stands and the bot is about to go in: the tools for the Nether first.
            _ if self.shield_only => shield::kit_row(&held(w)).unwrap_or("shield").to_owned(),
            _ if self.kit_row => "nether-kit".to_owned(),
            Aim::Race if stock_row => "stock".to_owned(),
            // The race gears up first (issue 16), then casts.
            Aim::Race if !geared(w) => "gear".to_owned(),
            Aim::Race if spare_row => "nether-kit".to_owned(),
            // No pickaxe, but the makings of one held: craft it before casting or digging to a
            // site, never a trip for wood (clojurecraft's `pickaxe-at-hand?`; blaze wall 8).
            Aim::Portal | Aim::Nether | Aim::Race
                if !holds_pickaxe(w) && self.pickaxe_at_hand(w, tick) =>
            {
                "stone-pickaxe".to_owned()
            }
            Aim::Nether | Aim::Race => "portal".to_owned(),
            _ => self.aim.name(),
        };
        let cook_goal = if cook_row {
            cook_goal(&held(w), &table.survival)
        } else {
            None
        };
        let Some(goal) = cook_goal.as_ref().or_else(|| table.until(&row)) else {
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
        // Stone over the junk keep, with a table at hand: eight pressed into a furnace, a slot
        // for 512, instead of a stack thrown down and picked up again 40 ticks later (wall 12:
        // a bot idled 14,700 ticks on one pile, race 45354021804 lane 1).
        if self.aim == Aim::Race
            && let Some(t) = self.at_hand(w, data::BlockKind::CraftingTable, tick)
            && let Some(r) = press_recipe(w, &table.survival)
        {
            self.pressed += 1;
            self.note(
                tick,
                format!("press stone into a furnace at the table {t:?}"),
                format!(
                    "{} stone held over the junk keep, {} pressed so far",
                    stone_over_keep(w, &table.survival),
                    self.pressed
                ),
            );
            return Task::Craft(Craft::at_table(r, t, tick));
        }
        // A stack of stone over the keep and the table in the pack: it is put down to press at,
        // and taken along again once the stone is pressed (wall 12).
        if self.aim == Aim::Race && !cook_row {
            let over = stone_over_keep(w, &table.survival);
            let carried = craft::count(w, data::ItemKind::CraftingTable) > 0;
            if over >= PRESS_AT
                && carried
                && press_recipe(w, &table.survival).is_some()
                && let Some(p) = Place::new(data::BlockKind::CraftingTable, tick)
            {
                self.press_table = true;
                self.note(
                    tick,
                    "place CraftingTable to press stone".to_owned(),
                    format!("{over} stone held over the junk keep"),
                );
                return Task::Place(p);
            }
            if self.press_table && press_recipe(w, &table.survival).is_none() {
                self.press_table = false;
                if let Some(t) = self.carry_table(w, tick) {
                    return t;
                }
            }
        }
        // Taken up front: the block needs are a crafting table and a furnace. The spares and the
        // Nether kit are made where the bot stands: a table remembered 44 blocks off, down the
        // old stairs in a cave, walked a bot to it for 600 ticks and it died there (race
        // 45337088346 lane 0).
        let table_cell = if spare_row || self.kit_row || cook_row {
            self.at_hand(w, data::BlockKind::CraftingTable, tick)
        } else {
            self.near_block(w, data::BlockKind::CraftingTable, tick)
        };
        let table_near = table_cell.is_some();
        // The cook's furnace too is one where the bot stands: the one remembered is down a mine.
        let furnace_cell = if cook_row {
            self.at_hand(w, data::BlockKind::Furnace, tick)
        } else {
            self.near_block(w, data::BlockKind::Furnace, tick)
        };
        let furnace_near = furnace_cell.is_some();
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
        let wear = self.wear(w, &table.survival, table_near);
        if wear.low_on_digs && !self.low_digs_noted {
            self.low_digs_noted = true;
            self.note(
                tick,
                "low on digs".to_owned(),
                format!(
                    "{} digs held, under the travel reserve {}: a spare's gather is worth it from anywhere",
                    dig_budget(w, table_near),
                    table.survival.travel_reserve_digs
                ),
            );
        }
        let choice = make::make_with_wear(&table, &counts, &near, gather_now, &wear, goal);
        // >>> shield (docs/problems/shield.md): the kit's tools held, the shields the held iron
        // and wood make are made next; an act that would be a trip (a gather, a mine) skips them.
        let (goal, choice) = match shield::kit_row(&counts).and_then(|r| table.until(r)) {
            Some(row) if self.kit_row && (self.shield_only || choice == make::Choice::Done) => {
                match make::make_with_wear(&table, &counts, &near, gather_now, &wear, row) {
                    make::Choice::Act(r) if shield::kit_act(r) => (row, make::Choice::Act(r)),
                    other => {
                        let why = match other {
                            make::Choice::Act(r) => format!("{} would be a trip", r.id),
                            make::Choice::Done => "held".to_owned(),
                            make::Choice::Stuck => "not makeable where the bot stands".to_owned(),
                        };
                        self.note(tick, format!("shields skipped: {why}"), row.id.clone());
                        if self.shield_only {
                            self.shield_done = true;
                            return Task::Idle;
                        }
                        (goal, choice)
                    }
                }
            }
            _ if self.shield_only => {
                self.shield_done = true;
                return Task::Idle;
            }
            _ => (goal, choice),
        };
        // <<< shield
        if matches!(choice, make::Choice::Act(_))
            && let Some(t) = self.opportunity(w, &table, goal, &near, gather_now, tick)
        {
            return t;
        }
        // The cook phase takes only what can be done where the bot stands: a craft, a placing,
        // a smelt, or a log gathered for fuel; never a mine. It is bounded by its tasks.
        if cook_row {
            // The first smelt waits for the hut (wall 11): table and furnace placed, the fuel
            // made, the bot walls itself in before the long cook.
            if let make::Choice::Act(r) = &choice
                && matches!(r.act, Some(make::Act::Smelt(_)))
                && let Some(t) = self.hut_step(w, tick)
            {
                return t;
            }
            let ok = matches!(
                &choice,
                make::Choice::Act(r)
                    if matches!(
                        r.act,
                        Some(
                            make::Act::Craft(_)
                                | make::Act::Place(_)
                                | make::Act::Smelt(_)
                                | make::Act::Gather
                        )
                    )
            );
            self.cook_tasks += 1;
            if !ok || self.cook_tasks > COOK_TASKS {
                self.cook_over = true;
                self.note(
                    tick,
                    "cooking given up".to_owned(),
                    format!(
                        "{} tasks begun; the next step toward {} was {}",
                        self.cook_tasks,
                        goal.id,
                        match &choice {
                            make::Choice::Act(r) => r.id.clone(),
                            make::Choice::Done => "done".to_owned(),
                            make::Choice::Stuck => "stuck".to_owned(),
                        }
                    ),
                );
                return Task::Idle;
            }
            if let make::Choice::Act(r) = &choice
                && let Some(make::Act::Smelt(s)) = r.act
            {
                self.note(
                    tick,
                    format!("cook {}", s.id),
                    format!(
                        "{} raw pieces held, {} food points; a batch of all of this kind",
                        eat::raw_held(&counts, &table.survival)
                            .iter()
                            .map(|r| r.1)
                            .sum::<i32>(),
                        eat::food_points(w, &table.survival)
                    ),
                );
            }
            // The shields at the cook's table (wall 8): each step says so.
            if goal.id == "cook-shield"
                && let make::Choice::Act(r) = &choice
            {
                self.note(
                    tick,
                    format!("shields at the cook site: {}", r.id),
                    format!(
                        "{} of {} shields held, {} iron ingots",
                        craft::count(w, data::ItemKind::Shield),
                        table.survival.kit_shields,
                        craft::count(w, data::ItemKind::IronIngot)
                    ),
                );
            }
        }
        // The spare pickaxes end here: the row's pickaxes held, or its next step no craft (a trip
        // for wood is never made for a spare). The table comes along.
        if spare_row {
            let fresh = fresh_pickaxes(w, table.survival.spare_uses);
            let pick: make::Counts = [(data::ItemKind::StonePickaxe, 1)].into_iter().collect();
            let want = goal
                .provides
                .iter()
                .find(|(n, _)| n.have(&pick) > 0)
                .map_or(0, |(_, n)| *n);
            let crafts = matches!(
                &choice,
                make::Choice::Act(r)
                    if matches!(r.act, Some(make::Act::Craft(_) | make::Act::Place(_)))
            );
            if fresh >= want || !crafts {
                self.sparing = false;
                if fresh < table.survival.spare_pickaxes {
                    self.spares_retry_at = tick + SPARES_RETRY_TICKS;
                }
                self.note(
                    tick,
                    if fresh >= want {
                        "spare pickaxes held".to_owned()
                    } else {
                        "spare pickaxes not makeable".to_owned()
                    },
                    format!("{fresh} of {want} stone pickaxes with uses to spare"),
                );
                return self.carry_table(w, tick).unwrap_or(Task::Idle);
            }
        }
        // The portal (issue 14c): its kit held, the cast; met once a portal is lit.
        // The Nether kit ends here: made, or not makeable (no wood), the bot goes in either way.
        if self.kit_row && !matches!(choice, make::Choice::Act(_)) {
            self.kit_done = true;
            self.note(
                tick,
                if choice == make::Choice::Done {
                    "nether kit held".to_owned()
                } else {
                    "nether kit not makeable".to_owned()
                },
                format!(
                    "{} pickaxes and {} tables held before the portal",
                    craft::count(w, data::ItemKind::StonePickaxe),
                    craft::count(w, data::ItemKind::CraftingTable)
                ),
            );
            return Task::Idle;
        }
        if matches!(self.aim, Aim::Portal | Aim::Nether | Aim::Race)
            && !self.kit_row
            && !spare_row
            && !cook_row
        {
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
                    let Some(t) = table_cell else {
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
                    let Some(f) = furnace_cell else {
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
                    // A cook's batch is what the first fuel held burns for: the furnace takes one
                    // kind of fuel, and a batch needing two kinds of planks loaded the one, took
                    // it out again for the other, and failed `TooManyClicks` 24 times in race
                    // 45343519424 (overworld-leg wall 4).
                    let count = if cook_row {
                        let first = smelt::fuels_held(w).first().copied();
                        let held = first.map_or(0, |(k, _)| craft::count(w, k));
                        // No more than the goal is short of (three ingots, not all the ore).
                        let short = goal
                            .provides
                            .first()
                            .map_or(count, |(_, n)| n - craft::count(w, r.result));
                        fuel_batch(
                            first.map(|(_, t)| t),
                            held,
                            r.ticks,
                            count.min(short.max(1)),
                        )
                    } else {
                        count
                    };
                    // >>> shield: a shield's smelt cooks the ingots it is short of and no more;
                    // the whole batch would burn the planks the shield is made of.
                    let count =
                        shield::smelt_count(&goal.id, &counts).map_or(count, |n| count.min(n));
                    // <<< shield
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
        // Water above the feet takes stairs up (blaze.md wall 5: 11 of 13 geared bots held a
        // bucket under a pool 20 blocks up and walked at it for three hours, refused here).
        self.dig_toward(
            w,
            bank,
            true,
            ("the bank", "water seen with no walk to a bank"),
            tick,
        )
    }

    /// Stairs up toward a log seen well over the feet with no walk to it, from under a rock
    /// roof: a bot in a cave with the trees 40 blocks up explored level for three hours
    /// (blaze.md wall 6). Under the open sky (a tall tree up a hill) the old way stands.
    fn stairs_to_log(&mut self, w: &World, log: Cell, tick: u64) -> Option<Task> {
        if !roofed_below(w, log.1) {
            return None;
        }
        self.dig_toward(
            w,
            log,
            true,
            ("the log", "a log over the feet with no walk to it"),
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
        // In the Nether a far target gets the travel tunnel: a searched way through rock, over
        // the lava sea's level, that comes to the target's own height at its end, where the cut
        // below goes six down for a target one lower and six up for one two higher (lane 14 of
        // race 45164371770 sawed between y 22 and y 28 under the lava sea:
        // docs/problems/nether-tunnel.md).
        let flat = (bank.0 - fx).abs() + (bank.2 - fz).abs();
        if above_ok && world::stage::in_nether(w) && flat > TUNNEL_FROM {
            return self.tunnel_toward(w, bank, (what, why), tick);
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
        // More than a block over the feet: stairs up (issue 16b, blaze wall 1).
        let up = above_ok && bank.1 > fy + 1;
        let dir = match (dx.abs() >= dz.abs(), dx, dz) {
            // Right over or under it: stairs need a heading; any one comes round to it.
            (_, 0, 0) if down || up => (1, 0),
            (_, 0, 0) => return None,
            (true, _, _) => (dx.signum(), 0),
            (false, _, _) => (0, dz.signum()),
        };
        self.note(
            tick,
            format!(
                "{} toward {what} at {bank:?}",
                if down {
                    "stairs down"
                } else if up {
                    "stairs up"
                } else {
                    "tunnel"
                }
            ),
            why.to_owned(),
        );
        Some(Task::Stairs(Box::new(if down {
            Stairs::new(dir, tick)
        } else if up {
            Stairs::up(dir, tick)
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
            // From under rock, a pool well over the feet is cut to by stairs up to its bank
            // before any walk: four of seven geared cave bots (race 45046883124) walked at the
            // same three pools 20 to 57 blocks up 96 to 174 times each, every walk failing
            // NoPath, and never dug (blaze.md wall 9). Three stalls refuse the bank as before.
            if roofed_below(w, c.1 + 1)
                && let Some(t) = self.stairs_to_bank(w, c, tick)
            {
                return t;
            }
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
        // A source the eye is already within a bucket's reach of is no walk's target: the walk
        // ends at once and the fetch, due again, finds it refused and gives the site up in the
        // same tick (cast.md wall 2: "walk toward lava to fetch" then "no lava to fetch" at
        // tick 29,920, 286 sources seen).
        let eye = w
            .body
            .map(|b| [b.pos[0], b.pos[1] + physics::EYE_HEIGHT, b.pos[2]]);
        let near = |c: &Cell| {
            eye.is_some_and(|e| {
                let p = [
                    f64::from(c.0) + 0.5,
                    f64::from(c.1) + 0.5,
                    f64::from(c.2) + 0.5,
                ];
                (0..3).map(|i| (e[i] - p[i]).powi(2)).sum::<f64>().sqrt() <= FETCH_NEAR
            })
        };
        // Taken before the scoops refuse what they try: a source no scoop routes to now may be
        // walked toward, where more of its pool shows.
        let (close, far): (Vec<Cell>, Vec<Cell>) = cast::lava_within(w, cast::FAR_RADIUS)
            .into_iter()
            .filter(|c| !ours(*c) && !self.refused_now(*c, tick))
            .partition(near);
        let skipped = close.len();
        let far: Vec<Cell> = far.into_iter().take(LAVA_FETCH_TRIES).collect();
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
                        "no scoop routes; walk {} of {LAVA_FETCH_WALKS}; {skipped} sources within reach skipped",
                        self.fetch_walks
                    ),
                );
                return Some(Task::Explore(walk));
            }
        }
        None
    }

    /// A block of `kind` (a crafting table, a furnace) remembered within `TABLE_AT_HAND` of the
    /// eye: one to work at with no walk to speak of.
    fn at_hand(&self, w: &World, kind: data::BlockKind, tick: u64) -> Option<Cell> {
        let t = self.near_block(w, kind, tick)?;
        let b = w.body?;
        let eye = [b.pos[0], b.pos[1] + physics::EYE_HEIGHT, b.pos[2]];
        let c = [
            f64::from(t.0) + 0.5,
            f64::from(t.1) + 0.5,
            f64::from(t.2) + 0.5,
        ];
        let d = (0..3).map(|i| (eye[i] - c[i]).powi(2)).sum::<f64>().sqrt();
        (d <= TABLE_AT_HAND).then_some(t)
    }

    /// Whether the race makes spare pickaxes now (overworld-leg wall 2): from `ready` on, in the
    /// overworld, before a portal is lit, with fewer than `spare_pickaxes` stone pickaxes with
    /// uses to spare and the makings of one in the pack. Once begun it holds until the row's
    /// pickaxes are held. Nine bots of races 45194253932 and 45204423376 spent 1,940 to 58,960
    /// ticks with no pickaxe, 21 to 25 sticks and a table in the pack.
    fn spares_wanted(&mut self, w: &World, rules: &make::SurvivalRules, tick: u64) -> bool {
        let overworld = w
            .dimension
            .as_ref()
            .is_some_and(|d| d.name == "minecraft:overworld");
        if self.aim != Aim::Race
            || !self.readied
            || self.kit_row
            || self.portal.is_some()
            || !overworld
            || tick < self.spares_retry_at
        {
            self.sparing = false;
            return false;
        }
        if self.sparing {
            return true;
        }
        let table_near = self
            .at_hand(w, data::BlockKind::CraftingTable, tick)
            .is_some();
        if !spares_due(w, rules, table_near) {
            return false;
        }
        self.sparing = true;
        self.note(
            tick,
            "spare pickaxes".to_owned(),
            format!(
                "{} stone pickaxes with {} uses or more held, under {}: made now, from the pack",
                fresh_pickaxes(w, rules.spare_uses),
                rules.spare_uses,
                rules.spare_pickaxes
            ),
        );
        true
    }

    /// The lava hunt's own stairs (overworld-leg wall 1): six steps down in rock, toward `pool`
    /// when one is known and not yet under the feet, else the next heading of a spiral; never
    /// below `SEA_FEET`. No terrain is read to choose the heading (strict no X-ray): each stair's
    /// own refusal turns it. Seven of sixteen bots died in the caves the hunt's walks led through
    /// before any cast.
    fn own_stairs(&mut self, feet: Cell, pool: Option<Cell>, tick: u64) -> Task {
        let mut dir = own_heading(feet, pool, self.descent_legs);
        // Walked back from a ledge (wall 9): the heading that led there is not taken again.
        let back = self.own_back.filter(|(c, _)| *c == feet);
        if let Some((_, bad)) = back
            && dir == bad
        {
            dir = (-bad.1, bad.0);
            self.note(
                tick,
                format!("own stairs turned from {bad:?} to {dir:?}"),
                format!("walked back to {feet:?} from a ledge that heading led to"),
            );
        }
        self.own_leg = Some((feet, dir));
        self.descent_legs += 1;
        let steps = u32::try_from(feet.1 - SEA_FEET)
            .unwrap_or(0)
            .min(stairs::STAIR_DEPTH);
        self.note(
            tick,
            format!(
                "own stairs down, leg {} heading {dir:?}, {steps} steps",
                self.descent_legs
            ),
            format!(
                "no site, the feet at y {} over {}: stairs in rock, no walk through caves",
                feet.1,
                pool.map_or_else(
                    || format!("the lava sea (feet y {SEA_FEET})"),
                    |p| format!("a pool of ten at {p:?}")
                )
            ),
        );
        let mut st = Stairs::new(dir, tick);
        // The last leg stops at the sea's level: the count starts short of a full leg.
        st.stairs = stairs::STAIR_DEPTH - steps;
        // Never a stair cut beside water (wall 7: a leg at y 55 let an aquifer in and the bot
        // was held in it to the cap).
        st.dry = true;
        Task::Stairs(Box::new(st))
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
            let (best, stalls) = landing_stalls(&mut self.landings, aim, d);
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
                    // More than a block over the feet: stairs up, as `dig_toward` has them. A
                    // level tunnel never rose to a landing four over the feet (wall 19).
                    return Task::Stairs(Box::new(if aim.1 < fy {
                        Stairs::new(dir, tick)
                    } else if aim.1 > fy + 1 {
                        Stairs::up(dir, tick)
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
        // No site, and over the lava sea's level: down by its own stairs (overworld-leg wall 1),
        // unless a pool of ten sources is known near the feet's height, which is walked to as
        // before. A far pool of fewer is one the site choice refuses on arrival (`spent`).
        // A far lava target the hunt comes no nearer to is given up (wall 13, a progress
        // invariant: lane 0 of race 45354021804 circled a pool of ten at (-30, 46, -254) for 8,000
        // ticks, its walks failing and its stairs re-walking cut steps).
        if let Some(body) = w.body
            && let Some(aim) = cast::far_pool(w, &self.given_up_sites)
                .or_else(|| cast::far_lava(w, &self.given_up_sites))
        {
            let (fx, fy, fz) = path::feet_cell(body.pos);
            let d =
                f64::from((aim.0 - fx).pow(2) + (aim.1 - fy).pow(2) + (aim.2 - fz).pow(2)).sqrt();
            let (best, stalls) = landing_stalls(&mut self.lava_aims, aim, d);
            if stalls >= LAVA_AIM_STALLS {
                self.lava_aims.retain(|(c, _, _)| *c != aim);
                self.given_up_sites.push(aim);
                self.note(
                    tick,
                    format!("lava at {aim:?} given up"),
                    format!("{stalls} looks came no nearer than {best:.1}: another target"),
                );
            }
        }
        if let Some(body) = w.body {
            let feet = path::feet_cell(body.pos);
            let pool = cast::far_pool(w, &self.given_up_sites);
            let overworld = w
                .dimension
                .as_ref()
                .is_some_and(|d| d.name == "minecraft:overworld");
            // Legs begun from this same cell: two that left the feet where they were (every
            // heading refused: a ledge over a cave, its floors air) and the old hunt takes over
            // from here, a walk where one goes.
            let legs_here = match self.own_from {
                Some((c, n)) if c == feet => n + 1,
                Some(_) | None => 0,
            };
            if overworld && feet.1 > SEA_FEET && pool.is_none_or(|p| feet.1 - p.1 > DESCENT_WALK) {
                self.own_from = Some((feet, legs_here));
                if legs_here == 0 {
                    // A new cell: the leg that led here, kept for a walk back from it.
                    // Not at a cell walked back to: the leg into it is the ledge's.
                    self.own_into = self
                        .own_leg
                        .filter(|(c, _)| *c != feet)
                        .filter(|_| self.own_back.is_none_or(|(b, _)| b != feet));
                }
                if legs_here < OWN_LEGS_STUCK {
                    return self.own_stairs(feet, pool, tick);
                }
                // Boxed on a ledge (wall 9: every heading's floor air over a cave, the old hunt's
                // stairs boxed too, relocations spent on walks with no path): back up the leg's
                // own stairs to where it began, once, and from there another heading.
                if let Some((start, dir)) = self.own_into.take()
                    && !self.refused_now(start, tick)
                {
                    let mut walk = Walk::new(start, Goal::Block { pos: start }, Arrive::Goal, tick);
                    if walk.route_now(w, tick) && walk.route == Some(path::Status::Found) {
                        self.own_back = Some((start, dir));
                        self.own_from = None;
                        self.walks += 1;
                        self.note(
                            tick,
                            format!("own stairs back to {start:?}"),
                            format!(
                                "{legs_here} legs begun from {feet:?} left the feet there: back up the leg that came {dir:?}, another heading from there"
                            ),
                        );
                        return Task::Explore(walk);
                    }
                    self.refuse(start, tick);
                }
                if legs_here == OWN_LEGS_STUCK {
                    self.note(
                        tick,
                        "own stairs make no way here".to_owned(),
                        format!("{legs_here} legs begun from {feet:?} left the feet there: the old hunt from here"),
                    );
                }
            }
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
                    // A travel tunnel's laid blocks and straight-steered steps, counted here.
                    let extra = if st.laid + st.steered > 0 {
                        format!(
                            " ({} blocks laid, {} steps steered straight)",
                            st.laid, st.steered
                        )
                    } else {
                        String::new()
                    };
                    self.note(
                        tick,
                        format!("cut {} stairs{extra}{}", st.stairs, water_turns(&st)),
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
                        format!(
                            "stairs failed: {f:?} after {} stairs{}",
                            st.stairs,
                            water_turns(&st)
                        ),
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

/// Whether a log at height `log_y` with no walk to it is worth stairs up from feet at `feet_y`:
/// more than 3 over the feet (nearer is a ledge or a bank, explored round), with `roof`, the
/// first block over the head, rock rather than open sky or a tree (blaze.md wall 6).
/// The bot stands under rock with `top_y` more than 3 over its feet: the first block over the
/// head that is not air (its underside faces the bot's air, so it is seen: no X-ray) is not a
/// tree. Then a log or a pool up there is cut to by stairs, not walked at (blaze walls 6 and 9).
fn roofed_below(w: &World, top_y: i32) -> bool {
    let Some(body) = w.body else {
        return false;
    };
    let (fx, fy, fz) = path::feet_cell(body.pos);
    let roof = ((fy + 2)..top_y)
        .map(|y| w.block_at(world::BlockPos::new(fx, y, fz)))
        .find(|k| !matches!(k, Some(data::BlockKind::Air | data::BlockKind::CaveAir)))
        .flatten();
    climb_to_log(fy, top_y, roof)
}

fn climb_to_log(feet_y: i32, log_y: i32, roof: Option<data::BlockKind>) -> bool {
    let tree = |k: data::BlockKind| {
        ["leaves", "logs"]
            .iter()
            .filter_map(|t| block_tag(t))
            .any(|ks| ks.contains(&k))
    };
    log_y > feet_y + 3 && roof.is_some_and(|k| !tree(k))
}

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

/// The feet height the lava hunt's own stairs stop at: vanilla fills every cave below y -54 with
/// lava, so y -54 is the lowest air and a tunnel there opens onto the lakes at their own level,
/// with no sea lava beside or over it (the sites the bots of races 45194253932 and 45204423376
/// cast at had their lava at y -55 and landings at y -52 to -55).
pub const SEA_FEET: i32 = -54;
/// A known pool of ten within this many blocks under the feet is walked to; deeper, the bot
/// goes down by its own stairs first (a stair's leg and two: the cave walks that killed seven
/// bots each went down 20 to 90 blocks).
pub const DESCENT_WALK: i32 = 8;
/// The spiral's headings, a quarter turn a leg: east, south, west, north.
pub const SPIRAL: [(i32, i32); 4] = [(1, 0), (0, 1), (-1, 0), (0, -1)];
/// Blocks from the eye a table counts as at hand for the spares and the Nether kit: a placed
/// table lands within two or three of the feet, and a walk of a few blocks stays in the bot's
/// own cut.
const TABLE_AT_HAND: f64 = 6.0;
/// Failed walks running that end the second stocking's look for animals: half the twelve that
/// cost a relocation.
const HUNT_WALK_FAILS: u32 = 6;
/// Legs of own stairs begun from one cell before the old hunt takes over there: a leg turns
/// through all four headings itself, so the second failure is no accident.
const OWN_LEGS_STUCK: u32 = 2;
/// Ticks before spare pickaxes are asked for again once they could not be made.
const SPARES_RETRY_TICKS: u64 = 2400;

/// The heading of the lava hunt's next leg of own stairs: toward `pool` along the longer axis
/// while it is more than a leg away on the plane, else the spiral's heading for leg `legs`. It
/// reads no terrain.
pub fn own_heading(feet: Cell, pool: Option<Cell>, legs: u32) -> (i32, i32) {
    let spiral = SPIRAL[legs as usize % 4];
    let Some(p) = pool else {
        return spiral;
    };
    let (dx, dz) = (p.0 - feet.0, p.2 - feet.2);
    if dx.abs().max(dz.abs()) <= stairs::STAIR_DEPTH as i32 {
        spiral
    } else if dx.abs() >= dz.abs() {
        (dx.signum(), 0)
    } else {
        (0, dz.signum())
    }
}

/// A dry staircase's turns from water, for its ending note (overworld-leg wall 7); empty for
/// any other staircase or none.
fn water_turns(st: &Stairs) -> String {
    let water = match st.water_refusals() {
        0 => String::new(),
        n => format!(" ({n} turned from water beside)"),
    };
    match st.cave_refusals() {
        0 => water,
        n => format!("{water} ({n} turned from a cave ahead)"),
    }
}

/// Junk stone (cobblestone and cobbled deepslate, what a furnace is made of) held over the
/// cobblestone keep of the junk list.
pub fn stone_over_keep(w: &World, rules: &make::SurvivalRules) -> i32 {
    let keep = rules
        .junk
        .iter()
        .find(|(k, _)| *k == data::ItemKind::Cobblestone)
        .map_or(64, |(_, n)| *n);
    craft::count(w, data::ItemKind::Cobblestone) + craft::count(w, data::ItemKind::CobbledDeepslate)
        - keep
}

/// The furnace recipe, when eight stone over the keep are held and fewer than 64 furnaces
/// (one slot): junk pressed eight to one (overworld-leg wall 12).
fn press_recipe(w: &World, rules: &make::SurvivalRules) -> Option<&'static data::types::Recipe> {
    if stone_over_keep(w, rules) < 8 || craft::count(w, data::ItemKind::Furnace) >= 64 {
        return None;
    }
    data::generated::recipes::RECIPES
        .iter()
        .find(|r| r.id == "furnace")
}

/// Stone pickaxes held with at least `spare_uses` uses left (one on the cursor counts).
pub fn fresh_pickaxes(w: &World, spare_uses: i32) -> i32 {
    let slots = w
        .inventory
        .slots
        .iter()
        .filter(|(slot, (k, _))| {
            **slot >= 1
                && *k == data::ItemKind::StonePickaxe
                && w.inventory
                    .uses_left(**slot)
                    .is_none_or(|u| u >= spare_uses)
        })
        .count();
    let carried = w
        .inventory
        .carried
        .is_some_and(|(k, _)| k == data::ItemKind::StonePickaxe);
    i32::try_from(slots).unwrap_or(i32::MAX) + i32::from(carried)
}

/// Whether spare pickaxes are due: fewer than `spare_pickaxes` fresh ones held and the makings
/// of one in the pack (overworld-leg wall 2).
pub fn spares_due(w: &World, rules: &make::SurvivalRules, table_near: bool) -> bool {
    rules.spare_pickaxes > 0
        && fresh_pickaxes(w, rules.spare_uses) < rules.spare_pickaxes
        && pickaxe_makeable(w, table_near)
}

/// Tasks the cook phase may begin: planks from five logs, a table and a furnace made and placed,
/// a batch a kind of meat and a log or two gathered are under twenty; the shields' wood and
/// crafts add up to twenty more (race 45354021804 lane 1 gave up at task 41 on its first shield
/// craft, wall 8); sixty leaves each a retry.
const COOK_TASKS: u32 = 60;

/// The goal the cook phase hands the needs planner (overworld-leg wall 4), from what is held:
/// while the coal and planks held burn for less than all the raw meat's cook time, the planks
/// that cover it (crafted from logs; with no log, a gather); then, for the first raw kind, all
/// of it cooked. `None` with nothing raw to cook.
pub fn cook_goal(counts: &make::Counts, rules: &make::SurvivalRules) -> Option<make::Row> {
    let raw = eat::raw_held(counts, rules);
    // The kit's ingots (the shields'), from held raw iron, once the meat is done; a shield made
    // counts as the ingot it took.
    let ingots = counts.get(&data::ItemKind::IronIngot).copied().unwrap_or(0);
    let shields = counts.get(&data::ItemKind::Shield).copied().unwrap_or(0);
    let raw_iron = counts.get(&data::ItemKind::RawIron).copied().unwrap_or(0);
    let iron = (rules.kit_ingots - ingots - shields).clamp(0, raw_iron.max(0));
    // Then the shields themselves, at the cook's table, from the ingots held (overworld-leg
    // wall 8): crafted under the open sky, not at a portal down a mine.
    let shield_more = (rules.kit_shields - shields).clamp(0, ingots.max(0));
    if raw.is_empty() && iron == 0 {
        if shield_more == 0 {
            return None;
        }
        return Some(make::Row {
            id: "cook-shield".to_owned(),
            needs: Vec::new(),
            provides: vec![(make::Need::parse("item:shield")?, shields + shield_more)],
            holds: Vec::new(),
            spares: Vec::new(),
            wants: Vec::new(),
            act: None,
        });
    }
    let need: i32 = raw
        .iter()
        .map(|(_, n, r)| n * i32::try_from(r.ticks).unwrap_or(200))
        .sum::<i32>()
        + iron * 200
        // The shields' planks too, six a shield at a plank's 300 ticks, so the wood is held
        // before the hut is closed (wall 11): no walk to the trees from inside it.
        + 6 * 300 * (rules.kit_shields - shields).clamp(0, ingots + iron);
    let planks = data::generated::tags::item_tag("planks").unwrap_or(&[]);
    let coals = data::generated::tags::item_tag("coals").unwrap_or(&[]);
    let burns = |k: &data::ItemKind| {
        data::generated::fuel::fuel_ticks(*k)
            .and_then(|t| i32::try_from(t).ok())
            .unwrap_or(0)
    };
    // Sticks over the reserve burn too (100 ticks, whatever planks made them: a stick is not in
    // `#non_flammable_wood`), the smelt's own last fuel (`smelt::fuels_held`).
    let sticks_over =
        (counts.get(&data::ItemKind::Stick).copied().unwrap_or(0) - make::STICK_RESERVE).max(0);
    let have: i32 = counts
        .iter()
        .filter(|(k, _)| planks.contains(k) || coals.contains(k))
        .map(|(k, n)| n * burns(k))
        .sum::<i32>()
        + sticks_over * burns(&data::ItemKind::Stick);
    let row = |id: &str, need: make::Need, count: i32| make::Row {
        id: id.to_owned(),
        needs: Vec::new(),
        provides: vec![(need, count)],
        holds: Vec::new(),
        spares: Vec::new(),
        wants: Vec::new(),
        act: None,
    };
    if have < need {
        let plank = burns(&data::ItemKind::OakPlanks).max(1);
        // Warped and crimson wood does not burn (the way home carries its stems): with any held,
        // the fuel row names the planks of a log that burns, so no stem is crafted for fuel.
        let logs = data::generated::tags::item_tag("logs").unwrap_or(&[]);
        let nether_wood = counts
            .iter()
            .any(|(k, n)| *n > 0 && (planks.contains(k) || logs.contains(k)) && burns(k) == 0);
        let burning_log = counts
            .iter()
            .filter(|(k, n)| **n > 0 && logs.contains(k) && burns(k) > 0)
            .filter_map(|(k, _)| {
                k.name()?
                    .strip_suffix("_log")
                    .map(|b| format!("{b}_planks"))
            })
            .find(|p| data::ItemKind::from_name(p).is_some());
        let stick = burns(&data::ItemKind::Stick).max(1);
        let (need_name, held) = match burning_log {
            Some(p) if nether_wood => {
                let held = data::ItemKind::from_name(&p)
                    .and_then(|k| counts.get(&k).copied())
                    .unwrap_or(0);
                (format!("item:{p}"), held)
            }
            // Only nether wood: sticks, crafted from its planks (a warped stem is 8 sticks).
            None if nether_wood => {
                let held = counts.get(&data::ItemKind::Stick).copied().unwrap_or(0);
                let more = (need - have + stick - 1) / stick;
                return Some(row(
                    "cook-fuel",
                    make::Need::parse("item:stick")?,
                    held + more,
                ));
            }
            _ => (
                "tag:planks".to_owned(),
                counts
                    .iter()
                    .filter(|(k, _)| planks.contains(k) && burns(k) > 0)
                    .map(|(_, n)| *n)
                    .sum(),
            ),
        };
        let more = (need - have + plank - 1) / plank;
        return Some(row(
            "cook-fuel",
            make::Need::parse(&need_name)?,
            held + more,
        ));
    }
    let Some((_, n, recipe)) = raw.first().copied() else {
        return Some(row(
            "cook-iron",
            make::Need::parse("item:iron_ingot")?,
            ingots + iron,
        ));
    };
    let cooked = counts.get(&recipe.result).copied().unwrap_or(0);
    let name = recipe.result.name()?;
    Some(row(
        "cook",
        make::Need::parse(&format!("item:{name}"))?,
        cooked + n,
    ))
}

/// The most of a batch of `count` that `held` items of a fuel burning `fuel_ticks` each cook, at
/// `cook_ticks` an item; the whole batch when no fuel is known (the smelt then says what is
/// missing), and never under one.
pub fn fuel_batch(fuel_ticks: Option<u32>, held: i32, cook_ticks: u32, count: i32) -> i32 {
    let Some(t) = fuel_ticks else {
        return count;
    };
    let burns = i64::from(t) * i64::from(held.max(0));
    let fits = i32::try_from(burns / i64::from(cook_ticks.max(1))).unwrap_or(i32::MAX);
    count.min(fits).max(1)
}

/// Whether a hunt may cut the task running: never a cast, a scoop or a portal's entry, each with
/// a bucket or a frame half done.
pub fn hunt_may_cut(task: &Task) -> bool {
    !matches!(task, Task::Cast(_) | Task::Scoop(_) | Task::Enter(_))
}

/// Whether the portal's food check looks for animals: never once the surface stocking is over
/// (`readied`: it hunted there on its own clock), else under the open sky for `clock` ticks
/// from the first look (overworld-leg wall 3; the review of 2026-11-24: with no clock a bot
/// walked 48-block legs away from its lit portal for the whole run).
pub fn portal_food_look(readied: bool, spent: u64, clock: u64, open_sky: bool) -> bool {
    !readied && open_sky && spent < clock
}

/// The hunger points a portal is entered with: `portal_food`, or the whole reserve when unset.
pub fn portal_food(rules: &make::SurvivalRules) -> i32 {
    if rules.portal_food > 0 {
        rules.portal_food.min(rules.food_reserve)
    } else {
        rules.food_reserve
    }
}

/// The distance and stalls kept for a cast landing: a look that comes no nearer than its best is
/// a stall, counted for that landing whatever was looked at between (the last `LANDINGS_KEPT`).
pub fn landing_stalls(kept: &mut Vec<(Cell, f64, u32)>, aim: Cell, d: f64) -> (f64, u32) {
    let was = kept
        .iter()
        .position(|(c, _, _)| *c == aim)
        .map(|i| kept.remove(i));
    let now = match was {
        Some((_, best, n)) if d >= best - 0.5 => (best, n + 1),
        Some(_) | None => (d, 0),
    };
    kept.push((aim, now.0, now.1));
    if kept.len() > LANDINGS_KEPT {
        kept.remove(0);
    }
    now
}

/// Landings whose stalls are kept: a lava lake offers a handful of sites at once.
const LANDINGS_KEPT: usize = 8;

/// Whether sticks, planks and logs make a crafting table (four planks' worth) and two sticks
/// beyond it: a log is four planks, a plank two sticks' worth.
pub fn wood_makes(sticks: i32, planks: i32, logs: i32) -> bool {
    let planks = planks + 4 * logs;
    planks >= 4 && sticks + 2 * (planks - 4) >= 2
}

/// Whether held wood (two sticks' worth) and stone (three) make a stone pickaxe, with a table
/// held or near (blaze wall 8), or the wood for a table held too (wall 19: bots that had left
/// their table dug by hand with four logs in the pack).
pub fn pickaxe_makeable(w: &World, table_near: bool) -> bool {
    let c = held(w);
    let tag = |t: &str| data::generated::tags::item_tag(t).unwrap_or(&[]);
    let sum = |kinds: &[data::ItemKind]| -> i32 {
        c.iter()
            .filter(|(k, _)| kinds.contains(k))
            .map(|(_, n)| *n)
            .sum()
    };
    let table = table_near
        || c.get(&data::ItemKind::CraftingTable).copied().unwrap_or(0) > 0
        || wood_makes(
            sum(&[data::ItemKind::Stick]),
            sum(tag("planks")),
            sum(tag("logs")),
        );
    let stone_kinds = data::generated::tags::item_tag("stone_tool_materials").unwrap_or(&[]);
    let stone: i32 = c
        .iter()
        .filter(|(k, _)| stone_kinds.contains(k))
        .map(|(_, n)| *n)
        .sum();
    table && wood_sticks(w) >= 2 && stone >= 3
}

/// Whether a pickaxe with uses left is held (blaze wall 8).
pub fn holds_pickaxe(w: &World) -> bool {
    let picks = data::generated::tags::item_tag("pickaxes").unwrap_or(&[]);
    // One on the cursor counts, as `held` counts it: a hold brings it back to the hand (blaze.md
    // wall 17: the two disagreed, and the cast paused for a pickaxe the planner saw as made).
    w.inventory.carried.is_some_and(|(k, _)| picks.contains(&k))
        || w.inventory.slots.iter().any(|(slot, (k, _))| {
            *slot >= 1 && picks.contains(k) && w.inventory.uses_left(*slot).is_none_or(|u| u > 0)
        })
}

/// A stone pickaxe's uses: what each pickaxe the held wood and stone make adds to the budget.
const STONE_PICKAXE_USES: i32 = 131;

/// Sticks the held wood makes: a stick each, two a plank, eight a log or stem (clojurecraft's
/// `pupkeep/wood-sticks`).
fn wood_sticks(w: &World) -> i32 {
    let tag = |t: &str| data::generated::tags::item_tag(t).unwrap_or(&[]);
    let (planks, logs) = (tag("planks"), tag("logs"));
    held(w)
        .iter()
        .map(|(k, n)| {
            if *k == data::ItemKind::Stick {
                *n
            } else if planks.contains(k) {
                2 * n
            } else if logs.contains(k) {
                8 * n
            } else {
                0
            }
        })
        .sum()
}

/// The digs held: every pickaxe's uses left, and a stone pickaxe's for each one the held wood
/// (two sticks) and stone (three) make, when a table is held or near (clojurecraft's
/// `picks/dig-budget`; blaze wall 7).
pub fn dig_budget(w: &World, table_near: bool) -> i32 {
    let picks = data::generated::tags::item_tag("pickaxes").unwrap_or(&[]);
    let held_uses: i32 = w
        .inventory
        .slots
        .iter()
        .filter(|(slot, (k, _))| **slot >= 1 && picks.contains(k))
        .filter_map(|(slot, _)| w.inventory.uses_left(*slot))
        .sum();
    let c = held(w);
    let table = table_near || c.get(&data::ItemKind::CraftingTable).copied().unwrap_or(0) > 0;
    let stone_kinds = data::generated::tags::item_tag("stone_tool_materials").unwrap_or(&[]);
    let stone: i32 = c
        .iter()
        .filter(|(k, _)| stone_kinds.contains(k))
        .map(|(_, n)| *n)
        .sum();
    let makeable = if table {
        (wood_sticks(w) / 2).min(stone / 3)
    } else {
        0
    };
    held_uses + makeable * STONE_PICKAXE_USES
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

/// What the clean flow asks of this tick's choice.
enum Flow {
    /// A task of its own: a climb, or a walk looking for animals.
    Task(Box<Task>),
    /// Work the `stock` row (logs).
    Stock,
    /// Cook the raw meat held (`cook_goal`).
    Cook,
    /// Nothing: the race goes on.
    On,
}

/// Blocks a climb to the surface rises per target, and headings refused before it is given up.
const SURFACE_RISE: i32 = 24;
const SURFACE_TRIES: u8 = 8;

/// Logs held, of every kind.
pub fn logs_held(w: &World) -> i32 {
    data::generated::tags::item_tag("logs")
        .map_or(0, |ks| ks.iter().map(|k| craft::count(w, *k)).sum())
}

/// Main slots (9 to 44) kept free: under this a stack of junk is dropped (blaze wall 13: every
/// slot full, digs failed `Uncollected` and the bucket could not be held).
/// Tasks the Nether kit may begin before it is given up: four pickaxes, planks, a table and its
/// placing are seven, so sixteen leaves each a retry.
const KIT_TASKS: u32 = 16;
/// Tasks begun for a shield made in the Nether: a table and a furnace put down, the furnace and
/// the planks crafted, a smelt, the shield and the table dug again are 8; half as many again for
/// a task that fails once.
const SHIELD_TASKS: u32 = 12;
/// Cook rounds after the shield: one a kind of raw meat, the two kinds a real bot carries
/// (the Nether snapshot of race 45194253932: 6 raw beef and 5 raw porkchop).
const SHIELD_COOKS: u32 = 2;
/// Digs begun to take the table along again after the shield; a third failure leaves it.
const SHIELD_CARRIES: u32 = 3;

/// Six since blaze wall 21: the Nether kit's four pickaxes and table had no slot to land in with
/// two, and a dig's drops take the rest.
pub const JUNK_FREE_SLOTS: usize = 6;

/// The window slot that drops what the cursor holds (a click outside the window).
pub const OUTSIDE: i16 = -999;

/// What to click with a stack on the cursor and no main slot free: outside the window when the
/// stack is junk, else the first junk stack's slot (a swap: the junk comes onto the cursor and
/// goes out on the next look). None with an empty cursor, a free slot, or no junk held.
pub fn cursor_step(
    slots: &std::collections::BTreeMap<i16, (data::ItemKind, i32)>,
    carried: Option<(data::ItemKind, i32)>,
    junk: &[(data::ItemKind, i32)],
) -> Option<i16> {
    let (kind, _) = carried?;
    if (9i16..=44).any(|s| !slots.contains_key(&s)) {
        return None;
    }
    if junk.iter().any(|(j, _)| *j == kind) {
        return Some(OUTSIDE);
    }
    junk_stack(slots, junk).map(|(s, _)| s)
}

/// The first main or hotbar slot holding an iron armour piece whose own armour slot (5 to 8 of
/// window 0) is empty: the piece a quick move puts on.
pub fn unworn_armour(
    slots: &std::collections::BTreeMap<i16, (data::ItemKind, i32)>,
) -> Option<i16> {
    slots.iter().find_map(|(slot, (k, _))| {
        let to = match k {
            data::ItemKind::IronHelmet => 5,
            data::ItemKind::IronChestplate => 6,
            data::ItemKind::IronLeggings => 7,
            data::ItemKind::IronBoots => 8,
            _ => return None,
        };
        (*slot >= 9 && !slots.contains_key(&to)).then_some(*slot)
    })
}

/// The main slot to drop, with its kind, when fewer than `JUNK_FREE_SLOTS` are free: the first
/// stack of a junk kind whose drop still leaves that kind's `keep` held. None with room to spare.
pub fn junk_slot(
    slots: &std::collections::BTreeMap<i16, (data::ItemKind, i32)>,
    junk: &[(data::ItemKind, i32)],
) -> Option<(i16, data::ItemKind)> {
    let free = (9i16..=44).filter(|s| !slots.contains_key(s)).count();
    if free >= JUNK_FREE_SLOTS {
        return None;
    }
    junk_stack(slots, junk)
}

/// The first main-slot stack of a junk kind whose drop still leaves that kind's `keep` held.
fn junk_stack(
    slots: &std::collections::BTreeMap<i16, (data::ItemKind, i32)>,
    junk: &[(data::ItemKind, i32)],
) -> Option<(i16, data::ItemKind)> {
    let main = |s: &i16| (9..=44).contains(s);
    slots
        .iter()
        .filter(|(s, _)| main(s))
        .find_map(|(s, (k, n))| {
            let keep = junk.iter().find(|(j, _)| j == k)?.1;
            let total: i32 = slots
                .iter()
                .filter(|(s2, (k2, _))| main(s2) && k2 == k)
                .map(|(_, (_, n2))| n2)
                .sum();
            (total - n >= keep).then_some((*s, *k))
        })
}

/// The race's gear (issue 16): an iron helmet, chestplate and leggings and an iron sword, worn or
/// held.
pub fn geared(w: &World) -> bool {
    [
        data::ItemKind::IronHelmet,
        data::ItemKind::IronChestplate,
        data::ItemKind::IronLeggings,
        data::ItemKind::IronSword,
    ]
    .iter()
    .all(|k| craft::count(w, *k) >= 1)
}

#[cfg(test)]
mod bank_tests {
    use super::*;

    fn at(y: f64) -> World {
        let mut w = World::new();
        w.body = Some(physics::Body {
            pos: [0.5, y, 0.5],
            ..physics::Body::default()
        });
        w
    }

    /// Blaze wall 6: a log 40 blocks over a cave bot's feet, stone over its head, is climbed to.
    #[test]
    fn a_log_far_over_a_roofed_bot_is_climbed_to() {
        assert!(climb_to_log(26, 68, Some(data::BlockKind::Stone)));
    }

    /// Under the open sky or a tree (a tall tree up a hill), or a log near the feet: no stairs.
    #[test]
    fn open_sky_a_tree_or_a_near_log_is_not_climbed_to() {
        assert!(!climb_to_log(72, 84, None));
        assert!(!climb_to_log(72, 84, Some(data::BlockKind::OakLeaves)));
        assert!(!climb_to_log(64, 66, Some(data::BlockKind::Stone)));
    }

    /// With no terrain loaded there is no roof to see: the old way stands.
    #[test]
    fn no_terrain_no_stairs_to_a_log() {
        let mut p = Plan::new(Aim::Race);
        let t = p.stairs_to_log(&at(26.0), (5, 68, -6), 0);
        assert!(t.is_none(), "stairs toward a log with no roof seen: {t:?}");
    }

    /// Blaze wall 9: the pool's bank 22 over a cave bot at y 4, stone overhead, is climbed to;
    /// with no terrain seen there is no roof, so the far pool is walked at as before.
    #[test]
    fn a_far_pool_over_a_roofed_bot_is_climbed_to() {
        assert!(climb_to_log(4, 26, Some(data::BlockKind::Stone)));
        assert!(!climb_to_log(4, 26, None));
        assert!(!roofed_below(&at(4.0), 26));
    }

    /// Blaze-fight wall 1: a held piece whose armour slot is empty is the one put on; a worn kind
    /// and anything that is not armour are left.
    #[test]
    fn held_armour_with_its_slot_empty_is_put_on() {
        use data::ItemKind::{Cobblestone, IronChestplate, IronHelmet, IronLeggings};
        let mut slots = std::collections::BTreeMap::new();
        slots.insert(9, (Cobblestone, 64));
        assert_eq!(unworn_armour(&slots), None);
        slots.insert(17, (IronHelmet, 1));
        slots.insert(18, (IronChestplate, 1));
        assert_eq!(unworn_armour(&slots), Some(17));
        slots.insert(5, (IronHelmet, 1));
        assert_eq!(unworn_armour(&slots), Some(18), "the helmet is worn");
        slots.insert(6, (IronChestplate, 1));
        slots.insert(7, (IronLeggings, 1));
        assert_eq!(unworn_armour(&slots), None);
    }

    /// Blaze wall 13: a full inventory drops a junk stack that leaves its keep; one with room, a
    /// kind not junk, or a drop that would go under the keep is left alone.
    #[test]
    fn a_full_inventory_drops_junk_above_its_keep() {
        use data::ItemKind::{Andesite, Bucket, Cobblestone};
        let junk = [(Cobblestone, 64), (Andesite, 0)];
        let mut slots = std::collections::BTreeMap::new();
        for s in 9i16..=44 {
            slots.insert(s, (Bucket, 1));
        }
        slots.insert(10, (Cobblestone, 64));
        assert_eq!(
            junk_slot(&slots, &junk),
            None,
            "the only cobblestone is the keep"
        );
        slots.insert(11, (Cobblestone, 64));
        assert_eq!(junk_slot(&slots, &junk), Some((10, Cobblestone)));
        slots.insert(9, (Andesite, 12));
        assert_eq!(junk_slot(&slots, &junk), Some((9, Andesite)));
        slots.retain(|s, _| !(20..=24).contains(s));
        assert_eq!(junk_slot(&slots, &junk), Some((9, Andesite)), "five free");
        slots.retain(|s, _| *s != 25);
        assert_eq!(junk_slot(&slots, &junk), None, "six slots free");
    }

    /// Blaze wall 21: every slot full and a pickaxe on the cursor: it is swapped onto a junk
    /// stack's slot, and the junk then on the cursor is dropped outside the window.
    #[test]
    fn a_cursor_stack_with_no_slot_free_is_swapped_for_junk_and_dropped() {
        use data::ItemKind::{Bucket, Cobblestone, StonePickaxe};
        let junk = [(Cobblestone, 64)];
        let mut slots = std::collections::BTreeMap::new();
        for s in 9i16..=44 {
            slots.insert(s, (Bucket, 1));
        }
        slots.insert(12, (Cobblestone, 64));
        slots.insert(13, (Cobblestone, 64));
        let pick = Some((StonePickaxe, 1));
        assert_eq!(cursor_step(&slots, pick, &junk), Some(12));
        assert_eq!(
            cursor_step(&slots, Some((Cobblestone, 64)), &junk),
            Some(OUTSIDE)
        );
        assert_eq!(cursor_step(&slots, None, &junk), None);
        slots.retain(|s, _| *s != 30);
        assert_eq!(cursor_step(&slots, pick, &junk), None, "a slot is free");
    }

    /// Blaze wall 5: a source 20 blocks over the feet is reached by stairs up, not refused.
    #[test]
    fn water_above_the_feet_takes_stairs_up() {
        let mut w = World::new();
        w.body = Some(physics::Body {
            pos: [0.5, 5.0, 0.5],
            ..physics::Body::default()
        });
        let mut p = Plan::new(Aim::Race);
        let t = p.stairs_to_bank(&w, (20, 25, 0), 0);
        assert!(
            matches!(&t, Some(Task::Stairs(s)) if s.up),
            "no stairs up toward water above: {t:?}"
        );
    }
}
