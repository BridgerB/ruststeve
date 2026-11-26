//! Issue 20e (agent E3, docs/problems/end-island.md): the End from the arrival platform to the
//! main island, a bunker in its end stone and a stock of end stone; then the hand-offs to the
//! crystals (E4, `spikes`) and the dragon (E5, `dragon`).
//!
//! - **Gaze**: every look still leaves through `endermen::keep_look`; in the End a look no act
//!   asks for is straight down (`end_look`, `Plan::end_gaze`), and a look an act asks for is
//!   tested against every enderman's eyes within 64 (`look::stares`) before it is sent.
//! - **The bridge**: west from the platform at (100, 48, 0) along z 0 at the platform's height, a
//!   block at a time against the side of the block stood on (as `walk.rs`'s bridge move), with the
//!   way home's guards on both sides (`home::guard_next`: Bridger's "rails always"), sneaking the
//!   whole way (a sneaking body is held at an edge), cut only while the dragon is more than
//!   `DRAGON_CLEAR` off or perched; otherwise the bot waits crouched in the cell's middle. A cliff
//!   of end stone ahead is climbed by stairs cut up into it (`Stairs::up`): inside the island the
//!   bot is under dragon-immune end stone.
//! - **The bunker**: a 1x2 cell dug four down into the end stone about `BUNKER_FROM_FOUNTAIN` from
//!   the fountain, its top cell laid back as a roof; its door (a stair out) is dug only to leave
//!   and filled again on the way in. Eat to `FED` and heal to `HEALED` there; every retreat goes
//!   there.
//! - **The stock**: level tunnels from the bunker (`Stairs::level`) until `STOCK_WANTED` end stone
//!   is held: pillars and walls for the crystals.
//!
//! Geography: the End is fixed by the seed (Bridger's ruling); each number says where it came
//! from. No X-ray: every dig is of a cell under the feet, beside an open cell, or ahead of one.

use data::{BlockKind, EntityKind, ItemKind};
use physics::Controls;
use protocol::metadata::MetaValue;
use world::{BlockPos, World};

use crate::dig::{Dig, DigStatus};
use crate::path::{self, Cell, Goal};
use crate::stairs::Stairs;
use crate::walk::{Arrive, Walk};
use crate::{Plan, Task, craft, eat, hold, look};

/// The End's level name.
pub const END: &str = "minecraft:the_end";
/// The arrival platform's floor centre: `ServerLevel.END_SPAWN_POINT` (100, 49, 0) is the feet,
/// the obsidian 5x5 platform one under it (`EndPlatformFeature`; config/starts.toml
/// dev-end-arrival builds the same).
pub const PLATFORM: Cell = (100, 48, 0);
/// The bridge's floor height (E1's measure of this seed, docs/problems/end-observe.md): the gap
/// is void at x 73 to 97, and the island ends at x 72 in a lip one block thick at y 52 (x 68 to
/// 72), thickening at x 65 (y 50 to 54). A bridge at the platform's height (floor 48) runs under
/// the lip into its side, so the bot first pillars up 4 on the platform and bridges at floor 52,
/// onto the lip's top.
pub const BRIDGE_FLOOR: i32 = 52;
/// The bridge runs west from the platform toward the fountain at (0, 0) along z 0 (the platform
/// is on the +x axis: `END_SPAWN_POINT`).
pub const BRIDGE_DIR: (i32, i32) = (-1, 0);
/// The fountain (the exit portal's bedrock) is at the End's origin (`EndDragonFight`'s
/// `origin`, 0 0).
pub const FOUNTAIN: [f64; 2] = [0.5, 0.5];
/// The bunker is dug this far from the fountain (the plan, 20e: about 12; the exit portal's
/// bedrock reaches 3 out and the perched dragon's body about 5, `EnderDragon` parts).
pub const BUNKER_FROM_FOUNTAIN: f64 = 12.0;
/// No bunker nearer the fountain than this (the dragon perches on it: the plan's 20g, within 6).
pub const BUNKER_MIN: f64 = 8.0;
/// A bunker is dug where the bot stands when it is within this of the fountain and the walks to
/// the site have failed (the `island` stage's 30, docs/problems/end.md).
pub const ISLAND_REACH: f64 = 30.0;
/// The walk to the bunker's site ends within this of it.
pub const SITE_RANGE: f64 = 2.0;
/// Bearings tried for the site, in degrees off the bot's own bearing from the fountain.
pub const SITE_TURNS: [f64; 5] = [0.0, 30.0, -30.0, 60.0, -60.0];
/// End stone held before the stock is over (the plan, 20e: about 160 for the pillars and walls
/// of 20f; spikes' tops at 76 to 103 over ground at 60 to 65).
pub const STOCK_WANTED: i32 = 160;
/// The stock's level tunnels run this far under the bunker's feet (stairs down first): the
/// ground's top within 30 of the fountain runs 57 to 64 (E1), and a tunnel one or two under it
/// came out under the sky where the ground dipped and a dragon fireball's cloud killed the bot in
/// it (races 45872058898 and 45876313549).
pub const STOCK_DEPTH: i32 = 6;
/// Stock tunnels (`Stairs::level`, six cells and twelve blocks a run) at most.
pub const STOCK_RUNS: u32 = 24;
/// The bridge is cut only while the dragon is farther than this, or perched (the plan, 20e).
pub const DRAGON_CLEAR: f64 = 48.0;
/// The dragon's phase: `EnderDragon.DATA_PHASE`, an int at metadata index 16 (after Mob's 15).
pub const DRAGON_PHASE_INDEX: u8 = 16;
/// Perched phases (`EnderDragonPhase`: 5 sitting flaming, 6 sitting scanning, 7 sitting
/// attacking): it does not fly at the bridge.
pub const PERCHED_PHASES: [i32; 3] = [5, 6, 7];
/// Ticks the bridge may go without a new cell gained (dragon waits not counted) before it fails.
pub const BRIDGE_STALL: u64 = 1200;
/// Cells the bridge lays at most (E1 measures the gap; the island's edge is under 80 west).
pub const BRIDGE_MAX: u32 = 96;
/// Clicks for one block (the walk's `MAX_PLACE_TRIES`), one every `PLACE_EVERY` ticks.
pub const PLACE_TRIES: u32 = 8;
pub const PLACE_EVERY: u64 = 6;
/// The feet within this of the cell's centre before a dig under them or a block laid ahead
/// (arrival's `CENTRE`, sneaking; the walk's `BRIDGE_STAND` is 0.45).
pub const CENTRE: f64 = 0.15;
/// A work (the bunker's digs and roof) with no op done this long fails.
pub const WORK_STALL: u64 = 900;
/// Stair runs (`Stairs::up`, six stairs each) up through a cliff of end stone at most.
pub const CLIFF_RUNS: u32 = 8;
/// Stair runs up out of the stock tunnels at most, when no walk leads back to the bunker.
pub const OUT_RUNS: u32 = 8;
/// Walks to the bunker's site at most.
pub const SITE_WALKS: u32 = 10;
/// Retreat to the bunker under this health (the danger line 8 plus 6, the plan's climb line).
pub const RETREAT_HEALTH: f32 = 14.0;
/// Held in the bunker until this health (and `FED` food, while food is held).
pub const HEALED: f32 = 18.0;
/// Eat in the bunker under this food (the plan, 20e: 18 or more lets health heal).
pub const FED: i32 = 18;
/// Walks back to the bunker at most a run before the bot holds where it stands.
pub const HOME_WALKS: u32 = 24;

const SIDES: [(i32, i32); 4] = [(1, 0), (-1, 0), (0, 1), (0, -1)];

/// The world is the End.
pub fn in_end(w: &World) -> bool {
    w.dimension.as_ref().is_some_and(|d| d.name == END)
}

/// The face of a block that points along `(dx, dz)`.
pub fn face_to((dx, dz): (i32, i32)) -> i8 {
    match (dx, dz) {
        (1, _) => 5,
        (-1, _) => 4,
        (_, 1) => 3,
        _ => 2,
    }
}

fn kind(w: &World, (x, y, z): Cell) -> Option<BlockKind> {
    w.block_at(BlockPos::new(x, y, z))
}

fn open(w: &World, c: Cell) -> bool {
    path::Cells::new(w).at(c) == path::Class::Clear
}

