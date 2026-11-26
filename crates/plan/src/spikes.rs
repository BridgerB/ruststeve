//! Issue 20f, the End's crystals by pillar and strike (agent E4, docs/problems/end-crystals.md):
//! the port of clojurecraft's `dragon.clj` crystal half (`approach-along`, `toward-crystals`,
//! `climb`, `:dig-down`, `:strike`).
//!
//! **The blast.** Any hit blows an end crystal up, an explosion of power 6 that kills a player
//! within about eight blocks with a clear line to it, so the bot never strikes in the open. It
//! strikes from the cell `STAND_OFF` out from the crystal's column along an axis, the feet level
//! with the spike's bedrock (the obsidian's top layer under them), with two blocks put in the cells
//! `WALL_OFF` out at the feet's and the eye's height: the blast's line to the body is shut, and the
//! attack, which the server judges by reach alone (26.1's `isWithinAttackRange`, eye to the box,
//! no line of sight: E1, jar 26.1.2), goes past them. The blast (power 6) at 3 off with the body
//! fully seen takes about 56, so every ray must be shut: the explosion is at the crystal's feet
//! (bedrock + 1), the body 1.8 high at the bedrock's height, so each ray to it crosses the wall's
//! plane under bedrock + 2, inside the two blocks, and within the stand's own column across.
//!
//! **The way up** is a pillar in the first column out from the spike's top layer along the same
//! axis (`STAND_OFF` out at the nearest): a block put under the feet at the top of each jump
//! (`arrival::rise_jump` and `arrival::rise_click`, the shaft's rise lifted out unchanged), looking
//! straight down. End stone only (dragon-immune), so the pillar the bot stands on is never smashed
//! by the dragon flying through it. A pillar is recorded with its base, so the way down digs back
//! exactly the blocks put, end stone on end stone ground included. On a radius-2 spike the
//! pillar's top is the stand; on a wider one the stand is on the spike's top, walked onto from the
//! pillar's top.
//!
//! **The box.** At the stand the body is closed in end stone: the walls, every other side at the
//! feet's and the eye's height (a ledge under a feet cell nothing holds), a block over one side
//! and the roof. A strike needs reach, not sight (e03), so the dragon's wings and charge find
//! nothing open to push the body out by (race 45871173293's wall 2).
//!
//! **Cages.** Two small spikes are caged in iron bars two out from the column, at the bedrock's
//! height to three over it: the bars are in the wall's own cells. Parity e03 (E6, run
//! 45864998071, Normal, full iron) struck through the intact bars and lost 0.42, the blast's
//! least (a bar on the ring's edge is a full-width pane facing the stand), so with `bars_shield`
//! (the default) the bars are the wall and the strike goes through them. Turned off, each wall
//! cell's bar is dug with the pickaxe and end stone put in its place first (the fallback).
//!
//! **One blast, one wall.** e03: the blast leaves both wall cells air (end stone or bars). The
//! wall is read from the world before every strike, so a crystal still standing (a strike that
//! missed) gets its wall again first; the crystal gone, the bot goes down.
//!
//! **Down** is the pillar dug back from on top of it, one cell under the feet at a time
//! (`portal_way.rs`'s one-cell dig). No jump.
//!
//! **Rules.** Climb only while the dragon is more than `CLIMB_FAR` off or perched (phases 5 to 7),
//! with health over danger plus `CLIMB_MARGIN`. While the dragon strafes or charges within
//! `THREAT_NEAR`, a bot up a pillar holds still crouched inside a complete guard ring, or digs
//! down. Never a strike under danger health. A pillar that rises no block in `PILLAR_STALL` ticks
//! fails, says why, and its approach is refused for a while.
//!
//! Every decision is read from the world each time (which crystal is nearest, which approach,
//! where the feet are against it), so a death or a reload resumes where the world says. Every
//! path writes a ledger note (`spikes: ...`).

use data::{BlockKind, EntityKind, ItemKind};
use physics::Controls;
use protocol::metadata::MetaValue;
use world::{Act, BlockPos, World};

use crate::dig::Dig;
use crate::path::{self, Cell, Goal};
use crate::walk::{Arrive, Walk};
use crate::{Plan, Task, arrival, craft, hold};

/// Cells along an axis from the crystal's column the strike stands in: the crystal's box (2 wide)
/// is then 2.0 from the eye, inside `STRIKE_REACH` (clojurecraft's `stand-off`).
pub const STAND_OFF: i32 = 3;
/// Cells out the two blocks between go, shutting the blast's line (`wall-off`).
pub const WALL_OFF: i32 = 2;
/// Eye to the crystal's box within which a strike attacks: the client's entity reach, the
/// server's `canInteractWithEntity` range (`strike-reach`).
pub const STRIKE_REACH: f64 = 3.0;
/// An end crystal's box: vanilla `EntityType.END_CRYSTAL` is sized 2.0 by 2.0.
pub const CRYSTAL_HALF: f64 = 1.0;
pub const CRYSTAL_HEIGHT: f64 = 2.0;
/// Cells out from the column a spike's top layer is followed for its edge (`spike-search`; the
/// widest spike is radius 5, vanilla `SpikeFeature`: 2 + i / 3 for i in 0..10).
pub const SPIKE_SEARCH: i32 = 9;
/// Cells down a column's ground is looked for (the island's ground is about 60 to 65, the
/// spikes' tops 76 to 103: `docs/problems/end.md`).
pub const GROUND_SEARCH: i32 = 64;
/// The blocks the shaft's items put (`arrival::SHAFT_BLOCKS`): none is End ground, so one under
/// the feet is a pillar even when no record says so.
pub const PLACED: [BlockKind; 17] = [
    BlockKind::SoulSoil,
    BlockKind::SoulSand,
    BlockKind::WarpedNylium,
    BlockKind::CrimsonNylium,
    BlockKind::WarpedWartBlock,
    BlockKind::SmoothBasalt,
    BlockKind::Granite,
    BlockKind::Tuff,
    BlockKind::Diorite,
    BlockKind::Andesite,
    BlockKind::Calcite,
    BlockKind::Dirt,
    BlockKind::CobbledDeepslate,
    BlockKind::Cobblestone,
    BlockKind::Netherrack,
    BlockKind::Blackstone,
    BlockKind::Basalt,
];
/// The dragon farther than this (blocks, from the feet) lets a climb go on (Bridger's rule,
/// issue 20f).
pub const CLIMB_FAR: f64 = 64.0;
/// The dragon strafing or charging within this (blocks) sends the bot into its guard ring or
/// down. Issue 20f's plan said 40; a strafing dragon fires its fireball from farther (vanilla
/// `DragonStrafePlayerPhase` fires within 64): race 45871173293 lane 0 held on an unguarded
/// pillar at y 69 with the dragon strafing 62 off, took a fireball's 6 at tick 1080 and fell 8
/// for 7.5 more. So the climb's own reach, `CLIMB_FAR`.
pub const THREAT_NEAR: f64 = CLIMB_FAR;
/// Health over the danger line a climb needs: a blast or a blow in mid-climb is then not a
/// death (issue 20f).
pub const CLIMB_MARGIN: f32 = 6.0;
/// The danger line when the planner has no goal table (`make`'s default `danger_health`).
pub const DANGER_DEFAULT: f32 = 8.0;
/// Ticks a pillar may rise no block before it fails (issue 20f's progress invariant; a rise is
/// about 12 ticks, a block in hand and a jump).
pub const PILLAR_STALL: u64 = 1200;
/// Ticks a block put waits for the block update before the click counts as refused (`place`'s
/// `CONFIRM_TICKS`), and the refusals before the cell is given up.
pub const PLACE_CONFIRM: u64 = 40;
pub const PLACE_TRIES: u32 = 3;
/// Ticks a hold (crouched, looking down) lasts before the planner looks again.
pub const HOLD_TICKS: u64 = 40;
/// Ticks between two attacks on a crystal (any hit blows it up; a second is for a lost packet),
/// and the most a strike lasts.
pub const SWING_EVERY: u64 = 10;
pub const STRIKE_TICKS: u64 = 200;
/// Ticks an approach that failed (a walk with no route, a stalled pillar) is refused.
pub const REFUSE_TICKS: u64 = 1200;
/// How near a cell's centre (blocks, each axis) a step ends, and the most it takes. A body 0.6
/// wide within 0.2 of the centre stands on its cell alone.
pub const STEP_NEAR: f64 = 0.2;
pub const STEP_TICKS: u64 = 60;
/// One blow of the dragon's (its breath's 6 through iron, race 45875049490; a wing's 5, a head's
/// 10): on the way down under danger plus this, the bot holds in its shaft while the dragon is
/// near.
pub const ONE_BLOW: f32 = 6.0;
/// Ticks after a blow inside the box in which the bot leaves it for the way down.
pub const BOX_BLOW_TICKS: u64 = 200;
/// Ticks a repeated note waits before it is written again.
pub const NOTE_EVERY: u64 = 200;
/// The dragon's phase is its metadata index 16, an INT (`EnderDragon.DATA_PHASE`, after the
/// mob's flags at 15; clojurecraft's `entity/phase-index`).
pub const DRAGON_PHASE_INDEX: u8 = 16;
/// `EnderDragonPhase` ids: 1 strafe player, 8 charging player; 5, 6, 7 the sitting phases.
pub const STRAFE_PHASE: i32 = 1;
pub const CHARGE_PHASE: i32 = 8;
pub const PERCH_PHASES: [i32; 3] = [5, 6, 7];

