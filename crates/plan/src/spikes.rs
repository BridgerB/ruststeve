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
//! **Rules.** Climb only while the dragon holds (phase 0) `CLIMB_NEAR` off or more, not closing at the start,
//! with health over danger plus `CLIMB_MARGIN`. A fireball near or a strafe within
//! `THREAT_NEAR` takes a bot up a pillar down at once, box or not: into the landing water, else
//! off the side onto the clutch, else the dig. A charge within `THREAT_NEAR` holds it crouched
//! in its guard ring or shaft. Never a strike under danger health. A pillar that rises no block in `PILLAR_STALL` ticks
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
/// End stone over a pillar's own height a climb wants held before it begins: the stand's two
/// wall blocks and two for a lost click.
pub const PILLAR_SPARE: i32 = 4;
/// Blocks past the landing cell's centre, away from the pillar, the landing pour aims at: from an
/// eye over the column's centre (within `STEP_NEAR`) the ray then clears the first block's top
/// edge (eye 1.62 over that top, 2.62 over the landing floor: at the edge the ray is past it).
pub const LANDING_AIM: f64 = 0.35;
pub const STEP_TICKS: u64 = 60;
/// One blow of the dragon's (its breath's 6 through iron, race 45875049490; a wing's 5, a head's
/// 10): on the way down under danger plus this, the bot holds in its shaft while the dragon is
/// near.
pub const ONE_BLOW: f32 = 6.0;
/// Ticks after a blow inside the box in which the bot leaves it for the way down.
pub const BOX_BLOW_TICKS: u64 = 200;
/// Ticks between two uses of a bucket that has not yet done its work, and the most it is tried.
pub const BUCKET_EVERY: u64 = 10;
pub const BUCKET_TICKS: u64 = 100;
/// End stone held at or over which the way down is the waterfall, the pillar left standing
/// (under it the pillar is dug back for its blocks).
pub const WATERFALL_KEEP: i32 = 48;
/// Blocks from the bunker within which the pool is made, and from the pool a refill walks.
pub const POOL_NEAR: i32 = 8;
pub const POOL_FAR: i32 = 64;
/// Ticks a dragon reading counts toward "closing", and the blocks closer it must have come: its
/// distance falling against a reading taken every `CLOSING_TICKS` / 2 (20) ticks (the
/// orchestrator's climb rule, after race 46071506005).
pub const CLOSING_TICKS: u64 = 40;
pub const CLOSING_BY: f64 = 1.0;
/// Blocks a holding dragon (phase 0) may come within and a climb still begin and go on: the
/// holding pattern's flight is what a climb is made through; the knock-offs came from charges,
/// perches and take-offs (the orchestrator, after race 46071506005: with "over 64" a 30-block
/// climb, about 360 ticks, almost never saw the dragon far enough for long enough, and 4 climbs
/// in a lane stopped low, 0 strikes in 5 lanes).
pub const CLIMB_NEAR: f64 = 40.0;
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

/// Every full block a pillar's body may be made of, end stone last (part 2, the orchestrator,
/// after race 45994686575 lane 5: "too few blocks" with 44 end stone, 28 cobblestone and 64
/// cobbled deepslate held). Only end stone is dragon-immune: the others go under the top two and
/// never in the stand's walls, so a dragon flying through the body may smash it (accepted: the
/// bot comes down by water, not by the pillar).
pub const PILLAR_BLOCKS: [ItemKind; 9] = [
    ItemKind::CobbledDeepslate,
    ItemKind::Cobblestone,
    ItemKind::Netherrack,
    ItemKind::Blackstone,
    ItemKind::Andesite,
    ItemKind::Diorite,
    ItemKind::Granite,
    ItemKind::Tuff,
    ItemKind::EndStone,
];
/// End stone blocks at a pillar's top (the two under the stand's feet).
pub const PILLAR_TOP: i32 = 2;

/// All the blocks held a pillar may be made of.
pub fn pillar_blocks(w: &World) -> i32 {
    PILLAR_BLOCKS.iter().map(|k| craft::count(w, *k)).sum()
}