fn solid(w: &World, c: Cell) -> bool {
    path::Cells::new(w).at(c) == path::Class::Solid
}

fn flat_to_fountain(pos: [f64; 3]) -> f64 {
    ((pos[0] - FOUNTAIN[0]).powi(2) + (pos[2] - FOUNTAIN[1]).powi(2)).sqrt()
}

fn dist(a: [f64; 3], b: [f64; 3]) -> f64 {
    (0..3).map(|i| (a[i] - b[i]).powi(2)).sum::<f64>().sqrt()
}

/// The look every act in the End uses: `want` when it stares at no enderman within 64 (from the
/// standing and the crouching eye), else straight down along the same yaw; straight down with
/// no look wanted.
pub fn end_look(w: &World, want: Option<(f32, f32)>) -> (f32, f32) {
    let down = crate::endermen::EYES_DOWN;
    let Some((yaw, pitch)) = want else {
        return (w.player.yaw, down);
    };
    let Some(body) = w.body else {
        return (yaw, down);
    };
    let eyes = [
        [body.pos[0], body.pos[1] + physics::EYE_HEIGHT, body.pos[2]],
        [body.pos[0], body.pos[1] + look::SNEAK_EYE, body.pos[2]],
    ];
    if eyes
        .iter()
        .all(|e| look::stares(w, *e, yaw, pitch).is_none())
    {
        (yaw, pitch)
    } else {
        (yaw, down)
    }
}

/// The dragon as the server sends it: its position and phase (`None` while not sent).
pub fn dragon(w: &World) -> Option<([f64; 3], Option<i32>)> {
    w.entities
        .values()
        .filter(|e| e.kind == EntityKind::EnderDragon && !e.dead)
        .map(|e| {
            let phase = match e.metadata.get(&DRAGON_PHASE_INDEX) {
                Some(MetaValue::Int(p)) => Some(*p),
                Some(_) | None => None,
            };
            (e.pos, phase)
        })
        .next()
}

/// The bunker's shaft is begun only with the dragon farther than this, or perched.
pub const DRAGON_DIG_CLEAR: f64 = 64.0;

/// No dragon sent, or farther than `range`, or perched.
pub fn dragon_away(w: &World, range: f64) -> bool {
    let Some(body) = w.body else {
        return true;
    };
    dragon(w).is_none_or(|(pos, phase)| {
        dist(pos, body.pos) > range || phase.is_some_and(|p| PERCHED_PHASES.contains(&p))
    })
}

/// The bridge may be cut: no dragon sent, or farther than `DRAGON_CLEAR`, or perched.
pub fn dragon_calm(w: &World) -> bool {
    let Some(body) = w.body else {
        return true;
    };
    dragon(w).is_none_or(|(pos, phase)| {
        dist(pos, body.pos) > DRAGON_CLEAR || phase.is_some_and(|p| PERCHED_PHASES.contains(&p))
    })
}

/// An end crystal the server has sent and not removed.
pub fn crystals_alive(w: &World) -> usize {
    w.entities
        .values()
        .filter(|e| e.kind == EntityKind::EndCrystal && !e.dead)
        .count()
}

/// A side of `feet` drops to the void: the cell beside its floor and every cell under it down to
/// the dimension's floor are loaded and clear (no ground under it in loaded chunks).
pub fn void_edge(w: &World, feet: Cell) -> bool {
    SIDES
        .iter()
        .any(|(dx, dz)| void_column(w, (feet.0 + dx, feet.1 - 1, feet.2 + dz)))
}

/// `c` and every cell under it down to the dimension's floor are loaded and clear: a fall from
/// there ends in the void.
pub fn void_column(w: &World, c: Cell) -> bool {
    let Some(min_y) = w.dimension.as_ref().and_then(|d| d.min_y) else {
        return false;
    };
    let mut cells = path::Cells::new(w);
    c.1 >= min_y && (min_y..=c.1).all(|y| cells.at((c.0, y, c.2)) == path::Class::Clear)
}

/// Under the end stone: a full block over the feet column and over every open side column
/// within the sky check (the bunker, its tunnels and the stairs down). There no pocket is sealed
/// from an angry enderman: the 3-high stair cells kept failing the 2-high test, and the pockets
/// sealed in them shut the way home (race 45884609352: 499 "provoked enderman", both stocks
/// ended "no way back to the bunker").
pub fn covered(w: &World) -> bool {
    let Some(body) = w.body else {
        return false;
    };
    let f = path::stand_cell(w, body.pos);
    let roofed =
        |c: Cell| (c.1 + 2..c.1 + 2 + world::stage::SKY_CHECK).any(|y| solid(w, (c.0, y, c.2)));
    open(w, f)
        && roofed(f)
        && SIDES.iter().all(|(dx, dz)| {
            let n = (f.0 + dx, f.1, f.2 + dz);
            !(open(w, n) && open(w, (n.0, n.1 + 1, n.2))) || roofed(n)
        })
}

/// Covered notes are written at most this often (a second's worth of angry endermen at once).
pub const COVERED_NOTE_EVERY: u64 = 600;

/// An area cloud's radius: `AreaEffectCloud.DATA_RADIUS`, a float at metadata index 8 (after
/// Entity's 0 to 7); 3 (its default) when not sent.
pub const CLOUD_RADIUS_INDEX: u8 = 8;
/// A route keeps this far outside a cloud's radius (the plan's 20g breath margin).
pub const CLOUD_MARGIN: f64 = 1.5;

/// The first cell of `cells` inside an area cloud (the dragon's breath, a fireball's splash),
/// with the cloud's centre: within its radius and `CLOUD_MARGIN` on the plane and 3 of its height.
pub fn cloud_on(w: &World, cells: &[Cell]) -> Option<(Cell, [f64; 3])> {
    let clouds: Vec<([f64; 3], f64)> = w
        .entities
        .values()
        .filter(|e| e.kind == EntityKind::AreaEffectCloud && !e.dead)
        .map(|e| {
            let r = match e.metadata.get(&CLOUD_RADIUS_INDEX) {
                Some(MetaValue::Float(r)) => f64::from(*r),
                Some(_) | None => 3.0,
            };
            (e.pos, r)
        })
        .collect();
    cells.iter().find_map(|c| {
        let at = [f64::from(c.0) + 0.5, f64::from(c.1), f64::from(c.2) + 0.5];
        clouds
            .iter()
            .find(|(p, r)| {
                ((at[0] - p[0]).powi(2) + (at[2] - p[2]).powi(2)).sqrt() <= r + CLOUD_MARGIN
                    && (at[1] - p[1]).abs() <= 3.0
            })
            .map(|(p, _)| (*c, *p))
    })
}

/// A walk with a whole route to its goal: a partial one (which `route_now` takes for a goal walk)
/// is no way home through tunnels, it wanders toward a shut cell.
fn found(walk: &mut Walk, w: &World, tick: u64) -> bool {
    walk.route_now(w, tick) && walk.route == Some(path::Status::Found)
}

/// The block a bunker or a bridge is laid with: end stone first (dragon-immune), then the walk's
/// bridge blocks.
pub fn lay_item(w: &World) -> Option<ItemKind> {
    if craft::count(w, ItemKind::EndStone) > 0 {
        return Some(ItemKind::EndStone);
    }
    path::bridge_item(w)
}

/// The highest full block in a column, read down from `top`: the surface (its top face is open
/// to the sky, seen). `None` with the column not loaded or empty.
pub fn surface_y(w: &World, x: i32, z: i32, top: i32) -> Option<i32> {
    let mut cells = path::Cells::new(w);
    (0..=top)
        .rev()
        .find(|y| cells.at((x, *y, z)) == path::Class::Solid)
}

/// The bunker's cells: its feet cell, the roof over its head and the door's cells (the stair out
/// along `door`: its first step's feet and head, the second step's feet).
pub fn bunker_cells(b: Cell, (dx, dz): (i32, i32)) -> (Cell, Cell, [Cell; 7]) {
    let roof = (b.0, b.1 + 2, b.2);
    // Three steps up to the surface (the bunker is four down: two of end stone over its head),
    // each step's feet and head cut and the room over the head before it.
    let at = |i: i32, y: i32| (b.0 + i * dx, b.1 + y, b.2 + i * dz);
    let door = [
        at(1, 1),
        at(1, 2),
        at(1, 3),
        at(2, 2),
        at(2, 3),
        at(3, 3),
        at(3, 4),
    ];
    (b, roof, door)
}

