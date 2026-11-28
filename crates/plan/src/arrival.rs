//! Issue 19, the overworld arrival (agent Q7, docs/problems/eyes-arrival.md): from the bot's own
//! overworld portal by the lava sea (feet y -53) up to the open sky, through the cave band that
//! killed 7 of 16 bots in issue 16 (docs/problems/overworld-leg.md wall 1).
//!
//! **The shaft.** The way home's stairs in rock (`Stairs::rock`) climb a block of height for
//! three digs and about 120 ticks in deepslate (race 45727358847: 33 blocks in 12,000 ticks, both
//! lanes). A one-wide shaft straight up climbs a block for one dig and a jump: the block over the
//! head is dug, the bot jumps and puts a block into the cell its feet left at the top of the jump
//! (the pearls hunt's pillar out of a hole, `pearls::out_of_hole`). The shaft is sealed below by
//! its own blocks and walled on every side by rock, so no mob reaches the bot in it; each dig
//! gives the block the next step places.
//!
//! A cell over the head is dug only when it is safe to open (the stairs' own rule, reading the
//! cells beside a cell about to be opened, the no-X-ray exception the stairs and the tunnel use):
//! no fluid in it or beside it, nothing that falls over it (gravel, sand, a dripstone), no open
//! cell beside it or over it (a cave) unless the sky is over it (the surface), and no block a
//! held tool takes longer than `MAX_BREAK` to break. Refused, the shaft ends and the way home's
//! stairs take one leg from there (they turn from caves); the next climb tries a shaft again from
//! the cell the leg ended in. The shaft is over once the open sky is over the head and a side is
//! walkable out of it.
//!
//! **The table.** The climb's pickaxe run (`base::CLIMB_STEPS`) took any table within
//! `TABLE_SEARCH` (64): from y -21 the bot walked back down its own stairs to the portal room's
//! table at y -53 and climbed again (race 45727358847, ticks 4,620 to 6,440 and 10,480). A table
//! farther than `TABLE_REACH` is refused for the run, so the base places the carried one where
//! the bot stands, and it is dug and taken along once the run is over.

use data::{BlockKind, ItemKind};
use physics::Controls;
use world::{BlockPos, World};

use crate::dig::{Dig, DigStatus};
use crate::path::{self, Cell};
use crate::{Plan, Task, craft, hold};

/// Blocks the shaft puts under the feet, the first held. The junk first: a full pack's junk
/// thrown down in a shaft or a stair lands at the feet and comes back 40 ticks later, and the
/// climb's pickaxe run dropped granite and tuff 59 times in 2,300 ticks and never crafted (race
/// 45731721046 lane 0, from tick 13,880); put under the feet it is gone for good. Then what the
/// climb digs (cobbled deepslate under y 0, cobblestone over it) and the trip's filler.
pub const SHAFT_BLOCKS: [ItemKind; 17] = [
    ItemKind::SoulSoil,
    ItemKind::SoulSand,
    ItemKind::WarpedNylium,
    ItemKind::CrimsonNylium,
    ItemKind::WarpedWartBlock,
    ItemKind::SmoothBasalt,
    ItemKind::Granite,
    ItemKind::Tuff,
    ItemKind::Diorite,
    ItemKind::Andesite,
    ItemKind::Calcite,
    ItemKind::Dirt,
    ItemKind::CobbledDeepslate,
    ItemKind::Cobblestone,
    ItemKind::Netherrack,
    ItemKind::Blackstone,
    ItemKind::Basalt,
];

/// Ticks with no rise before a shaft is given up: a dig of deepslate with a wooden pickaxe is
/// 45 ticks (hardness 3, speed 2), a step about 70 with the jump; 900 is over a hand dig of
/// deepslate (`MAX_BREAK_HAND` 600) and a jump.
pub const SHAFT_STALL: u64 = 900;
/// The longest break the shaft waits for (ticks, with the best tool held): deepslate with a
/// wooden pickaxe is 45, ores a stone pickaxe cannot take (diamond, gold) 300 by hand.
pub const MAX_BREAK: u64 = 100;
// >>> Q8 (issue 19): with no pickaxe held the shaft digs by hand: stone 150 ticks, deepslate 300,
// no drop, but the shaft only needs the cells open (the frontier bot of race 45783611857 came
// through with no pickaxe; all six of its lanes stood in the portal).
pub const MAX_BREAK_HAND: u64 = 600;

/// Blocks to pillar on are held (netherrack, cobblestone and the like: `SHAFT_BLOCKS`' filler).
pub fn pillar_blocks(items: &[ItemKind]) -> bool {
    items.iter().any(|k| {
        matches!(
            k,
            ItemKind::Netherrack
                | ItemKind::Cobblestone
                | ItemKind::CobbledDeepslate
                | ItemKind::Blackstone
                | ItemKind::Basalt
        )
    })
}