/// The block a pillar and its walls are made of: end stone alone. Vanilla's `dragon_immune` is
/// end stone, obsidian, iron bars and bedrock (E1, jar 26.1.2); cobblestone and netherrack are
/// smashed by the dragon flying through them, a pillar with the bot on it among them.
pub const PILLAR_ITEM: ItemKind = ItemKind::EndStone;

/// The pillar's block when one is held.
pub fn pillar_item(w: &World) -> Option<ItemKind> {
    (craft::count(w, PILLAR_ITEM) > 0).then_some(PILLAR_ITEM)
}

/// A spike: its column's centre, its radius (the top layer holds the cells with
/// dx² + dz² <= r² + 1, vanilla `SpikeFeature`), the y of its bedrock (E1's "top": the obsidian runs
/// to the cell under it, the crystal stands on it, at `top_y + 1`), and whether iron bars cage it. Read from
/// `config/end.toml`'s `[[spikes]]` (E1), else from the blocks round the crystal.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Spike {
    pub centre: [i32; 2],
    pub radius: i32,
    pub top_y: i32,
    #[serde(default)]
    pub caged: bool,
}

/// An axis out from a spike's column.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Axis {
    East,
    West,
    South,
    North,
}

impl Axis {
    pub const ALL: [Axis; 4] = [Axis::East, Axis::West, Axis::South, Axis::North];

    /// The step along it, (dx, dz).
    pub fn step(self) -> (i32, i32) {
        match self {
            Axis::East => (1, 0),
            Axis::West => (-1, 0),
            Axis::South => (0, 1),
            Axis::North => (0, -1),
        }
    }
}

/// Where a crystal is struck from along an axis: the pillar's column (x, z), the stand (the feet
/// cell `STAND_OFF` out, level with the bedrock), the two wall cells `WALL_OFF` out at the feet's
/// and the eye's height, and whether the stand is on the spike's top (a wide spike: walked onto
/// from the pillar's top).
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct StandPlan {
    pub axis: Axis,
    pub column: (i32, i32),
    pub stand: Cell,
    pub walls: [Cell; 2],
    pub on_spike: bool,
}

/// The stand for `spike` along `axis`, pure: the pillar in the first column out from the top
/// layer, `STAND_OFF` out at the nearest.
pub fn stand(spike: &Spike, axis: Axis) -> StandPlan {
    let (dx, dz) = axis.step();
    let [cx, cz] = spike.centre;
    let r2 = spike.radius * spike.radius + 1;
    let edge = (1..=SPIKE_SEARCH)
        .find(|k| k * k > r2)
        .unwrap_or(SPIKE_SEARCH + 1);
    let out = edge.max(STAND_OFF);
    let feet = spike.top_y;
    let at = |k: i32, y: i32| (cx + k * dx, y, cz + k * dz);
    StandPlan {
        axis,
        column: (cx + out * dx, cz + out * dz),
        stand: at(STAND_OFF, feet),
        walls: [at(WALL_OFF, feet), at(WALL_OFF, feet + 1)],
        on_spike: out > STAND_OFF,
    }
}

/// An approach found in the world: the stand plan and the pillar's foot (the feet cell on the
/// ground of its column).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Approach {
    pub plan: StandPlan,
    pub foot: Cell,
}

fn kind(w: &World, (x, y, z): Cell) -> Option<BlockKind> {
    w.block_at(BlockPos::new(x, y, z))
}

/// A cell a block may go into (air of any kind).
pub fn open(k: Option<BlockKind>) -> bool {
    matches!(
        k,
        Some(BlockKind::Air | BlockKind::CaveAir | BlockKind::VoidAir)
    )
}

fn full(k: Option<BlockKind>) -> bool {
    k.and_then(data::generated::shapes::collision) == Some(data::types::Collision::Full)
}

/// The feet height of the first ground in column (x, z) down from `top`: a full block seen with
/// an exposed face (no X-ray), the cells over it open. `None` over the void or unloaded.
pub fn ground(w: &World, x: i32, z: i32, top: i32) -> Option<i32> {
    for y in (top - GROUND_SEARCH..=top).rev() {
        let k = kind(w, (x, y, z));
        if full(k) {
            return crate::cast::seen(w, (x, y, z)).map(|_| y + 1);
        }
        if !open(k) {
            return None;
        }
    }
    None
}

/// The spike under a crystal at `pos`: from the geography when one there is known, else from the
/// blocks seen round it (its bedrock under the crystal, its top layer followed out along +x, bars
/// two out over it).
pub fn spike_for(w: &World, geography: &[Spike], pos: [f64; 3]) -> Option<Spike> {
    let (cx, cz) = (pos[0].floor() as i32, pos[2].floor() as i32);
    if let Some(s) = geography
        .iter()
        .find(|s| (s.centre[0] - cx).abs() <= 1 && (s.centre[1] - cz).abs() <= 1)
    {
        return Some(*s);
    }
    // The crystal stands on its bedrock (`SpikeFeature`: bedrock at the spike's height, the
    // crystal one over it), in the fire it lights.
    let by = (pos[1] + 0.01).floor() as i32 - 1;
    if crate::cast::seen(w, (cx, by, cz)) != Some(BlockKind::Bedrock) {
        return None;
    }
    let top_y = by;
    let radius = (1..=SPIKE_SEARCH)
        .take_while(|k| crate::cast::seen(w, (cx + k, by - 1, cz)) == Some(BlockKind::Obsidian))
        .count();
    let radius = i32::try_from(radius).ok().filter(|r| *r >= 1)?;
    let caged = Axis::ALL.iter().any(|a| {
        let (dx, dz) = a.step();
        kind(w, (cx + 2 * dx, by + 1, cz + 2 * dz)) == Some(BlockKind::IronBars)
    });
    Some(Spike {
        centre: [cx, cz],
        radius,
        top_y,
        caged,
    })
}

/// The approach along `axis`, or `None` when its column has no ground under the stand's height
/// or something in the way of the climb (the feet's and head's cells up to the top).
pub fn approach_along(w: &World, spike: &Spike, axis: Axis) -> Option<Approach> {
    let plan = stand(spike, axis);
    let top = plan.stand.1;
    let (x, z) = plan.column;
    // Down from under the stand: the box's roof over the stand is no ground (CI run
    // 45876961897: with it the approach was lost and the bot walked off to another).
    let base = ground(w, x, z, top - 1)?;
    if base > top {
        return None;
    }
    if !(base..=top + 1).all(|y| open(kind(w, (x, y, z)))) {
        return None;
    }
    Some(Approach {
        plan,
        foot: (x, base, z),
    })
}

/// The approach whose pillar foot is nearest the feet on the plane, of the four not refused.
pub fn best_approach(
    w: &World,
    spike: &Spike,
    feet: [f64; 3],
    refused: &[(i32, i32, Axis, u64)],
    tick: u64,
) -> Option<Approach> {
    let flat = |c: Cell| {
        let (dx, dz) = (
            f64::from(c.0) + 0.5 - feet[0],
            f64::from(c.2) + 0.5 - feet[2],
        );
        dx * dx + dz * dz
    };
    let mut found: Vec<Approach> = Axis::ALL
        .iter()
        .filter(|a| {
            !refused
                .iter()
                .any(|(x, z, r, until)| [*x, *z] == spike.centre && r == *a && tick < *until)
        })
        .filter_map(|a| approach_along(w, spike, *a))
        .collect();
    found.sort_by(|a, b| flat(a.foot).total_cmp(&flat(b.foot)));
    found.into_iter().next()
}

/// The face of a solid neighbour of `cell` a block put into `cell` is clicked against: the one
/// under first, then the sides, then the one over. `None` when nothing holds it.
pub fn support(w: &World, (x, y, z): Cell) -> Option<(Cell, i8)> {
    [
        ((x, y - 1, z), 1),
        ((x - 1, y, z), 5),
        ((x + 1, y, z), 4),
        ((x, y, z - 1), 3),
        ((x, y, z + 1), 2),
        ((x, y + 1, z), 0),
    ]
    .into_iter()
    .find(|(n, _)| full(kind(w, *n)))
}

/// What a block is put for.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Role {
    /// A side of the stand's box, at the feet's or the eye's height.
    Guard,
    /// Under a side's cell, so the side has a face to go against.
    Ledge,
    /// One of the two cells between the stand and the crystal.
    Wall,
    /// Over the head, the box's top.
    Roof,
}