/// A side of `c` with a full block beside it at its height, never `not`: the block a block laid
/// into `c` is clicked on, with the face toward `c`.
pub fn support(w: &World, c: Cell, not: Option<(i32, i32)>) -> Option<(Cell, i8)> {
    SIDES
        .iter()
        .filter(|s| Some(**s) != not)
        .map(|(dx, dz)| ((c.0 + dx, c.1, c.2 + dz), face_to((-dx, -dz))))
        .find(|(n, _)| solid(w, *n))
}

/// The bunker's shaft from `feet` (on the ground), `against` the ground block beside its top
/// cell: never open to the sky (race 45878860222 lane 1: a fireball into the open shaft). A post
/// of two blocks on `against` first, then four times: a dig under the feet, the fall, and the
/// cell over the new head laid at once (on the post above the ground, on the shaft's side below).
pub fn shaft_ops(feet: Cell, against: Cell) -> Vec<Op> {
    let s = feet.1 - 1;
    let post = [(against.0, s + 1, against.2), (against.0, s + 2, against.2)];
    let mut ops = vec![
        Op::Centre,
        Op::Place {
            cell: post[0],
            against,
            face: 1,
        },
        Op::Place {
            cell: post[1],
            against: post[0],
            face: 1,
        },
    ];
    for d in 0..4 {
        ops.extend([
            Op::Centre,
            Op::Dig((feet.0, s - d, feet.2)),
            Op::Settle,
            Op::Roof((feet.0, s - d + 2, feet.2)),
        ]);
    }
    ops
}

/// One op of a work, in order.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum Op {
    /// The feet to their cell's centre, sneaking (before a dig under them).
    Centre,
    /// A cell dug, unless it is open already.
    Dig(Cell),
    /// The feet on the ground again (after a fall into a cell dug under them).
    Settle,
    /// A block laid into `cell`, clicked on `against`'s `face`; skipped when the cell is not
    /// open.
    Place { cell: Cell, against: Cell, face: i8 },
    /// A block laid into `cell` (over the head) on whichever side block is there when it is
    /// laid; skipped when the cell is not open. Counted in `EndState::roofs`.
    Roof(Cell),
    /// One block up: a jump from the cell's centre and a block put into the cell the feet left
    /// at the jump's top (arrival's shaft, the pearls hunt's pillar).
    Rise,
}

/// A stock run out: the stairs down, or a level arm.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum StockRun {
    Down,
    Arm(u8),
}

/// What a work is for.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum WorkKind {
    /// The bunker dug four down and roofed two deep.
    Build,
    /// A pillar up on the platform to the bridge's height.
    Pillar,
    /// The roof and the door laid back from inside.
    Seal,
    /// The roof and the door's stair dug, to leave.
    Open,
}

/// A short list of digs and blocks laid, a tick at a time.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Work {
    pub kind: WorkKind,
    pub ops: Vec<Op>,
    pub dig: Option<Dig>,
    pub clicks: u32,
    pub click_at: u64,
    pub op_at: u64,
    pub done: u32,
    /// A rise's jump: the cell it left, and whether the block went in.
    #[serde(default)]
    pub jump_from: Option<Cell>,
    #[serde(default)]
    pub jumped_click: bool,
}

/// The bridge in hand.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Bridge {
    pub since: u64,
    /// The cell a block is being laid into (the floor ahead or a guard), with its clicks.
    pub laying: Option<Cell>,
    pub clicks: u32,
    pub click_at: u64,
    /// The progress invariant: the farthest cell reached along the bridge, and when.
    pub best: i32,
    pub best_at: u64,
    /// Dragon waits: whether one is in hand, its ticks in all.
    pub waiting: bool,
    pub wait_ticks: u64,
}

/// The End's state (E3's, one field of `Plan`).
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct EndState {
    /// Arrival noted.
    #[serde(default)]
    pub arrived: bool,
    /// The bridge is over: the feet on the island's end stone.
    #[serde(default)]
    pub bridged: bool,
    #[serde(default)]
    pub bridge: Option<Bridge>,
    /// Bridge blocks laid, guards laid, guard cells given up, dragon waits begun.
    #[serde(default)]
    pub laid: u32,
    #[serde(default)]
    pub guards: u32,
    #[serde(default)]
    pub guard_refused: Vec<Cell>,
    #[serde(default)]
    pub waits: u32,
    /// Stair runs up through a cliff.
    #[serde(default)]
    pub cliff_runs: u32,
    /// Walks to the bunker's site begun, and the bearing tried next.
    #[serde(default)]
    pub site_walks: u32,
    /// The work in hand, and the eat in hand.
    #[serde(default)]
    pub work: Option<Work>,
    #[serde(default)]
    pub eat: Option<eat::Eat>,
    /// The bunker's door: the stair out runs this way.
    #[serde(default)]
    pub door: Option<(i32, i32)>,
    /// Stock runs begun, and the stock over.
    #[serde(default)]
    pub stock_runs: u32,
    #[serde(default)]
    pub stock_over: bool,
    /// In the bunker to heal (until `HEALED`), retreats begun, walks back begun.
    #[serde(default)]
    pub healing: bool,
    #[serde(default)]
    pub retreats: u32,
    #[serde(default)]
    pub home_walks: u32,
    /// Hand-offs begun, and the last hold noted.
    #[serde(default)]
    pub handoffs: u32,
    #[serde(default)]
    pub hold_noted: bool,
    /// Ticks the bunker or a tunnel stood for a pocket from an angry enderman, and the last note.
    #[serde(default)]
    pub covered_ticks: u64,
    #[serde(default)]
    pub covered_noted: Option<u64>,
    /// Stair runs up out of the tunnels begun.
    #[serde(default)]
    pub out_runs: u32,
    /// The stock's runs: the one out (none between runs), the stairs' foot, the arms (heading,
    /// tip), the arm walked to, whether the bot is on its way back, and runs ended at home.
    #[serde(default)]
    pub stock_out: Option<StockRun>,
    #[serde(default)]
    pub stock_foot: Option<Cell>,
    #[serde(default)]
    pub arms: Vec<((i32, i32), Option<Cell>)>,
    #[serde(default)]
    pub going: Option<u8>,
    #[serde(default)]
    pub returning: bool,
    #[serde(default)]
    pub stock_back: u32,
    /// Bunker shafts begun, cells over the head laid in them, dragon waits before one.
    #[serde(default)]
    pub shafts: u32,
    #[serde(default)]
    pub roofs: u32,
    #[serde(default)]
    pub dig_waits: u32,
    #[serde(default)]
    pub dig_waiting: bool,
    /// Walks to the bunker held back for a cloud on the way.
    #[serde(default)]
    pub cloud_waits: u32,
    /// Up a pillar handed to the crystals' task for the way down, noted.
    #[serde(default)]
    pub down_noted: bool,
}

/// Where a work stands after a tick.
enum WorkStep {
    Running(Controls),
    Done,
    Failed(String),
}

fn down(w: &World) -> Controls {
    let (yaw, pitch) = end_look(w, None);
    Controls {
        yaw: f64::from(yaw),
        pitch: Some(pitch),
        ..Controls::default()
    }
}

/// Toward the feet cell's centre, sneaking; `None` once within `CENTRE`.
fn centre(w: &World, feet: Cell) -> Option<Controls> {
    let body = w.body?;
    let (dx, dz) = (
        f64::from(feet.0) + 0.5 - body.pos[0],
        f64::from(feet.2) + 0.5 - body.pos[2],
    );
    (dx.abs() > CENTRE || dz.abs() > CENTRE).then(|| Controls {
        forward: true,
        sneak: true,
        yaw: (-dx).atan2(dz).to_degrees(),
        ..down(w)
    })
}