/// The longest break the shaft waits for with `items` held: `MAX_BREAK`, or by hand
/// `MAX_BREAK_HAND` when no pickaxe is held.
pub fn max_break(items: &[ItemKind]) -> u64 {
    let pickaxe = items.iter().any(|k| {
        matches!(
            k,
            ItemKind::WoodenPickaxe
                | ItemKind::StonePickaxe
                | ItemKind::IronPickaxe
                | ItemKind::GoldenPickaxe
                | ItemKind::DiamondPickaxe
                | ItemKind::NetheritePickaxe
        )
    });
    if pickaxe { MAX_BREAK } else { MAX_BREAK_HAND }
}
// <<< Q8
/// Blocks a table may be from the eye for the climb's pickaxe run: a click's reach (4.5) and a
/// little.
pub const TABLE_REACH: f64 = 5.0;
/// How far the feet may be off the cell's centre on either axis before a jump: a body 0.6 wide
/// at x 124.7 touches the next column, and its jump is stopped by the rock over it (race
/// 45731721046 lane 0: six shafts of seven dug one cell and never rose, the body at 124.7 or
/// -109.5 + 0.2; the one centred at 113.5 rose 8 in 631 ticks).
pub const CENTRE: f64 = 0.12;
/// Steps aside a shaft takes with no rise between before it ends: a refused cell over the head
/// (gravel, a cave beside) is often one column wide; three tries the other sides of the feet.
pub const SIDES_MAX: u32 = 3;
/// Cells along a level tunnel a step aside may go: gravel lies in layers wider than a block (race
/// 45741154601: no cell beside the shaft at (124, -20, -110) was free of it, the stairs given the
/// leg cut under it, 3 health "in_wall" in both lanes).
pub const SIDE_REACH: i32 = 3;
/// Ticks a stack stays on the cursor before the shaft puts it away: a swap to the hotbar holds
/// the item on the cursor between its clicks, and throwing it then undid every swap (race
/// 45737410143: "the cursor's SoulSoil put away" 14 times a shaft, nine shafts that never rose).
/// The planner's own cursor rule waits 100 (`lib.rs`, "cursor put down").
pub const CURSOR_WAIT: u64 = 100;
/// Where the walk out for animals heads: the stronghold's `locate` point (1648, -16), known
/// geography (Bridger's ruling of 2026-11-24; docs/problems/eyes-observe.md), 1,537 east of the
/// portal over plains (y 65 to 81 for the first 320, then a river, forest). Round the portal and
/// the old stairs, where issue 16's bots hunted, no animal was in view in 12 looks (race
/// 45737410143, both lanes, from (112, 65, -73)); fresh land east has them, and the eyes' way is
/// east too.
pub const FOOD_POINT: [f64; 2] = [1648.0, -16.0];
/// A leg's length out for animals (the hunt sees prey within `hunt_radius` 24: a leg of 48 looks
/// over new ground at each end), the shortest leg taken, and the step the target is sought in.
pub const FOOD_LEG: i32 = 48;
pub const FOOD_LEG_MIN: i32 = 24;
pub const FOOD_STEP: i32 = 8;
/// Legs out for animals at most: 24 of 48 is about 1,150 blocks, most of the way east.
pub const FOOD_LEGS: u32 = 24;
/// The bearings tried for a leg, off the one to `FOOD_POINT` (degrees).
const FOOD_TURNS: [f64; 5] = [0.0, 30.0, -30.0, 60.0, -60.0];
/// Bridges a leg may lay (a stream), and the blocks kept: the overland's `bridge_max` and
/// `bridge_keep` ([overland], config/goals.toml).
const FOOD_BRIDGES: usize = 40;
const FOOD_KEEP: i64 = 32;
/// Food at or over which the way home hands over to the eyes on the surface by day, unfed (the
/// coordinator, 2026-11-25: the overland walk hunts on the way, as Q4's dev runs ate on it; 6 is
/// where sprinting stops, vanilla, and `home::COOK_FOOD_FLOOR`). Under it the walk east for
/// animals runs first.
pub const HAND_OVER_FOOD: i32 = 6;
/// Digs of the block over the head (the hut's roof) before the hand-over at most.
pub const ROOF_DIGS: u32 = 2;
/// Cells over a cell checked for the sky: `world::stage::SKY_CHECK`, the open-sky rule's own.
const SKY: i32 = world::stage::SKY_CHECK;

/// The arrival's state, one field of the way home's (`HomeState::arrival`).
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct Arrival {
    /// The shaft in hand, driven a tick at a time while no task runs (`Plan::arrival_step`).
    pub shaft: Option<Shaft>,
    /// The feet's cell where the last shaft was refused or ended short: the stairs take a leg
    /// from it before another shaft.
    pub refused_at: Option<Cell>,
    /// Shafts begun, blocks risen in them, refusals.
    pub shafts: u32,
    pub risen: i32,
    pub refusals: u32,
    /// A far table was refused for the pickaxe run: the one the base places is taken along.
    pub table_out: bool,
    /// Tables taken along after a run.
    pub tables_taken: u32,
    /// Legs walked (or refused) out for animals toward `FOOD_POINT`.
    pub food_legs: u32,
    /// Digs of the block over the head before the hand-over.
    pub roof_digs: u32,
}