/// The next block of the stand's box (the orchestrator, after race 45871173293's wall 2: a
/// strike needs reach, not sight, so the stand is closed): every side at the feet's and the
/// eye's height but the wall's own cells, a ledge under a feet cell nothing holds, then the roof.
/// End stone round the whole body leaves the dragon's wings and charge nothing to push it out
/// with. `None` with the box whole.
pub fn box_gap(w: &World, stand: Cell, walls: &[Cell; 2]) -> Option<(Cell, Role)> {
    let (x, y, z) = stand;
    let side = |a: &Axis, dy: i32| {
        let (dx, dz) = a.step();
        (x + dx, y + dy, z + dz)
    };
    let feet = Axis::ALL.iter().find_map(|a| {
        let c = side(a, 0);
        if walls.contains(&c) || !open(kind(w, c)) {
            return None;
        }
        Some(if support(w, c).is_some() {
            (c, Role::Guard)
        } else {
            ((c.0, c.1 - 1, c.2), Role::Ledge)
        })
    });
    let eye = || {
        Axis::ALL
            .iter()
            .map(|a| side(a, 1))
            .find(|c| !walls.contains(c) && open(kind(w, *c)))
            .map(|c| (c, Role::Guard))
    };
    let roof = (x, y + 2, z);
    // The roof has no face of its own to go against (the sides at the eye's height are under its
    // corners' neighbours, not under it): a block over one of them first (E6's sim, CI run
    // 45880921319: "nothing holds" the roof over a cage's stand, every tick).
    let roof_step = || {
        if !open(kind(w, roof)) {
            return None;
        }
        if support(w, roof).is_some() {
            return Some((roof, Role::Roof));
        }
        Axis::ALL
            .iter()
            .map(|a| side(a, 2))
            .find(|c| open(kind(w, *c)) && support(w, *c).is_some())
            .map(|c| (c, Role::Ledge))
    };
    feet.or_else(eye).or_else(roof_step)
}

/// Is the body boxed in where it stands: every side at the feet's and the eye's height filled,
/// and the cell over the head?
pub fn boxed(w: &World, feet: Cell) -> bool {
    let (x, y, z) = feet;
    let sides = Axis::ALL.iter().all(|a| {
        let (dx, dz) = a.step();
        (0..=1).all(|dy| !open(kind(w, (x + dx, y + dy, z + dz))))
    });
    sides && !open(kind(w, (x, y + 2, z)))
}

/// The first wall cell still open, or one holding a bar when bars are no shield (dug, then a
/// block put back). `None` when the line is shut.
pub fn wall_gap(w: &World, plan: &StandPlan, bars_shield: bool) -> Option<Cell> {
    plan.walls.iter().copied().find(|c| {
        let k = kind(w, *c);
        open(k) || (!bars_shield && k == Some(BlockKind::IronBars))
    })
}

/// The nearest live end crystal: its id and position.
pub fn nearest_crystal(w: &World, pos: [f64; 3]) -> Option<(i32, [f64; 3])> {
    let d = |p: [f64; 3]| (0..3).map(|i| (p[i] - pos[i]).powi(2)).sum::<f64>();
    w.entities
        .iter()
        .filter(|(_, e)| e.kind == EntityKind::EndCrystal && !e.dead)
        .min_by(|a, b| d(a.1.pos).total_cmp(&d(b.1.pos)))
        .map(|(id, e)| (*id, e.pos))
}

/// Blocks from `eye` to a crystal's box at `pos`.
pub fn crystal_reach(eye: [f64; 3], pos: [f64; 3]) -> f64 {
    let near = [
        eye[0].clamp(pos[0] - CRYSTAL_HALF, pos[0] + CRYSTAL_HALF),
        eye[1].clamp(pos[1], pos[1] + CRYSTAL_HEIGHT),
        eye[2].clamp(pos[2] - CRYSTAL_HALF, pos[2] + CRYSTAL_HALF),
    ];
    (0..3)
        .map(|i| (near[i] - eye[i]).powi(2))
        .sum::<f64>()
        .sqrt()
}

/// The dragon as the crystals' rules read it: its distance from the feet and its phase when the
/// server has said it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DragonNear {
    pub id: i32,
    pub dist: f64,
    pub phase: Option<i32>,
}

/// The live dragon nearest `pos`.
pub fn dragon_near(w: &World, pos: [f64; 3]) -> Option<DragonNear> {
    w.entities
        .iter()
        .filter(|(_, e)| e.kind == EntityKind::EnderDragon && !e.dead)
        .map(|(id, e)| DragonNear {
            id: *id,
            dist: (0..3)
                .map(|i| (e.pos[i] - pos[i]).powi(2))
                .sum::<f64>()
                .sqrt(),
            phase: match e.metadata.get(&DRAGON_PHASE_INDEX) {
                Some(MetaValue::Int(p)) => Some(*p),
                Some(_) | None => None,
            },
        })
        .min_by(|a, b| a.dist.total_cmp(&b.dist))
}

/// Is the dragon on its perch (a sitting phase)?
pub fn perched(d: &DragonNear) -> bool {
    d.phase.is_some_and(|p| PERCH_PHASES.contains(&p))
}

/// Blocks within which a charging dragon (phase 8) holds a bot on a pillar still in its column
/// (the orchestrator, after race 45885217441 lane 1: the charge struck it 2 over the ground as it
/// dug, 10 and 5, and flung it 48 up).
pub const CHARGE_NEAR: f64 = 40.0;

/// Is the dragon charging within `CHARGE_NEAR`?
pub fn charging(d: Option<&DragonNear>) -> bool {
    d.is_some_and(|d| d.dist <= CHARGE_NEAR && d.phase == Some(CHARGE_PHASE))
}

/// Is the dragon strafing or charging within `THREAT_NEAR`?
pub fn threatens(d: Option<&DragonNear>) -> bool {
    d.is_some_and(|d| d.dist <= THREAT_NEAR && matches!(d.phase, Some(STRAFE_PHASE | CHARGE_PHASE)))
}

/// Why a climb may not go on now, or `None` when it may: the dragon more than `CLIMB_FAR` off or
/// perched, and health over danger plus `CLIMB_MARGIN`.
pub fn climb_refusal(d: Option<&DragonNear>, health: Option<f32>, danger: f32) -> Option<String> {
    if let Some(h) = health
        && h <= danger + CLIMB_MARGIN
    {
        return Some(format!(
            "health {h} not over danger {danger} + {CLIMB_MARGIN}"
        ));
    }
    match d {
        Some(d) if d.dist <= CLIMB_FAR && !perched(d) => Some(format!(
            "the dragon {:.0} off in phase {:?}, not perched",
            d.dist, d.phase
        )),
        Some(_) | None => None,
    }
}

/// Why the dragon keeps the bot where it is (on the ground, or in its box), or `None`: it is
/// more than `CLIMB_FAR` off or perched.
pub fn dragon_refusal(d: Option<&DragonNear>) -> Option<String> {
    match d {
        Some(d) if d.dist <= CLIMB_FAR && !perched(d) => Some(format!(
            "the dragon {:.0} off in phase {:?}, not perched",
            d.dist, d.phase
        )),
        Some(_) | None => None,
    }
}

/// A pillar the bot put up: its column, its base (the feet's height on the ground) and top.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct PillarRec {
    pub x: i32,
    pub z: i32,
    pub base: i32,
}

/// The block under the feet is one of a pillar: a pillar the bot put up there (recorded, the
/// feet over its base), or a placed block no End ground is made of.
pub fn on_pillar(w: &World, feet: Cell, pillars: &[PillarRec]) -> bool {
    let under = kind(w, (feet.0, feet.1 - 1, feet.2));
    let recorded = pillars
        .iter()
        .any(|p| p.x == feet.0 && p.z == feet.2 && feet.1 > p.base);
    let placed = under.is_some_and(|k| PLACED.contains(&k));
    full(under) && (recorded || placed)
}

/// The feet stand on a spike's top: three obsidian under them (the End's platform is one thick).
pub fn on_spike(w: &World, feet: Cell) -> bool {
    (1..=3).all(|d| kind(w, (feet.0, feet.1 - d, feet.2)) == Some(BlockKind::Obsidian))
}

/// The crystals' next step from where the world says the bot is (clojurecraft's
/// `toward-crystals`), before the rules: the rules (`Plan::spikes_task`) may hold it back.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Next {
    /// Walk to this cell: the pillar's foot, a wide spike's stand, or back to the pillar's top.
    Walk {
        to: Cell,
        approach: Option<(i32, i32, Axis)>,
    },
    /// Rise in this column until the feet are at `top`.
    Pillar {
        column: (i32, i32),
        top: i32,
        axis: Axis,
        centre: [i32; 2],
    },
    /// Put a block into this cell.
    Build(Cell, Role),
    /// Dig a bar out of a wall cell (bars no shield).
    Bar(Cell),
    /// Dig a side of the box open, the way back to the pillar's top from a wide spike's stand.
    Open(Cell),
    /// Move to this cell's centre (sneaking, or not: into a pillar's dug shaft).
    Step { to: Cell, sneak: bool },
    /// Attack this crystal.
    Strike(i32),
    /// Dig this cell, the one under the feet: the pillar back down.
    DigDown(Cell),
    /// Nothing to do now, and why.
    Wait(String),
    /// No crystal left and the feet down.
    Done,
}