/// The stock tunnel's heading for run `n`: a widening square spiral from the bunker (legs of
/// 1, 1, 2, 2, 3, 3 ... runs), starting a quarter turn from the door so the stair out stays clear.
pub fn stock_dir(door: (i32, i32), n: u32) -> (i32, i32) {
    let mut leg = 0u32;
    let mut left = n;
    loop {
        let len = leg / 2 + 1;
        if left < len {
            break;
        }
        left -= len;
        leg += 1;
    }
    let mut d = (-door.1, door.0);
    for _ in 0..leg % 4 {
        d = (-d.1, d.0);
    }
    d
}

impl Plan {
    // >>> E3 (issue 20e)
    /// The End's next task, or `None` with a step of E3's own in hand (the bridge, a work, an
    /// eat) or nothing to begin: arrival, bridge, island, bunker, stock; then the crystals while
    /// one is alive, then the dragon.
    pub fn end_task(&mut self, w: &World, tick: u64) -> Option<Task> {
        if !in_end(w) {
            return None;
        }
        let body = w.body?;
        let feet = path::stand_cell(w, body.pos);
        let floor = kind(w, (feet.0, feet.1 - 1, feet.2));
        if !self.end.arrived {
            self.end.arrived = true;
            self.note(
                tick,
                format!("end: arrived at {feet:?}"),
                format!(
                    "{:.0} from the fountain; {} end stone, {} bridge blocks held",
                    flat_to_fountain(body.pos),
                    craft::count(w, ItemKind::EndStone),
                    path::bridge_blocks(w)
                ),
            );
        }
        // The planner's give-up counts walks across phases; the End's own steps bound themselves.
        self.failed_in_row = 0;
        // 1. The bridge, then any cliff of end stone climbed from inside.
        if !self.end.bridged {
            if floor == Some(BlockKind::EndStone) && world::stage::open_sky(w) {
                self.end.bridged = true;
                self.note(
                    tick,
                    format!("end: on the island at {feet:?}"),
                    format!(
                        "end stone under the feet, {:.0} from the fountain; {} bridge blocks, {} guards laid, {} dragon waits",
                        flat_to_fountain(body.pos),
                        self.end.laid,
                        self.end.guards,
                        self.end.waits
                    ),
                );
            } else if self.cliff_ahead(w, feet) || floor == Some(BlockKind::EndStone) {
                return self.cliff_stairs(w, feet, tick);
            } else if feet.1 <= BRIDGE_FLOOR && body.on_ground {
                // Under the bridge's height on the platform: a pillar up first.
                let rises = BRIDGE_FLOOR + 1 - feet.1;
                let mut ops = vec![Op::Centre];
                ops.extend((0..rises).map(|_| Op::Rise));
                self.note(
                    tick,
                    format!("end: pillar up {rises} at {feet:?}"),
                    format!(
                        "the bridge runs at floor {BRIDGE_FLOOR}, onto the island's lip; {} blocks held",
                        path::bridge_blocks(w)
                    ),
                );
                self.end.work = Some(Work::new(WorkKind::Pillar, ops, tick));
                return None;
            } else {
                self.end.bridge = Some(Bridge {
                    since: tick,
                    laying: None,
                    clicks: 0,
                    click_at: 0,
                    best: feet.0,
                    best_at: tick,
                    waiting: false,
                    wait_ticks: 0,
                });
                self.note(
                    tick,
                    format!("end: bridge west from {feet:?}"),
                    format!(
                        "{} bridge blocks held; guards on both sides, sneaking; cut only with the dragon over {DRAGON_CLEAR} off or perched",
                        path::bridge_blocks(w)
                    ),
                );
                return None;
            }
        }
        // 2. The bunker.
        let Some(b) = self.bunker.map(|[x, y, z]| (x, y, z)) else {
            return self.bunker_site(w, feet, tick);
        };
        let door = *self.end.door.get_or_insert((1, 0));
        let (_, roof, door_cells) = bunker_cells(b, door);
        let inside = feet == b;
        let sealed = !open(w, roof) && !open(w, door_cells[0]);
        // Up a pillar or a spike, or a pillar of E4's still standing: the crystals' task brings
        // the bot down first, crystals left or not (E6's closed loop: the last crystal gone, the
        // bot stayed 18 up and asked for walks to the bunker 24 times).
        let up = crate::spikes::on_pillar(w, feet, &self.spikes.pillars)
            || crate::spikes::on_spike(w, feet)
            || !self.spikes.pillars.is_empty();
        if up && let Some(t) = self.end_spikes(w, tick) {
            if !self.end.down_noted {
                self.end.down_noted = true;
                self.note(
                    tick,
                    format!("end: up a pillar at {feet:?}, the crystals' task first"),
                    format!(
                        "{} crystals alive, {} pillars standing",
                        crystals_alive(w),
                        self.spikes.pillars.len()
                    ),
                );
            }
            return Some(t);
        }
        self.end.down_noted = false;
        // 3. Retreat, eat and heal.
        let health = w.player.health.unwrap_or(20.0);
        let food = w.player.food.unwrap_or(20);
        let rules = self.table.as_ref().map(|t| t.survival.clone());
        let food_held = rules
            .as_ref()
            .and_then(|r| eat::best_food(w, r))
            .filter(|_| food < FED);
        if health < RETREAT_HEALTH && !self.end.healing {
            self.end.healing = true;
            self.end.retreats += 1;
            self.note(
                tick,
                "end: retreat to the bunker".to_owned(),
                format!(
                    "health {health:.1} under {RETREAT_HEALTH}; food {food}; retreat {}",
                    self.end.retreats
                ),
            );
        }
        if self.end.healing && health >= HEALED {
            self.end.healing = false;
            self.note(
                tick,
                "end: healed in the bunker".to_owned(),
                format!("health {health:.1}, food {food}"),
            );
        }
        if self.end.healing || food_held.is_some() {
            if !inside {
                return self.walk_home(w, b, tick, "to heal and eat");
            }
            if !sealed {
                self.begin_seal(w, b, door, tick);
                return None;
            }
            if let Some(item) = food_held {
                self.note(
                    tick,
                    format!("end: eat {item:?} in the bunker"),
                    format!("food {food} under {FED}, health {health:.1}"),
                );
                self.end.eat = Some(eat::Eat::new(w, item, tick));
                return None;
            }
            return None;
        }
        // 4. The stock, under the end stone.
        let stone = craft::count(w, ItemKind::EndStone);
        if !self.end.stock_over && (stone >= STOCK_WANTED || self.end.stock_runs >= STOCK_RUNS) {
            self.end.stock_over = true;
            self.note(
                tick,
                "end: stock over".to_owned(),
                format!(
                    "{stone} end stone held ({STOCK_WANTED} wanted) after {} runs",
                    self.end.stock_runs
                ),
            );
        }
        if !self.end.stock_over {
            return self.stock_step(w, b, door, feet, inside, sealed, tick);
        }
        // 5. The hand-offs: the crystals while one is alive, then the dragon.
        let next = if crystals_alive(w) > 0 {
            self.end_spikes(w, tick)
        } else {
            self.end_dragon(w, tick)
        };
        if let Some(t) = next {
            // Out from under the end stone first: from the tunnel back to the bunker, then its
            // roof and stair dug; the task is chosen again once the sky is over the head.
            let shut = !open(w, roof) || !open(w, door_cells[0]);
            if !world::stage::open_sky(w) && !inside && !open(w, door_cells[0]) {
                let mut probe = Walk::new(b, Goal::Block { pos: b }, Arrive::Goal, tick);
                if found(&mut probe, w, tick) || self.end.out_runs >= OUT_RUNS {
                    return self.walk_home(w, b, tick, "to leave by the bunker's stair");
                }
                // No way back through the tunnels (race 45878860222 lane 0: 23 walks refused from
                // 52 deep after the stock): stairs up from where the bot stands to the sky.
                self.end.out_runs += 1;
                let dir = stock_dir(door, self.end.out_runs);
                self.note(
                    tick,
                    format!("end: stairs up out of the tunnels from {feet:?} toward {dir:?}"),
                    format!(
                        "no walk back to the bunker {b:?}; run {} of {OUT_RUNS}",
                        self.end.out_runs
                    ),
                );
                let mut st = Stairs::up(dir, tick);
                st.dry = true;
                return Some(Task::Stairs(Box::new(st)));
            }
            if inside && shut {
                // Out of the bunker first, the task chosen again once the door is open.
                self.note(
                    tick,
                    "end: open the bunker".to_owned(),
                    format!(
                        "to hand over to the {}; health {health:.1}, food {food}",
                        if crystals_alive(w) > 0 {
                            "crystals"
                        } else {
                            "dragon"
                        }
                    ),
                );
                self.end.work = Some(Work::new(
                    WorkKind::Open,
                    std::iter::once(roof)
                        .chain(door_cells)
                        .map(Op::Dig)
                        .collect(),
                    tick,
                ));
                return None;
            }
            self.end.handoffs += 1;
            self.end.hold_noted = false;
            return Some(t);
        }
        // 6. Nothing to hand over to: held sealed in the bunker.
        if !inside {
            return self.walk_home(w, b, tick, "nothing to hand over to");
        }
        if !sealed {
            self.begin_seal(w, b, door, tick);
            return None;
        }
        if !self.end.hold_noted {
            self.end.hold_noted = true;
            self.note(
                tick,
                format!("end: hold in the bunker at {b:?}"),
                format!(
                    "{} crystals seen alive; no hand-off ready; {stone} end stone, health {health:.1}, food {food}",
                    crystals_alive(w)
                ),
            );
        }
        None
    }