/// A shaft straight up.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Shaft {
    /// The feet's cell it began from, the highest feet y, when it began and last rose.
    pub from: Cell,
    pub top: i32,
    pub since: u64,
    pub rose_at: u64,
    /// The dig in hand of the cell over the head, and the cell this shaft opened last.
    pub dig: Option<Dig>,
    #[serde(default)]
    pub opened: Option<Cell>,
    /// The feet's cell a jump left, and whether its block was clicked in.
    #[serde(default)]
    pub jump_from: Option<Cell>,
    #[serde(default)]
    pub clicked: bool,
    #[serde(default)]
    pub dug: u32,
    #[serde(default)]
    pub placed: u32,
    /// A step aside in hand (the feet's cell beside, dug and walked into), the steps aside since
    /// the last rise, and in all.
    #[serde(default)]
    pub side: Option<Cell>,
    #[serde(default)]
    pub sides: u32,
    #[serde(default)]
    pub sides_all: u32,
    /// Since when a stack has been on the cursor.
    #[serde(default)]
    pub cursor_at: Option<u64>,
    // >>> E2 (issue 20c): a shaft that ends once the feet stand at this y (`arrival_rise_to`).
    #[serde(default)]
    pub stop_at: Option<i32>,
    // <<< E2
}

/// The shaft's next step from the feet.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Step {
    /// Dig this cell (the one over the head).
    Dig(Cell),
    /// The cell over the head is open: jump and put a block under the feet.
    Rise,
    /// Out: the sky over the head and a side walkable.
    Out,
    /// Not safe to go on: why.
    Refused(String),
}

fn kind(w: &World, (x, y, z): Cell) -> Option<BlockKind> {
    w.block_at(BlockPos::new(x, y, z))
}

fn open(k: Option<BlockKind>) -> bool {
    matches!(
        k,
        Some(BlockKind::Air | BlockKind::CaveAir | BlockKind::VoidAir)
    )
}

fn full(k: Option<BlockKind>) -> bool {
    k.and_then(data::generated::shapes::collision) == Some(data::types::Collision::Full)
}

fn fluid(k: Option<BlockKind>) -> Option<&'static str> {
    match k {
        Some(
            BlockKind::Water
            | BlockKind::BubbleColumn
            | BlockKind::Kelp
            | BlockKind::KelpPlant
            | BlockKind::Seagrass
            | BlockKind::TallSeagrass,
        ) => Some("water"),
        Some(BlockKind::Lava) => Some("lava"),
        Some(_) | None => None,
    }
}

/// A block that falls when what holds it is dug (or hangs and falls: a dripstone).
fn falls(k: Option<BlockKind>) -> bool {
    matches!(
        k,
        Some(
            BlockKind::Sand
                | BlockKind::RedSand
                | BlockKind::Gravel
                | BlockKind::SuspiciousSand
                | BlockKind::SuspiciousGravel
                | BlockKind::PointedDripstone
        )
    )
}

/// The sky over `c`: `c` and the `SKY` cells over it open (leaves and logs count, as in
/// `world::stage::open_sky`), or over the top of the loaded column.
pub fn sky_over(w: &World, c: Cell) -> bool {
    let tree = |k: BlockKind| {
        ["leaves", "logs"]
            .iter()
            .filter_map(|t| data::generated::tags::block_tag(t))
            .any(|ks| ks.contains(&k))
    };
    (c.1..c.1 + SKY).all(|y| match kind(w, (c.0, y, c.2)) {
        Some(BlockKind::Air | BlockKind::CaveAir) => true,
        Some(k) => tree(k),
        None => y > 319,
    })
}

const SIDES: [(i32, i32); 4] = [(1, 0), (-1, 0), (0, 1), (0, -1)];

/// A side the bot walks out of the shaft by: a floor beside the feet with room over it, or a
/// block beside the feet with room over it to jump onto.
fn walk_out(w: &World, (x, y, z): Cell) -> bool {
    SIDES.iter().any(|(dx, dz)| {
        let at = |dy: i32| kind(w, (x + dx, y + dy, z + dz));
        (open(at(0)) && open(at(1)) && full(at(-1))) || (full(at(0)) && open(at(1)) && open(at(2)))
    })
}