/// The state the crystals keep.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct SpikeState {
    /// The spikes' geography from `config/end.toml` (E1), set by the shell; empty reads the
    /// blocks round each crystal.
    pub geography: Vec<Spike>,
    /// Bars count as the wall (true, the default: parity e03 struck through intact bars for
    /// 0.42, the blast's least). False is the fallback: a cage's wall cells are dug and end
    /// stone put in.
    pub bars_shield: bool,
    /// The step the driver runs while no task does (`Plan::spikes_step`).
    pub doing: Option<Doing>,
    /// Pillars put up and not yet dug back.
    pub pillars: Vec<PillarRec>,
    /// Approaches refused (centre x, z, axis) until a tick.
    pub refused: Vec<(i32, i32, Axis, u64)>,
    /// The crystal struck last and when.
    pub struck: Option<(i32, u64)>,
    /// Counts for the ledger: pillars up, blocks risen, pillars down, strikes, crystals gone,
    /// blocks put, stalls.
    pub pillars_up: u32,
    pub risen: i32,
    pub pillars_down: u32,
    pub strikes: u32,
    pub gone: u32,
    pub placed: u32,
    pub stalls: u32,
    /// The last note's what and tick, so a held wait is not written every tick.
    pub last_note: Option<(String, u64)>,
    /// The water clutch in hand (a fall caught with the water bucket), and how many fired.
    pub clutch: Option<Clutch>,
    pub clutches: u32,
    /// Blocks put when the last climb reached its top, the stand last noted closed, and the
    /// health last seen (a blow inside the box is noted).
    pub top_placed: u32,
    pub closed_at: Option<Cell>,
    pub last_health: Option<f32>,
    /// When a blow last landed inside the box.
    pub box_blow_at: Option<u64>,
}

/// A fall caught with the water bucket (parity e06, E6: a 21-block fall onto water poured while
/// falling cost 0; with none, 18).
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Clutch {
    pub since: u64,
    /// The feet's height when the fall was caught.
    pub from_y: f64,
    /// The cell the water went into, once poured.
    pub poured: Option<Cell>,
    /// When the feet landed after the pour.
    pub landed_at: Option<u64>,
}

impl Default for SpikeState {
    fn default() -> Self {
        Self {
            geography: Vec::new(),
            bars_shield: true,
            doing: None,
            pillars: Vec::new(),
            refused: Vec::new(),
            struck: None,
            pillars_up: 0,
            risen: 0,
            pillars_down: 0,
            strikes: 0,
            gone: 0,
            placed: 0,
            stalls: 0,
            last_note: None,
            clutch: None,
            clutches: 0,
            top_placed: 0,
            closed_at: None,
            last_health: None,
            box_blow_at: None,
        }
    }
}

/// What the driver runs.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum Doing {
    /// Crouched, looking down, until a tick.
    Hold {
        until: u64,
    },
    Climb(Climb),
    Build(Build),
    Strike(Strike),
    /// To a cell's centre, until a tick.
    Step {
        to: Cell,
        sneak: bool,
        until: u64,
    },
}

/// A pillar's climb.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Climb {
    pub column: (i32, i32),
    pub goal: i32,
    pub centre: [i32; 2],
    pub axis: Axis,
    pub top: i32,
    pub since: u64,
    pub rose_at: u64,
    pub jump_from: Option<Cell>,
    pub clicked: bool,
}

/// A block put into a cell.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Build {
    pub cell: Cell,
    pub role: Role,
    pub clicked_at: Option<u64>,
    pub tries: u32,
}

/// A crystal struck.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Strike {
    pub target: i32,
    pub since: u64,
    pub last: Option<u64>,
}

/// The crystals' next step (see `Next`).
pub fn next(w: &World, st: &SpikeState, tick: u64) -> Next {
    let Some(body) = w.body else {
        return Next::Wait("no body".to_owned());
    };
    if !body.on_ground {
        // No route starts in the air (clojurecraft gym chain-w11: a walk asked every tick while
        // falling, twelve failed in 0.6 s).
        return Next::Wait("falling".to_owned());
    }
    let feet = path::stand_cell(w, body.pos);
    let crystal = nearest_crystal(w, body.pos);
    let approach = crystal.and_then(|(_, pos)| {
        let spike = spike_for(w, &st.geography, pos)?;
        best_approach(w, &spike, body.pos, &st.refused, tick).map(|a| (spike, a))
    });
    if let (Some((id, _)), Some((spike, a))) = (crystal, approach) {
        let p = a.plan;
        if feet == p.stand {
            // The blast's line, then the strike at once: no other block first. Race 45880674581
            // took its blows while the rest of a box went up, and E6's sim (CI run 45880795371)
            // stranded a wide spike's bot behind a box side over its own pillar's top.
            if let Some(c) = wall_gap(w, &p, st.bars_shield) {
                return if kind(w, c) == Some(BlockKind::IronBars) {
                    Next::Bar(c)
                } else {
                    Next::Build(c, Role::Wall)
                };
            }
            return Next::Strike(id);
        }
        if (feet.0, feet.2) == p.column && feet.1 < p.stand.1 {
            return Next::Pillar {
                column: p.column,
                top: p.stand.1,
                axis: p.axis,
                centre: spike.centre,
            };
        }
        if feet == (p.column.0, p.stand.1, p.column.1) {
            return Next::Walk {
                to: p.stand,
                approach: Some((spike.centre[0], spike.centre[1], p.axis)),
            };
        }
    }
    // Over a pillar's dug shaft, held up by the cell beside (the body off the column's centre):
    // into the shaft (race 45885217441 lane 0: the dig under (28, 93, 24) left the body on the
    // spike's edge, and it waited "no approach" up there for 1,600 ticks).
    if st
        .pillars
        .iter()
        .any(|r| (r.x, r.z) == (feet.0, feet.2) && feet.1 > r.base)
        && open(kind(w, (feet.0, feet.1 - 1, feet.2)))
    {
        return Next::Step {
            to: feet,
            sneak: false,
        };
    }
    // Held up by the floor beside the feet's cell (a body over an edge: E3's race 45890445738,
    // x 14.2 half on a stair's step over the bunker's air, every walk refused from there for
    // 14,000 ticks): onto the cell whose floor holds it first.
    if !full(kind(w, (feet.0, feet.1 - 1, feet.2)))
        && let Some(c) = held_by(w, body.pos, feet)
    {
        return Next::Step { to: c, sneak: true };
    }
    if on_spike(w, feet) {
        // Back to a pillar's top at the feet's height, or into its shaft once dug: the way down.
        return st
            .pillars
            .iter()
            .filter(|r| {
                (r.x - feet.0).abs() <= SPIKE_SEARCH
                    && (r.z - feet.2).abs() <= SPIKE_SEARCH
                    && feet.1 > r.base
                    && open(kind(w, (r.x, feet.1, r.z)))
                    && open(kind(w, (r.x, feet.1 + 1, r.z)))
            })
            .min_by_key(|r| (r.x - feet.0).abs() + (r.z - feet.2).abs())
            .map_or_else(
                || Next::Wait("stranded on a spike's top with no pillar down".to_owned()),
                |r| {
                    let (dx, dz) = ((r.x - feet.0).signum(), (r.z - feet.2).signum());
                    // The box's side toward the pillar opened first (its feet and eye cells).
                    let way = (0..=1)
                        .map(|dy| (feet.0 + dx, feet.1 + dy, feet.2 + dz))
                        .find(|c| !open(kind(w, *c)) && kind(w, *c) != Some(BlockKind::Obsidian));
                    let top = (r.x, feet.1, r.z);
                    let shaft = !full(kind(w, (r.x, feet.1 - 1, r.z)));
                    // The cells beside the shaft along an axis a body can stand in (race
                    // 45895161187 lane 0: a diagonal "beside" was walked to 900 times).
                    let beside: Vec<Cell> = Axis::ALL
                        .iter()
                        .map(|a| {
                            let (ax, az) = a.step();
                            (r.x + ax, feet.1, r.z + az)
                        })
                        .filter(|c| {
                            full(kind(w, (c.0, c.1 - 1, c.2)))
                                && open(kind(w, *c))
                                && open(kind(w, (c.0, c.1 + 1, c.2)))
                        })
                        .collect();
                    match way {
                        Some(c) => Next::Open(c),
                        // A dug shaft: walk to beside it on the spike, then step in.
                        None if shaft && beside.contains(&feet) => Next::Step {
                            to: top,
                            sneak: false,
                        },
                        None if shaft => beside
                            .iter()
                            .min_by_key(|c| (c.0 - feet.0).abs() + (c.2 - feet.2).abs())
                            .map_or_else(
                                || Next::Wait("no cell beside the pillar's shaft".to_owned()),
                                |c| Next::Walk {
                                    to: *c,
                                    approach: None,
                                },
                            ),
                        None => Next::Walk {
                            to: top,
                            approach: None,
                        },
                    }
                },
            );
    }
    if on_pillar(w, feet, &st.pillars) {
        return Next::DigDown((feet.0, feet.1 - 1, feet.2));
    }
    match (crystal, approach) {
        (Some(_), Some((spike, a))) => Next::Walk {
            to: a.foot,
            approach: Some((spike.centre[0], spike.centre[1], a.plan.axis)),
        },
        (Some((_, pos)), None) => Next::Wait(format!(
            "no approach to the crystal at ({:.1}, {:.1}, {:.1})",
            pos[0], pos[1], pos[2]
        )),
        (None, _) => Next::Done,
    }
}