    /// The stock, a run at a time, every run begun and ended at the bunker: the stairs down
    /// `STOCK_DEPTH` under it first (their foot kept), then level arms from that foot, each run
    /// walked out to its arm's tip, cut, and walked back. A run with no walk back ends the stock
    /// (the invariant: the tunnels always lead home).
    #[allow(clippy::too_many_arguments)]
    fn stock_step(
        &mut self,
        w: &World,
        b: Cell,
        door: (i32, i32),
        feet: Cell,
        inside: bool,
        sealed: bool,
        tick: u64,
    ) -> Option<Task> {
        let stone = craft::count(w, ItemKind::EndStone);
        // A run just ended: its end kept, then home.
        if let Some(run) = self.end.stock_out.take() {
            match run {
                StockRun::Down => self.end.stock_foot = Some(feet),
                StockRun::Arm(i) => {
                    if let Some(a) = self.end.arms.get_mut(usize::from(i)) {
                        a.1 = Some(feet);
                    }
                }
            }
            self.end.returning = true;
            self.note(
                tick,
                format!("end: stock run {} over at {feet:?}", self.end.stock_runs),
                format!("{stone} end stone held; back to the bunker {b:?}"),
            );
        }
        if let Some(i) = self.end.going.take() {
            let tip = self.arm_start(i);
            if feet == tip {
                return Some(self.begin_arm(i, feet, stone, tick));
            }
            self.note(
                tick,
                format!("end: stock walk to {tip:?} ended at {feet:?}"),
                "back to the bunker first".to_owned(),
            );
            self.end.returning = true;
        }
        if !inside {
            let mut walk = Walk::new(b, Goal::Block { pos: b }, Arrive::Goal, tick);
            if found(&mut walk, w, tick) {
                self.walks += 1;
                self.note(
                    tick,
                    format!("end: stock back to the bunker {b:?}"),
                    format!("from {feet:?}; {stone} end stone held"),
                );
                return Some(Task::Explore(walk));
            }
            if world::stage::open_sky(w) {
                return self.walk_home(w, b, tick, "to tunnel for end stone");
            }
            // The invariant broken: the stock is over where it stands.
            self.end.stock_over = true;
            self.note(
                tick,
                format!("end: no way back to the bunker from the stock at {feet:?}"),
                format!(
                    "{stone} end stone held after {} runs; the stock ends",
                    self.end.stock_runs
                ),
            );
            return None;
        }
        if self.end.returning {
            self.end.returning = false;
            self.end.stock_back += 1;
            self.note(
                tick,
                format!(
                    "end: back at the bunker after stock run {}",
                    self.end.stock_runs
                ),
                format!(
                    "{} runs back home; {stone} end stone held",
                    self.end.stock_back
                ),
            );
        }
        if !sealed {
            self.begin_seal(w, b, door, tick);
            return None;
        }
        let d0 = stock_dir(door, 0);
        if self.end.stock_foot.is_none() {
            self.end.stock_runs += 1;
            self.end.stock_out = Some(StockRun::Down);
            self.note(
                tick,
                format!("end: stock run {} toward {d0:?}", self.end.stock_runs),
                format!("{stone} end stone held, {STOCK_WANTED} wanted; stairs down {STOCK_DEPTH} under the bunker"),
            );
            let mut st = Stairs::new(d0, tick);
            st.dry = true;
            st.rock = Some(feet);
            return Some(Task::Stairs(Box::new(st)));
        }
        // The arms from the foot: on along the stairs' heading and to both sides, never back
        // under the stairs.
        if self.end.arms.is_empty() {
            let side = (-d0.1, d0.0);
            self.end.arms = vec![(d0, None), (side, None), ((-side.0, -side.1), None)];
        }
        let n = u8::try_from(self.end.arms.len()).unwrap_or(1).max(1);
        let i = u8::try_from(self.end.stock_runs % u32::from(n)).unwrap_or(0);
        let start = self.arm_start(i);
        let mut walk = Walk::new(start, Goal::Block { pos: start }, Arrive::Goal, tick);
        if !found(&mut walk, w, tick) {
            self.end.stock_over = true;
            self.note(
                tick,
                format!("end: no walk to the stock arm's tip {start:?}"),
                format!("{stone} end stone held; the stock ends"),
            );
            return None;
        }
        self.end.going = Some(i);
        self.walks += 1;
        self.note(
            tick,
            format!("end: stock walk to {start:?}"),
            format!("arm {i}, run {} next", self.end.stock_runs + 1),
        );
        Some(Task::Explore(walk))
    }

    /// Where arm `i`'s next run begins: its tip, or the stairs' foot.
    fn arm_start(&self, i: u8) -> Cell {
        self.end
            .arms
            .get(usize::from(i))
            .and_then(|a| a.1)
            .or(self.end.stock_foot)
            .unwrap_or_default()
    }

    fn begin_arm(&mut self, i: u8, feet: Cell, stone: i32, tick: u64) -> Task {
        let dir = self.end.arms.get(usize::from(i)).map_or((1, 0), |a| a.0);
        self.end.stock_runs += 1;
        self.end.stock_out = Some(StockRun::Arm(i));
        self.note(
            tick,
            format!("end: stock run {} toward {dir:?}", self.end.stock_runs),
            format!("{stone} end stone held, {STOCK_WANTED} wanted; arm {i}, a level tunnel deep under the ground"),
        );
        let mut st = Stairs::level(dir, tick);
        st.dry = true;
        st.rock = Some(feet);
        Task::Stairs(Box::new(st))
    }

    /// The crystals' hand-off: E4's `Plan::spikes_task` (issue 20f), the pillar and strike.
    fn end_spikes(&mut self, w: &World, tick: u64) -> Option<Task> {
        self.spikes_task(w, tick)
    }

    /// The dragon's hand-off: E5's `Plan::dragon_task` once it lands.
    fn end_dragon(&mut self, _w: &World, _tick: u64) -> Option<Task> {
        None
    }

    /// The feet-level or head-level cell ahead along the bridge is end stone: a cliff.
    fn cliff_ahead(&self, w: &World, feet: Cell) -> bool {
        let (dx, dz) = BRIDGE_DIR;
        [0, 1]
            .iter()
            .any(|dy| kind(w, (feet.0 + dx, feet.1 + dy, feet.2 + dz)) == Some(BlockKind::EndStone))
    }

    /// Stairs up into a cliff of end stone, west first: under the island's end stone the dragon
    /// cannot reach.
    fn cliff_stairs(&mut self, w: &World, feet: Cell, tick: u64) -> Option<Task> {
        if self.end.cliff_runs >= CLIFF_RUNS {
            // On the island's end stone with no sky over it after the runs: the island it is.
            self.end.bridged = true;
            self.note(
                tick,
                format!("end: cliff stairs over at {feet:?}"),
                format!(
                    "{CLIFF_RUNS} runs; the sky over the head {}",
                    world::stage::open_sky(w)
                ),
            );
            return None;
        }
        self.end.cliff_runs += 1;
        self.note(
            tick,
            format!("end: cliff at {feet:?}, stairs up into the end stone"),
            format!(
                "run {} of {CLIFF_RUNS}; {:.0} from the fountain",
                self.end.cliff_runs,
                w.body.map_or(-1.0, |b| flat_to_fountain(b.pos))
            ),
        );
        let mut st = Stairs::up(BRIDGE_DIR, tick);
        st.dry = true;
        Some(Task::Stairs(Box::new(st)))
    }