/// The shaft's next step from `feet`, with `opened` the cell this shaft dug last and `items` what
/// is held (for the break time). Pure: a hand-built world gives its answer.
pub fn step(w: &World, feet: Cell, opened: Option<Cell>, items: &[ItemKind]) -> Step {
    let (x, y, z) = feet;
    let over = (x, y + 2, z);
    let above = (x, y + 3, z);
    if open(kind(w, over)) && sky_over(w, over) && walk_out(w, feet) {
        return Step::Out;
    }
    let k = kind(w, over);
    if open(k) {
        if opened == Some(over) || sky_over(w, over) {
            return Step::Rise;
        }
        // >>> Q8: a room over the head with a ceiling within 8 is pillared through on held blocks
        // (the frontier: the overworld portal's south room is open from y -53 to -49).
        if pillar_blocks(items) && (3..=8).any(|d| full(kind(w, (x, y + d, z)))) {
            return Step::Rise;
        }
        // <<< Q8
        return Step::Refused(format!("cave over the head at {over:?}"));
    }
    let Some(block) = k else {
        return Step::Refused(format!("{over:?} not loaded"));
    };
    if let Some(f) = fluid(k) {
        return Step::Refused(format!("{f} at {over:?}"));
    }
    if falls(k) || falls(kind(w, above)) {
        return Step::Refused(format!("a falling block at {over:?} or over it"));
    }
    let round = SIDES
        .iter()
        .map(|(dx, dz)| (x + dx, y + 2, z + dz))
        .chain(std::iter::once(above));
    let near: Vec<Cell> = round.collect();
    if let Some((c, f)) = near
        .iter()
        .find_map(|c| fluid(kind(w, *c)).map(|f| (*c, f)))
    {
        return Step::Refused(format!("{f} beside {over:?} at {c:?}"));
    }
    // >>> Q8: past a cave beside the shaft (not in its own column) with blocks held (the frontier
    // bot's first shaft was refused for a cave beside, the stairs then dug obsidian).
    if !sky_over(w, above)
        && !pillar_blocks(items)
        && let Some(c) = near.iter().find(|c| open(kind(w, **c)))
    {
        return Step::Refused(format!("cave beside {over:?} at {c:?}"));
    }
    let tool = data::dig::best_tool(block, items.iter().copied());
    match data::dig::break_ticks(block, tool, false, true) {
        Some(t) if t <= max_break(items) => Step::Dig(over),
        Some(t) => Step::Refused(format!("{block:?} at {over:?} takes {t} ticks")),
        None => Step::Refused(format!("{block:?} at {over:?} does not break")),
    }
}

/// A cell beside the feet to step aside into when the shaft is refused here (race 45734424933:
/// gravel over the shaft at (124, -18, -110); the stairs given the leg cut under it and the
/// gravel fell on the head, 3 health "in_wall"): on a full floor, its two cells rock or open and
/// neither a falling block nor under one, no fluid in or beside them, no open cell beside them
/// that is not the feet's own column, each quick to break, and a shaft allowed from it.
pub fn side_cell(w: &World, feet: Cell, items: &[ItemKind]) -> Option<Cell> {
    let (x, y, z) = feet;
    let fine = |cell: Cell, back: (i32, i32)| {
        let k = kind(w, cell);
        if falls(k) || fluid(k).is_some() {
            return false;
        }
        let quick = k.is_some_and(|b| {
            open(k)
                || data::dig::break_ticks(
                    b,
                    data::dig::best_tool(b, items.iter().copied()),
                    false,
                    true,
                )
                .is_some_and(|t| t <= max_break(items))
        });
        let round_ok = SIDES.iter().all(|(ex, ez)| {
            let n = (cell.0 + ex, cell.1, cell.2 + ez);
            let nk = kind(w, n);
            fluid(nk).is_none() && ((n.0, n.2) == back || !open(nk))
        });
        quick && round_ok
    };
    // Nearest first: a cell beside the feet on every side, then two along, then three.
    (1..=SIDE_REACH).find_map(|reach| {
        SIDES.iter().find_map(|(dx, dz)| {
            let along = (1..=reach).all(|i| {
                let c = (x + dx * i, y, z + dz * i);
                let back = (x + dx * (i - 1), z + dz * (i - 1));
                full(kind(w, (c.0, y - 1, c.2)))
                    && !falls(kind(w, (c.0, y + 2, c.2)))
                    && fluid(kind(w, (c.0, y + 2, c.2))).is_none()
                    && fine(c, back)
                    && fine((c.0, y + 1, c.2), back)
            });
            let c = (x + dx * reach, y, z + dz * reach);
            (along && matches!(step(w, c, None, items), Step::Dig(_))).then_some(c)
        })
    })
}

// >>> E4 (issue 20f): the rise's two halves, shared by the shaft and the spike pillar
// (`spikes`), lifted out of `shaft_tick` unchanged.
/// In the air on a rise's jump from `j`: at its top (the feet a block over `j`), the block into
/// `j` against the top of the one under it, once (`clicked`), with no other act this tick and no
/// click pending.
pub(crate) fn rise_click(
    w: &World,
    body: &physics::Body,
    j: Cell,
    clicked: &mut bool,
    acts: &mut Vec<world::Act>,
) {
    if !*clicked
        && body.pos[1] >= f64::from(j.1) + 1.0
        && open(kind(w, j))
        && acts.is_empty()
        && w.click.is_none()
    {
        *clicked = true;
        acts.push(world::Act::UseOn {
            pos: BlockPos::new(j.0, j.1 - 1, j.2),
            face: 1,
        });
    }
}