/// The top of the ground under the feet (the first full block down, its top face), or `None`
/// over the void or past `GROUND_SEARCH`.
pub fn ground_under(w: &World, pos: [f64; 3]) -> Option<f64> {
    let (x, y, z) = path::feet_cell(pos);
    (y - GROUND_SEARCH..=y)
        .rev()
        .find(|cy| full(kind(w, (x, *cy, z))))
        .map(|cy| f64::from(cy + 1))
}

/// Feet over the ground at which a fall is caught (e06: a fall from a pillar's top; the clutch is
/// for more than a stumble), and at or under which the bucket is used, every tick until it pours
/// (e06's `above` 4.0 and 2.0 both poured; the use reaches 4.5 from the eye).
pub const CLUTCH_FROM: f64 = 4.0;
/// Blocks from the eye to the ground's top face a pour reaches: the player's block interaction
/// range 4.5, less a tenth.
pub const POUR_REACH: f64 = 4.4;

/// Whether the pour goes this tick (the orchestrator, after race 45885217441 lane 1's miss at 48
/// over the ground): the position this tick's movement sends (the feet plus the velocity, the
/// move before the tick's gravity and drag) has the eye within `POUR_REACH` of the ground's top
/// face and the feet still over it, and the next tick's does not. That is the last tick in reach,
/// predicted, not waited for: at about 3.9 a tick the reach is crossed in one or two ticks.
pub fn pour_now(feet_y: f64, vy: f64, ground: f64) -> bool {
    let p1 = feet_y + vy;
    let p2 = p1 + (vy - physics::GRAVITY) * physics::VERTICAL_DRAG;
    let in_reach = |p: f64| p > ground && p + physics::EYE_HEIGHT - ground <= POUR_REACH;
    in_reach(p1) && !in_reach(p2)
}
/// Ticks after landing the water is tried to be taken back, and the most a clutch lasts.
pub const CLUTCH_TAKE_TICKS: u64 = 40;
pub const CLUTCH_TICKS: u64 = 200;

/// The dig of the pillar's cell under the feet, its drop not waited for: it falls into the cell
/// the body falls into and is picked up there. Race 45871173293 dug a block every 40 ticks with
/// the drop's collect; lane 0 was struck off its pillar 280 ticks into a 33-block way down.
fn dig_down(w: &World, c: Cell, tick: u64) -> Dig {
    let mut d = Dig::new(w, c, tick);
    d.drop = None;
    d.others.clear();
    d
}

/// The cell beside the feet's whose full floor the body's box (0.6 wide) rests on, when the
/// feet's own cell has none.
pub fn held_by(w: &World, pos: [f64; 3], feet: Cell) -> Option<Cell> {
    let half = physics::HALF_WIDTH;
    let (fx, fz) = (pos[0] - f64::from(feet.0), pos[2] - f64::from(feet.2));
    [
        (-1, 0),
        (1, 0),
        (0, -1),
        (0, 1),
        (-1, -1),
        (-1, 1),
        (1, -1),
        (1, 1),
    ]
    .into_iter()
    .filter(|(dx, dz)| {
        let over_x = match dx {
            -1 => fx < half,
            1 => fx > 1.0 - half,
            _ => true,
        };
        let over_z = match dz {
            -1 => fz < half,
            1 => fz > 1.0 - half,
            _ => true,
        };
        over_x && over_z
    })
    .map(|(dx, dz)| (feet.0 + dx, feet.1, feet.2 + dz))
    .find(|c| full(kind(w, (c.0, c.1 - 1, c.2))) && open(kind(w, *c)))
}

fn eye(body: &physics::Body) -> [f64; 3] {
    [body.pos[0], body.pos[1] + physics::EYE_HEIGHT, body.pos[2]]
}

impl Plan {
    // >>> E4 (issue 20f)
    pub(crate) fn spikes_note(&mut self, tick: u64, what: String, why: String) {
        if let Some((last, at)) = &self.spikes.last_note
            && *last == what
            && tick.saturating_sub(*at) < NOTE_EVERY
        {
            return;
        }
        self.spikes.last_note = Some((what.clone(), tick));
        self.note(tick, what, why);
    }

    /// Is the bot up high on the crystals' work in the End: on a pillar it put up, on a spike's
    /// top, or climbing or stepping in its column?
    pub fn spikes_up_high(&self, w: &World) -> bool {
        if w.dimension.as_ref().map(|d| d.name.as_str()) != Some("minecraft:the_end") {
            return false;
        }
        let Some(body) = w.body else {
            return false;
        };
        let feet = path::stand_cell(w, body.pos);
        on_pillar(w, feet, &self.spikes.pillars)
            || on_spike(w, feet)
            || matches!(
                self.spikes.doing,
                Some(Doing::Climb(_) | Doing::Step { .. } | Doing::Build(_) | Doing::Strike(_))
            )
    }

    fn spikes_danger(&self) -> f32 {
        self.table
            .as_ref()
            .map_or(DANGER_DEFAULT, |t| t.survival.danger_health)
    }

    fn spikes_refuse(&mut self, centre: [i32; 2], axis: Axis, tick: u64) {
        self.spikes.refused.retain(|r| tick < r.3);
        self.spikes
            .refused
            .push((centre[0], centre[1], axis, tick + REFUSE_TICKS));
    }

    /// The crystal struck last gone from the world: noted once (CrystalGone).
    fn spikes_gone(&mut self, w: &World, tick: u64) {
        let Some((id, at)) = self.spikes.struck else {
            return;
        };
        let alive = w.entities.get(&id).is_some_and(|e| !e.dead);
        if alive {
            return;
        }
        self.spikes.struck = None;
        self.spikes.gone += 1;
        self.note(
            tick,
            format!("spikes: crystal gone {id}"),
            format!(
                "struck at tick {at}, gone {} ticks later; {} gone, {} strikes; health {:?}",
                tick.saturating_sub(at),
                self.spikes.gone,
                self.spikes.strikes,
                w.player.health
            ),
        );
    }