    /// The bunker's site: dug where the bot stands within reach of the fountain, else a walk
    /// toward `BUNKER_FROM_FOUNTAIN` off the fountain on the bot's side.
    fn bunker_site(&mut self, w: &World, feet: Cell, tick: u64) -> Option<Task> {
        let body = w.body?;
        let d = flat_to_fountain(body.pos);
        let floor = kind(w, (feet.0, feet.1 - 1, feet.2));
        let here = floor == Some(BlockKind::EndStone)
            && body.on_ground
            && d >= BUNKER_MIN
            && (d <= BUNKER_FROM_FOUNTAIN + SITE_RANGE + 1.0
                || (self.end.site_walks >= SITE_WALKS && d <= ISLAND_REACH));
        if here {
            // Begun only with the dragon far or perched, as the bridge (the coordinator, wall 4).
            if !dragon_away(w, DRAGON_DIG_CLEAR) {
                if !self.end.dig_waiting {
                    self.end.dig_waiting = true;
                    self.end.dig_waits += 1;
                    self.note(
                        tick,
                        format!("end: the bunker waits for the dragon at {feet:?}"),
                        format!(
                            "the dragon within {DRAGON_DIG_CLEAR} and flying; wait {}",
                            self.end.dig_waits
                        ),
                    );
                }
                self.retry_at = tick + crate::RETRY_TICKS;
                return None;
            }
            self.end.dig_waiting = false;
            return self.begin_build(w, feet, tick);
        }
        if self.end.site_walks >= SITE_WALKS {
            self.note(
                tick,
                format!("end: no bunker site from {feet:?}"),
                format!("{SITE_WALKS} walks; {d:.0} from the fountain, floor {floor:?}"),
            );
            self.retry_at = tick + crate::RETRY_TICKS * 10;
            return None;
        }
        let turn = SITE_TURNS[(self.end.site_walks as usize) % SITE_TURNS.len()];
        self.end.site_walks += 1;
        let bearing =
            (body.pos[2] - FOUNTAIN[1]).atan2(body.pos[0] - FOUNTAIN[0]) + turn.to_radians();
        #[allow(clippy::cast_possible_truncation)]
        let (sx, sz) = (
            (FOUNTAIN[0] + BUNKER_FROM_FOUNTAIN * bearing.cos()).floor() as i32,
            (FOUNTAIN[1] + BUNKER_FROM_FOUNTAIN * bearing.sin()).floor() as i32,
        );
        let sy = surface_y(w, sx, sz, 120).map_or(feet.1, |y| y + 1);
        let target = (sx, sy, sz);
        let mut walk = Walk::far(target, SITE_RANGE, tick);
        if !walk.route_now(w, tick) {
            self.note(
                tick,
                format!("end: no walk to the bunker site {target:?}"),
                format!(
                    "walk {} of {SITE_WALKS}, {turn:.0} degrees off the bearing",
                    self.end.site_walks
                ),
            );
            return None;
        }
        self.walks += 1;
        self.note(
            tick,
            format!("end: walk to the bunker site {target:?}"),
            format!(
                "{BUNKER_FROM_FOUNTAIN} from the fountain, {turn:.0} degrees off the bot's bearing; walk {} of {SITE_WALKS}",
                self.end.site_walks
            ),
        );
        Some(Task::Explore(walk))
    }

    /// Dig four down where the feet stand and lay the top two cells back as the roof: two of end
    /// stone over the head and over every stock tunnel, so a dip of one in the ground opens none
    /// to the sky (race 45872058898: both bots killed in their tunnels by a dragon fireball's
    /// cloud, the tunnels one under the surface).
    fn begin_build(&mut self, w: &World, feet: Cell, tick: u64) -> Option<Task> {
        let s = feet.1 - 1;
        let top = (feet.0, s, feet.2);
        let Some((against, _)) = support(w, top, None) else {
            self.note(
                tick,
                format!("end: no bunker at {feet:?}"),
                "no full block beside the top cell to roof it from".to_owned(),
            );
            self.end.site_walks += 1;
            self.retry_at = tick + crate::RETRY_TICKS;
            return None;
        };
        // The door runs to a side other than the roof's support.
        let roof_side = (against.0 - top.0, against.2 - top.2);
        let door = SIDES
            .iter()
            .copied()
            .find(|s| *s != roof_side)
            .unwrap_or((1, 0));
        self.end.door = Some(door);
        let ops = shaft_ops(feet, against);
        let post = (against.0, s + 1, against.2);
        self.end.shafts += 1;
        self.note(
            tick,
            format!("end: dig the bunker at {feet:?}"),
            format!(
                "{:.0} from the fountain; four down, the cell over the head laid after every dig (a post at {:?}); door {door:?}",
                w.body.map_or(-1.0, |b| flat_to_fountain(b.pos)),
                post
            ),
        );
        self.end.work = Some(Work::new(WorkKind::Build, ops, tick));
        None
    }

    fn begin_seal(&mut self, w: &World, b: Cell, door: (i32, i32), tick: u64) {
        let (_, roof, cells) = bunker_cells(b, door);
        let mut ops = Vec::new();
        if open(w, roof)
            && let Some((against, face)) = support(w, roof, Some(door))
        {
            ops.push(Op::Place {
                cell: roof,
                against,
                face,
            });
        }
        let d = cells[0];
        ops.push(Op::Place {
            cell: d,
            against: (d.0, d.1 - 1, d.2),
            face: 1,
        });
        self.note(
            tick,
            format!("end: seal the bunker at {b:?}"),
            format!("the roof {roof:?} and the door {d:?} laid back"),
        );
        self.end.work = Some(Work::new(WorkKind::Seal, ops, tick));
    }

    /// A walk back to the bunker's cell (`HOME_WALKS` at most, then a hold where the bot is).
    fn walk_home(&mut self, w: &World, b: Cell, tick: u64, why: &str) -> Option<Task> {
        if self.end.home_walks >= HOME_WALKS {
            if !self.end.hold_noted {
                self.end.hold_noted = true;
                self.note(
                    tick,
                    "end: no more walks to the bunker".to_owned(),
                    format!("{HOME_WALKS} walks; held where the bot stands"),
                );
            }
            return None;
        }
        self.end.home_walks += 1;
        let mut walk = Walk::new(b, Goal::Block { pos: b }, Arrive::Goal, tick);
        if !found(&mut walk, w, tick) {
            self.note(
                tick,
                format!("end: no walk to the bunker {b:?}"),
                format!("{why}; walk {} of {HOME_WALKS}", self.end.home_walks),
            );
            self.retry_at = tick + crate::RETRY_TICKS;
            return None;
        }
        // Never through a cloud (E4's race 45875049490 lane 1 died walking this route through the
        // dragon's breath): wait where the bot is until it is gone.
        if let Some((c, at)) = cloud_on(w, walk.waypoints.as_deref().unwrap_or_default()) {
            self.end.home_walks -= 1;
            self.end.cloud_waits += 1;
            self.note(
                tick,
                format!("end: the way to the bunker waits for a cloud at {c:?}"),
                format!(
                    "{why}; the cloud at {:.0}, {:.0}, {:.0}; wait {}",
                    at[0], at[1], at[2], self.end.cloud_waits
                ),
            );
            self.retry_at = tick + crate::RETRY_TICKS * 2;
            return None;
        }
        self.walks += 1;
        self.note(
            tick,
            format!("end: back to the bunker {b:?}"),
            format!("{why}; walk {} of {HOME_WALKS}", self.end.home_walks),
        );
        Some(Task::Explore(walk))
    }