/// On the ground at `feet`, a rise's next step: to the cell's centre first, slowly (sneaking
/// keeps the feet off any edge), then `item` to hand, then the jump (`jump_from` kept). `None`
/// when the item is not in the inventory.
pub(crate) fn rise_jump(
    w: &World,
    body: &physics::Body,
    feet: Cell,
    item: ItemKind,
    down: Controls,
    jump_from: &mut Option<Cell>,
    acts: &mut Vec<world::Act>,
) -> Option<Controls> {
    let centre = [f64::from(feet.0) + 0.5, f64::from(feet.2) + 0.5];
    let (dx, dz) = (centre[0] - body.pos[0], centre[1] - body.pos[2]);
    if dx.abs() > CENTRE || dz.abs() > CENTRE {
        return Some(Controls {
            forward: true,
            sneak: true,
            yaw: (-dx).atan2(dz).to_degrees(),
            ..down
        });
    }
    match hold::toward(w, item) {
        hold::Hold::Held => {
            *jump_from = Some(feet);
            Some(Controls { jump: true, ..down })
        }
        hold::Hold::Act(a) => {
            if acts.is_empty() {
                acts.push(a);
            }
            Some(down)
        }
        hold::Hold::Wait => Some(down),
        hold::Hold::Missing => None,
    }
}
// <<< E4

fn items(w: &World) -> Vec<ItemKind> {
    w.inventory
        .slots
        .values()
        .chain(w.inventory.carried.iter())
        .map(|(k, _)| *k)
        .collect()
}

impl Plan {
    // >>> Q7: the climb's own entry, called from the way home's climb (`home_climb`).
    /// A shaft up from the feet when one is safe here (noted), else `None` and the stairs take a
    /// leg (refused here, noted once). `Some(Task::Idle)` while a shaft runs: `arrival_step`
    /// drives it.
    pub(crate) fn arrival_climb(&mut self, w: &World, tick: u64) -> Option<Task> {
        self.arrival_shaft(w, tick, None)
    }

    // >>> E2 (issue 20c, docs/problems/end-portal.md): the same shaft, ended once the feet stand
    // at `top` (the End kit's way up: a drop of 4 in its own way from the portal room).
    pub(crate) fn arrival_rise_to(&mut self, w: &World, tick: u64, top: i32) -> Option<Task> {
        self.arrival_shaft(w, tick, Some(top))
    }
    // <<< E2

    fn arrival_shaft(&mut self, w: &World, tick: u64, stop_at: Option<i32>) -> Option<Task> {
        let body = w.body?;
        let feet = path::feet_cell(body.pos);
        if self.ender.home.arrival.shaft.is_some() {
            self.retry_at = tick + 1;
            return Some(Task::Idle);
        }
        // The run's table taken along before the next shaft (race 45734424933: the shaft began
        // the tick the run ended, and the table was left).
        if let Some(t) = self.arrival_table(w, tick, false) {
            return Some(t);
        }
        if self.ender.home.arrival.refused_at == Some(feet) || !body.on_ground {
            return None;
        }
        if SHAFT_BLOCKS.iter().all(|k| craft::count(w, *k) == 0) {
            return None;
        }
        match step(w, feet, None, &items(w)) {
            Step::Refused(why) => {
                self.ender.home.arrival.refused_at = Some(feet);
                self.ender.home.arrival.refusals += 1;
                self.note(
                    tick,
                    format!("arrival: no shaft from {feet:?}"),
                    format!("{why}; the stairs take a leg"),
                );
                None
            }
            Step::Out => None,
            Step::Dig(_) | Step::Rise => {
                self.ender.home.arrival.shafts += 1;
                self.ender.home.arrival.shaft = Some(Shaft {
                    from: feet,
                    top: feet.1,
                    since: tick,
                    rose_at: tick,
                    dig: None,
                    opened: None,
                    jump_from: None,
                    clicked: false,
                    dug: 0,
                    placed: 0,
                    side: None,
                    sides: 0,
                    sides_all: 0,
                    cursor_at: None,
                    stop_at,
                });
                self.note(
                    tick,
                    format!("arrival: shaft up from {feet:?}"),
                    format!(
                        "shaft {}; {} risen in shafts so far; health {:?}, food {:?}",
                        self.ender.home.arrival.shafts,
                        self.ender.home.arrival.risen,
                        w.player.health,
                        w.player.food
                    ),
                );
                self.retry_at = tick + 1;
                Some(Task::Idle)
            }
        }
    }