    /// The crystals' task in the End (called by `end_task` while a crystal is alive; `None` once
    /// none is left and the feet are down). `Some(Task::Idle)` while the driver
    /// (`spikes_step`) runs a climb, a block, a strike or a hold.
    pub fn spikes_task(&mut self, w: &World, tick: u64) -> Option<Task> {
        self.spikes_gone(w, tick);
        if self.spikes.doing.is_some() {
            self.retry_at = tick + 1;
            return Some(Task::Idle);
        }
        let body = w.body?;
        let feet = path::stand_cell(w, body.pos);
        // A pillar dug back to its base: noted (PillarDown) and forgotten.
        if let Some(i) = self
            .spikes
            .pillars
            .iter()
            .position(|p| p.x == feet.0 && p.z == feet.2 && feet.1 <= p.base && body.on_ground)
        {
            let p = self.spikes.pillars.remove(i);
            self.spikes.pillars_down += 1;
            self.note(
                tick,
                format!("spikes: pillar down at {feet:?}"),
                format!(
                    "the pillar in ({}, {}) dug back to its base {}; {} down",
                    p.x, p.z, p.base, self.spikes.pillars_down
                ),
            );
        }
        let danger = self.spikes_danger();
        let health = w.player.health;
        let d = dragon_near(w, body.pos);
        let n = next(w, &self.spikes, tick);
        let climbing = self
            .spikes
            .pillars
            .iter()
            .any(|p| (p.x, p.z) == (feet.0, feet.2) && feet.1 > p.base);
        // Out of the box (down, or out toward the pillar's top) only with the dragon far or
        // perched: inside it a blow pushes nothing out; on the way down it can.
        // A blow inside the box within `BOX_BLOW_TICKS` says the box is no shelter from this
        // dragon: down at once (race 45880674581: the wings and head struck through an open-topped
        // box, 5, 5 and 10).
        let struck = self
            .spikes
            .box_blow_at
            .is_some_and(|t| tick.saturating_sub(t) <= BOX_BLOW_TICKS);
        // A breath on the feet up a pillar: down out of it at once, ahead of every hold (E5's walk
        // out of a breath stands aside up high: there is no cell to walk to).
        if body.on_ground
            && on_pillar(w, feet, &self.spikes.pillars)
            && let Some(b) = crate::dragon::breath_on(&crate::dragon::breaths(w), body.pos)
        {
            let c = (feet.0, feet.1 - 1, feet.2);
            self.spikes_note(
                tick,
                format!("spikes: dig down out of the breath at {c:?}"),
                format!(
                    "a breath {:.1} off, radius {:.1}; health {health:?}",
                    crate::dragon::flat(b.pos, body.pos),
                    b.radius
                ),
            );
            return Some(Task::Dig(dig_down(w, c, tick)));
        }
        // A charge near: a bot up its pillar's column holds still there, neither digging the last
        // cells nor climbing on.
        if charging(d.as_ref())
            && body.on_ground
            && on_pillar(w, feet, &self.spikes.pillars)
            && matches!(
                n,
                Next::DigDown(_) | Next::Pillar { .. } | Next::Step { .. }
            )
        {
            self.spikes_note(
                tick,
                "spikes: hold in the shaft from the charge".to_owned(),
                format!(
                    "the dragon {:?} at {feet:?}; health {health:?}",
                    d.map(|d| (d.dist, d.phase))
                ),
            );
            return Some(self.spikes_hold(tick));
        }
        // Hurt on the way down (under danger plus one blow) with the dragon near and not perched:
        // held in the shaft, eating when food is short, until it is far or perched (the
        // orchestrator, after race 45895161187 lane 1: struck dead at 9 going down).
        if health.is_some_and(|h| h < danger + ONE_BLOW)
            && body.on_ground
            && on_pillar(w, feet, &self.spikes.pillars)
            && matches!(n, Next::DigDown(_) | Next::Step { .. })
            && let Some(why) = dragon_refusal(d.as_ref())
        {
            let food = self
                .table
                .as_ref()
                .and_then(|t| crate::eat::best_food(w, &t.survival));
            if w.player.food.is_some_and(|f| f < 20)
                && let Some(f) = food
            {
                self.spikes_note(
                    tick,
                    "spikes: eat in the shaft".to_owned(),
                    format!("{f:?}; health {health:?}, food {:?}; {why}", w.player.food),
                );
                return Some(Task::Eat(crate::eat::Eat::new(w, f, tick)));
            }
            self.spikes_note(
                tick,
                "spikes: hold in the shaft, hurt".to_owned(),
                format!(
                    "health {health:?} under {}; {why} at {feet:?}",
                    danger + ONE_BLOW
                ),
            );
            return Some(self.spikes_hold(tick));
        }
        if boxed(w, feet)
            && !struck
            && matches!(n, Next::DigDown(_) | Next::Open(_) | Next::Walk { .. })
            && let Some(why) = dragon_refusal(d.as_ref())
        {
            self.spikes_note(
                tick,
                "spikes: hold in the box".to_owned(),
                format!("{why}; at {feet:?}, health {health:?}"),
            );
            return Some(self.spikes_hold(tick));
        }
        match n {
            Next::Done => {
                self.spikes_note(
                    tick,
                    "spikes: no crystal left".to_owned(),
                    format!(
                        "feet down at {feet:?}; {} gone, {} pillars up, {} down",
                        self.spikes.gone, self.spikes.pillars_up, self.spikes.pillars_down
                    ),
                );
                None
            }
            Next::Wait(why) => {
                self.spikes_note(
                    tick,
                    "spikes: wait".to_owned(),
                    format!("{why} at {feet:?}"),
                );
                Some(self.spikes_hold(tick))
            }
            Next::Walk { to, approach } => {
                let mut walk = Walk::new(to, Goal::Block { pos: to }, Arrive::Goal, tick);
                if !walk.route_now(w, tick) || walk.route != Some(path::Status::Found) {
                    self.expanded += walk.expanded;
                    if let Some((x, z, a)) = approach {
                        self.spikes_refuse([x, z], a, tick);
                    }
                    self.spikes_note(
                        tick,
                        format!("spikes: no way to {to:?}"),
                        format!("from {feet:?}; approach {approach:?} refused"),
                    );
                    return Some(self.spikes_hold(tick));
                }
                self.spikes_note(
                    tick,
                    format!("spikes: walk to {to:?}"),
                    format!("from {feet:?}; approach {approach:?}"),
                );
                Some(Task::Explore(walk))
            }
            Next::Pillar {
                column,
                top,
                axis,
                centre,
            } => {
                // At the foot the rules; once up, no wait on a half-built pillar: on to the box.
                let refusal = climb_refusal(d.as_ref(), health, danger).or_else(|| {
                    threatens(d.as_ref()).then(|| "the dragon strafes or charges near".to_owned())
                });
                if let Some(why) = refusal {
                    if !climbing {
                        self.spikes_note(
                            tick,
                            "spikes: no climb".to_owned(),
                            format!("{why}; at {feet:?} under {top}"),
                        );
                        return Some(self.spikes_hold(tick));
                    }
                    self.spikes_note(
                        tick,
                        "spikes: climb on through".to_owned(),
                        format!("{why}; at {feet:?}, {} under {top}", top - feet.1),
                    );
                }
                if pillar_item(w).is_none() && climbing {
                    let c = (feet.0, feet.1 - 1, feet.2);
                    self.spikes_note(
                        tick,
                        format!("spikes: out of blocks, dig down at {c:?}"),
                        format!("{} under {top}", top - feet.1),
                    );
                    return Some(Task::Dig(dig_down(w, c, tick)));
                }
                if pillar_item(w).is_none() {
                    self.spikes_note(
                        tick,
                        "spikes: no block to pillar with".to_owned(),
                        format!("at {feet:?} under {top}"),
                    );
                    return Some(self.spikes_hold(tick));
                }
                if !self.spikes.pillars.iter().any(|p| (p.x, p.z) == column) {
                    self.spikes.pillars.push(PillarRec {
                        x: column.0,
                        z: column.1,
                        base: feet.1,
                    });
                }
                self.spikes_note(
                    tick,
                    format!("spikes: pillar up from {feet:?}"),
                    format!(
                        "to feet {top} for the spike at {centre:?} along {axis:?}; health {health:?}, the dragon {:?}",
                        d.map(|d| (d.dist, d.phase))
                    ),
                );
                self.spikes.doing = Some(Doing::Climb(Climb {
                    column,
                    goal: top,
                    centre,
                    axis,
                    top: feet.1,
                    since: tick,
                    rose_at: tick,
                    jump_from: None,
                    clicked: false,
                }));
                self.retry_at = tick + 1;
                Some(Task::Idle)
            }
            Next::Build(cell, role) => {
                if pillar_item(w).is_none() {
                    self.spikes_note(
                        tick,
                        "spikes: no block for the stand".to_owned(),
                        format!("{role:?} at {cell:?}"),
                    );
                    // No wait at an open stand: down.
                    if on_pillar(w, feet, &self.spikes.pillars) {
                        return Some(Task::Dig(dig_down(w, (feet.0, feet.1 - 1, feet.2), tick)));
                    }
                    return Some(self.spikes_hold(tick));
                }
                self.spikes_note(
                    tick,
                    format!("spikes: {role:?} block at {cell:?}"),
                    format!("standing at {feet:?}"),
                );
                self.spikes.doing = Some(Doing::Build(Build {
                    cell,
                    role,
                    clicked_at: None,
                    tries: 0,
                }));
                self.retry_at = tick + 1;
                Some(Task::Idle)
            }
            Next::Bar(cell) => {
                self.spikes_note(
                    tick,
                    format!("spikes: bar dug at {cell:?}"),
                    "bars no shield: end stone goes in its place".to_owned(),
                );
                // The bar's drop falls on the spike's top two out, off the stand: not fetched.
                let mut d = Dig::new(w, cell, tick);
                d.drop = None;
                d.others.clear();
                Some(Task::Dig(d))
            }
            Next::Open(cell) => {
                self.spikes_note(
                    tick,
                    format!("spikes: box opened at {cell:?}"),
                    format!("the way back to the pillar from {feet:?}"),
                );
                Some(Task::Dig(dig_down(w, cell, tick)))
            }
            Next::Strike(id) => {
                if health.is_some_and(|h| h <= danger) {
                    self.spikes_note(
                        tick,
                        "spikes: no strike under danger health".to_owned(),
                        format!("health {health:?}, danger {danger}"),
                    );
                    return Some(self.spikes_hold(tick));
                }
                if self.spikes.closed_at != Some(feet) {
                    self.spikes.closed_at = Some(feet);
                    self.note(
                        tick,
                        format!("spikes: stand walled at {feet:?}"),
                        format!(
                            "{} blocks put at the top; health {health:?}, the dragon {:?}",
                            self.spikes.placed.saturating_sub(self.spikes.top_placed),
                            d.map(|d| (d.dist, d.phase))
                        ),
                    );
                }
                self.spikes.doing = Some(Doing::Strike(Strike {
                    target: id,
                    since: tick,
                    last: None,
                }));
                self.retry_at = tick + 1;
                Some(Task::Idle)
            }
            Next::DigDown(c) => {
                // Centred over the column first: a body over its edge stays on the cell beside
                // when the block goes (race 45885217441 lane 0).
                let off = (f64::from(feet.0) + 0.5 - body.pos[0])
                    .abs()
                    .max((f64::from(feet.2) + 0.5 - body.pos[2]).abs());
                if off > STEP_NEAR {
                    self.spikes_note(
                        tick,
                        format!("spikes: centre over the pillar at {feet:?}"),
                        format!("{off:.2} off its centre before the dig down"),
                    );
                    self.spikes.doing = Some(Doing::Step {
                        to: feet,
                        sneak: true,
                        until: tick + STEP_TICKS,
                    });
                    self.retry_at = tick + 1;
                    return Some(Task::Idle);
                }
                self.spikes_note(
                    tick,
                    format!("spikes: dig down at {c:?}"),
                    format!("on a pillar at {feet:?}"),
                );
                Some(Task::Dig(dig_down(w, c, tick)))
            }
            Next::Step { to, sneak } => {
                self.spikes_note(
                    tick,
                    if sneak {
                        format!("spikes: step onto the floor at {to:?}")
                    } else {
                        format!("spikes: step into the shaft at {to:?}")
                    },
                    format!("from {feet:?}, sneaking {sneak}"),
                );
                self.spikes.doing = Some(Doing::Step {
                    to,
                    sneak,
                    until: tick + STEP_TICKS,
                });
                self.retry_at = tick + 1;
                Some(Task::Idle)
            }
        }
    }