    /// One tick of E3's own steps in the End (the bridge, a work, an eat) while no task runs, or
    /// the choice of the next (`end_task`). `None` outside the End or while a task runs.
    pub(crate) fn end_step(&mut self, w: &World, tick: u64) -> Option<Controls> {
        if !in_end(w) || !matches!(self.task, Task::Idle) {
            return None;
        }
        let hold = Controls {
            sneak: true,
            ..down(w)
        };
        if let Some(mut b) = self.end.bridge.take() {
            if let Some(c) = self.bridge_tick(w, tick, &mut b) {
                self.end.bridge = Some(b);
                return Some(c);
            }
            self.retry_at = tick;
            return Some(hold);
        }
        if let Some(mut work) = self.end.work.take() {
            match self.work_tick(w, tick, &mut work) {
                WorkStep::Running(c) => {
                    self.end.work = Some(work);
                    return Some(c);
                }
                WorkStep::Done => self.work_done(w, &work, tick),
                WorkStep::Failed(why) => {
                    self.note(
                        tick,
                        format!("end: {:?} work failed", work.kind),
                        format!("{why}; {} ops done", work.done),
                    );
                    if work.kind == WorkKind::Build {
                        self.end.site_walks += 1;
                    }
                    self.retry_at = tick + crate::RETRY_TICKS;
                }
            }
            return Some(hold);
        }
        if let Some(mut e) = self.end.eat.take() {
            let (act, status) = e.step(w, tick);
            self.acts.extend(act);
            match status {
                eat::EatStatus::Running => {
                    self.end.eat = Some(e);
                }
                eat::EatStatus::Done => {
                    self.meals += 1;
                    self.note(
                        tick,
                        format!("end: ate {:?} in the bunker", e.item),
                        format!("food {:?}", w.player.food),
                    );
                }
                eat::EatStatus::Failed(f) => {
                    self.note(
                        tick,
                        format!("end: eat failed {f:?}"),
                        "choose again".to_owned(),
                    );
                    self.retry_at = tick + crate::RETRY_TICKS;
                }
            }
            return Some(hold);
        }
        // Waiting: the planner's own idle wait, and the steps after this one (E4's driver) run.
        if tick < self.retry_at {
            return None;
        }
        if let Some(t) = self.end_task(w, tick) {
            self.begin(t);
            return None;
        }
        Some(hold)
    }

    fn work_done(&mut self, w: &World, work: &Work, tick: u64) {
        let feet = w.body.map(|b| path::stand_cell(w, b.pos));
        match work.kind {
            WorkKind::Build => {
                if let Some(f) = feet {
                    self.bunker = Some([f.0, f.1, f.2]);
                    self.note(
                        tick,
                        format!("end: bunker dug at {f:?}"),
                        format!(
                            "roofed; {} ops; {} end stone held; door {:?}",
                            work.done,
                            craft::count(w, ItemKind::EndStone),
                            self.end.door
                        ),
                    );
                }
            }
            WorkKind::Seal => {
                self.end.home_walks = 0;
                self.note(
                    tick,
                    "end: bunker sealed".to_owned(),
                    format!(
                        "{} ops; health {:?}, food {:?}",
                        work.done, w.player.health, w.player.food
                    ),
                );
            }
            WorkKind::Pillar => self.note(
                tick,
                format!("end: pillar up to the bridge's height at {feet:?}"),
                format!("{} ops; floor {BRIDGE_FLOOR}", work.done),
            ),
            WorkKind::Open => self.note(
                tick,
                "end: bunker opened".to_owned(),
                format!("the roof and the stair out dug; {} ops", work.done),
            ),
        }
    }

    fn work_tick(&mut self, w: &World, tick: u64, work: &mut Work) -> WorkStep {
        let Some(body) = w.body else {
            return WorkStep::Failed("no body".to_owned());
        };
        if tick.saturating_sub(work.op_at) > WORK_STALL {
            return WorkStep::Failed(format!(
                "no op done for {WORK_STALL} ticks; next {:?}",
                work.ops.first()
            ));
        }
        let feet = path::stand_cell(w, body.pos);
        let Some(op) = work.ops.first().cloned() else {
            return WorkStep::Done;
        };
        let next = |work: &mut Work| {
            work.ops.remove(0);
            work.op_at = tick;
            work.done += 1;
            work.clicks = 0;
            WorkStep::Running(down(w))
        };
        match op {
            Op::Centre => match centre(w, feet) {
                Some(c) => WorkStep::Running(c),
                None => next(work),
            },
            Op::Settle => {
                if body.on_ground {
                    next(work)
                } else {
                    WorkStep::Running(Controls {
                        sneak: true,
                        ..down(w)
                    })
                }
            }
            Op::Dig(c) => {
                if work.dig.is_none() && open(w, c) {
                    return next(work);
                }
                let d = work.dig.get_or_insert_with(|| Dig::new(w, c, tick));
                let (ctl, act, status) = d.step(w, tick);
                self.acts.extend(act);
                match status {
                    DigStatus::Running => WorkStep::Running(ctl),
                    DigStatus::Done => {
                        work.dig = None;
                        next(work)
                    }
                    // The cell open: the dig did its work, whatever became of the drop.
                    DigStatus::Failed(_) if open(w, c) => {
                        work.dig = None;
                        next(work)
                    }
                    DigStatus::Failed(f) => WorkStep::Failed(format!(
                        "the dig of {c:?} failed {f:?} ({:?})",
                        kind(w, c)
                    )),
                }
            }
            Op::Rise => {
                if let Some(j) = work.jump_from {
                    if body.on_ground && feet.1 > j.1 {
                        work.jump_from = None;
                        work.jumped_click = false;
                        return next(work);
                    }
                    if !body.on_ground {
                        if !work.jumped_click
                            && body.pos[1] >= f64::from(j.1) + 1.0
                            && open(w, j)
                            && self.acts.is_empty()
                            && w.click.is_none()
                        {
                            work.jumped_click = true;
                            self.acts.push(world::Act::UseOn {
                                pos: BlockPos::new(j.0, j.1 - 1, j.2),
                                face: 1,
                            });
                        }
                        return WorkStep::Running(down(w));
                    }
                    // Back on the ground where it began: the block did not go in; again.
                    work.jump_from = None;
                    work.jumped_click = false;
                }
                if !body.on_ground {
                    return WorkStep::Running(down(w));
                }
                if let Some(c) = centre(w, feet) {
                    return WorkStep::Running(c);
                }
                let Some(item) = lay_item(w) else {
                    return WorkStep::Failed("no block held to rise on".to_owned());
                };
                match hold::toward(w, item) {
                    hold::Hold::Held => {
                        work.jump_from = Some(feet);
                        WorkStep::Running(Controls {
                            jump: true,
                            ..down(w)
                        })
                    }
                    hold::Hold::Act(a) => {
                        if self.acts.is_empty() {
                            self.acts.push(a);
                        }
                        WorkStep::Running(down(w))
                    }
                    hold::Hold::Wait => WorkStep::Running(down(w)),
                    hold::Hold::Missing => WorkStep::Failed(format!("{item:?} went missing")),
                }
            }
            Op::Roof(cell) => {
                if !open(w, cell) {
                    self.end.roofs += 1;
                    return next(work);
                }
                let Some((against, face)) = support(w, cell, None) else {
                    return WorkStep::Failed(format!("no side block to roof {cell:?} from"));
                };
                match self.lay_block(w, tick, against, face, &mut work.clicks, &mut work.click_at) {
                    Ok(()) => WorkStep::Running(Controls {
                        sneak: true,
                        ..down(w)
                    }),
                    Err(why) => WorkStep::Failed(format!("{cell:?}: {why}")),
                }
            }
            Op::Place {
                cell,
                against,
                face,
            } => {
                if !open(w, cell) {
                    return next(work);
                }
                if crate::fortress_way::body_in_cell(w, cell) {
                    return WorkStep::Failed(format!("{cell:?} is in the body's box"));
                }
                match self.lay_block(w, tick, against, face, &mut work.clicks, &mut work.click_at) {
                    Ok(()) => WorkStep::Running(Controls {
                        sneak: true,
                        ..down(w)
                    }),
                    Err(why) => WorkStep::Failed(format!("{cell:?}: {why}")),
                }
            }
        }
    }