    /// The climb's pickaxe run: tables past `TABLE_REACH` are refused for it (noted), so the
    /// base places the carried table where the bot stands; once the run is over, that table is
    /// dug and taken along. `Some` with the dig.
    pub(crate) fn arrival_table(&mut self, w: &World, tick: u64, running: bool) -> Option<Task> {
        let body = w.body?;
        let eye = [body.pos[0], body.pos[1] + physics::EYE_HEIGHT, body.pos[2]];
        let d = |c: Cell| {
            let p = [
                f64::from(c.0) + 0.5,
                f64::from(c.1) + 0.5,
                f64::from(c.2) + 0.5,
            ];
            (0..3).map(|i| (eye[i] - p[i]).powi(2)).sum::<f64>().sqrt()
        };
        if running {
            for _ in 0..4 {
                let Some(c) = self.near_block(w, BlockKind::CraftingTable, tick) else {
                    break;
                };
                if d(c) <= TABLE_REACH {
                    break;
                }
                self.refuse(c, tick);
                self.ender.home.arrival.table_out = true;
                self.note(
                    tick,
                    format!("arrival: the table at {c:?} is far"),
                    format!(
                        "{:.0} from the eye, over {TABLE_REACH}: the pickaxes are made here ({} tables held)",
                        d(c),
                        craft::count(w, ItemKind::CraftingTable)
                    ),
                );
            }
            return None;
        }
        if !self.ender.home.arrival.table_out || craft::count(w, ItemKind::CraftingTable) > 0 {
            self.ender.home.arrival.table_out = false;
            return None;
        }
        self.ender.home.arrival.table_out = false;
        // >>> Q8: never the floor under the body (F3, race 45800967549: the table placed under the
        // shaft's cell and dug back left the shaft over a hole; the centring, sneaking, could not
        // step over it, no rise for 900 ticks in four lanes).
        let feet = crate::path::stand_cell(w, body.pos);
        let under_body =
            |c: Cell| c.1 == feet.1 - 1 && (c.0 - feet.0).abs() <= 1 && (c.2 - feet.2).abs() <= 1;
        // <<< Q8
        let c = self
            .near_block(w, BlockKind::CraftingTable, tick)
            .filter(|c| d(*c) <= TABLE_REACH && !under_body(*c))?;
        self.ender.home.arrival.tables_taken += 1;
        self.note(
            tick,
            format!("arrival: the table at {c:?} taken along"),
            format!(
                "the climb's pickaxes made; {} taken",
                self.ender.home.arrival.tables_taken
            ),
        );
        Some(Task::Dig(Dig::new(w, c, tick)))
    }

    /// On the surface by day, hungry, no prey in view: a far walk of `FOOD_LEG` toward
    /// `FOOD_POINT` (a bearing off it when that has no leg), noted; `None` after `FOOD_LEGS`.
    pub(crate) fn arrival_food_walk(&mut self, w: &World, tick: u64) -> Option<Task> {
        if self.ender.home.arrival.food_legs >= FOOD_LEGS
            || w.player.food.unwrap_or(0) >= HAND_OVER_FOOD
        {
            return None;
        }
        let body = w.body?;
        let feet = path::feet_cell(body.pos);
        self.ender.home.arrival.food_legs += 1;
        let n = self.ender.home.arrival.food_legs;
        let from = [body.pos[0], body.pos[2]];
        let deg = crate::overland::bearing(from, FOOD_POINT);
        for turn in FOOD_TURNS {
            let Some(target) =
                crate::overland::leg_target(w, feet, deg + turn, FOOD_LEG, FOOD_LEG_MIN, FOOD_STEP)
            else {
                continue;
            };
            let goal = path::Goal::Near {
                pos: target,
                range: 4.0,
            };
            let mut walk = crate::walk::Walk::far_to(target, goal, tick);
            walk.more.keep = Some(FOOD_KEEP);
            walk.more.water = true;
            let ok = walk.route_now(w, tick);
            self.expanded += walk.expanded;
            if !ok || walk.bridges_ahead() > FOOD_BRIDGES {
                continue;
            }
            self.walks += 1;
            self.note(
                tick,
                format!("arrival: a walk for animals to {target:?}"),
                format!(
                    "leg {n} of {FOOD_LEGS}, bearing {:.0} (turned {turn:.0}) toward {FOOD_POINT:?}; food {:?}, health {:?}",
                    deg + turn,
                    w.player.food,
                    w.player.health
                ),
            );
            return Some(Task::Explore(walk));
        }
        self.note(
            tick,
            "arrival: no walk for animals".to_owned(),
            format!("leg {n} of {FOOD_LEGS}: no leg on any bearing from {feet:?}"),
        );
        None
    }

    /// On the surface by day (not night, dusk more than `DUSK_MARGIN` off) with food at
    /// `HAND_OVER_FOOD` or more: the way home is over and the eyes take over at once (throw 1,
    /// then the overland legs, which hunt on the way). Noted; `true` when it hands over.
    pub(crate) fn arrival_hand_over(&mut self, w: &World, tick: u64) -> Option<Option<Task>> {
        let food = w.player.food.unwrap_or(0);
        let day = w.is_night() == Some(false)
            && w.day_time()
                .is_some_and(|t| world::time::ticks_to_dusk(t) >= crate::DUSK_MARGIN);
        if self.ender.home.done || !day || food < HAND_OVER_FOOD {
            return None;
        }
        let feet = w.body.map(|b| path::feet_cell(b.pos));
        // The night's hut roofs the head: the first throw wants the open sky (race 45741154601:
        // "eye throw 1 (first) waits (no open sky over the feet)" 11 times to the cap, in the
        // hut, both lanes). The block over the head is dug first, twice at most.
        if let Some(f) = feet
            && !world::stage::open_sky(w)
            && self.ender.home.arrival.roof_digs < ROOF_DIGS
        {
            let roof = (f.0, f.1 + 2, f.2);
            if !open(kind(w, roof)) {
                self.ender.home.arrival.roof_digs += 1;
                self.note(
                    tick,
                    format!("arrival: the roof over the head opened at {roof:?}"),
                    format!(
                        "{:?} over the head: the first throw wants the open sky; dig {} of {ROOF_DIGS}",
                        kind(w, roof),
                        self.ender.home.arrival.roof_digs
                    ),
                );
                return Some(Some(Task::Dig(Dig::new(w, roof, tick))));
            }
        }
        self.ender.home.done = true;
        self.note(
            tick,
            "home: over on the surface, the eyes take over".to_owned(),
            format!(
                "at {feet:?} by day (day time {:?}), food {food} of {HAND_OVER_FOOD} or more, health {:?}; {} ticks since the arrival; the overland walk hunts on the way",
                w.day_time(),
                w.player.health,
                tick.saturating_sub(self.ender.home.arrived_at.unwrap_or(tick))
            ),
        );
        Some(None)
    }