    /// The water clutch (Bridger's plan 20f, parity e06): in the End, falling with more than
    /// `CLUTCH_FROM` under the feet and a water bucket held, the bucket to hand, used looking
    /// straight down in the last tick the eye is in reach of the ground (`pour_now`); landed, the water taken
    /// back. Every step noted.
    pub fn spikes_clutch(&mut self, w: &World, tick: u64) -> Option<Controls> {
        if w.dimension.as_ref().map(|d| d.name.as_str()) != Some("minecraft:the_end") {
            self.spikes.clutch = None;
            return None;
        }
        let body = w.body?;
        let down = Controls {
            yaw: w.player.yaw.into(),
            pitch: Some(crate::endermen::EYES_DOWN),
            ..Controls::default()
        };
        let ground = ground_under(w, body.pos);
        let height = ground.map(|g| body.pos[1] - g);
        let Some(mut c) = self.spikes.clutch.take() else {
            let falling = !body.on_ground && !body.in_water && body.vel[1] < 0.0;
            if !falling
                || height.is_none_or(|h| h <= CLUTCH_FROM)
                || craft::count(w, ItemKind::WaterBucket) == 0
            {
                return None;
            }
            self.spikes.clutches += 1;
            self.note(
                tick,
                format!("spikes: clutch begun at {:?}", path::feet_cell(body.pos)),
                format!(
                    "falling with {:.1} under the feet (velocity {:.2}); clutch {}; health {:?}; held {:?}, the bucket in slot {:?}",
                    height.unwrap_or(0.0),
                    body.vel[1],
                    self.spikes.clutches,
                    w.player.health,
                    crate::dig::held_item(w),
                    w.inventory
                        .slots
                        .iter()
                        .find(|(_, (k, _))| *k == ItemKind::WaterBucket)
                        .map(|(s, _)| *s)
                ),
            );
            self.spikes.clutch = Some(Clutch {
                since: tick,
                from_y: body.pos[1],
                poured: None,
                landed_at: None,
            });
            return Some(self.spikes_clutch_hold(w, ItemKind::WaterBucket, down));
        };
        let landed = body.on_ground || body.in_water;
        if tick.saturating_sub(c.since) > CLUTCH_TICKS {
            self.note(
                tick,
                "spikes: clutch over, out of time".to_owned(),
                format!("poured {:?}; health {:?}", c.poured, w.player.health),
            );
            return None;
        }
        match c.poured {
            None if landed => {
                self.note(
                    tick,
                    format!("spikes: clutch missed at {:?}", path::feet_cell(body.pos)),
                    format!(
                        "landed before the water poured, from {:.1}; health {:?}",
                        c.from_y, w.player.health
                    ),
                );
                None
            }
            None => {
                let ctl = self.spikes_clutch_hold(w, ItemKind::WaterBucket, down);
                if crate::dig::held_item(w) == Some(ItemKind::Bucket) {
                    let cell = ground.map(|g| {
                        let f = path::feet_cell(body.pos);
                        (f.0, g as i32, f.2)
                    });
                    c.poured = cell.or(Some(path::feet_cell(body.pos)));
                    self.note(
                        tick,
                        format!("spikes: clutch poured at {:?}", c.poured),
                        format!(
                            "{:.1} over the ground, falling {:.2}",
                            height.unwrap_or(0.0),
                            body.vel[1]
                        ),
                    );
                } else if crate::dig::held_item(w) == Some(ItemKind::WaterBucket)
                    && ground.is_some_and(|g| pour_now(body.pos[1], body.vel[1], g))
                    && self.acts.is_empty()
                {
                    self.note(
                        tick,
                        "spikes: clutch pour sent".to_owned(),
                        format!(
                            "{:.2} over the ground, falling {:.2}",
                            height.unwrap_or(0.0),
                            body.vel[1]
                        ),
                    );
                    self.acts.push(Act::UseItem);
                }
                self.spikes.clutch = Some(c);
                Some(ctl)
            }
            Some(cell) => {
                if !landed {
                    self.spikes.clutch = Some(c);
                    return Some(down);
                }
                let at = *c.landed_at.get_or_insert(tick);
                let water = matches!(kind(w, cell), Some(BlockKind::Water));
                if !water || crate::dig::held_item(w) == Some(ItemKind::WaterBucket) {
                    self.note(
                        tick,
                        format!("spikes: clutch over at {:?}", path::feet_cell(body.pos)),
                        format!(
                            "fell from {:.1}; the water {}; health {:?}",
                            c.from_y,
                            if water { "left" } else { "taken back" },
                            w.player.health
                        ),
                    );
                    return None;
                }
                if tick.saturating_sub(at) > CLUTCH_TAKE_TICKS {
                    self.note(
                        tick,
                        format!("spikes: clutch water left at {cell:?}"),
                        format!("not taken back in {CLUTCH_TAKE_TICKS} ticks"),
                    );
                    return None;
                }
                let target = [
                    f64::from(cell.0) + 0.5,
                    f64::from(cell.1) + 0.2,
                    f64::from(cell.2) + 0.5,
                ];
                let (yaw, pitch) = crate::walk::look_at(eye(&body), target);
                let ctl = self.spikes_clutch_hold(w, ItemKind::Bucket, down);
                if crate::dig::held_item(w) == Some(ItemKind::Bucket) && self.acts.is_empty() {
                    self.acts.push(Act::UseItem);
                }
                self.spikes.clutch = Some(c);
                Some(Controls {
                    yaw,
                    pitch: Some(pitch),
                    ..ctl
                })
            }
        }
    }

    /// `item` to hand for the clutch (an act when a swap is needed), looking down.
    fn spikes_clutch_hold(&mut self, w: &World, item: ItemKind, down: Controls) -> Controls {
        if let hold::Hold::Act(a) = hold::toward(w, item)
            && self.acts.is_empty()
        {
            self.acts.push(a);
        }
        down
    }

    fn spikes_hold(&mut self, tick: u64) -> Task {
        self.spikes.doing = Some(Doing::Hold {
            until: tick + HOLD_TICKS,
        });
        self.retry_at = tick + 1;
        Task::Idle
    }

    /// One tick of the crystals' driver while no task runs (the planner's step calls it ahead of
    /// the choice). `None` when nothing runs or it just ended.
    pub fn spikes_step(&mut self, w: &World, tick: u64) -> Option<Controls> {
        // A blow taken inside a closed box: noted (the box's own test).
        if let (Some(body), Some(h)) = (w.body, w.player.health) {
            let feet = path::stand_cell(w, body.pos);
            if let Some(last) = self.spikes.last_health
                && h + 0.01 < last
                && boxed(w, feet)
            {
                self.note(
                    tick,
                    format!("spikes: blow in the box at {feet:?}"),
                    format!(
                        "health {last} to {h}; the dragon {:?}; last strike {:?}",
                        dragon_near(w, body.pos).map(|d| (d.dist, d.phase)),
                        self.spikes.struck
                    ),
                );
            }
            if self.spikes.last_health.is_some_and(|l| h + 0.01 < l) && boxed(w, feet) {
                self.spikes.box_blow_at = Some(tick);
            }
            self.spikes.last_health = Some(h);
        }
        if !matches!(self.task, Task::Idle) {
            return None;
        }
        let mut doing = self.spikes.doing.take()?;
        let c = self.spikes_tick(w, tick, &mut doing);
        if c.is_some() {
            self.spikes.doing = Some(doing);
        } else {
            self.retry_at = tick;
        }
        c
    }

    fn spikes_tick(&mut self, w: &World, tick: u64, doing: &mut Doing) -> Option<Controls> {
        let body = w.body?;
        let down = Controls {
            yaw: w.player.yaw.into(),
            pitch: Some(crate::endermen::EYES_DOWN),
            ..Controls::default()
        };
        match doing {
            Doing::Hold { until } => {
                // A hold on a pillar outside a box ends the moment the dragon strafes or charges
                // near: the planner digs down (race 45871173293 lane 0, tick 1080).
                let feet = path::stand_cell(w, body.pos);
                let exposed =
                    body.on_ground && on_pillar(w, feet, &self.spikes.pillars) && !boxed(w, feet);
                let near = dragon_near(w, body.pos);
                if on_pillar(w, feet, &self.spikes.pillars)
                    && crate::dragon::breath_on(&crate::dragon::breaths(w), body.pos).is_some()
                {
                    self.note(
                        tick,
                        format!("spikes: hold ended by a breath at {feet:?}"),
                        "up a pillar; the way down out of it".to_owned(),
                    );
                    return None;
                }
                if exposed && threatens(near.as_ref()) && !charging(near.as_ref()) {
                    self.note(
                        tick,
                        format!("spikes: hold ended by the dragon at {feet:?}"),
                        "unguarded on a pillar; the way down".to_owned(),
                    );
                    return None;
                }
                (tick < *until).then_some(Controls {
                    sneak: true,
                    ..down
                })
            }
            Doing::Climb(c) => self.spikes_climb(w, tick, &body, c, down),
            Doing::Step { to, sneak, until } => {
                let (dx, dz) = (
                    f64::from(to.0) + 0.5 - body.pos[0],
                    f64::from(to.2) + 0.5 - body.pos[2],
                );
                if (dx.abs() <= STEP_NEAR && dz.abs() <= STEP_NEAR) || tick >= *until {
                    return None;
                }
                Some(Controls {
                    forward: true,
                    sneak: *sneak,
                    yaw: (-dx).atan2(dz).to_degrees(),
                    ..down
                })
            }
            Doing::Build(b) => self.spikes_build(w, tick, b, down),
            Doing::Strike(s) => self.spikes_strike(w, tick, &body, s, down),
        }
    }