    /// Hold a block and click `against`'s `face`, every `PLACE_EVERY` ticks, `PLACE_TRIES` at most.
    fn lay_block(
        &mut self,
        w: &World,
        tick: u64,
        against: Cell,
        face: i8,
        clicks: &mut u32,
        click_at: &mut u64,
    ) -> Result<(), String> {
        let Some(item) = lay_item(w) else {
            return Err("no block held to lay".to_owned());
        };
        match hold::toward(w, item) {
            hold::Hold::Act(a) => {
                if self.acts.is_empty() {
                    self.acts.push(a);
                }
                Ok(())
            }
            hold::Hold::Wait => Ok(()),
            hold::Hold::Missing => Err(format!("{item:?} went missing")),
            hold::Hold::Held => {
                if (*clicks > 0 && tick.saturating_sub(*click_at) < PLACE_EVERY)
                    || w.click.is_some()
                {
                    return Ok(());
                }
                if *clicks >= PLACE_TRIES {
                    return Err(format!("{PLACE_TRIES} clicks on {against:?} face {face}"));
                }
                *clicks += 1;
                *click_at = tick;
                self.acts.push(world::Act::UseOn {
                    pos: BlockPos::new(against.0, against.1, against.2),
                    face,
                });
                Ok(())
            }
        }
    }

    /// One tick of the bridge; `None` once it has ended (noted).
    fn bridge_tick(&mut self, w: &World, tick: u64, b: &mut Bridge) -> Option<Controls> {
        let body = w.body?;
        let feet = path::stand_cell(w, body.pos);
        let (dx, dz) = BRIDGE_DIR;
        let floor = kind(w, (feet.0, feet.1 - 1, feet.2));
        let sneak = Controls {
            sneak: true,
            ..down(w)
        };
        // The progress invariant: a cell gained along the bridge, dragon waits not counted.
        let along = feet.0 * dx + feet.2 * dz;
        if along > b.best {
            b.best = along;
            b.best_at = tick;
        }
        if !body.on_ground {
            return Some(sneak);
        }
        // Over: on the island's end stone, or a cliff of it ahead.
        if floor == Some(BlockKind::EndStone) || self.cliff_ahead(w, feet) {
            self.note(
                tick,
                format!("end: bridge over at {feet:?}"),
                format!(
                    "floor {floor:?}, cliff ahead {}; {} laid, {} guards, {} waits ({} ticks) in {} ticks",
                    self.cliff_ahead(w, feet),
                    self.end.laid,
                    self.end.guards,
                    self.end.waits,
                    b.wait_ticks,
                    tick.saturating_sub(b.since)
                ),
            );
            return None;
        }
        if tick.saturating_sub(b.best_at) > BRIDGE_STALL || self.end.laid >= BRIDGE_MAX {
            self.note(
                tick,
                format!("end: bridge failed at {feet:?}"),
                format!(
                    "no cell gained for {BRIDGE_STALL} ticks or {BRIDGE_MAX} laid ({} laid); floor {floor:?}, {} blocks held",
                    self.end.laid,
                    path::bridge_blocks(w)
                ),
            );
            self.retry_at = tick + crate::RETRY_TICKS;
            return None;
        }
        // The dragon near and flying: wait crouched in the cell's middle.
        if !dragon_calm(w) {
            if !b.waiting {
                b.waiting = true;
                self.end.waits += 1;
                let (pos, phase) = dragon(w).unwrap_or(([0.0; 3], None));
                self.note(
                    tick,
                    format!("end: bridge waits for the dragon at {feet:?}"),
                    format!(
                        "the dragon {:.0} off, phase {phase:?}; wait {}",
                        dist(pos, body.pos),
                        self.end.waits
                    ),
                );
            }
            b.wait_ticks += 1;
            b.best_at += 1;
            return Some(centre(w, feet).unwrap_or(sneak));
        }
        if b.waiting {
            b.waiting = false;
            self.note(
                tick,
                format!("end: bridge goes on from {feet:?}"),
                format!(
                    "the dragon far or perched after {} ticks waited",
                    b.wait_ticks
                ),
            );
        }
        // A block in hand being laid: shown, counted; else clicked again.
        if let Some(cell) = b.laying
            && !open(w, cell)
        {
            b.laying = None;
            b.clicks = 0;
            return Some(sneak);
        }
        // Rails first: the guards on both sides across the bridge (none on the platform's
        // obsidian: it is 5 wide).
        let guard = (floor != Some(BlockKind::Obsidian))
            .then(|| crate::home::guard_next(w, Some(BRIDGE_DIR), &self.end.guard_refused))
            .flatten();
        if let Some((cell, against, face)) = guard {
            if b.laying != Some(cell) {
                b.laying = Some(cell);
                b.clicks = 0;
                self.end.guards += 1;
                if self.end.guards % 10 == 1 {
                    self.note(
                        tick,
                        format!("end: guard into {cell:?}"),
                        format!(
                            "guard {} on the bridge; {} blocks held",
                            self.end.guards,
                            path::bridge_blocks(w)
                        ),
                    );
                }
            }
            if let Some(c) = centre(w, feet) {
                return Some(c);
            }
            if let Err(why) = self.lay_block(w, tick, against, face, &mut b.clicks, &mut b.click_at)
            {
                self.end.guard_refused.push(cell);
                b.laying = None;
                b.clicks = 0;
                self.note(tick, format!("end: guard given up at {cell:?}"), why);
            }
            return Some(sneak);
        }
        let ahead = (feet.0 + dx, feet.1, feet.2 + dz);
        let ahead_floor = (ahead.0, ahead.1 - 1, ahead.2);
        if !solid(w, ahead_floor) {
            if b.laying != Some(ahead_floor) {
                b.laying = Some(ahead_floor);
                b.clicks = 0;
                self.end.laid += 1;
                self.note(
                    tick,
                    format!("end: bridge block into {ahead_floor:?}"),
                    format!(
                        "{} on the bridge, {} blocks held, {:.0} from the fountain",
                        self.end.laid,
                        path::bridge_blocks(w),
                        flat_to_fountain(body.pos)
                    ),
                );
            }
            if let Some(c) = centre(w, feet) {
                return Some(c);
            }
            let under = (feet.0, feet.1 - 1, feet.2);
            if let Err(why) = self.lay_block(
                w,
                tick,
                under,
                face_to(BRIDGE_DIR),
                &mut b.clicks,
                &mut b.click_at,
            ) {
                self.note(
                    tick,
                    format!("end: bridge block at {ahead_floor:?} not laid"),
                    why,
                );
                self.retry_at = tick + crate::RETRY_TICKS;
                return None;
            }
            return Some(sneak);
        }
        // The floor ahead is there: a step onto it, sneaking.
        let to = [f64::from(ahead.0) + 0.5, f64::from(ahead.2) + 0.5];
        Some(Controls {
            forward: true,
            sneak: true,
            yaw: (-(to[0] - body.pos[0]))
                .atan2(to[1] - body.pos[2])
                .to_degrees(),
            ..down(w)
        })
    }

    /// In the End, covered (`covered`) with an angry enderman near: no pocket is sealed, the work
    /// goes on; noted every `COVERED_NOTE_EVERY` ticks with the count.
    pub(crate) fn end_covered(&mut self, w: &World, tick: u64) -> bool {
        if !in_end(w) || crate::endermen::angry(w).is_none() || !covered(w) {
            return false;
        }
        self.end.covered_ticks += 1;
        if self
            .end
            .covered_noted
            .is_none_or(|t| tick >= t + COVERED_NOTE_EVERY)
        {
            self.end.covered_noted = Some(tick);
            self.note(
                tick,
                "end: covered from an angry enderman, no pocket".to_owned(),
                format!(
                    "a roofed 2-high cell with 2-high sides; {} ticks covered in the run",
                    self.end.covered_ticks
                ),
            );
        }
        true
    }

    /// In the End a look no act asked for is straight down (`end_look`); every look still goes
    /// through `keep_look` after this.
    pub(crate) fn end_gaze(&self, w: &World, c: Controls) -> Controls {
        if !in_end(w) || c.pitch.is_some() {
            return c;
        }
        let (_, pitch) = end_look(w, None);
        Controls {
            pitch: Some(pitch),
            ..c
        }
    }
    // <<< E3
}

impl Work {
    pub fn new(kind: WorkKind, ops: Vec<Op>, tick: u64) -> Self {
        Self {
            kind,
            ops,
            dig: None,
            clicks: 0,
            click_at: 0,
            op_at: tick,
            done: 0,
            jump_from: None,
            jumped_click: false,
        }
    }
}