    /// One tick of the shaft in hand, while no task runs (the planner's step calls it ahead of
    /// the choice). `None` when no shaft runs or it just ended (noted).
    pub(crate) fn arrival_step(&mut self, w: &World, tick: u64) -> Option<Controls> {
        if !matches!(self.task, Task::Idle) {
            return None;
        }
        let mut s = self.ender.home.arrival.shaft.take()?;
        let c = self.shaft_tick(w, tick, &mut s);
        if c.is_some() {
            self.ender.home.arrival.shaft = Some(s);
        } else {
            self.retry_at = tick;
        }
        c
    }

    fn shaft_end(&mut self, tick: u64, s: &Shaft, feet: Cell, why: &str, short: bool) {
        if short {
            self.ender.home.arrival.refused_at = Some(feet);
            self.ender.home.arrival.refusals += 1;
        }
        let risen = s.top - s.from.1;
        self.note(
            tick,
            format!("arrival: shaft over at {feet:?}"),
            format!(
                "{why}; risen {risen} in {} ticks ({} dug, {} placed, {} steps aside); {} risen in {} shafts",
                tick.saturating_sub(s.since),
                s.dug,
                s.placed,
                s.sides_all,
                self.ender.home.arrival.risen,
                self.ender.home.arrival.shafts
            ),
        );
    }