    fn spikes_climb(
        &mut self,
        w: &World,
        tick: u64,
        body: &physics::Body,
        c: &mut Climb,
        down: Controls,
    ) -> Option<Controls> {
        let feet = path::feet_cell(body.pos);
        if body.on_ground && feet.1 > c.top {
            self.spikes.risen += feet.1 - c.top;
            c.top = feet.1;
            c.rose_at = tick;
            c.jump_from = None;
            c.clicked = false;
        }
        if body.on_ground && feet.1 >= c.goal {
            self.spikes.pillars_up += 1;
            self.spikes.top_placed = self.spikes.placed;
            self.note(
                tick,
                format!("spikes: pillar up to {feet:?}"),
                format!(
                    "risen to {} in {} ticks for the spike at {:?}; {} pillars up, {} risen",
                    c.goal,
                    tick.saturating_sub(c.since),
                    c.centre,
                    self.spikes.pillars_up,
                    self.spikes.risen
                ),
            );
            return None;
        }
        if tick.saturating_sub(c.rose_at) > PILLAR_STALL {
            self.spikes.stalls += 1;
            self.spikes_refuse(c.centre, c.axis, tick);
            self.note(
                tick,
                format!("spikes: pillar stalled at {feet:?}"),
                format!(
                    "no rise for {PILLAR_STALL} ticks (body at {:.2}, {:.2}, {:.2}, on the ground {}, over the head {:?}, held {:?}); the approach along {:?} refused",
                    body.pos[0],
                    body.pos[1],
                    body.pos[2],
                    body.on_ground,
                    kind(w, (feet.0, feet.1 + 2, feet.2)),
                    crate::dig::held_item(w),
                    c.axis
                ),
            );
            return None;
        }
        if let Some(j) = c.jump_from
            && !body.on_ground
        {
            arrival::rise_click(w, body, j, &mut c.clicked, &mut self.acts);
            return Some(down);
        }
        if !body.on_ground {
            return Some(down);
        }
        c.jump_from = None;
        c.clicked = false;
        // Off the column (a knock-back): the planner looks again.
        if (feet.0, feet.2) != c.column {
            self.note(
                tick,
                format!("spikes: climb off its column at {feet:?}"),
                format!("the column {:?}", c.column),
            );
            return None;
        }
        // A charge near: the climb stops in the column, held there (spikes_task).
        let d = dragon_near(w, body.pos);
        if charging(d.as_ref()) {
            self.note(
                tick,
                format!("spikes: climb held from the charge at {feet:?}"),
                format!("the dragon {:?}", d.map(|d| (d.dist, d.phase))),
            );
            return None;
        }
        // No wait on a half-built pillar (the orchestrator, after wall 2: lane 1 of race
        // 45871173293 held at y 85 and was struck off): the rules are read at the foot, and
        // the climb goes on to the box, noted when they would have held it.
        let refusal = climb_refusal(d.as_ref(), w.player.health, self.spikes_danger());
        if let Some(why) = refusal.or_else(|| {
            threatens(d.as_ref()).then(|| "the dragon strafes or charges near".to_owned())
        }) {
            self.spikes_note(
                tick,
                "spikes: climb on through".to_owned(),
                format!(
                    "{why}; at {feet:?}, {} under the goal {}",
                    c.goal - feet.1,
                    c.goal
                ),
            );
        }
        if !open(kind(w, (feet.0, feet.1 + 2, feet.2))) {
            self.spikes.stalls += 1;
            self.spikes_refuse(c.centre, c.axis, tick);
            self.note(
                tick,
                format!("spikes: pillar boxed at {feet:?}"),
                format!(
                    "{:?} over the head; the approach along {:?} refused",
                    kind(w, (feet.0, feet.1 + 2, feet.2)),
                    c.axis
                ),
            );
            return None;
        }
        let Some(item) = pillar_item(w) else {
            self.note(
                tick,
                format!("spikes: climb out of blocks at {feet:?}"),
                format!("{} under the goal", c.goal - feet.1),
            );
            return None;
        };
        let r = arrival::rise_jump(w, body, feet, item, down, &mut c.jump_from, &mut self.acts);
        if r.is_none() {
            self.note(
                tick,
                format!("spikes: climb lost its block at {feet:?}"),
                format!("{item:?} missing"),
            );
        }
        r
    }

    fn spikes_build(
        &mut self,
        w: &World,
        tick: u64,
        b: &mut Build,
        down: Controls,
    ) -> Option<Controls> {
        if !open(kind(w, b.cell)) {
            if b.clicked_at.is_some() {
                self.spikes.placed += 1;
            }
            self.note(
                tick,
                format!("spikes: {:?} in at {:?}", b.role, b.cell),
                format!(
                    "{:?} there after {} tries; {} blocks put",
                    kind(w, b.cell),
                    b.tries,
                    self.spikes.placed
                ),
            );
            return None;
        }
        if let Some(at) = b.clicked_at {
            if tick.saturating_sub(at) < PLACE_CONFIRM {
                return Some(Controls {
                    sneak: true,
                    ..down
                });
            }
            b.clicked_at = None;
        }
        if b.tries >= PLACE_TRIES {
            self.note(
                tick,
                format!("spikes: {:?} refused at {:?}", b.role, b.cell),
                format!("{} clicks with no block shown", b.tries),
            );
            return None;
        }
        let Some((n, face)) = support(w, b.cell) else {
            self.spikes_note(
                tick,
                format!("spikes: nothing holds {:?}", b.cell),
                format!("{:?} with no solid neighbour", b.role),
            );
            return None;
        };
        let Some(item) = pillar_item(w) else {
            self.note(
                tick,
                format!("spikes: no block for {:?}", b.cell),
                format!("{:?}", b.role),
            );
            return None;
        };
        match hold::toward(w, item) {
            hold::Hold::Held => {
                if self.acts.is_empty() && w.click.is_none() {
                    b.tries += 1;
                    b.clicked_at = Some(tick);
                    self.acts.push(Act::UseOn {
                        pos: BlockPos::new(n.0, n.1, n.2),
                        face,
                    });
                }
                Some(Controls {
                    sneak: true,
                    ..down
                })
            }
            hold::Hold::Act(a) => {
                if self.acts.is_empty() {
                    self.acts.push(a);
                }
                Some(Controls {
                    sneak: true,
                    ..down
                })
            }
            hold::Hold::Wait => Some(Controls {
                sneak: true,
                ..down
            }),
            hold::Hold::Missing => None,
        }
    }

    fn spikes_strike(
        &mut self,
        w: &World,
        tick: u64,
        body: &physics::Body,
        s: &mut Strike,
        down: Controls,
    ) -> Option<Controls> {
        let Some(e) = w.entities.get(&s.target).filter(|e| !e.dead) else {
            self.spikes_gone(w, tick);
            return None;
        };
        let reach = crystal_reach(eye(body), e.pos);
        let danger = self.spikes_danger();
        if w.player.health.is_some_and(|h| h <= danger) {
            self.note(
                tick,
                "spikes: strike dropped under danger health".to_owned(),
                format!("health {:?}, danger {danger}", w.player.health),
            );
            return None;
        }
        if reach > STRIKE_REACH {
            self.note(
                tick,
                format!("spikes: crystal {} out of reach", s.target),
                format!("{reach:.2} from the eye"),
            );
            return None;
        }
        if tick.saturating_sub(s.since) > STRIKE_TICKS {
            self.note(
                tick,
                format!("spikes: strike on {} gave up", s.target),
                format!("still there after {STRIKE_TICKS} ticks"),
            );
            return None;
        }
        let centre = [e.pos[0], e.pos[1] + CRYSTAL_HEIGHT / 2.0, e.pos[2]];
        let (yaw, pitch) = crate::walk::look_at(eye(body), centre);
        if s.last.is_none_or(|l| tick.saturating_sub(l) >= SWING_EVERY) && self.acts.is_empty() {
            s.last = Some(tick);
            self.spikes.strikes += 1;
            self.spikes.struck = Some((s.target, tick));
            self.note(
                tick,
                format!("spikes: strike {}", s.target),
                format!(
                    "reach {reach:.2} from {:?}; health {:?}; strike {}",
                    path::feet_cell(body.pos),
                    w.player.health,
                    self.spikes.strikes
                ),
            );
            self.acts.push(Act::Attack { entity: s.target });
        }
        Some(Controls {
            sneak: true,
            yaw,
            pitch: Some(pitch),
            ..down
        })
    }
    // <<< E4
}