/// The block a climb puts next: end stone for the top `PILLAR_TOP` (the block put at the feet's
/// height lifts them one), else the first held of the others, end stone kept; any held when the
/// preferred is not.
pub fn pillar_block_for(w: &World, feet_y: i32, goal: i32) -> Option<ItemKind> {
    let held = |k: &ItemKind| craft::count(w, *k) > 0;
    if feet_y >= goal - PILLAR_TOP && held(&ItemKind::EndStone) {
        return Some(ItemKind::EndStone);
    }
    PILLAR_BLOCKS.iter().copied().find(held)
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

/// Blocks within which a dragon fireball in flight keeps the bot off its pillars (the strafe
/// fires from 64, `THREAT_NEAR`).
pub const FIREBALL_NEAR: f64 = 64.0;

/// The distance to the nearest dragon fireball in flight within `FIREBALL_NEAR`, if any.
pub fn fireball_near(w: &World, pos: [f64; 3]) -> Option<f64> {
    w.entities
        .values()
        .filter(|e| e.kind == EntityKind::DragonFireball && !e.dead)
        .map(|e| {
            (0..3)
                .map(|i| (e.pos[i] - pos[i]).powi(2))
                .sum::<f64>()
                .sqrt()
        })
        .filter(|d| *d <= FIREBALL_NEAR)
        .min_by(f64::total_cmp)
}

/// The hold step toward a water bucket that is not the clutch's: another hotbar one, else one
/// from the main slots swapped into a hotbar slot that is not the clutch's.
pub fn pour_hold(w: &World) -> hold::Hold {
    if w.click.is_some() || w.window.is_some() || w.inventory.carried.is_some() {
        return hold::Hold::Wait;
    }
    let water = |s: &i16| {
        w.inventory
            .slots
            .get(s)
            .is_some_and(|(k, _)| *k == ItemKind::WaterBucket)
    };
    let hot: Vec<i16> = (36i16..=44).filter(|s| water(s)).collect();
    let clutch = hot.iter().max().copied();
    let held = 36 + i16::from(w.held);
    if let Some(other) = hot.iter().copied().find(|s| Some(*s) != clutch) {
        if other == held {
            return hold::Hold::Held;
        }
        return hold::Hold::Act(Act::Hold {
            slot: u8::try_from(other - 36).unwrap_or(0),
        });
    }
    match (9i16..=35).find(|s| water(s)) {
        Some(m) => {
            let button = (0i8..=8)
                .find(|b| Some(36 + i16::from(*b)) != clutch)
                .unwrap_or(0);
            hold::Hold::Act(Act::Click {
                slot: m,
                button,
                mode: 2,
            })
        }
        // Only the clutch's: it is poured (the landing still catches the drop).
        None if clutch == Some(held) => hold::Hold::Held,
        None => hold::toward(w, ItemKind::WaterBucket),
    }
}

/// Is the cell a water source by the server's word (its block state's level 0)?
pub fn source(w: &World, c: Cell) -> bool {
    w.fluid(c.0, c.1, c.2)
        .is_some_and(|f| f.kind == physics::FluidKind::Water && f.level == 0)
}

/// The clutch's water round a landed body: a source in the feet's cell or the one under, else in
/// a cell beside either (a body knocked sideways lands off the cell it poured over). None when
/// no source is there (E5's read of frontier race 45954274931: bots that stood in clutch water
/// spread over the ground found no start for a walk or a dodge, NoPath to their deaths).
pub fn clutch_water(w: &World, feet: Cell) -> Option<Cell> {
    let near = [0, -1]
        .into_iter()
        .flat_map(|dy| {
            let c = (feet.0, feet.1 + dy, feet.2);
            std::iter::once(c).chain(Axis::ALL.iter().map(move |a| {
                let (dx, dz) = a.step();
                (c.0 + dx, c.1, c.2 + dz)
            }))
        })
        .find(|c| source(w, *c));
    // Further out: within `CLUTCH_WATER_REACH` on the plane, from 2 under the feet to them, the
    // nearest (race 46059621531 lane 2: poured on an edge's column, landed on a higher one, "the
    // water gone, not taken back; 0 water buckets held").
    near.or_else(|| {
        let r = CLUTCH_WATER_REACH;
        let mut cells: Vec<Cell> = (-r..=r)
            .flat_map(|dx| (-r..=r).flat_map(move |dz| (-2..=0).map(move |dy| (dx, dy, dz))))
            .map(|(dx, dy, dz)| (feet.0 + dx, feet.1 + dy, feet.2 + dz))
            .filter(|c| source(w, *c))
            .collect();
        cells.sort_by_key(|c| {
            (
                (c.0 - feet.0).abs() + (c.1 - feet.1).abs() + (c.2 - feet.2).abs(),
                *c,
            )
        });
        cells.into_iter().next()
    })
}

/// Blocks on the plane round a left water cell a dry stand to refill from is looked for in, and
/// the failed walks to it before it is dropped from the record.
pub const LEFT_WATER_STAND: i32 = 3;
pub const LEFT_WATER_TRIES: u32 = 5;
pub const LEFT_WATER_RETRY: u64 = 200;
/// Ticks a bot up an open pillar with no water way down is held crouched for the dragon before
/// it digs down all the same (a charge passes in about 100).
pub const OPEN_HOLD_MAX: u64 = 200;

/// Blocks on the plane round the landed feet the clutch's water is looked for in.
pub const CLUTCH_WATER_REACH: i32 = 2;

/// Off a spike's top with no pillar down: the nearest cell of its top, at the feet's height,
/// with a side whose fall lands on ground (not the void), and that side's cell. `None` with none
/// within `SPIKE_SEARCH`.
pub fn spike_edge(w: &World, feet: Cell) -> Option<(Cell, Cell)> {
    let r = SPIKE_SEARCH;
    let mut found: Vec<(i32, Cell, Cell)> = Vec::new();
    for dx in -r..=r {
        for dz in -r..=r {
            let s = (feet.0 + dx, feet.1, feet.2 + dz);
            if !(on_spike(w, s) && passable(kind(w, s)) && passable(kind(w, (s.0, s.1 + 1, s.2)))) {
                continue;
            }
            for a in Axis::ALL {
                let (ax, az) = a.step();
                let o = (s.0 + ax, s.1, s.2 + az);
                let lands = (o.1 - GROUND_SEARCH..o.1).any(|y| full(kind(w, (o.0, y, o.2))));
                if passable(kind(w, o))
                    && passable(kind(w, (o.0, o.1 + 1, o.2)))
                    && open(kind(w, (o.0, o.1 - 1, o.2)))
                    && lands
                {
                    found.push((dx.abs() + dz.abs(), s, o));
                }
            }
        }
    }
    found.sort_by_key(|(d, s, o)| (*d, *s, *o));
    found.into_iter().next().map(|(_, s, o)| (s, o))
}

/// A cell a body passes through: open, or water (a waterfall's pour on a pillar's top).
fn passable(k: Option<BlockKind>) -> bool {
    open(k) || k == Some(BlockKind::Water)
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
    /// A step's floor of a dig-out's stair, put where it was missing.
    Stair,
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
    crystals_by_distance(w, pos).into_iter().next()
}

/// The live crystals, nearest first (ties by id, so the order is the same every tick).
pub fn crystals_by_distance(w: &World, pos: [f64; 3]) -> Vec<(i32, [f64; 3])> {
    let d = |p: [f64; 3]| (0..3).map(|i| (p[i] - pos[i]).powi(2)).sum::<f64>();
    let mut all: Vec<(i32, [f64; 3])> = w
        .entities
        .iter()
        .filter(|(_, e)| e.kind == EntityKind::EndCrystal && !e.dead)
        .map(|(id, e)| (*id, e.pos))
        .collect();
    all.sort_by(|a, b| d(a.1).total_cmp(&d(b.1)).then(a.0.cmp(&b.0)));
    all
}

/// Can a crystal at `pos` heal the perched dragon: within its box grown by the heal's range on x
/// and z from the perch point (E5's jar read, `EnderDragon.checkCrystals`, javap probe
/// 46039100437: `dragon::HEAL_RANGE` 32 plus `dragon::HEAL_BOX_HALF` 8, 40 a side; 8 of the 10
/// spikes are in it, only (42, 0) and (-42, -1) are not).
pub fn heals_the_perch(pos: [f64; 3]) -> bool {
    let reach = crate::dragon::HEAL_RANGE + crate::dragon::HEAL_BOX_HALF;
    let [px, _, pz] = crate::dragon::PERCH_POINT;
    (pos[0] - px).abs() <= reach && (pos[2] - pz).abs() <= reach
}

/// The crystals in the order they are worked: those that heal the perch first, nearest first
/// among them, then the rest nearest first (E5 fights the perch once none standing heals it:
/// dev race 46048455555 lane 1 took the caged (42, 0) first and left (12, 39), which healed the
/// dragon back to 200).
pub fn crystals_in_order(w: &World, pos: [f64; 3]) -> Vec<(i32, [f64; 3])> {
    let mut all = crystals_by_distance(w, pos);
    all.sort_by_key(|(_, p)| !heals_the_perch(*p));
    all
}

/// Cells a walk from the feet may reach before the feet are judged free; fewer, and the bot
/// is shut in a pocket (a sealed bunker, a notch) and digs out at once.
pub const POCKET_CELLS: usize = 48;

/// Is the bot shut in: fewer than `POCKET_CELLS` standable cells reachable from the feet by
/// steps of one along an axis, up one (room over the head) or down up to three (race
/// 45949063425 lane 1: sealed in its bunker, every approach of a spike was walked at and
/// refused, 40 ticks each, before the dig-out began).
pub fn shut_in(w: &World, feet: Cell) -> bool {
    let stands = |c: Cell| {
        full(kind(w, (c.0, c.1 - 1, c.2)))
            && passable(kind(w, c))
            && passable(kind(w, (c.0, c.1 + 1, c.2)))
    };
    let mut seen: std::collections::BTreeSet<Cell> = std::collections::BTreeSet::new();
    let mut todo = vec![feet];
    seen.insert(feet);
    while let Some(c) = todo.pop() {
        if seen.len() >= POCKET_CELLS {
            return false;
        }
        for a in Axis::ALL {
            let (dx, dz) = a.step();
            let up = passable(kind(w, (c.0, c.1 + 2, c.2)));
            for dy in [1, 0, -1, -2, -3] {
                let n = (c.0 + dx, c.1 + dy, c.2 + dz);
                let way = match dy {
                    1 => up,
                    0 => true,
                    _ => (dy + 1..=0).all(|y| {
                        passable(kind(w, (n.0, c.1 + y, n.2)))
                            && passable(kind(w, (n.0, c.1 + y + 1, n.2)))
                    }),
                };
                if way && stands(n) && seen.insert(n) {
                    todo.push(n);
                }
            }
        }
    }
    seen.len() < POCKET_CELLS
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

/// `EnderDragonPhase` ids for the landing and the take-off: with the charge, the phases whose
/// flight passes over the bot and knocks it.
pub const LANDING_PHASE: i32 = 3;
pub const TAKEOFF_PHASE: i32 = 4;
/// Blocks within which a landing, take-off or charging dragon knocks a bot off its pillar or
/// out of a drop (part 2, the orchestrator, after race 45994686575 lanes 3 and 4:
/// `death.fell.assist`, flung over the void).
pub const KNOCK_NEAR: f64 = 30.0;

/// Why the dragon would knock the bot off now, or `None`: landing, taking off or charging
/// within `KNOCK_NEAR`.
pub fn knock_risk(d: Option<&DragonNear>) -> Option<String> {
    d.filter(|d| {
        d.dist <= KNOCK_NEAR
            && matches!(d.phase, Some(LANDING_PHASE | TAKEOFF_PHASE | CHARGE_PHASE))
    })
    .map(|d| format!("the dragon {:.0} off in phase {:?}", d.dist, d.phase))
}

/// Is the dragon strafing or charging within `THREAT_NEAR`?
pub fn threatens(d: Option<&DragonNear>) -> bool {
    d.is_some_and(|d| d.dist <= THREAT_NEAR && matches!(d.phase, Some(STRAFE_PHASE | CHARGE_PHASE)))
}

/// Why a climb may not begin or go on now, or `None` when it may: the dragon holding (phase 0)
/// `CLIMB_NEAR` off or more, and health over danger plus `CLIMB_MARGIN` (closing bars only the
/// start, in `spikes_task`).
pub fn climb_refusal(d: Option<&DragonNear>, health: Option<f32>, danger: f32) -> Option<String> {
    if let Some(h) = health
        && h <= danger + CLIMB_MARGIN
    {
        return Some(format!(
            "health {h} not over danger {danger} + {CLIMB_MARGIN}"
        ));
    }
    // Only with the dragon holding (phase 0), `CLIMB_NEAR` off or more: a perched dragon scans and
    // then charges whoever it sees (E9's read of race 46038776204 lanes 1, 3 and 4: the charge
    // came 80 to 160 ticks after "sitting-scanning -> charging"; lane 1 began its climb with the
    // dragon sitting at the fountain and was struck 11 up).
    match d {
        Some(d) if d.dist < CLIMB_NEAR || d.phase != Some(HOLDING_PHASE) => Some(format!(
            "the dragon {:.0} off in phase {:?}, not holding {CLIMB_NEAR:.0} off or more",
            d.dist, d.phase
        )),
        Some(_) | None => None,
    }
}

/// Blocks under the stand within which a climb finishes through a refusal (the orchestrator's
/// one exception, after race 46075095202: "climb stopped at (34, 70, -29) (the dragon 46 off in
/// phase 0 …; 1 under the goal 71)", 0 strikes in 6 lanes).
pub const STAND_NEAR: i32 = 5;
/// The phases that deny the finish within `KNOCK_NEAR`: the landing, the sitting phases, the
/// charge.
pub const FINISH_DENIED: [i32; 5] = [3, 5, 6, 7, 8];

/// The one exception to "a refusal mid-climb stops it": within `STAND_NEAR` of the stand, the
/// climb finishes in any phase but a landing, sitting or charge within `KNOCK_NEAR` (the
/// take-off's knock is held by `knock_risk` first).
pub fn climb_on_near_stand(d: Option<&DragonNear>, under: i32) -> bool {
    under <= STAND_NEAR
        && !d.is_some_and(|d| {
            d.dist <= KNOCK_NEAR && d.phase.is_some_and(|p| FINISH_DENIED.contains(&p))
        })
}

/// The dragon's holding pattern (`EnderDragonPhase` 0), the one phase a climb begins in.
pub const HOLDING_PHASE: i32 = 0;
/// The phases that end a stay up a pillar: the landing approach and the landing, the sitting
/// phases (a scan ends in a charge at whoever it sees), the charge.
pub const DESCEND_PHASES: [i32; 6] = [2, 3, 5, 6, 7, 8];

/// Is the dragon in a phase that ends a stay up a pillar (`DESCEND_PHASES`)?
pub fn descend_from(d: Option<&DragonNear>) -> bool {
    d.is_some_and(|d| d.phase.is_some_and(|p| DESCEND_PHASES.contains(&p)))
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
    // Unrecorded, a placed block is a pillar only standing free, three sides of it open (frontier
    // race 45954274931 lane 0: E3's stair tunnel put cobbled deepslate under the feet, read as a
    // pillar, and the bot dug its own floor down into a pit it could not walk out of).
    let free = || {
        Axis::ALL
            .iter()
            .filter(|a| {
                let (dx, dz) = a.step();
                !full(kind(w, (feet.0 + dx, feet.1 - 1, feet.2 + dz)))
            })
            .count()
            >= 3
    };
    let placed = under.is_some_and(|k| PLACED.contains(&k)) && free();
    full(under) && (recorded || placed || tower(w, feet))
}

/// Blocks of a free-standing column under the feet that make it a pillar with no record.
pub const TOWER_HEIGHT: i32 = 3;

/// The feet on a free-standing column of full blocks at least `TOWER_HEIGHT` high, three sides
/// of each open: a pillar whatever its block and record (the crystals frontier, race 46103123144:
/// the bow line's snapshot stands on its own end stone pillar at (42, 82, 3), no record after the
/// resume, end stone no `PLACED` block, and with every crystal gone the crystals' task handed
/// the bot to the fight 20 up).
pub fn tower(w: &World, feet: Cell) -> bool {
    (1..=TOWER_HEIGHT).all(|d| {
        let y = feet.1 - d;
        full(kind(w, (feet.0, y, feet.2)))
            && Axis::ALL
                .iter()
                .filter(|a| {
                    let (dx, dz) = a.step();
                    !full(kind(w, (feet.0 + dx, y, feet.2 + dz)))
                })
                .count()
                >= 3
    })
}

/// The feet up on a spike: on its top (`on_spike`), or on its cage's bars or its bedrock.
pub fn on_spike_top(w: &World, feet: Cell) -> bool {
    on_spike(w, feet)
        || matches!(
            kind(w, (feet.0, feet.1 - 1, feet.2)),
            Some(BlockKind::IronBars | BlockKind::Bedrock)
        )
}

/// Inside a spike's cage with no edge: the bar beside the feet to break, at the feet's height
/// first and then the head's, on the side facing the island's centre best (race 46070173620:
/// the crystals_5 stand on the caged [-34, 24] was inside its bars, "no edge over ground" 12 to
/// 53 times, 6 of 8 dead). `None` with no bar beside.
pub fn cage_bar(w: &World, feet: Cell) -> Option<Cell> {
    let facing = |(dx, dz): (i32, i32)| -(dx * feet.0.signum() + dz * feet.2.signum());
    let mut sides: Vec<(i32, i32)> = Axis::ALL
        .iter()
        .map(|a| a.step())
        .filter(|(dx, dz)| {
            (0..=1).any(|dy| {
                kind(w, (feet.0 + dx, feet.1 + dy, feet.2 + dz)) == Some(BlockKind::IronBars)
            })
        })
        .collect();
    sides.sort_by_key(|d| (-facing(*d), *d));
    let (dx, dz) = *sides.first()?;
    (0..=1)
        .map(|dy| (feet.0 + dx, feet.1 + dy, feet.2 + dz))
        .find(|c| kind(w, *c) == Some(BlockKind::IronBars))
}

/// The feet stand on a spike's top: three obsidian under them (the End's platform is one thick).
pub fn on_spike(w: &World, feet: Cell) -> bool {
    (1..=3).all(|d| kind(w, (feet.0, feet.1 - d, feet.2)) == Some(BlockKind::Obsidian))
}

/// A crystal (its id and place) with the spike it stands on and the approach worked to it.
pub type Chosen = ((i32, [f64; 3]), (Spike, Approach));

/// The crystal worked next and its spike and approach: the crystals in their order
/// (`crystals_in_order`), the first with an approach not refused at `tick`, a column whose
/// landing water is poured first. The one choice `next` and `blocks_wanted` both read.
pub fn chosen_approach(w: &World, st: &SpikeState, pos: [f64; 3], tick: u64) -> Option<Chosen> {
    let approach_to = |cpos: [f64; 3]| {
        let spike = spike_for(w, &st.geography, cpos)?;
        // A column whose landing water is poured goes first (race 45927433568 lane 0: poured at
        // one pillar's foot, the next choice was another axis, and the bucket was lost to it).
        let refused = |a: &Axis| {
            st.refused
                .iter()
                .any(|(x, z, r, until)| [*x, *z] == spike.centre && r == a && tick < *until)
        };
        let landed = st.landings.iter().find_map(|(col, _)| {
            Axis::ALL
                .iter()
                .filter(|a| !refused(a))
                .filter_map(|a| approach_along(w, &spike, *a))
                .find(|a| a.plan.column == *col)
        });
        // Then a stub of its own: a stopped climb resumes there, not on a new column (race
        // 46071506005 lane 0: four columns begun at [33, -25], each stopped low).
        let stubbed = || {
            st.stubs.iter().find_map(|r| {
                Axis::ALL
                    .iter()
                    .filter(|a| !refused(a))
                    .filter_map(|a| approach_along(w, &spike, *a))
                    .find(|a| a.plan.column == (r.x, r.z))
            })
        };
        landed
            .or_else(stubbed)
            .or_else(|| best_approach(w, &spike, pos, &st.refused, tick))
            .map(|a| (spike, a))
    };
    crystals_in_order(w, pos)
        .into_iter()
        // >>> E8 (issue 20, the bow line): the crystals the bow may still take are left to it.
        .filter(|(id, _)| !st.bow_keep.contains(id))
        // <<< E8
        .find_map(|c| approach_to(c.1).map(|sa| (c, sa)))
}

/// The blocks a pillar wants held before its walk and its climb begin: from its foot to its
/// stand, plus `PILLAR_SPARE` (the stand's two wall blocks and two for a lost click).
pub fn pillar_wants(w: &World, st: &SpikeState, a: &Approach) -> i32 {
    // A stub of the bot's own: from the stub's top to the stand, plus the side pillar's height
    // still to put up to reach that top (from the side column's top now: a side pillar begun
    // lowers it; the orchestrator, after race 46099479836: ~520 "too few blocks").
    let floor = stub_side(w, st, a).map_or(a.foot.1, |side| {
        ground(w, side.0, side.2, a.foot.1).map_or(side.1, |g| g.max(side.1))
    });
    a.plan.stand.1 - floor + PILLAR_SPARE
}

/// The foot of the side pillar a stub of the bot's own is climbed back onto: a column beside the
/// stub with its ground at the stub's base (or under) and open from there to over the stub's top
/// (the orchestrator, after wall 43: a spike has 4 axes, and stubs left on each must never leave
/// it with no approach). `None` when the approach's column is no stub of its own, or no side
/// is open.
pub fn stub_side(w: &World, st: &SpikeState, a: &Approach) -> Option<Cell> {
    let rec = st.stubs.iter().find(|r| (r.x, r.z) == a.plan.column)?;
    let top = a.foot.1;
    // The side pillar already begun beside it first, its base the side's foot.
    let begun = st.pillars.iter().find(|q| {
        (q.x - rec.x).abs() + (q.z - rec.z).abs() == 1
            && (top..=top + 1).all(|y| open(kind(w, (q.x, y, q.z))))
    });
    if let Some(q) = begun {
        return Some((q.x, q.base, q.z));
    }
    Axis::ALL.iter().find_map(|ax| {
        let (dx, dz) = ax.step();
        let (x, z) = (rec.x + dx, rec.z + dz);
        let g = ground(w, x, z, top)?;
        (g <= rec.base + 1 && (g..=top + 1).all(|y| open(kind(w, (x, y, z))))).then_some((x, g, z))
    })
}

/// The blocks the crystals want held for the pillar they work next: exactly what "no walk to
/// the foot: too few blocks" and "no climb: too few blocks" compare against, so the stock
/// (E3) stops at the same number (frontier race 46055289196: the stock stopped at its own 22 to
/// 40 while the pillar wanted 35 to 41, and nothing mined). `None` with no crystal to work or no
/// body. Counted against `pillar_blocks`.
pub fn blocks_wanted(plan: &crate::Plan, w: &World) -> Option<i32> {
    let body = w.body?;
    chosen_approach(w, &plan.spikes, body.pos, plan.spikes.now)
        .map(|(_, (_, a))| pillar_wants(w, &plan.spikes, &a))
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
    /// Dig this cell, shutting the bot in, toward an approach's foot.
    DigOut { cell: Cell, toward: Cell },
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
    // >>> E8 (issue 20, the bow line)
    /// Crystals the bow may still take (`Plan::bow_task` sets it): `next` leaves them.
    #[serde(default)]
    pub bow_keep: Vec<i32>,
    // <<< E8
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
    /// The water pool by the bunker (two sources, an infinite one), the cell it is filled from,
    /// and the counts of waterfalls ridden down and buckets refilled.
    pub pool: Option<[Cell; 2]>,
    /// The pool's whole 2 by 2 square (`pool` holds its two diagonal corners, poured).
    pub pool_square: Option<[Cell; 4]>,
    /// Digs begun for the pool's cells, and whether the pool was given up.
    pub pool_digs: u32,
    pub pool_off: bool,
    /// Pool cells a refill got nothing from, with the block state then: tried again only once
    /// the server shows them otherwise.
    pub refill_failed: Vec<(Cell, u32)>,
    pub pool_stand: Option<Cell>,
    pub pool_made: bool,
    pub waterfalls: u32,
    pub refills: u32,
    /// The dragon's distance when last read, and when: is it closing?
    pub dragon_seen: Option<(u64, f64)>,
    /// The pillar column a waterfall is being ridden down beside.
    pub riding: Option<(i32, i32)>,
    /// Deliberate drops off a pillar onto the clutch, from a fireball or a strafe.
    pub clutch_drops: u32,
    /// The dig-out's start (tick, blocks from its target) for its progress invariant, and the
    /// tick until which a failed one stays off.
    pub dig_out_since: Option<(u64, f64)>,
    pub dig_out_off: u64,
    /// The dig-out's stair along the other axis, after walks up the first failed.
    pub dig_out_turn: bool,
    /// Walks with no approach (a dig-out's stair) that failed in a row, and their target.
    pub stair_walk_fails: Option<(Cell, u32)>,
    /// Water buckets held when the clutch's take-back use went: taken back only when more are
    /// held after (E9's read of race 46038776204 lane 7: "the water taken back" with water_bucket
    /// 2 -> 1 the same tick).
    pub take_back_from: Option<i32>,
    /// Pillars of the bot's own left standing (come down by water, or a climb stopped): their
    /// columns and bases, so their approach is resumed beside them, never refused for them.
    pub stubs: Vec<PillarRec>,
    /// The tick the crystals' task last ran at: the refusals `blocks_wanted` reads are those
    /// active then.
    pub now: u64,
    /// Water the bot poured and left (a clutch's or a landing's not taken back): an empty bucket
    /// refills from it first.
    pub left_water: Vec<Cell>,
    /// Failed walks to each left water cell and the last one's tick (dropped at
    /// `LEFT_WATER_TRIES`, tried again `LEFT_WATER_RETRY` ticks after a failure).
    pub left_water_fails: Vec<(Cell, u32, u64)>,
    /// The tick a hold up an open pillar with no water way down began (capped by
    /// `OPEN_HOLD_MAX`).
    pub open_hold_since: Option<u64>,
    /// Each pillar's landing water (its column, the cell at its foot the water was poured in).
    pub landings: Vec<((i32, i32), Cell)>,
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
    /// When the pour was sent: one only, until the bucket shows it (a second use of the emptied
    /// bucket would take the water back up).
    #[serde(default)]
    pub sent_at: Option<u64>,
}

impl Default for SpikeState {
    fn default() -> Self {
        Self {
            geography: Vec::new(),
            bow_keep: Vec::new(),
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
            pool: None,
            pool_square: None,
            pool_digs: 0,
            pool_off: false,
            refill_failed: Vec::new(),
            pool_stand: None,
            pool_made: false,
            waterfalls: 0,
            refills: 0,
            dragon_seen: None,
            riding: None,
            landings: Vec::new(),
            dig_out_since: None,
            clutch_drops: 0,
            dig_out_off: 0,
            dig_out_turn: false,
            stair_walk_fails: None,
            take_back_from: None,
            now: 0,
            stubs: Vec::new(),
            left_water: Vec::new(),
            left_water_fails: Vec::new(),
            open_hold_since: None,
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
    /// A bucket used at a point (a pour, a fill) until `want` is held `count` times.
    Bucket {
        look: [f64; 3],
        item: ItemKind,
        want: ItemKind,
        count: i32,
        until: u64,
        last: Option<u64>,
        why: String,
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
    // The crystals nearest first; the first with an approach not refused is the one worked
    // (race 45949063425 lane 1: four approaches of one spike refused in turn, and back to it,
    // none of the nine others tried).
    let order = crystals_in_order(w, body.pos);
    // >>> E8 (issue 20, the bow line): the crystals the bow may still take are left to it.
    let order: Vec<(i32, [f64; 3])> = order
        .into_iter()
        .filter(|(id, _)| !st.bow_keep.contains(id))
        .collect();
    // <<< E8
    let chosen = chosen_approach(w, st, body.pos, tick);
    let crystal = chosen.map(|(c, _)| c).or_else(|| order.first().copied());
    let approach = chosen.map(|(_, sa)| sa);
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
        // A stub of its own on this approach: up a side pillar beside it to its top, then onto it.
        if let Some(side) = stub_side(w, st, &a) {
            if (feet.0, feet.2) == (side.0, side.2) && feet.1 < a.foot.1 {
                return Next::Pillar {
                    column: (side.0, side.2),
                    top: a.foot.1,
                    axis: p.axis,
                    centre: spike.centre,
                };
            }
            if feet == (side.0, a.foot.1, side.2) {
                return Next::Step {
                    to: (p.column.0, a.foot.1, p.column.1),
                    sneak: false,
                };
            }
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
                    && passable(kind(w, (r.x, feet.1, r.z)))
                    && passable(kind(w, (r.x, feet.1 + 1, r.z)))
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
    let shut = crystal.is_some() && shut_in(w, feet);
    match (crystal, approach) {
        (Some(_), Some((spike, a))) if !shut => Next::Walk {
            to: stub_side(w, st, &a).unwrap_or(a.foot),
            approach: Some((spike.centre[0], spike.centre[1], a.plan.axis)),
        },
        (Some((_, pos)), approach) => {
            // Every approach refused for want of a way: the cells shutting the bot in dug toward
            // the nearest approach (the frontier race 45898928805 lane 1: an enderman's pocket put
            // blocks on the bunker's stair, (12, 60, 0) and (13, 61, 0), and every walk from
            // (13, 59, 0) was refused for 12,000 ticks).
            // Never a dig-out from a spike's top: its own blocks or its cage's bars are no way out
            // (race 46070173620: "dig the way out at (-34, 79, 26) … shut in a pocket" in a cage).
            if on_spike_top(w, feet) {
                return Next::Wait("stranded on a spike's top with no pillar down".to_owned());
            }
            let out = spike_for(w, &st.geography, pos)
                .and_then(|sp| best_approach(w, &sp, body.pos, &[], tick))
                .and_then(|a| {
                    dig_out_plan_turned(w, feet, a.foot, st.dig_out_turn).map(|p| (p, a.foot))
                });
            let stands = |c: Cell| {
                full(kind(w, (c.0, c.1 - 1, c.2)))
                    && open(kind(w, c))
                    && open(kind(w, (c.0, c.1 + 1, c.2)))
            };
            let gap = out.and_then(|(_, toward)| stair_floor_gap(w, feet, toward, st.dig_out_turn));
            match out {
                Some(((Some(cell), _), toward)) => Next::DigOut { cell, toward },
                // A step with no floor: one put there, then the walk up.
                Some(((None, top), _)) if top != feet && gap.is_some() => gap
                    .map_or(Next::Wait("no stair gap".to_owned()), |c| {
                        Next::Build(c, Role::Stair)
                    }),
                // The stair open: up it, and the dig-out goes on from its top.
                Some(((None, top), _)) if top != feet && stands(top) => Next::Walk {
                    to: top,
                    approach: None,
                },
                // Shut in with the stair already open: the walk is tried after all.
                Some(_) | None if approach.is_some() => approach.map_or_else(
                    || Next::Wait("no approach".to_owned()),
                    |(spike, a)| Next::Walk {
                        to: a.foot,
                        approach: Some((spike.centre[0], spike.centre[1], a.plan.axis)),
                    },
                ),
                Some(_) | None => Next::Wait(format!(
                    "no approach to the crystal at ({:.1}, {:.1}, {:.1})",
                    pos[0], pos[1], pos[2]
                )),
            }
        }
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

/// Half the body's width: the columns a falling body's box overlaps are those under its centre
/// plus or minus this.
pub const BODY_HALF: f64 = 0.3;

/// The column a falling body lands on and its ground's top: of the columns its box overlaps now
/// and a tick on (the velocity on the plane: a body flung sideways lands where it is going), the
/// one with the highest ground within `GROUND_SEARCH` under the feet (frontier race 45964846590
/// lane 3: flung about 85 off the island by the dragon, no ground was ever known under the
/// column of its centre, no pour went, and it died of the fall).
pub fn landing_ground(w: &World, pos: [f64; 3], vel: [f64; 3]) -> Option<((i32, i32), f64)> {
    let y = path::feet_cell(pos).1;
    let span = |c: f64, v: f64| {
        let (lo, hi) = (c.min(c + v) - BODY_HALF, c.max(c + v) + BODY_HALF);
        (lo.floor() as i32)..=(hi.floor() as i32)
    };
    let zs = span(pos[2], vel[2]);
    span(pos[0], vel[0])
        .flat_map(|x| zs.clone().map(move |z| (x, z)))
        .filter_map(|(x, z)| {
            (y - GROUND_SEARCH..=y)
                .rev()
                .find(|cy| full(kind(w, (x, *cy, z))))
                .map(|cy| ((x, z), f64::from(cy + 1)))
        })
        .max_by(|a, b| a.1.total_cmp(&b.1).then(b.0.cmp(&a.0)))
}

/// Gravity and drag of a falling player per tick (vanilla `LivingEntity.travel`: 0.08 down, then
/// the vertical speed times 0.98; in the air the horizontal speed times 0.91).
pub const FALL_GRAVITY: f64 = 0.08;
pub const FALL_DRAG: f64 = 0.98;
pub const AIR_DRAG: f64 = 0.91;
/// Ticks of a fall the landing is looked for over (a fall from y 120 to 0 takes about 60).
pub const FALL_TICKS: u32 = 120;

/// The column a falling body will land on, its ground's top and the ticks to it: the fall
/// stepped tick by tick with its gravity and drags, the first tick the feet reach the ground
/// under a column the box overlaps (E9's read of race 46038776204 lanes 1 and 4: flung 60 blocks
/// sideways, the pour went to the column beside the feet and the body landed far past it).
pub fn predict_landing(w: &World, pos: [f64; 3], vel: [f64; 3]) -> Option<((i32, i32), f64, u32)> {
    let mut p = pos;
    let mut v = vel;
    let mut cache: Vec<((i32, i32), Option<f64>)> = Vec::new();
    let top = path::feet_cell(pos).1;
    let mut ground = |x: i32, z: i32| {
        if let Some((_, g)) = cache.iter().find(|(c, _)| *c == (x, z)) {
            return *g;
        }
        let g = (top - GROUND_SEARCH..=top)
            .rev()
            .find(|cy| full(kind(w, (x, *cy, z))))
            .map(|cy| f64::from(cy + 1));
        cache.push(((x, z), g));
        g
    };
    for t in 1..=FALL_TICKS {
        let next = [p[0] + v[0], p[1] + v[1], p[2] + v[2]];
        let xs = [
            (next[0] - BODY_HALF).floor() as i32,
            (next[0] + BODY_HALF).floor() as i32,
        ];
        let zs = [
            (next[2] - BODY_HALF).floor() as i32,
            (next[2] + BODY_HALF).floor() as i32,
        ];
        let mut best: Option<((i32, i32), f64)> = None;
        for x in xs {
            for z in zs {
                if let Some(g) = ground(x, z)
                    && g <= p[1] + 1e-6
                    && next[1] <= g
                    && best.is_none_or(|(_, b)| g > b)
                {
                    best = Some(((x, z), g));
                }
            }
        }
        if let Some((c, g)) = best {
            return Some((c, g, t));
        }
        p = next;
        v = [
            v[0] * AIR_DRAG,
            (v[1] - FALL_GRAVITY) * FALL_DRAG,
            v[2] * AIR_DRAG,
        ];
    }
    None
}

/// Where the pour looks: the landing column's top face, at the point of it nearest under the
/// body's centre (so the use's ray from the eye meets that column, not the void beside it).
pub fn landing_aim(pos: [f64; 3], (x, z): (i32, i32), ground: f64) -> [f64; 3] {
    [
        pos[0].clamp(f64::from(x) + 0.1, f64::from(x) + 0.9),
        ground,
        pos[2].clamp(f64::from(z) + 0.1, f64::from(z) + 0.9),
    ]
}

/// Feet over the ground at which a fall is caught (e06: a fall from a pillar's top; the clutch is
/// for more than a stumble), and at or under which the bucket is used, every tick until it pours
/// (e06's `above` 4.0 and 2.0 both poured; the use reaches 4.5 from the eye).
pub const CLUTCH_FROM: f64 = 4.0;
/// Over no known ground, the fall speed (blocks a tick) at which a clutch begins anyway: a real
/// fall, not a step's or a jump's dip.
pub const CLUTCH_VOID_FALL: f64 = 0.5;
/// Blocks from the eye to the ground's top face a pour reaches: the player's block interaction
/// range 4.5, less a tenth.
pub const POUR_REACH: f64 = 4.4;
/// The hotbar index the water bucket is kept in up high (the last: the tools and blocks the
/// climb and dig hold are selected elsewhere).
pub const CLUTCH_HOTBAR: u8 = 8;

/// Whether the pour goes this tick (the orchestrator, after race 45885217441 lane 1's miss at 48
/// over the ground): the position this tick's movement sends (the feet plus the velocity, the
/// move before the tick's gravity and drag) has the eye within `POUR_REACH` of the ground's top
/// face and the feet still over it, and the next tick's does not. That is the last tick in reach,
/// predicted, not waited for: at about 3.9 a tick the reach is crossed in one or two ticks.
pub fn pour_now(feet_y: f64, vy: f64, ground: f64) -> bool {
    pour_now_at(feet_y, vy, Some(ground), Some(ground))
}

/// `pour_now` with the ground under each predicted position: `g1` under the position this
/// tick sends, `g2` under the next. A body flung sideways (race 45903987747: both bots knocked
/// off near the island's edge) lands on another column than the one it fell from.
///
/// One tick early (the orchestrator, after races 45920382795 and 45924374354: a pour sent in
/// the last tick in reach did not save the bot twice; the server places the water when it
/// handles the packet, which may be after the feet arrive): the pour goes in the tick whose sent
/// position is in reach while the one two ticks on is not, so the last tick in reach and the one
/// before it both qualify, and the clutch sends at the first (`Clutch::sent_at` keeps it to one).
///
/// Measured (E6's parity e09-pour, run 45929976619, docs/problems/end-model.md "The water
/// clutch window"): the server judges the use from the last position it heard, the body's
/// before this tick's move, and places water only with the eye within 4.5 of the ground there
/// (every use with the eye at 3.93 or less placed it, every one at 4.56 or more nothing). So the
/// pour goes by the body's current height, the feet `POUR_FEET` or less over the ground under
/// them, every tick until the water shows; a late pour is never too late (K1 saved every drop
/// up to 60). `vy` and `g2` are kept for the callers and unused.
pub fn pour_now_at(feet_y: f64, vy: f64, g1: Option<f64>, g2: Option<f64>) -> bool {
    let _ = (vy, g2);
    g1.is_some_and(|g| feet_y > g && feet_y - g <= POUR_FEET)
}

/// Feet over the ground at or under which the clutch pours: the measured reach, eye 4.5 less
/// the eye's 1.62, is 2.88; less a margin.
pub const POUR_FEET: f64 = 2.7;

/// Ticks until feet at `feet_y` falling at `vy` reach `ground` (for the ledger).
pub fn ticks_to_land(feet_y: f64, vy: f64, ground: f64) -> u32 {
    let (mut y, mut v, mut n) = (feet_y, vy, 0);
    while y > ground && n < 200 {
        y += v;
        v = (v - physics::GRAVITY) * physics::VERTICAL_DRAG;
        n += 1;
    }
    n
}
/// Ticks after landing the water is tried to be taken back, and the most a clutch lasts.
pub const CLUTCH_TAKE_TICKS: u64 = 40;
/// Ticks after a pour before another is sent (the bucket's change seen by then).
pub const POUR_RESEND: u64 = 2;
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

/// The next cell to dig out of a pit or a shut cell toward `target`, along its larger axis: a
/// step up (the cell over the head, then the step's cell and the one over it) when the target
/// is higher, else the way along (the feet's and the head's cells). Only a block with an open
/// face beside the bot's own cell, never obsidian or bedrock. `None` when the way is open.
pub fn dig_out(w: &World, feet: Cell, target: Cell) -> Option<Cell> {
    dig_out_plan(w, feet, target).and_then(|(cell, _)| cell)
}

/// The dig-out's next cell (`None` when the stair is open) and the stair's top step, the feet
/// cell to walk up to once the stair is open. The stair rises to the higher of the target's
/// height and the ground's over the feet (the frontier race 45918666403 lane 2: from a notch 2
/// under its bunker the stair rose only to the foot's height, 59, under the ground at 62, and
/// every walk from the feet was refused for 19,560 ticks).
pub fn dig_out_plan(w: &World, feet: Cell, target: Cell) -> Option<(Option<Cell>, Cell)> {
    dig_out_plan_turned(w, feet, target, false)
}

/// The dig-out's direction and rise: along the larger axis toward the target, or the other one
/// when `turn` (its stair's walks failed), and up to the higher of the target and the ground.
fn stair_line(w: &World, feet: Cell, target: Cell, turn: bool) -> Option<(i32, i32, i32)> {
    let (ax, az) = (target.0 - feet.0, target.2 - feet.2);
    let along_x = (ax.abs() >= az.abs()) != turn;
    let (dx, dz) = if along_x {
        (if ax == 0 { 1 } else { ax.signum() }, 0)
    } else {
        (0, if az == 0 { 1 } else { az.signum() })
    };
    if (ax, az) == (0, 0) {
        return None;
    }
    let (x, y, z) = feet;
    let surface = [(x, z), (x + dx, z + dz)]
        .iter()
        .filter_map(|(gx, gz)| ground(w, *gx, *gz, y + DIG_OUT_SKY))
        .max()
        .unwrap_or(y);
    // A stair of up to `DIG_OUT_STEPS` steps (each within a dig's reach), or the way along.
    Some((dx, dz, (target.1.max(surface) - y).clamp(0, DIG_OUT_STEPS)))
}

/// The first step of the dig-out's stair with no floor under it (frontier race 45988967677 lane
/// 1: surface mining had dug the first step's floor, the stair read as open, and its walk failed
/// NoPath every 200 ticks for 2,700). `None` when every step has its floor.
pub fn stair_floor_gap(w: &World, feet: Cell, target: Cell, turn: bool) -> Option<Cell> {
    let (dx, dz, rise) = stair_line(w, feet, target, turn)?;
    let (x, y, z) = feet;
    (0..rise)
        .map(|k| (x + (k + 1) * dx, y + k, z + (k + 1) * dz))
        .find(|c| kind(w, *c).is_some() && !full(kind(w, *c)))
}

/// `dig_out_plan` along the other axis when `turn`.
pub fn dig_out_plan_turned(
    w: &World,
    feet: Cell,
    target: Cell,
    turn: bool,
) -> Option<(Option<Cell>, Cell)> {
    let (dx, dz, rise) = stair_line(w, feet, target, turn)?;
    let (x, y, z) = feet;
    let top = if rise > 0 {
        (x + rise * dx, y + rise, z + rise * dz)
    } else {
        (x + dx, y, z + dz)
    };
    let cells: Vec<Cell> = if rise > 0 {
        (0..rise)
            .flat_map(|k| {
                [
                    (x + k * dx, y + k + 2, z + k * dz),
                    (x + (k + 1) * dx, y + k + 1, z + (k + 1) * dz),
                    (x + (k + 1) * dx, y + k + 2, z + (k + 1) * dz),
                ]
            })
            .collect()
    } else {
        vec![(x + dx, y, z + dz), (x + dx, y + 1, z + dz)]
    };
    let cell = cells.into_iter().find(|c| {
        let k = kind(w, *c);
        !open(k) && k.is_some() && !matches!(k, Some(BlockKind::Obsidian | BlockKind::Bedrock))
    });
    Some((cell, top))
}

/// Cells over the feet the ground over a dig-out is looked for from.
pub const DIG_OUT_SKY: i32 = 24;
/// Ticks a dig-out may get no nearer its target before it fails and hands back, and the ticks
/// it then stays off (the progress-invariant rule).
pub const DIG_OUT_STALL: u64 = 600;
pub const DIG_OUT_OFF: u64 = 1200;

/// Steps of a stair out a dig-out looks along (each cell within a dig's reach of the feet).
pub const DIG_OUT_STEPS: i32 = 3;
/// Failed walks in a row up a dig-out's stair before the way out turns.
pub const STAIR_WALK_TRIES: u32 = 3;

/// A 2 by 2 square of end stone ground beside the feet, one under the feet's level, walled below
/// and round by full blocks: the pool's cells, the near corner first and the far diagonal last.
/// Water poured in those two diagonal corners fills the other two, and every cell then has two
/// sources beside it: an infinite source. A 2 by 1 pit is not one: race 45920382795 refilled one
/// bucket from it and the second "gave up" (each cell had one source beside it).
pub fn pool_cells(w: &World, feet: Cell) -> Option<[Cell; 4]> {
    let (x, y, z) = feet;
    Axis::ALL.iter().find_map(|a| {
        let (dx, dz) = a.step();
        // Across the axis, either way.
        let across: [(i32, i32); 2] = if dx == 0 {
            [(1, 0), (-1, 0)]
        } else {
            [(0, 1), (0, -1)]
        };
        across.iter().find_map(|(bx, bz)| {
            let at = |i: i32, j: i32| (x + dx * (1 + i) + bx * j, y - 1, z + dz * (1 + i) + bz * j);
            let sq = [at(0, 0), at(1, 0), at(0, 1), at(1, 1)];
            let ground = |c: Cell| {
                kind(w, c) == Some(BlockKind::EndStone)
                    && full(kind(w, (c.0, c.1 - 1, c.2)))
                    && open(kind(w, (c.0, c.1 + 1, c.2)))
            };
            let walled = |c: Cell| {
                Axis::ALL.iter().all(|b| {
                    let (ex, ez) = b.step();
                    let n = (c.0 + ex, c.1, c.2 + ez);
                    sq.contains(&n) || full(kind(w, n))
                })
            };
            sq.iter().all(|c| ground(*c) && walled(*c)).then_some(sq)
        })
    })
}

/// The cell beside a pillar's foot its landing water goes in: open with a full floor at the
/// foot's height, and open air over it to over the top, so a body stepped off the top falls
/// straight into it. `None` when no side has one.
pub fn landing_cell(w: &World, column: (i32, i32), base: i32, top: i32) -> Option<Cell> {
    Axis::ALL.iter().find_map(|a| {
        let (dx, dz) = a.step();
        let (x, z) = (column.0 + dx, column.1 + dz);
        let clear = (base..=top + 1).all(|y| open(kind(w, (x, y, z))));
        (clear && full(kind(w, (x, base - 1, z)))).then_some((x, base, z))
    })
}

/// Columns round the feet a pool's site is looked for in.
pub const POOL_SITE: i32 = 8;
/// Columns (on the plane, either axis) a pool's stand and cells keep from the bunker's column:
/// its stair runs four out from it (E3: three down and a step), so no misplaced pour runs into
/// the shelter (race 45930464018 lane 0 drowned in a bunker its pool's water had run into).
pub const POOL_KEEP: i32 = 5;
/// Digs for the pool's four cells before it is given up (four, and room for a few refused).
pub const POOL_DIGS: u32 = 8;

/// The nearest standable cell within `POOL_SITE` of the feet (a floor under it, room over it)
/// with a pool's two cells beside it, and those cells.
pub fn pool_site(w: &World, feet: Cell, bunker: Option<[i32; 3]>) -> Option<(Cell, [Cell; 4])> {
    // Never near the bunker and its stair, nor uphill of it (water runs down to it).
    let surface = bunker.and_then(|b| ground(w, b[0], b[2], b[1] + 12));
    let clear =
        |c: Cell| bunker.is_none_or(|b| (c.0 - b[0]).abs().max((c.2 - b[2]).abs()) >= POOL_KEEP);
    let mut best: Option<(i32, Cell, [Cell; 4])> = None;
    for dx in -POOL_SITE..=POOL_SITE {
        for dz in -POOL_SITE..=POOL_SITE {
            for dy in -1..=1 {
                let c = (feet.0 + dx, feet.1 + dy, feet.2 + dz);
                let stands = full(kind(w, (c.0, c.1 - 1, c.2)))
                    && open(kind(w, c))
                    && open(kind(w, (c.0, c.1 + 1, c.2)));
                if !stands {
                    continue;
                }
                if !clear(c) || surface.is_some_and(|sy| c.1 > sy) {
                    continue;
                }
                if let Some(cells) = pool_cells(w, c).filter(|sq| sq.iter().all(|q| clear(*q))) {
                    let d = dx.abs() + dz.abs() + dy.abs();
                    if best.is_none_or(|(b, _, _)| d < b) {
                        best = Some((d, c, cells));
                    }
                }
            }
        }
    }
    best.map(|(_, c, cells)| (c, cells))
}

/// Why no pool fits beside the feet, the first refusal along each axis: for the ledger.
pub fn pool_refusal(w: &World, feet: Cell) -> String {
    let (x, y, z) = feet;
    Axis::ALL
        .iter()
        .map(|a| {
            let (dx, dz) = a.step();
            let why = [(x + dx, y - 1, z + dz), (x + 2 * dx, y - 1, z + 2 * dz)]
                .iter()
                .find_map(|c| {
                    let k = kind(w, *c);
                    if k != Some(BlockKind::EndStone) {
                        return Some(format!("{c:?} is {k:?}"));
                    }
                    if !full(kind(w, (c.0, c.1 - 1, c.2))) {
                        return Some(format!("nothing under {c:?}"));
                    }
                    if !open(kind(w, (c.0, c.1 + 1, c.2))) {
                        return Some(format!("{:?} over {c:?}", kind(w, (c.0, c.1 + 1, c.2))));
                    }
                    None
                })
                .unwrap_or_else(|| "a side open".to_owned());
            format!("{a:?}: {why}")
        })
        .collect::<Vec<_>>()
        .join("; ")
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
        self.spikes.now = tick;
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
        // A pillar of no record under the feet (a resume, the bow line's own): adopted, its base
        // where the free-standing column begins, so its way down is the crystals' own.
        if body.on_ground
            && !self
                .spikes
                .pillars
                .iter()
                .any(|r| (r.x, r.z) == (feet.0, feet.2))
            && tower(w, feet)
        {
            let height = (1..=GROUND_SEARCH)
                .take_while(|d| {
                    let y = feet.1 - d;
                    full(kind(w, (feet.0, y, feet.2)))
                        && Axis::ALL
                            .iter()
                            .filter(|a| {
                                let (dx, dz) = a.step();
                                !full(kind(w, (feet.0 + dx, y, feet.2 + dz)))
                            })
                            .count()
                            >= 3
                })
                .count();
            let base = feet.1 - i32::try_from(height).unwrap_or(0);
            self.spikes.pillars.push(PillarRec {
                x: feet.0,
                z: feet.2,
                base,
            });
            self.note(
                tick,
                format!("spikes: a pillar of no record at {feet:?}"),
                format!(
                    "{height} free-standing blocks under the feet; its base {base}, the way down"
                ),
            );
        }
        let n = next(w, &self.spikes, tick);
        if !on_pillar(w, feet, &self.spikes.pillars) {
            self.spikes.open_hold_since = None;
        }
        let climbing = self
            .spikes
            .pillars
            .iter()
            .any(|p| (p.x, p.z) == (feet.0, feet.2) && feet.1 > p.base);
        if let Some(now) = d
            && self
                .spikes
                .dragon_seen
                .is_none_or(|(at, _)| tick.saturating_sub(at) >= CLOSING_TICKS / 2)
        {
            self.spikes.dragon_seen = Some((tick, now.dist));
        }
        // A waterfall ridden down: landed by its pillar at its base, the pillar left standing.
        if body.on_ground
            && !on_pillar(w, feet, &self.spikes.pillars)
            && let Some(col) = self.spikes.riding
            && let Some(i) = self
                .spikes
                .pillars
                .iter()
                .position(|p| (p.x, p.z) == col && feet.1 <= p.base + 1)
        {
            self.spikes.riding = None;
            let p = self.spikes.pillars.remove(i);
            self.spikes.pillars_down += 1;
            // Left standing: a stub of its own, its approach resumed beside it.
            if full(kind(w, (p.x, p.base, p.z)))
                && !self.spikes.stubs.iter().any(|r| (r.x, r.z) == (p.x, p.z))
            {
                self.spikes.stubs.push(p);
            }
            if let Some(j) = self
                .spikes
                .landings
                .iter()
                .position(|(c, _)| *c == (p.x, p.z))
            {
                let (_, l) = self.spikes.landings.remove(j);
                if kind(w, l) == Some(BlockKind::Water) && craft::count(w, ItemKind::Bucket) > 0 {
                    self.note(
                        tick,
                        format!("spikes: landing water taken back at {l:?}"),
                        "the bucket filled again".to_owned(),
                    );
                    self.spikes.doing = Some(Doing::Bucket {
                        look: [
                            f64::from(l.0) + 0.5,
                            f64::from(l.1) + 0.5,
                            f64::from(l.2) + 0.5,
                        ],
                        item: ItemKind::Bucket,
                        want: ItemKind::WaterBucket,
                        count: craft::count(w, ItemKind::WaterBucket) + 1,
                        until: tick + BUCKET_TICKS,
                        last: None,
                        why: "the landing water's take-back".to_owned(),
                    });
                }
            }
            self.note(
                tick,
                format!("spikes: waterfall down to {feet:?}"),
                format!(
                    "from the pillar in ({}, {}), base {}; {} waterfalls; health {health:?}",
                    p.x, p.z, p.base, self.spikes.waterfalls
                ),
            );
            if self.spikes.doing.is_some() {
                self.retry_at = tick + 1;
                return Some(Task::Idle);
            }
        }
        // A fireball in flight near while up a pillar: down at once, before its impact (E5's jar
        // read, DragonFireball.onHit: its cloud goes on the first living entity within 4 of the
        // impact, walls or not; up high there is no room to dodge). Into the landing water when
        // it is poured and the feet are at the top, else the dig.
        // A strafe (phase 1) within 64 counts the same: it fires (frontier race 45947478869 lane
        // 1: the strafe began with the bot climbing at y 70, the fireball came at 74, and the
        // dig down, a block every 40 ticks, could not beat it; dead in the cloud).
        // Up a pillar with the dragon landing, taking off or charging within `KNOCK_NEAR`: no
        // drop, no climb, no stand; held crouched until it passes (part 2: race 45994686575 lane
        // 3, struck off its pillar at y 65 by the charge and flung over the void).
        if body.on_ground
            && on_pillar(w, feet, &self.spikes.pillars)
            && self
                .spikes
                .pillars
                .iter()
                .any(|r| (r.x, r.z) == (feet.0, feet.2) && feet.1 > r.base + 1)
            && let Some(why) = knock_risk(d.as_ref())
        {
            self.spikes_note(
                tick,
                "spikes: hold from the knock-off".to_owned(),
                format!("{why}; up the pillar at {feet:?}"),
            );
            return Some(self.spikes_hold(tick));
        }
        // The dragon landing, sitting or charging with the bot up a pillar: down by water now
        // (E9's read of race 46038776204), a strike due at the stand first.
        if body.on_ground
            && descend_from(d.as_ref())
            && !matches!(n, Next::Pillar { top, .. } if climb_on_near_stand(d.as_ref(), top - feet.1))
            && !matches!(
                n,
                Next::Strike(_) | Next::Bar(_) | Next::Build(_, Role::Wall)
            )
            && let Some(rec) = self
                .spikes
                .pillars
                .iter()
                .find(|r| (r.x, r.z) == (feet.0, feet.2) && feet.1 > r.base)
                .copied()
        {
            let why = format!("the dragon {:?}", d.map(|d| (d.dist, d.phase)));
            if let Some(t) = self.spikes_descend(w, tick, feet, rec, &why) {
                return Some(t);
            }
        }
        let strafing = d.is_some_and(|d| d.phase == Some(STRAFE_PHASE) && d.dist <= THREAT_NEAR);
        if body.on_ground
            && on_pillar(w, feet, &self.spikes.pillars)
            && let Some(fb) =
                fireball_near(w, body.pos).or_else(|| strafing.then(|| d.map_or(0.0, |d| d.dist)))
        {
            let rec = self
                .spikes
                .pillars
                .iter()
                .find(|r| (r.x, r.z) == (feet.0, feet.2))
                .copied();
            let landing = rec.and_then(|r| {
                self.spikes
                    .landings
                    .iter()
                    .find(|(c, _)| *c == (r.x, r.z))
                    .map(|(_, l)| (r, *l))
            });
            if let Some((r, l)) = landing
                && kind(w, l) == Some(BlockKind::Water)
            {
                let (dx, dz) = (l.0 - r.x, l.2 - r.z);
                let edge = (feet.0 + dx, feet.1, feet.2 + dz);
                if passable(kind(w, edge)) && passable(kind(w, (edge.0, edge.1 + 1, edge.2))) {
                    self.spikes.riding = Some((r.x, r.z));
                    self.spikes.waterfalls += 1;
                    self.note(
                        tick,
                        format!("spikes: down from a fireball at {feet:?}"),
                        format!(
                            "a fireball {fb:.0} off; off the side into the landing water at {l:?}"
                        ),
                    );
                    self.spikes.doing = Some(Doing::Step {
                        to: edge,
                        sneak: false,
                        until: tick + STEP_TICKS,
                    });
                    self.retry_at = tick + 1;
                    return Some(Task::Idle);
                }
            }
            // No landing water: off the pillar's side with the clutch's bucket in the hotbar,
            // a fall in about 2 s the clutch catches (E6's window), where the dig is a block
            // every 40 ticks.
            let clutch = w
                .inventory
                .slots
                .iter()
                .any(|(sl, (k, _))| (36..=44).contains(sl) && *k == ItemKind::WaterBucket);
            let side = Axis::ALL.iter().map(|a| a.step()).find(|(dx, dz)| {
                let c = (feet.0 + dx, feet.1, feet.2 + dz);
                passable(kind(w, c))
                    && passable(kind(w, (c.0, c.1 + 1, c.2)))
                    && open(kind(w, (c.0, c.1 - 1, c.2)))
            });
            if clutch && let Some((dx, dz)) = side {
                let edge = (feet.0 + dx, feet.1, feet.2 + dz);
                self.spikes.clutch_drops += 1;
                self.note(
                    tick,
                    format!("spikes: drop with the clutch from {feet:?}"),
                    format!(
                        "a fireball or the strafe {fb:.0} off; off the side ({dx}, {dz}); {} drops",
                        self.spikes.clutch_drops
                    ),
                );
                self.spikes.doing = Some(Doing::Step {
                    to: edge,
                    sneak: false,
                    until: tick + STEP_TICKS,
                });
                self.retry_at = tick + 1;
                return Some(Task::Idle);
            }
            let c = (feet.0, feet.1 - 1, feet.2);
            self.spikes_note(
                tick,
                format!("spikes: down from a fireball at {feet:?}"),
                format!("a fireball {fb:.0} off; the dig at {c:?}"),
            );
            return Some(Task::Dig(dig_down(w, c, tick)));
        }
        if let Some(t) = self.spikes_water(w, tick, &body, feet, &n) {
            return Some(t);
        }
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
                // Up a spike's top with no pillar down: never a wait there (race 46059621531, the
                // crystals_5 resume: 7 of 10 waited "stranded" until a fireball's cloud came):
                // to its edge over ground and off it onto the clutch.
                if body.on_ground && on_spike_top(w, feet) {
                    if let Some(k) = knock_risk(d.as_ref()) {
                        self.spikes_note(
                            tick,
                            "spikes: hold from the knock-off".to_owned(),
                            format!("{k}; on the spike's top at {feet:?}"),
                        );
                        return Some(self.spikes_hold(tick));
                    }
                    let clutch =
                        w.inventory.slots.iter().any(|(sl, (k, _))| {
                            (36..=44).contains(sl) && *k == ItemKind::WaterBucket
                        });
                    match (clutch, spike_edge(w, feet)) {
                        (true, Some((s, o))) if s == feet => {
                            self.spikes.clutch_drops += 1;
                            self.note(
                                tick,
                                format!("spikes: off the spike's top onto the clutch at {feet:?}"),
                                format!("{why}; off to {o:?}, ground under it"),
                            );
                            self.spikes.doing = Some(Doing::Step {
                                to: o,
                                sneak: false,
                                until: tick + STEP_TICKS,
                            });
                            self.retry_at = tick + 1;
                            return Some(Task::Idle);
                        }
                        (true, Some((s, _))) => {
                            let mut walk = Walk::new(s, Goal::Block { pos: s }, Arrive::Goal, tick);
                            if walk.route_now(w, tick) && walk.route == Some(path::Status::Found) {
                                self.spikes_note(
                                    tick,
                                    format!("spikes: to the spike's edge at {s:?}"),
                                    format!("{why}; from {feet:?}, to step off onto the clutch"),
                                );
                                return Some(Task::Explore(walk));
                            }
                            self.expanded += walk.expanded;
                        }
                        (true, None) => {
                            // Caged: out through the bar facing the island's centre best.
                            if let Some(bar) = cage_bar(w, feet) {
                                self.spikes_note(
                                    tick,
                                    format!("spikes: out of the cage at {bar:?}"),
                                    format!(
                                        "{why}; from {feet:?}, the bar toward the island's centre"
                                    ),
                                );
                                let mut dg = Dig::new(w, bar, tick);
                                dg.drop = None;
                                dg.others.clear();
                                return Some(Task::Dig(dg));
                            }
                        }
                        (false, _) => {}
                    }
                    self.spikes_note(
                        tick,
                        "spikes: stranded on the spike's top".to_owned(),
                        format!(
                            "{why} at {feet:?}; {}",
                            if clutch {
                                "no edge over ground"
                            } else {
                                "no water bucket in the hotbar"
                            }
                        ),
                    );
                    if !clutch {
                        self.spikes_bucket_swap(w, tick);
                    }
                    return Some(self.spikes_hold(tick));
                }
                self.spikes_note(
                    tick,
                    "spikes: wait".to_owned(),
                    format!("{why} at {feet:?}"),
                );
                Some(self.spikes_hold(tick))
            }
            Next::Walk { to, approach } => {
                // No walk to a pillar's foot without the blocks for its pillar (frontier race
                // 45969992660 lane 1: each time E3's restock paused, the walk took the bot back
                // out to the foot, 25 from its bunker, to refuse the climb there).
                // The one count `blocks_wanted` gives (E3's stock stops at it).
                if approach.is_some()
                    && let Some((_, (_, a))) = chosen_approach(w, &self.spikes, body.pos, tick)
                    && (to == a.foot || stub_side(w, &self.spikes, &a) == Some(to))
                    && to.1 < a.plan.stand.1
                {
                    let need = pillar_wants(w, &self.spikes, &a);
                    let held = pillar_blocks(w);
                    if held < need {
                        self.spikes_note(
                            tick,
                            "spikes: no walk to the foot: too few blocks".to_owned(),
                            format!("{held} blocks held, {need} wanted for the pillar at {to:?}"),
                        );
                        return None;
                    }
                }
                let mut walk = Walk::new(to, Goal::Block { pos: to }, Arrive::Goal, tick);
                if !walk.route_now(w, tick) || walk.route != Some(path::Status::Found) {
                    self.expanded += walk.expanded;
                    if let Some((x, z, a)) = approach {
                        self.spikes_refuse([x, z], a, tick);
                    } else {
                        // A walk with no approach to refuse (the dig-out's stair): after
                        // `STAIR_WALK_TRIES` failed in a row, a new way, not the same retried
                        // (frontier race 45988967677 lane 1: 2,700 ticks of the same walk).
                        let n = match self.spikes.stair_walk_fails {
                            Some((t, n)) if t == to => n + 1,
                            _ => 1,
                        };
                        self.spikes.stair_walk_fails = Some((to, n));
                        if n >= STAIR_WALK_TRIES {
                            self.spikes.stair_walk_fails = None;
                            self.spikes.dig_out_turn = !self.spikes.dig_out_turn;
                            self.note(
                                tick,
                                format!("spikes: the way out turned at {feet:?}"),
                                format!(
                                    "{n} walks to {to:?} failed; the stair along the other axis now ({})",
                                    if self.spikes.dig_out_turn { "turned" } else { "straight" }
                                ),
                            );
                        }
                    }
                    self.spikes_note(
                        tick,
                        format!("spikes: no way to {to:?}"),
                        format!("from {feet:?}; approach {approach:?} refused"),
                    );
                    return Some(self.spikes_hold(tick));
                }
                // A route found: the failed walks' count over; out to a foot, the way out
                // straight again.
                self.spikes.stair_walk_fails = None;
                if approach.is_some() {
                    self.spikes.dig_out_turn = false;
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
                // No climb begins without the blocks for the whole pillar and its two wall
                // blocks: frontier race 45959448248 lane 3 held 0 end stone, put up the one block
                // each dig down gave back, 104 times to the cap, and its standing pillar record
                // kept E3's stock (paused for a strafe) from ever resuming. Handed back, so the
                // stock runs again.
                let need = chosen_approach(w, &self.spikes, body.pos, tick)
                    .filter(|(_, (_, a))| {
                        a.plan.column == column
                            || stub_side(w, &self.spikes, a).is_some_and(|s| (s.0, s.2) == column)
                    })
                    .map_or(top - feet.1 + PILLAR_SPARE, |(_, (_, a))| {
                        pillar_wants(w, &self.spikes, &a)
                    });
                let held = pillar_blocks(w);
                if !climbing && held < need {
                    self.spikes_note(
                        tick,
                        "spikes: no climb: too few blocks".to_owned(),
                        format!("{held} blocks held, {need} wanted for the pillar to {top} from {feet:?}"),
                    );
                    return None;
                }
                // No climb begins without a full water bucket in the hotbar (the clutch's), the
                // orchestrator's invariant after race 45908299055: with none, the pool refills
                // it (`spikes_water`, ahead of this) or the bot climbs no pillar at all.
                if !climbing {
                    let hotbar =
                        w.inventory.slots.iter().any(|(sl, (k, _))| {
                            (36..=44).contains(sl) && *k == ItemKind::WaterBucket
                        });
                    if !hotbar {
                        let held = craft::count(w, ItemKind::WaterBucket) > 0;
                        if held {
                            self.spikes_bucket_swap(w, tick);
                        }
                        self.spikes_note(
                            tick,
                            "spikes: no climb: no water".to_owned(),
                            format!(
                                "no water bucket in the hotbar at {feet:?}{}",
                                if held { "; one moved there" } else { "" }
                            ),
                        );
                        return Some(self.spikes_hold(tick));
                    }
                }
                // The landing water beside the pillar's foot: the way down is a drop into it.
                // Poured from the first block up, never from the foot: poured there it ran into
                // the foot's cell and a body in water cannot jump-place (race 45949063425 lane 0:
                // three climbs, none rose). Two buckets held (one stays for the clutch).
                if let Some(l) = self.landing_due(w, column, feet, top) {
                    // Centred over the column, and aimed at the landing floor's far side: off the
                    // centre the ray met the first block's top and the water went into the feet's
                    // own cell, the column read as shut and the pillar dug away (frontier race
                    // 45954274931 lanes 1 and 2; `why` at tick 12805: water at (12, 63, 36)).
                    let off = (f64::from(feet.0) + 0.5 - body.pos[0])
                        .abs()
                        .max((f64::from(feet.2) + 0.5 - body.pos[2]).abs());
                    if off > STEP_NEAR {
                        self.spikes_note(
                            tick,
                            format!("spikes: centre for the landing water at {feet:?}"),
                            format!("{off:.2} off before the pour"),
                        );
                        self.spikes.doing = Some(Doing::Step {
                            to: feet,
                            sneak: true,
                            until: tick + STEP_TICKS,
                        });
                        self.retry_at = tick + 1;
                        return Some(Task::Idle);
                    }
                    self.spikes.landings.push((column, l));
                    self.spikes_note(
                        tick,
                        format!("spikes: landing water poured at {l:?}"),
                        format!("from the first block up at {feet:?}, the drop from {top}"),
                    );
                    let (sx, sz) = (
                        f64::from((l.0 - column.0).signum()),
                        f64::from((l.2 - column.1).signum()),
                    );
                    self.spikes.doing = Some(Doing::Bucket {
                        look: [
                            f64::from(l.0) + 0.5 + LANDING_AIM * sx,
                            f64::from(l.1),
                            f64::from(l.2) + 0.5 + LANDING_AIM * sz,
                        ],
                        item: ItemKind::WaterBucket,
                        want: ItemKind::Bucket,
                        count: craft::count(w, ItemKind::Bucket) + 1,
                        until: tick + BUCKET_TICKS,
                        last: None,
                        why: "the landing water's pour".to_owned(),
                    });
                    self.retry_at = tick + 1;
                    return Some(Task::Idle);
                }
                if climbing
                    && self.first_block_up(column, feet)
                    && !self.spikes.landings.iter().any(|(c, _)| *c == column)
                {
                    let water = craft::count(w, ItemKind::WaterBucket);
                    let why = if water < 2 {
                        format!("{water} water buckets held: the clutch's only")
                    } else {
                        "no side of the foot with a floor and clear air to the top".to_owned()
                    };
                    self.spikes_note(
                        tick,
                        "spikes: no landing water".to_owned(),
                        format!("{why}; at {feet:?}, the way down the dig"),
                    );
                }
                // At the foot the rules; once up, no wait on a half-built pillar: on to the box.
                let fireball_refusal = fireball_near(w, body.pos)
                    .map(|fb| format!("a fireball in flight {fb:.0} off"));
                // The dragon by E3's one calm rule (`Plan::calm`: none, or holding 40 off or more
                // and not closing over 20 ticks), for the start and the going on alike.
                let calm_refusal = self.spikes_calm_refusal(w, tick, d.as_ref());
                let refusal = climb_refusal(None, health, danger)
                    .or(calm_refusal)
                    .or(fireball_refusal)
                    .or_else(|| {
                        threatens(d.as_ref())
                            .then(|| "the dragon strafes or charges near".to_owned())
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
                    // Up already: no climb on through (E5's read of race 46063334187 lane 5,
                    // struck off 3 under its stand with the dragon 63 off): down by water, or
                    // held, never on, but near the stand with the dragon holding and not closing.
                    if climb_on_near_stand(d.as_ref(), top - feet.1) {
                        self.spikes_note(
                            tick,
                            "spikes: climb on, the stand near".to_owned(),
                            format!("{why}; at {feet:?}, {} under {top}", top - feet.1),
                        );
                    } else {
                        // Calm (the refusal health's) and no fireball: down by the dig, the blocks
                        // back; the water only when the dragon is not calm.
                        if self.calm(w, tick) && fireball_near(w, body.pos).is_none() {
                            let c = (feet.0, feet.1 - 1, feet.2);
                            self.spikes_note(
                                tick,
                                "spikes: climb stopped: down by the dig, calm".to_owned(),
                                format!("{why}; at {feet:?}"),
                            );
                            return Some(Task::Dig(dig_down(w, c, tick)));
                        }
                        if let Some(rec) = self
                            .spikes
                            .pillars
                            .iter()
                            .find(|r| (r.x, r.z) == column)
                            .copied()
                            && let Some(t) = self.spikes_descend(w, tick, feet, rec, &why)
                        {
                            return Some(t);
                        }
                        self.spikes_note(
                            tick,
                            "spikes: climb stopped, held".to_owned(),
                            format!("{why}; at {feet:?}, no water way down"),
                        );
                        return Some(self.spikes_hold(tick));
                    }
                }
                if pillar_blocks(w) == 0 && climbing {
                    let c = (feet.0, feet.1 - 1, feet.2);
                    self.spikes_note(
                        tick,
                        format!("spikes: out of blocks, dig down at {c:?}"),
                        format!("{} under {top}", top - feet.1),
                    );
                    return Some(Task::Dig(dig_down(w, c, tick)));
                }
                if pillar_blocks(w) == 0 {
                    self.spikes_note(
                        tick,
                        "spikes: no block to pillar with".to_owned(),
                        format!("at {feet:?} under {top}"),
                    );
                    return Some(self.spikes_hold(tick));
                }
                if !self.spikes.pillars.iter().any(|p| (p.x, p.z) == column) {
                    // Onto a stub of its own: its record again, from its base; the side pillar
                    // that reached it is left a stub.
                    let base = match self.spikes.stubs.iter().position(|r| (r.x, r.z) == column) {
                        Some(i) => {
                            let r = self.spikes.stubs.remove(i);
                            let sides: Vec<PillarRec> = self
                                .spikes
                                .pillars
                                .iter()
                                .filter(|q| (q.x - column.0).abs() + (q.z - column.1).abs() == 1)
                                .copied()
                                .collect();
                            self.spikes
                                .pillars
                                .retain(|q| (q.x - column.0).abs() + (q.z - column.1).abs() != 1);
                            self.spikes.stubs.extend(sides);
                            self.note(
                                tick,
                                format!("spikes: on the stub at {feet:?}"),
                                format!("its base {}; the climb goes on from its top", r.base),
                            );
                            r.base
                        }
                        None => feet.1,
                    };
                    self.spikes.pillars.push(PillarRec {
                        x: column.0,
                        z: column.1,
                        base,
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
                if pillar_blocks(w) == 0 {
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
                // Never the dig on in the open column through a charge: the water drop ran first
                // (`spikes_water`, the landing water or the clutch); with neither, held crouched
                // until the charge passes (E9's read of race 46038776204 lane 3: ten digs from 91,
                // on through a charge that began at 7,800, struck at y 83 and flung over the void).
                if !boxed(w, feet)
                    && self
                        .spikes
                        .pillars
                        .iter()
                        .any(|r| (r.x, r.z) == (feet.0, feet.2) && feet.1 > r.base + 1)
                    && let Some(dd) =
                        d.filter(|dd| dd.phase == Some(CHARGE_PHASE) && dd.dist <= THREAT_NEAR)
                {
                    // Never held for good: past `OPEN_HOLD_MAX` the dig goes on all the same (E9's
                    // read of race 46099479836 lane 5: "hold ended by the dragon … the way down"
                    // 45 times with no water, no dig, knocked off by two blows at 34,300).
                    let since = *self.spikes.open_hold_since.get_or_insert(tick);
                    if tick.saturating_sub(since) <= OPEN_HOLD_MAX {
                        self.spikes_note(
                            tick,
                            "spikes: hold, not the dig, from the charge".to_owned(),
                            format!(
                                "the dragon {:.0} off charging; up the open pillar at {feet:?}",
                                dd.dist
                            ),
                        );
                        return Some(self.spikes_hold(tick));
                    }
                    self.note(
                        tick,
                        format!("spikes: down by the dig, held too long at {feet:?}"),
                        format!(
                            "held {} ticks, over {OPEN_HOLD_MAX}; no water way down",
                            tick.saturating_sub(since)
                        ),
                    );
                }
                self.spikes.open_hold_since = None;
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
            Next::DigOut { cell, toward } => {
                // The progress invariant: no nearer its target in `DIG_OUT_STALL` ticks, the
                // dig-out fails, says why and hands back.
                let dist = f64::from(
                    (toward.0 - feet.0).abs()
                        + (toward.1 - feet.1).abs()
                        + (toward.2 - feet.2).abs(),
                );
                if tick < self.spikes.dig_out_off {
                    return None;
                }
                match self.spikes.dig_out_since {
                    Some((_, best)) if dist < best - 1.0 => {
                        self.spikes.dig_out_since = Some((tick, dist));
                    }
                    Some((at, best)) if tick.saturating_sub(at) > DIG_OUT_STALL => {
                        self.spikes.dig_out_since = None;
                        self.spikes.dig_out_off = tick + DIG_OUT_OFF;
                        self.note(
                            tick,
                            format!("spikes: dig-out: no progress at {feet:?}"),
                            format!(
                                "no nearer {toward:?} than {best:.0} in {DIG_OUT_STALL} ticks (the cell {cell:?} next); handed back for {DIG_OUT_OFF}"
                            ),
                        );
                        return None;
                    }
                    Some(_) => {}
                    None => self.spikes.dig_out_since = Some((tick, dist)),
                }
                // The approaches refused for want of a way are tried again once it is dug.
                self.spikes.refused.clear();
                self.spikes_note(
                    tick,
                    format!("spikes: dig the way out at {cell:?}"),
                    format!(
                        "from {feet:?} toward the approach's foot {toward:?}; {}",
                        if shut_in(w, feet) {
                            "shut in a pocket"
                        } else {
                            "every approach refused"
                        }
                    ),
                );
                Some(Task::Dig(dig_down(w, cell, tick)))
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
        // A waterfall ridden: the water carries the body down, and no other row (the escape out
        // of water) takes it off the fall (race 45912100302 lane 1: "trapped in water" at y 77
        // walked it back onto the spike's top, stranded to the cap).
        if self.spikes.riding.is_some()
            && !body.on_ground
            && (body.in_water || body.eye_in_water)
            && self.spikes.clutch.is_none()
        {
            self.spikes_note(
                tick,
                "spikes: riding the waterfall".to_owned(),
                format!("at {:?}", path::feet_cell(body.pos)),
            );
            return Some(down);
        }
        let landing = predict_landing(w, body.pos, body.vel)
            .map(|(c, g, _)| (c, g))
            .or_else(|| landing_ground(w, body.pos, body.vel));
        let ground = landing.map(|(_, g)| g);
        let column = landing.map_or(
            {
                let f = path::feet_cell(body.pos);
                (f.0, f.2)
            },
            |(c, _)| c,
        );
        // The pour looks at the landing column's top (straight down over its centre).
        let aim = landing.map_or(down, |(col, g)| {
            let (yaw, pitch) = crate::walk::look_at(eye(&body), landing_aim(body.pos, col, g));
            Controls {
                yaw,
                pitch: Some(pitch),
                ..Controls::default()
            }
        });
        let height = ground.map(|g| body.pos[1] - g);
        let Some(mut c) = self.spikes.clutch.take() else {
            let falling = !body.on_ground && !body.in_water && body.vel[1] < 0.0;
            // Over the void the clutch begins too: a body flung sideways may come over ground.
            if !falling
                || height.is_some_and(|h| h <= CLUTCH_FROM)
                || (height.is_none() && body.vel[1] > -CLUTCH_VOID_FALL)
                || craft::count(w, ItemKind::WaterBucket) == 0
            {
                return None;
            }
            self.spikes.clutches += 1;
            // The clutch owns the hand (the orchestrator, after race 45899557907 lane 0: the eat
            // row cut in the tick the fall began, and no pour went): any eating stopped, its task
            // dropped, the bucket's slot selected below.
            // The use key let go whatever it held (harmless when nothing is in use).
            self.acts.push(Act::ReleaseUse);
            if matches!(self.task, Task::Eat(_)) {
                self.task = Task::Idle;
                self.note(
                    tick,
                    "spikes: clutch stops the eating".to_owned(),
                    "the hand to the water bucket".to_owned(),
                );
            }
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
                sent_at: None,
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
        // Landed with the pour sent and water at the feet: it poured, the hand's bucket not yet
        // seen emptied (frontier race 45954274931 lane 1: "missed" from 90.9 at health 20, in the
        // water, which was never taken back; the bucket lost and every walk from the water cell
        // refused for 1,600 ticks).
        let late = (c.poured.is_none() && landed && c.sent_at.is_some())
            .then(|| clutch_water(w, path::feet_cell(body.pos)))
            .flatten();
        if let Some(cell) = late {
            c.poured = Some(cell);
            self.note(
                tick,
                format!("spikes: clutch poured late at {cell:?}"),
                format!(
                    "landed in it from {:.1}; health {:?}; to be taken back",
                    c.from_y, w.player.health
                ),
            );
            self.spikes.clutch = Some(c);
            return Some(down);
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
                let ctl = self.spikes_clutch_hold(w, ItemKind::WaterBucket, aim);
                if crate::dig::held_item(w) == Some(ItemKind::Bucket) {
                    let cell = ground.map(|g| (column.0, g as i32, column.1));
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
                    && c.sent_at
                        .is_none_or(|t| tick.saturating_sub(t) >= POUR_RESEND)
                    // Again every tick until the water shows under the feet (E6: retry until
                    // it shows), never once it is there: a use of the emptied bucket would take
                    // it back up.
                    && ground.is_some_and(|g| {
                        kind(w, (column.0, g as i32, column.1)) != Some(BlockKind::Water)
                    })
                    && pour_now_at(body.pos[1], body.vel[1], ground, None)
                    // Never rising (lane 9: "clutch pour sent … falling 1.23", upward, at an
                    // edge: the water went where the body never landed).
                    && body.vel[1] < 0.0
                    && self.acts.is_empty()
                {
                    let land = ground.map(|g| ticks_to_land(body.pos[1], body.vel[1], g));
                    let f = path::feet_cell(body.pos);
                    self.note(
                        tick,
                        "spikes: clutch pour sent".to_owned(),
                        format!(
                            "{:.2} over the ground, falling {:.2}; sent at tick {tick}, landing predicted in {land:?} ticks; on the column {column:?}{}",
                            height.unwrap_or(0.0),
                            body.vel[1],
                            if column == (f.0, f.2) { "" } else { ", beside the feet's (an edge)" }
                        ),
                    );
                    c.sent_at = Some(tick);
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
                // Every landing takes its water back, wherever it went: not in the cell it was
                // poured over, the source round the feet (E5, frontier race 45954274931).
                let cell = if source(w, cell) {
                    cell
                } else if let Some(near) = clutch_water(w, path::feet_cell(body.pos)) {
                    if c.poured != Some(near) {
                        self.note(
                            tick,
                            format!("spikes: clutch water found at {near:?}"),
                            format!("poured over {cell:?}; to be taken back"),
                        );
                    }
                    c.poured = Some(near);
                    near
                } else {
                    cell
                };
                // Over once no source is left there, never because a water bucket is in hand
                // (frontier race 45978279345 lane 4: two water buckets held, the other one in
                // hand read as the water taken back, "the water left" on the ground).
                let water_now = craft::count(w, ItemKind::WaterBucket);
                if !source(w, cell) {
                    // Taken back only when the bucket count went up.
                    let taken = self.spikes.take_back_from.is_some_and(|n| water_now > n);
                    self.spikes.take_back_from = None;
                    self.note(
                        tick,
                        format!("spikes: clutch over at {:?}", path::feet_cell(body.pos)),
                        format!(
                            "fell from {:.1}; the water {}; {water_now} water buckets held; health {:?}",
                            c.from_y,
                            if taken { "taken back" } else { "gone, not taken back" },
                            w.player.health
                        ),
                    );
                    return None;
                }
                if tick.saturating_sub(at) > CLUTCH_TAKE_TICKS {
                    self.spikes.take_back_from = None;
                    if !self.spikes.left_water.contains(&cell) {
                        self.spikes.left_water.push(cell);
                    }
                    self.note(
                        tick,
                        format!("spikes: clutch water left at {cell:?}"),
                        format!("not taken back in {CLUTCH_TAKE_TICKS} ticks; a refill's source"),
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
                    self.spikes.take_back_from.get_or_insert(water_now);
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
            && self.acts.iter().all(|x| matches!(x, Act::ReleaseUse))
        {
            self.acts.push(a);
        }
        down
    }

    /// Up a pillar or a spike, the water bucket kept in the hotbar (`CLUTCH_HOTBAR`), so a fall
    /// selects it with one packet and never waits on a window's clicks. Noted.
    fn spikes_bucket_to_hotbar(&mut self, w: &World, tick: u64) {
        if !self.spikes_up_high(w) {
            return;
        }
        self.spikes_bucket_swap(w, tick);
    }

    /// A water bucket from the main slots into the hotbar (`CLUTCH_HOTBAR`) when none is there.
    fn spikes_bucket_swap(&mut self, w: &World, tick: u64) {
        if !self.acts.is_empty()
            || w.click.is_some()
            || w.window.is_some()
            || w.inventory.carried.is_some()
        {
            return;
        }
        let slots = &w.inventory.slots;
        if slots
            .iter()
            .any(|(s, (k, _))| (36..=44).contains(s) && *k == ItemKind::WaterBucket)
        {
            return;
        }
        let Some(from) = slots
            .iter()
            .find(|(s, (k, _))| (9..=35).contains(*s) && *k == ItemKind::WaterBucket)
            .map(|(s, _)| *s)
        else {
            return;
        };
        let button = if w.held == CLUTCH_HOTBAR {
            CLUTCH_HOTBAR - 1
        } else {
            CLUTCH_HOTBAR
        };
        self.acts.push(Act::Click {
            slot: from,
            button: i8::try_from(button).unwrap_or(8),
            mode: 2,
        });
        self.spikes_note(
            tick,
            "spikes: water bucket to the hotbar".to_owned(),
            format!("from slot {from} to hotbar {button}, up high"),
        );
    }

    /// The water's steps (the orchestrator's waterfall descent): the pool by the bunker made and
    /// the buckets refilled from it on the ground; at a pillar's top with the crystal gone, the
    /// pour on the top and the step off into the falling water, the pillar left standing. One
    /// water bucket is always kept for the clutch: the waterfall pours a second.
    fn spikes_water(
        &mut self,
        w: &World,
        tick: u64,
        body: &physics::Body,
        feet: Cell,
        n: &Next,
    ) -> Option<Task> {
        let water = craft::count(w, ItemKind::WaterBucket);
        let empty = craft::count(w, ItemKind::Bucket);
        // The way down (race 45916169323 lane 1: a pour on the pillar's top made a waterfall too
        // slow to ride, 10 blocks in 400 ticks, and the bot drowned in it): at the top with the
        // crystal gone, a step off the side over the landing water poured at the pillar's foot
        // before the climb, a free fall into it (water at the landing cancels the fall).
        // Calm and no fireball: the way down is the dig, its blocks back to the stock (the
        // orchestrator, after race 46099479836: every stop came down by water and left its stub,
        // ~520 "too few blocks"); the water only when the dragon is not calm or a fireball near.
        if let Next::DigDown(_) = n
            && body.on_ground
            && self.calm(w, tick)
            && fireball_near(w, body.pos).is_none()
        {
            self.spikes_note(
                tick,
                "spikes: down by the dig, calm".to_owned(),
                format!("at {feet:?}: the blocks back to the stock"),
            );
            return None;
        }
        // No drop while the dragon would knock the bot out of it: held crouched until it passes.
        if let Next::DigDown(_) = n
            && body.on_ground
            && self
                .spikes
                .pillars
                .iter()
                .any(|r| (r.x, r.z) == (feet.0, feet.2) && feet.1 > r.base + 4)
            && let Some(why) = knock_risk(dragon_near(w, body.pos).as_ref())
        {
            self.spikes_note(
                tick,
                "spikes: no drop: the knock-off".to_owned(),
                format!("{why}; held at {feet:?}"),
            );
            return Some(self.spikes_hold(tick));
        }
        if let Next::DigDown(_) = n
            && body.on_ground
            && let Some(rec) = self
                .spikes
                .pillars
                .iter()
                .find(|r| (r.x, r.z) == (feet.0, feet.2) && feet.1 > r.base + 4)
                .copied()
            && let Some((_, l)) = self
                .spikes
                .landings
                .iter()
                .find(|(c, _)| *c == (rec.x, rec.z))
                .copied()
            && kind(w, l) == Some(BlockKind::Water)
        {
            let (dx, dz) = (l.0 - rec.x, l.2 - rec.z);
            let edge = (feet.0 + dx, feet.1, feet.2 + dz);
            if passable(kind(w, edge)) && passable(kind(w, (edge.0, edge.1 + 1, edge.2))) {
                self.spikes.riding = Some((rec.x, rec.z));
                self.spikes.waterfalls += 1;
                self.note(
                    tick,
                    format!("spikes: waterfall down from {feet:?}"),
                    format!(
                        "off its side ({dx}, {dz}) into the landing water at {l:?}; {} drops; health {:?}",
                        self.spikes.waterfalls, w.player.health
                    ),
                );
                self.spikes.doing = Some(Doing::Step {
                    to: edge,
                    sneak: false,
                    until: tick + STEP_TICKS,
                });
                self.retry_at = tick + 1;
                return Some(Task::Idle);
            }
        }
        // No landing water: down by the clutch, off a side that falls clear, not by the dig (part
        // 2: the dig back wore out the pickaxe, and race 45994686575 lane 3 dug from 91 to 65 in
        // 1,800 ticks, caught on the pillar by the charge). The pillar is left standing.
        if let Next::DigDown(_) = n
            && body.on_ground
            && let Some(rec) = self
                .spikes
                .pillars
                .iter()
                .find(|r| (r.x, r.z) == (feet.0, feet.2) && feet.1 > r.base + 4)
                .copied()
            && w.inventory
                .slots
                .iter()
                .any(|(sl, (k, _))| (36..=44).contains(sl) && *k == ItemKind::WaterBucket)
            && let Some((dx, dz)) = Axis::ALL.iter().map(|a| a.step()).find(|(dx, dz)| {
                let c = (feet.0 + dx, feet.1, feet.2 + dz);
                passable(kind(w, c))
                    && passable(kind(w, (c.0, c.1 + 1, c.2)))
                    && open(kind(w, (c.0, c.1 - 1, c.2)))
            })
        {
            let edge = (feet.0 + dx, feet.1, feet.2 + dz);
            self.spikes.riding = Some((rec.x, rec.z));
            self.spikes.clutch_drops += 1;
            self.note(
                tick,
                format!("spikes: clutch down from {feet:?}"),
                format!(
                    "no landing water; off the side ({dx}, {dz}) onto the clutch, the pillar left standing; {} drops; health {:?}",
                    self.spikes.clutch_drops, w.player.health
                ),
            );
            self.spikes.doing = Some(Doing::Step {
                to: edge,
                sneak: false,
                until: tick + STEP_TICKS,
            });
            self.retry_at = tick + 1;
            return Some(Task::Idle);
        }
        if !body.on_ground || self.spikes_up_high(w) {
            return None;
        }
        // Landing water poured for a pillar not climbed while the way now goes elsewhere: taken
        // back first (race 45937952400 lane 0: poured at (38, 59, 0), the West approach then
        // refused from the bunker, the South pillar climbed and dug down, the bucket lost).
        let toward = match n {
            Next::Pillar { column, .. } => Some(*column),
            Next::Walk {
                to,
                approach: Some(_),
            } => Some((to.0, to.2)),
            _ => None,
        };
        if let Some(col) = toward
            && let Some(i) = self.spikes.landings.iter().position(|(c, _)| {
                *c != col && !self.spikes.pillars.iter().any(|r| (r.x, r.z) == *c)
            })
        {
            let (c, l) = self.spikes.landings[i];
            let target = [
                f64::from(l.0) + 0.5,
                f64::from(l.1) + 0.5,
                f64::from(l.2) + 0.5,
            ];
            let reach = (0..3)
                .map(|k| (target[k] - eye(body)[k]).powi(2))
                .sum::<f64>()
                .sqrt();
            if kind(w, l) != Some(BlockKind::Water) || empty == 0 {
                self.spikes.landings.remove(i);
                self.note(
                    tick,
                    format!("spikes: landing water forgotten at {l:?}"),
                    format!("{:?} there, {empty} empty buckets", kind(w, l)),
                );
            } else if reach <= POUR_REACH {
                self.spikes.landings.remove(i);
                self.note(
                    tick,
                    format!("spikes: landing water taken back at {l:?}"),
                    format!("unused: the way goes to {col:?}, not {c:?}"),
                );
                self.spikes.doing = Some(Doing::Bucket {
                    look: target,
                    item: ItemKind::Bucket,
                    want: ItemKind::WaterBucket,
                    count: water + 1,
                    until: tick + BUCKET_TICKS,
                    last: None,
                    why: "the unused landing water's take-back".to_owned(),
                });
                self.retry_at = tick + 1;
                return Some(Task::Idle);
            } else {
                let foot = (c.0, l.1, c.1);
                let mut walk = Walk::new(foot, Goal::Block { pos: foot }, Arrive::Goal, tick);
                if walk.route_now(w, tick) && walk.route == Some(path::Status::Found) {
                    self.spikes_note(
                        tick,
                        format!("spikes: back for the landing water at {l:?}"),
                        format!("unused: the way goes to {col:?}"),
                    );
                    return Some(Task::Explore(walk));
                }
                self.expanded += walk.expanded;
                self.spikes.landings.remove(i);
                if !self.spikes.left_water.contains(&l) {
                    self.spikes.left_water.push(l);
                }
                self.note(
                    tick,
                    format!("spikes: landing water left at {l:?}"),
                    format!("no way back to it from {feet:?}"),
                );
            }
        }
        // The pool by the bunker: two cells dug one deep and both buckets poured in.
        if !self.spikes.pool_made
            && !self.spikes.pool_off
            && let Some(b) = self.bunker
            && (b[0] - feet.0).abs().max((b[2] - feet.2).abs()) <= POOL_NEAR
        {
            // A pool is begun only with both its sources in hand: one bucket alone makes no
            // infinite source, and would be lost.
            if self.spikes.pool.is_none() {
                // Race 45912100302 wrote nothing of why no pool was made: every refusal says so.
                if water < 2 {
                    self.spikes_note(
                        tick,
                        "spikes: no pool: buckets".to_owned(),
                        format!("{water} water buckets held, two wanted, at {feet:?}"),
                    );
                } else {
                    match pool_site(w, feet, self.bunker) {
                        Some((stand, sq)) if stand == feet => {
                            self.spikes.pool_square = Some(sq);
                            self.spikes.pool = Some([sq[0], sq[3]]);
                            self.spikes.pool_stand = Some(stand);
                        }
                        Some((stand, _)) => {
                            let mut walk =
                                Walk::new(stand, Goal::Block { pos: stand }, Arrive::Goal, tick);
                            if walk.route_now(w, tick) && walk.route == Some(path::Status::Found) {
                                self.spikes_note(
                                    tick,
                                    format!("spikes: to the pool site at {stand:?}"),
                                    format!("from {feet:?}, the bunker at {b:?}"),
                                );
                                return Some(Task::Explore(walk));
                            }
                            self.expanded += walk.expanded;
                            self.spikes_note(
                                tick,
                                "spikes: no pool: no way to its site".to_owned(),
                                format!("{stand:?} from {feet:?}"),
                            );
                        }
                        None => self.spikes_note(
                            tick,
                            "spikes: no pool: no site".to_owned(),
                            format!(
                                "no two walled end stone cells within {POOL_SITE} of {feet:?}; {}",
                                pool_refusal(w, feet)
                            ),
                        ),
                    }
                }
            }
            if let Some([c0, c1]) = self.spikes.pool {
                let square = self.spikes.pool_square.unwrap_or([c0, c1, c0, c1]);
                // The pool is worked from its stand only (race 45934621769 lane 0: back in its
                // bunker 6 off, the dig of a pool cell was refused 25 times to the cap).
                let stand = self.spikes.pool_stand.unwrap_or(feet);
                if feet != stand
                    && square
                        .iter()
                        .any(|c| full(kind(w, *c)) || open(kind(w, *c)))
                {
                    let mut walk = Walk::new(stand, Goal::Block { pos: stand }, Arrive::Goal, tick);
                    if walk.route_now(w, tick) && walk.route == Some(path::Status::Found) {
                        self.spikes_note(
                            tick,
                            format!("spikes: to the pool site at {stand:?}"),
                            format!("from {feet:?}, to work the pool"),
                        );
                        return Some(Task::Explore(walk));
                    }
                    self.expanded += walk.expanded;
                    self.spikes.pool_off = true;
                    self.note(
                        tick,
                        "spikes: no pool: given up".to_owned(),
                        format!("no way from {feet:?} to its stand {stand:?}"),
                    );
                    return None;
                }
                for c in square {
                    if full(kind(w, c)) {
                        self.spikes.pool_digs += 1;
                        if self.spikes.pool_digs > POOL_DIGS {
                            self.spikes.pool_off = true;
                            self.note(
                                tick,
                                "spikes: no pool: given up".to_owned(),
                                format!(
                                    "{} digs for its four cells; {c:?} still there",
                                    self.spikes.pool_digs
                                ),
                            );
                            return None;
                        }
                        self.spikes_note(
                            tick,
                            format!("spikes: water pool dug at {c:?}"),
                            format!("by the bunker at {b:?}"),
                        );
                        return Some(Task::Dig(dig_down(w, c, tick)));
                    }
                }
                for c in [c0, c1] {
                    // The water stays inside the dug square: its floor full and every side full
                    // or the square's own (E5, frontier race 45954274931: water spread over the
                    // ground left no start for a walk).
                    let held_in = full(kind(w, (c.0, c.1 - 1, c.2)))
                        && Axis::ALL.iter().all(|a| {
                            let (dx, dz) = a.step();
                            let n = (c.0 + dx, c.1, c.2 + dz);
                            square.contains(&n) || full(kind(w, n))
                        });
                    if open(kind(w, c)) && water > 0 && !held_in {
                        self.spikes.pool_off = true;
                        self.note(
                            tick,
                            "spikes: no pool: not walled".to_owned(),
                            format!("{c:?} would run out over the ground; square {square:?}"),
                        );
                        return None;
                    }
                    if open(kind(w, c)) && water > 0 {
                        // Centred on the stand first, and aimed at the far side of the cell's
                        // floor: race 45930464018 lane 0 stood at x 10.8 off its stand's centre,
                        // the ray met the stand's own floor, the water went in the stand's cell
                        // and down the bunker's stair, and the bot drowned in its bunker.
                        let stand = self.spikes.pool_stand.unwrap_or(feet);
                        let off = (f64::from(stand.0) + 0.5 - body.pos[0])
                            .abs()
                            .max((f64::from(stand.2) + 0.5 - body.pos[2]).abs());
                        if off > STEP_NEAR {
                            self.spikes_note(
                                tick,
                                format!("spikes: centre on the pool's stand at {stand:?}"),
                                format!("{off:.2} off before the pour"),
                            );
                            self.spikes.doing = Some(Doing::Step {
                                to: stand,
                                sneak: true,
                                until: tick + STEP_TICKS,
                            });
                            self.retry_at = tick + 1;
                            return Some(Task::Idle);
                        }
                        let (sx, sz) = (
                            f64::from((c.0 - stand.0).signum()),
                            f64::from((c.2 - stand.2).signum()),
                        );
                        self.spikes_note(
                            tick,
                            format!("spikes: water pool poured at {c:?}"),
                            format!("{water} water buckets held"),
                        );
                        self.spikes.doing = Some(Doing::Bucket {
                            look: [
                                f64::from(c.0) + 0.5 + 0.3 * sx,
                                f64::from(c.1),
                                f64::from(c.2) + 0.5 + 0.3 * sz,
                            ],
                            item: ItemKind::WaterBucket,
                            want: ItemKind::Bucket,
                            count: empty + 1,
                            until: tick + BUCKET_TICKS,
                            last: None,
                            why: "the pool's pour".to_owned(),
                        });
                        self.retry_at = tick + 1;
                        return Some(Task::Idle);
                    }
                }
                if square.iter().all(|c| kind(w, *c) == Some(BlockKind::Water)) {
                    self.spikes.pool_made = true;
                    self.note(
                        tick,
                        format!("spikes: water pool at {c0:?} and {c1:?}"),
                        "an infinite source by the bunker".to_owned(),
                    );
                } else if water == 0 {
                    // Both buckets poured, the pool's update not yet seen: wait by it, never
                    // leave with no water (race 45908299055: both bots walked off with none,
                    // no clutch and no waterfall, and fell dead).
                    self.spikes_note(
                        tick,
                        "spikes: waiting on the pool".to_owned(),
                        format!("{c0:?} {:?}, {c1:?} {:?}", kind(w, c0), kind(w, c1)),
                    );
                    return Some(self.spikes_hold(tick));
                }
            }
        }
        // No water bucket and an empty one held: refilled from the water the bot left first, its
        // own task (E9's read of race 46038776204 lanes 7 and 9: "no climb: no water" for 10,000
        // ticks and more, holding empty buckets, its clutch and landing water left on the ground).
        self.spikes.left_water.retain(|c| source(w, *c));
        if water == 0
            && empty > 0
            && let Some(c) = self
                .spikes
                .left_water
                .iter()
                .copied()
                .filter(|c| {
                    !self.spikes.left_water_fails.iter().any(|(x, n, at)| {
                        let wait = if *n >= LEFT_WATER_TRIES {
                            LEFT_WATER_RETRY * 5
                        } else {
                            LEFT_WATER_RETRY
                        };
                        x == c && tick.saturating_sub(*at) < wait
                    })
                })
                .min_by_key(|c| (c.0 - feet.0).abs() + (c.1 - feet.1).abs() + (c.2 - feet.2).abs())
        {
            let target = [
                f64::from(c.0) + 0.5,
                f64::from(c.1) + 0.5,
                f64::from(c.2) + 0.5,
            ];
            let reach = (0..3)
                .map(|i| (target[i] - eye(body)[i]).powi(2))
                .sum::<f64>()
                .sqrt();
            if reach <= POUR_REACH {
                self.note(
                    tick,
                    format!("spikes: refill from the water left at {c:?}"),
                    format!("{empty} empty, {water} full"),
                );
                self.spikes.doing = Some(Doing::Bucket {
                    look: target,
                    item: ItemKind::Bucket,
                    want: ItemKind::WaterBucket,
                    count: water + 1,
                    until: tick + BUCKET_TICKS,
                    last: None,
                    why: "the refill from the water left".to_owned(),
                });
                self.retry_at = tick + 1;
                return Some(Task::Idle);
            }
            // A dry cell to stand in within reach of the source (race 46079924222 lane 5: the
            // water spread round the cell it was poured from, the goal beside it was flowing
            // water, the walk found no route, and the record was dropped at once).
            let beside = (-LEFT_WATER_STAND..=LEFT_WATER_STAND)
                .flat_map(|dx| {
                    (-LEFT_WATER_STAND..=LEFT_WATER_STAND)
                        .flat_map(move |dz| (-1..=1).map(move |dy| (c.0 + dx, c.1 + dy, c.2 + dz)))
                })
                .filter(|b| {
                    full(kind(w, (b.0, b.1 - 1, b.2)))
                        && open(kind(w, *b))
                        && open(kind(w, (b.0, b.1 + 1, b.2)))
                        && (0..3)
                            .map(|i| {
                                let e = [
                                    f64::from(b.0) + 0.5,
                                    f64::from(b.1) + 1.62,
                                    f64::from(b.2) + 0.5,
                                ];
                                (target[i] - e[i]).powi(2)
                            })
                            .sum::<f64>()
                            .sqrt()
                            <= POUR_REACH
                })
                .min_by_key(|b| {
                    (
                        (b.0 - feet.0).abs() + (b.1 - feet.1).abs() + (b.2 - feet.2).abs(),
                        *b,
                    )
                });
            if let Some(b) = beside {
                let mut walk = Walk::new(b, Goal::Block { pos: b }, Arrive::Goal, tick);
                if walk.route_now(w, tick) && walk.route == Some(path::Status::Found) {
                    self.spikes_note(
                        tick,
                        format!("spikes: to the water left at {c:?}"),
                        format!("from {feet:?}, to refill"),
                    );
                    return Some(Task::Explore(walk));
                }
                self.expanded += walk.expanded;
            }
            // Kept for a later try; dropped only after `LEFT_WATER_TRIES` failed walks.
            // A walk failed shut in (a notch, a pit) is no count against the water (race
            // 46099479836 lane 2: 5 walks from one stuck cell 22 off, then dropped, then "no
            // climb: no water" from 17,060); and the record is never dropped for walks, only
            // when its source is gone.
            let pocket = shut_in(w, feet);
            let n = match self
                .spikes
                .left_water_fails
                .iter_mut()
                .find(|(x, _, _)| *x == c)
            {
                Some((_, n, at)) => {
                    if !pocket {
                        *n += 1;
                    }
                    *at = tick;
                    *n
                }
                None => {
                    self.spikes
                        .left_water_fails
                        .push((c, u32::from(!pocket), tick));
                    u32::from(!pocket)
                }
            };
            // Past `LEFT_WATER_TRIES` the retries come further apart, never stop.
            self.spikes_note(
                tick,
                format!("spikes: no way yet to the water left at {c:?}"),
                format!(
                    "from {feet:?}; {n} walks failed{}, kept",
                    if pocket {
                        " (this one shut in, not counted)"
                    } else {
                        ""
                    }
                ),
            );
        }
        // A refill: an empty bucket held and water in a pool cell (made or not: never leave the
        // pool with an empty hand).
        // Each try takes the next cell of the square in turn (race 45924374354 lane 0: the
        // second refill aimed at the cell the first had just emptied, and gave up).
        if empty > 0
            && let Some(pour) = self.spikes.pool
            && let Some(c0) = {
                let square = self
                    .spikes
                    .pool_square
                    .unwrap_or([pour[0], pour[1], pour[0], pour[1]]);
                let turn = usize::try_from(self.spikes.refills).unwrap_or(0);
                // A source by the server's word (water level 0), never by prediction; a cell
                // that gave nothing waits for the server to show it otherwise (the
                // orchestrator, after race 45937952400's give-ups).
                let failed = &self.spikes.refill_failed;
                (0..4).map(|i| square[(turn + i) % 4]).find(|c| {
                    let state = w.state_at(BlockPos::new(c.0, c.1, c.2));
                    source(w, *c) && !failed.iter().any(|(f, st)| f == c && Some(*st) == state)
                })
            }
        {
            let target = [
                f64::from(c0.0) + 0.5,
                f64::from(c0.1) + 0.5,
                f64::from(c0.2) + 0.5,
            ];
            let reach = (0..3)
                .map(|i| (target[i] - eye(body)[i]).powi(2))
                .sum::<f64>()
                .sqrt();
            if reach <= POUR_REACH {
                self.spikes.refills += 1;
                self.note(
                    tick,
                    format!("spikes: refilled at {c0:?}"),
                    format!(
                        "{empty} empty, {water} full; refill {}",
                        self.spikes.refills
                    ),
                );
                self.spikes.doing = Some(Doing::Bucket {
                    look: target,
                    item: ItemKind::Bucket,
                    want: ItemKind::WaterBucket,
                    count: water + 1,
                    until: tick + BUCKET_TICKS,
                    last: None,
                    why: "the refill".to_owned(),
                });
                self.retry_at = tick + 1;
                return Some(Task::Idle);
            }
            if let Some(stand) = self.spikes.pool_stand
                && (stand.0 - feet.0).abs().max((stand.2 - feet.2).abs()) <= POOL_FAR
            {
                let mut walk = Walk::new(stand, Goal::Block { pos: stand }, Arrive::Goal, tick);
                if walk.route_now(w, tick) && walk.route == Some(path::Status::Found) {
                    self.spikes_note(
                        tick,
                        format!("spikes: to the pool at {stand:?}"),
                        format!("{empty} empty buckets"),
                    );
                    return Some(Task::Explore(walk));
                }
                self.expanded += walk.expanded;
                self.spikes_note(
                    tick,
                    "spikes: no refill: no way to the pool".to_owned(),
                    format!("{stand:?} from {feet:?}; {empty} empty, {water} full"),
                );
            } else {
                self.spikes_note(
                    tick,
                    "spikes: no refill: the pool too far".to_owned(),
                    format!(
                        "its stand {:?} over {POOL_FAR} from {feet:?}",
                        self.spikes.pool_stand
                    ),
                );
            }
        } else if empty > 0 && water == 0 && self.spikes.pool.is_some() {
            // Frontier race 45954274931 lane 1: no water, the climbs refused 2,000 ticks, and
            // nothing said why no refill was gone for.
            self.spikes_note(
                tick,
                "spikes: no refill: no source seen".to_owned(),
                format!(
                    "the pool {:?}: {:?}; from {feet:?}",
                    self.spikes.pool_square,
                    self.spikes.pool_square.map(|sq| sq.map(|c| kind(w, c)))
                ),
            );
        }
        None
    }

    /// Down off a pillar by water: off the side over its landing water when poured, else off a
    /// side that falls clear onto the clutch (its bucket in the hotbar); the pillar left
    /// standing, its record let go on landing. `None` when neither is there.
    fn spikes_descend(
        &mut self,
        w: &World,
        tick: u64,
        feet: Cell,
        rec: PillarRec,
        why: &str,
    ) -> Option<Task> {
        let landing = self
            .spikes
            .landings
            .iter()
            .find(|(c, _)| *c == (rec.x, rec.z))
            .map(|(_, l)| *l)
            .filter(|l| kind(w, *l) == Some(BlockKind::Water));
        let toward_water = landing
            .map(|l| (l.0 - rec.x, l.2 - rec.z))
            .filter(|(dx, dz)| {
                let c = (feet.0 + dx, feet.1, feet.2 + dz);
                passable(kind(w, c)) && passable(kind(w, (c.0, c.1 + 1, c.2)))
            });
        let clutch = w
            .inventory
            .slots
            .iter()
            .any(|(sl, (k, _))| (36..=44).contains(sl) && *k == ItemKind::WaterBucket);
        let clear_side = || {
            Axis::ALL.iter().map(|a| a.step()).find(|(dx, dz)| {
                let c = (feet.0 + dx, feet.1, feet.2 + dz);
                passable(kind(w, c))
                    && passable(kind(w, (c.0, c.1 + 1, c.2)))
                    && open(kind(w, (c.0, c.1 - 1, c.2)))
            })
        };
        let (dx, dz, what) = match (toward_water, clutch) {
            (Some((dx, dz)), _) => (dx, dz, "waterfall"),
            (None, true) => {
                let (dx, dz) = clear_side()?;
                (dx, dz, "clutch")
            }
            (None, false) => return None,
        };
        let edge = (feet.0 + dx, feet.1, feet.2 + dz);
        self.spikes.riding = Some((rec.x, rec.z));
        if what == "waterfall" {
            self.spikes.waterfalls += 1;
        } else {
            self.spikes.clutch_drops += 1;
        }
        self.note(
            tick,
            format!("spikes: {what} down from {feet:?}"),
            format!("{why}; off the side ({dx}, {dz}), the pillar left standing"),
        );
        self.spikes.doing = Some(Doing::Step {
            to: edge,
            sneak: false,
            until: tick + STEP_TICKS,
        });
        self.retry_at = tick + 1;
        Some(Task::Idle)
    }

    /// Why the dragon bars a climb now by E3's calm rule (`Plan::calm`), or `None` when calm.
    fn spikes_calm_refusal(
        &mut self,
        w: &World,
        tick: u64,
        d: Option<&DragonNear>,
    ) -> Option<String> {
        (!self.calm(w, tick)).then(|| {
            format!(
                "the dragon not calm: {:?} (holding {CLIMB_NEAR:.0} off or more, not closing)",
                d.map(|d| (d.dist, d.phase))
            )
        })
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
        self.spikes_bucket_to_hotbar(w, tick);
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
            Doing::Bucket {
                look,
                item,
                want,
                count,
                until,
                last,
                why,
            } => {
                if craft::count(w, *want) >= *count {
                    self.note(
                        tick,
                        format!("spikes: {why} done"),
                        format!("{want:?} held {}", craft::count(w, *want)),
                    );
                    return None;
                }
                if tick >= *until {
                    let slots: Vec<i16> = w
                        .inventory
                        .slots
                        .iter()
                        .filter(|(_, (k, _))| k == item)
                        .map(|(sl, _)| *sl)
                        .collect();
                    let cell = (
                        look[0].floor() as i32,
                        look[1].floor() as i32,
                        look[2].floor() as i32,
                    );
                    if why == "the refill"
                        && let Some(st) = w.state_at(BlockPos::new(cell.0, cell.1, cell.2))
                    {
                        self.spikes.refill_failed.retain(|(c, _)| *c != cell);
                        self.spikes.refill_failed.push((cell, st));
                    }
                    self.note(
                        tick,
                        format!("spikes: {why} gave up"),
                        format!(
                            "{want:?} held {}, wanted {count}; held {:?}, {item:?} in slots {slots:?}; aimed at {look:?}, the cell {cell:?} last seen {:?}",
                            craft::count(w, *want),
                            crate::dig::held_item(w),
                            w.fluid(cell.0, cell.1, cell.2)
                        ),
                    );
                    return None;
                }
                let (yaw, pitch) = crate::walk::look_at(eye(&body), *look);
                // The landing water is poured from a bucket other than the clutch's (the one in
                // the hotbar's highest water slot), so the clutch's stays in the hotbar
                // (race 45942992471 lane 1: the landing pour took it, and "no climb: no water"
                // cost a pass).
                let step = if why == "the landing water's pour" {
                    pour_hold(w)
                } else {
                    hold::toward(w, *item)
                };
                match step {
                    hold::Hold::Held => {
                        if self.acts.is_empty()
                            && last.is_none_or(|l| tick.saturating_sub(l) >= BUCKET_EVERY)
                        {
                            *last = Some(tick);
                            self.acts.push(Act::UseItem);
                        }
                    }
                    hold::Hold::Act(a) => {
                        if self.acts.is_empty() {
                            self.acts.push(a);
                        }
                    }
                    hold::Hold::Wait => {}
                    hold::Hold::Missing => return None,
                }
                Some(Controls {
                    sneak: true,
                    yaw,
                    pitch: Some(pitch),
                    ..down
                })
            }
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

    /// Are the feet on the first block of this column's pillar?
    fn first_block_up(&self, column: (i32, i32), feet: Cell) -> bool {
        (feet.0, feet.2) == column
            && self
                .spikes
                .pillars
                .iter()
                .any(|p| (p.x, p.z) == column && feet.1 == p.base + 1)
    }

    /// The landing water's cell when it is due now: the feet on the first block up, none poured
    /// for this column, two water buckets (one stays the clutch's), end stone to spare.
    fn landing_due(&self, w: &World, column: (i32, i32), feet: Cell, top: i32) -> Option<Cell> {
        (self.first_block_up(column, feet)
            && craft::count(w, ItemKind::WaterBucket) >= 2
            && !self.spikes.landings.iter().any(|(c, _)| *c == column))
        .then(|| landing_cell(w, column, feet.1 - 1, top))
        .flatten()
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
        // The first block up: the climb hands back for the landing water's pour (spikes_task).
        if body.on_ground && self.landing_due(w, c.column, feet, c.goal).is_some() {
            self.note(
                tick,
                format!("spikes: climb paused for the landing water at {feet:?}"),
                format!("the first block up of the pillar in {:?}", c.column),
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
        // A fireball near, or the strafe within 64: the climb stops, and the planner takes the
        // bot down (spikes_task).
        let strafe = dragon_near(w, body.pos)
            .filter(|d| d.phase == Some(STRAFE_PHASE) && d.dist <= THREAT_NEAR)
            .map(|d| d.dist);
        if let Some(fb) = fireball_near(w, body.pos).or(strafe) {
            self.note(
                tick,
                format!("spikes: climb stopped by a fireball at {feet:?}"),
                format!("a fireball {fb:.0} off"),
            );
            return None;
        }
        // A charge near: the climb stops in the column, held there (spikes_task).
        let d = dragon_near(w, body.pos);
        if let Some(why) = knock_risk(d.as_ref()) {
            self.note(
                tick,
                format!("spikes: climb held from the knock-off at {feet:?}"),
                why,
            );
            return None;
        }
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
        if descend_from(d.as_ref()) && !climb_on_near_stand(d.as_ref(), c.goal - feet.1) {
            self.note(
                tick,
                format!("spikes: climb stopped by the perch at {feet:?}"),
                format!(
                    "the dragon {:?}; the way down by water",
                    d.map(|d| (d.dist, d.phase))
                ),
            );
            return None;
        }
        let refusal = climb_refusal(None, w.player.health, self.spikes_danger())
            .or_else(|| self.spikes_calm_refusal(w, tick, d.as_ref()));
        if let Some(why) = refusal.or_else(|| {
            threatens(d.as_ref()).then(|| "the dragon strafes or charges near".to_owned())
        }) {
            if !climb_on_near_stand(d.as_ref(), c.goal - feet.1) {
                self.note(
                    tick,
                    format!("spikes: climb stopped at {feet:?}"),
                    format!(
                        "{why}; {} under the goal {}; the way down by water",
                        c.goal - feet.1,
                        c.goal
                    ),
                );
                return None;
            }
            self.spikes_note(
                tick,
                "spikes: climb on, the stand near".to_owned(),
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
        let Some(item) = pillar_block_for(w, feet.1, c.goal) else {
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
        // The stand's blocks in end stone (dragon-immune), any other when none is left; a
        // stair's floor in any, the end stone kept.
        let item = match b.role {
            Role::Stair => pillar_block_for(w, i32::MIN, 0),
            Role::Guard | Role::Ledge | Role::Wall | Role::Roof => {
                pillar_item(w).or_else(|| pillar_block_for(w, i32::MIN, 0))
            }
        };
        let Some(item) = item else {
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