    fn shaft_tick(&mut self, w: &World, tick: u64, s: &mut Shaft) -> Option<Controls> {
        let body = w.body?;
        let feet = path::feet_cell(body.pos);
        if w.dimension.as_ref().map(|d| d.name.as_str()) != Some(crate::home::OVERWORLD) {
            return None;
        }
        // The stall first, whatever is in hand: a dig waiting on a held stack never ends itself
        // (race 45734424933: a crafted pickaxe on the cursor, a full pack, 12,800 ticks still).
        if tick.saturating_sub(s.rose_at) > SHAFT_STALL {
            let why = format!(
                "no rise for {SHAFT_STALL} ticks (body at {:.2}, {:.2}, {:.2}, on the ground {}, over the head {:?}, on the cursor {:?})",
                body.pos[0],
                body.pos[1],
                body.pos[2],
                body.on_ground,
                kind(w, (feet.0, feet.1 + 2, feet.2)),
                w.inventory.carried
            );
            self.shaft_end(tick, s, feet, &why, true);
            return None;
        }
        let down = Controls {
            yaw: w.player.yaw.into(),
            pitch: Some(crate::endermen::EYES_DOWN),
            ..Controls::default()
        };
        // A stack left on the cursor `CURSOR_WAIT` ticks: into a free slot, else swapped into a
        // junk block's slot and the junk thrown (it is put under the feet when it comes back).
        let since = match w.inventory.carried {
            Some(_) => *s.cursor_at.get_or_insert(tick),
            None => {
                s.cursor_at = None;
                tick
            }
        };
        if let Some((k, _)) = w.inventory.carried
            && tick.saturating_sub(since) >= CURSOR_WAIT
            && w.window.is_none()
            && w.click.is_none()
        {
            if self.acts.is_empty() {
                s.cursor_at = Some(tick);
                let junk = |j: ItemKind| SHAFT_BLOCKS[..12].contains(&j);
                let act = if junk(k) {
                    Some(world::Act::Click {
                        slot: -999,
                        button: 0,
                        mode: 0,
                    })
                } else if let Some(slot) = hold::put_down_slot(w) {
                    Some(world::Act::Click {
                        slot,
                        button: 0,
                        mode: 0,
                    })
                } else {
                    w.inventory
                        .slots
                        .iter()
                        .find(|(slot, (j, _))| (9..36).contains(*slot) && junk(*j))
                        .map(|(slot, _)| world::Act::Click {
                            slot: *slot,
                            button: 0,
                            mode: 0,
                        })
                };
                if let Some(a) = act {
                    self.note(
                        tick,
                        format!("arrival: the cursor's {k:?} put away"),
                        format!("{a:?}"),
                    );
                    self.acts.push(a);
                }
            }
            return Some(down);
        }
        if let Some(d) = s.dig.as_mut() {
            let (c, act, status) = d.step(w, tick);
            self.acts.extend(act);
            match status {
                DigStatus::Running => return Some(c),
                DigStatus::Done => {
                    s.opened = Some(d.target);
                    s.dig = None;
                    s.dug += 1;
                }
                // The cell open: the dig did its work, whatever became of the drop (a full pack:
                // "failed Uncollected" ended three shafts in race 45731721046).
                DigStatus::Failed(_) if open(kind(w, d.target)) => {
                    s.opened = Some(d.target);
                    s.dig = None;
                    s.dug += 1;
                }
                DigStatus::Failed(f) => {
                    let why = format!("the dig of {:?} failed {f:?}", d.target);
                    self.shaft_end(tick, s, feet, &why, true);
                    return None;
                }
            }
        }
        if body.on_ground && feet.1 > s.top {
            self.ender.home.arrival.risen += feet.1 - s.top;
            s.top = feet.1;
            s.rose_at = tick;
            s.sides = 0;
            if s.jump_from.take().is_some() && s.clicked {
                s.placed += 1;
            }
            s.clicked = false;
        }
        // >>> E2: risen to its stop.
        if let Some(top) = s.stop_at
            && body.on_ground
            && feet.1 >= top
        {
            self.shaft_end(tick, s, feet, "risen to its stop", false);
            return None;
        }
        // <<< E2
        // In the air on a jump: at its top, the block into the cell the feet left.
        if let Some(j) = s.jump_from
            && !body.on_ground
        {
            rise_click(w, &body, j, &mut s.clicked, &mut self.acts);
            return Some(down);
        }
        if !body.on_ground {
            return Some(down);
        }
        // Back on the ground where the jump began: the block did not go in; jump again.
        s.jump_from = None;
        s.clicked = false;
        // A step aside in hand, a cell at a time along its line: the next cell's head dug, then
        // its feet, then walked into.
        if let Some(c) = s.side {
            let on_line = feet.1 == c.1 && (feet.0 == c.0 || feet.2 == c.2);
            if feet == c {
                s.side = None;
            } else if !on_line {
                let why = format!("off the step aside to {c:?}");
                self.shaft_end(tick, s, feet, &why, true);
                return None;
            } else {
                let next = (
                    feet.0 + (c.0 - feet.0).signum(),
                    feet.1,
                    feet.2 + (c.2 - feet.2).signum(),
                );
                for cell in [(next.0, next.1 + 1, next.2), next] {
                    if !open(kind(w, cell)) {
                        let mut d = Dig::new(w, cell, tick);
                        let (ctl, act, _) = d.step(w, tick);
                        self.acts.extend(act);
                        s.dig = Some(d);
                        return Some(ctl);
                    }
                }
                let (dx, dz) = (
                    f64::from(next.0) + 0.5 - body.pos[0],
                    f64::from(next.2) + 0.5 - body.pos[2],
                );
                return Some(Controls {
                    forward: true,
                    yaw: (-dx).atan2(dz).to_degrees(),
                    ..down
                });
            }
        }
        match step(w, feet, s.opened, &items(w)) {
            Step::Out => {
                self.shaft_end(tick, s, feet, "out: the sky over the head", false);
                None
            }
            Step::Refused(why) => {
                if s.sides < SIDES_MAX
                    && let Some(c) = side_cell(w, feet, &items(w))
                {
                    s.side = Some(c);
                    s.sides += 1;
                    s.sides_all += 1;
                    s.rose_at = tick;
                    self.note(
                        tick,
                        format!("arrival: shaft steps aside to {c:?}"),
                        format!("{why}; step aside {} of {SIDES_MAX}", s.sides),
                    );
                    return Some(down);
                }
                self.shaft_end(tick, s, feet, &why, true);
                None
            }
            Step::Dig(c) => {
                // The climb's pickaxes first, when the way home's run would make them.
                let r = self
                    .table
                    .as_ref()
                    .map_or_else(crate::home::HomeRules::default, |t| t.home.clone());
                let low = crate::base::reserve_pick_uses(w) < r.climb_pick_uses
                    && craft::count(w, ItemKind::Cobblestone)
                        + craft::count(w, ItemKind::CobbledDeepslate)
                        >= 3
                    && self.ender.home.pick_runs < r.pick_runs
                    // >>> E2: a shaft with a stop (the End kit's) digs by hand: no table or wood
                    // under the room makes a pickaxe (race 45870075221: "the pickaxes first"
                    // ended both pillars at once).
                    && s.stop_at.is_none();
                // <<< E2
                if low {
                    self.shaft_end(tick, s, feet, "the pickaxes first", false);
                    return None;
                }
                let mut d = Dig::new(w, c, tick);
                let (ctl, act, _) = d.step(w, tick);
                self.acts.extend(act);
                s.dig = Some(d);
                Some(ctl)
            }
            Step::Rise => {
                let Some(item) = SHAFT_BLOCKS
                    .iter()
                    .copied()
                    .find(|k| craft::count(w, *k) > 0)
                else {
                    self.shaft_end(tick, s, feet, "no block held to put under the feet", true);
                    return None;
                };
                match rise_jump(w, &body, feet, item, down, &mut s.jump_from, &mut self.acts) {
                    Some(c) => Some(c),
                    None => {
                        self.shaft_end(tick, s, feet, "the block went missing", true);
                        None
                    }
                }
            }
        }
    }
    // <<< Q7
}
