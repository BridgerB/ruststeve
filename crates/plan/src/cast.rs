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
/// Failed tries kept for the note.
pub const OP_TRAIL: usize = 6;
/// Ticks with no op done before the cast gives its site up (the progress invariant).
pub const STALL_TICKS: u64 = 2400;
/// Eye to a block's centre a dig or a place is worked from (the game's 4.5 less a margin).
pub const REACH: f64 = 4.0;
/// Failed walks or digs toward the station before a cast refills from the nearest sources.
const STATION_MISSES: u32 = 2;
/// Blocks a way toward one op's stand may dig: the room is about ten wide, so a way longer than
/// that is not the room's.
const WAY_DIGS: u32 = 12;
/// Refill steps kept for the failure note.
const REFILL_TRAIL: usize = 8;
/// Blocks a cast may dig to open sight lines to lava with no station: a few per refill.
const SIGHT_DIGS: u32 = 24;
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

/// The items a cast's floors and walls are laid from, in the order taken (clojurecraft's
/// `filler/fillers` at be85a92): cobblestone, the stones that drop themselves, dirt, then the
/// Nether's rock. Never gravel or sand, which fall out of a floor. Not ported yet: its
/// `stone-kept`, the stone held back for a spare pickaxe.
pub const FILLERS: [ItemKind; 11] = [
    ItemKind::Cobblestone,
    ItemKind::CobbledDeepslate,
    ItemKind::Andesite,
    ItemKind::Diorite,
    ItemKind::Granite,
    ItemKind::Tuff,
    ItemKind::Dirt,
    ItemKind::Netherrack,
    ItemKind::SoulSoil,
    ItemKind::Blackstone,
    ItemKind::Basalt,
];

/// The first filler held, or none.
pub fn filler(w: &World) -> Option<ItemKind> {
    FILLERS.into_iter().find(|i| crate::craft::count(w, *i) > 0)
}

/// Whether the body stands on cell `c`: the feet one over it and the box over its column.
pub fn standing_on(body: &physics::Body, c: Cell) -> bool {
    feet(body).1 == c.1 + 1 && overlaps(body, (c.0, c.1 + 1, c.2))
}

/// Whether the player's box overlaps cell `c` (strictly: a box touching a face does not).
pub fn overlaps(body: &physics::Body, (x, y, z): Cell) -> bool {
    let b = physics::aabb(body.pos);
    let lo = [f64::from(x), f64::from(y), f64::from(z)];
    (0..3).all(|i| b[i] < lo[i] + 1.0 && b[i + 3] > lo[i])
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
/// The highest first stand over the room's floor a site is preferred at (wall 49): the failing
/// `dig :landing` casts read had h 3 and 4.
pub const LOW_SITE: i32 = 2;
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
    /// A walk to the lava station's stand, or to the landing at its height (wall 18).
    ToStation(Walk),
    /// A dig on the way to the station's stand, or opening a source's sight from it (wall 18).
    WayDig(Box<Dig>),
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
    /// Times the first op not done moved back to an earlier one (a later op undid it), and the
    /// last op that undid one with the op it undid, for the notes (wall 17).
    #[serde(default)]
    pub regressions: u32,
    /// Walks and digs toward the station that failed this cast; at `STATION_MISSES` the refill
    /// takes the nearest sources instead (a stand a step up the walk never reached, wall 18).
    #[serde(default)]
    station_misses: u32,
    /// Blocks dug on straight ways toward the op's stand (reset with the op).
    #[serde(default)]
    way_digs: u32,
    /// The refill's last steps and why each ended, for the failure note (the recording holds no
    /// planner state, and a refill that failed in 230 ticks left only a held bucket to read).
    #[serde(default)]
    refill_trail: Vec<String>,
    /// The refills' bucket uses, summed (`Scoop::uses`).
    #[serde(default)]
    scoop_uses: [u32; 4],
    /// Blocks dug to open a refill's sight to lava with no station (wall 24).
    #[serde(default)]
    sight_digs: u32,
    #[serde(default)]
    pub last_regression: Option<(usize, usize)>,
    /// Tunnels cut toward the pool for the refill in hand.
    #[serde(default)]
    tunnels: u32,
    /// Sources a refill's scoop failed at, never chosen again by this cast (cast.md wall 1: the
    /// refill chose (-7, -55, -233) six times running, each "routed" then `NoSight`, and the site
    /// went; the next source tried filled in 160 ticks).
    #[serde(default)]
    refused: Vec<Cell>,
    /// Ticks an op was worked from a place in reach of its cell's box and not of its centre
    /// (cast.md wall 3), for the notes.
    #[serde(default)]
    box_reach: u32,
    /// The feet's height a refill's stairs down stop at: one over the source they head for.
    #[serde(default)]
    tunnel_floor: Option<i32>,
    /// Why each try at the op in hand failed, for the failure note (overworld-leg wall 5: a cast
    /// ended "Op after 13 ops, at :dig :room" with nothing to say what the dig met).
    #[serde(default)]
    op_trail: Vec<String>,
}

fn eye(b: &physics::Body) -> [f64; 3] {
    [b.pos[0], b.pos[1] + physics::EYE_HEIGHT, b.pos[2]]
}

fn centre((x, y, z): Cell) -> [f64; 3] {
    [f64::from(x) + 0.5, f64::from(y) + 0.5, f64::from(z) + 0.5]
}

/// The eye's distance to the nearest point of cell `c`'s box (0 inside it).
pub fn box_dist(e: [f64; 3], (x, y, z): Cell) -> f64 {
    let lo = [f64::from(x), f64::from(y), f64::from(z)];
    (0..3)
        .map(|i| (lo[i] - e[i]).max(e[i] - (lo[i] + 1.0)).max(0.0).powi(2))
        .sum::<f64>()
        .sqrt()
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
/// How far round the frame's origin a cast looks for its own poured water: the frame and room
/// lie within six of it.
const OWN_WATER: i32 = 7;

/// Water sources seen within `OWN_WATER` of the frame's origin, nearest the origin first: water
/// the cast poured and did not get back (wall 28). Strict no X-ray: a cell is read only when it
/// has a see-through neighbour.
pub fn own_water(w: &World, frame: Frame) -> Vec<Cell> {
    let o = frame.origin;
    let mut out: Vec<(i32, Cell)> = Vec::new();
    for dx in -OWN_WATER..=OWN_WATER {
        for dy in -OWN_WATER..=OWN_WATER {
            for dz in -OWN_WATER..=OWN_WATER {
                let c = (o.0 + dx, o.1 + dy, o.2 + dz);
                let source = w
                    .fluid(c.0, c.1, c.2)
                    .is_some_and(|f| f.kind == physics::FluidKind::Water && f.level == 0);
                if source && seen(w, c).is_some() {
                    out.push((dx * dx + dy * dy + dz * dz, c));
                }
            }
        }
    }
    out.sort();
    out.into_iter().map(|(_, c)| c).collect()
}

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
            op_trail: Vec::new(),
            last_done: tick,
            done_ops: 0,
            regressions: 0,
            station_misses: 0,
            way_digs: 0,
            refill_trail: Vec::new(),
            scoop_uses: [0; 4],
            sight_digs: 0,
            last_regression: None,
            tunnels: 0,
            refused: Vec::new(),
            box_reach: 0,
            tunnel_floor: None,
        }
    }

    fn trail(&mut self, tick: u64, what: String) {
        self.refill_trail.push(format!("{tick} {what}"));
        if self.refill_trail.len() > REFILL_TRAIL {
            self.refill_trail.remove(0);
        }
    }

    fn count_uses(&mut self, uses: [u32; 4]) {
        for (mine, n) in self.scoop_uses.iter_mut().zip(uses) {
            *mine += n;
        }
    }

    /// The cast's refill bucket uses by outcome and by what the exact sight line said, for the
    /// notes (`Scoop::uses`): a race's own count of where the exact line and the server differ.
    /// Why the tries at the op in hand failed, oldest first.
    /// Sources this cast's refills refused after a failed scoop.
    pub fn refused_sources(&self) -> &[Cell] {
        &self.refused
    }

    /// The refused sources, for the notes: the count is the fix's firings (cast.md wall 1).
    pub fn refused_text(&self) -> String {
        format!(
            "refused sources {}, box reach {}",
            self.refused.len(),
            self.box_reach
        )
    }

    pub fn tries_text(&self) -> String {
        if self.op_trail.is_empty() {
            "no failed try".to_owned()
        } else {
            format!("tries: {}", self.op_trail.join(" / "))
        }
    }

    pub fn uses_text(&self) -> String {
        let [fe, fb, ee, eb] = self.scoop_uses;
        format!(
            "uses filled {fe} exact-clear {fb} exact-blocked, empty {ee} exact-clear {eb} exact-blocked"
        )
    }

    /// The refill's last steps, for the failure note.
    pub fn refill_text(&self) -> String {
        if self.refill_trail.is_empty() {
            "no refill".to_owned()
        } else {
            format!("refill: {}", self.refill_trail.join(" / "))
        }
    }

    /// How the cast went back, for the notes: the count and the last op that undid an earlier one.
    pub fn regression_text(&self) -> String {
        match self.last_regression {
            Some((was, to)) => format!(
                "{} regressions, last from {} back to {}",
                self.regressions,
                self.plan
                    .get(was)
                    .map(crate::build::op_text)
                    .unwrap_or_default(),
                self.plan
                    .get(to)
                    .map(crate::build::op_text)
                    .unwrap_or_default()
            ),
            None => "no regressions".to_owned(),
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
            if let Some(was) = self.op {
                if i < was {
                    self.regressions += 1;
                    self.last_regression = Some((was, i));
                } else {
                    self.done_ops += 1;
                    self.last_done = tick;
                }
            }
            self.op = Some(i);
            self.work = Work::Idle;
            self.tries = 0;
            self.op_trail.clear();
            self.way_digs = 0;
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
    fn try_again(&mut self, why: String) -> bool {
        self.op_trail.push(why);
        if self.op_trail.len() > OP_TRAIL {
            self.op_trail.remove(0);
        }
        self.tries += 1;
        self.work = Work::Idle;
        self.tries <= OP_TRIES
    }

    /// The first block of the straight way at the feet's height toward `to`'s column, when a way
    /// may dig it and it is in reach; `None` past `WAY_DIGS` digs for one op.
    fn way_block(&self, w: &World, body: &physics::Body, to: Cell) -> Option<Cell> {
        if self.way_digs >= WAY_DIGS {
            return None;
        }
        let f = feet(body);
        let mut kept = crate::station::kept(self.site.frame);
        kept.extend(
            self.plan
                .iter()
                .filter(|op| op.kind == OpKind::Place)
                .filter_map(|op| op.pos),
        );
        let level = (to.0, f.1, to.2);
        let crate::station::Way::Dig(block) = crate::station::stance_block(w, f, level, &kept)
        else {
            return None;
        };
        (dist(eye(body), centre(block)) <= REACH).then_some(block)
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
        if let Work::WayDig(d) = &mut self.work {
            let (c, act, status) = d.step(w, tick);
            return Err(match status {
                DigStatus::Running => (c, act, CastStatus::Running),
                DigStatus::Done => {
                    self.work = Work::Idle;
                    self.last_done = tick;
                    (c, act, CastStatus::Running)
                }
                DigStatus::Failed(f) => {
                    if self.try_again(format!("way dig failed {f:?}")) {
                        (c, act, CastStatus::Running)
                    } else {
                        self.fail()
                    }
                }
            });
        }
        // In reach by the cell's box, not its centre: the server takes a dig or a click when the
        // eye is within 5.5 of the block's box (`canInteractWithBlock`, the 4.5 range plus 1).
        // A room cell five over the floor and the wall behind the frame's top cells lie 3.9 to
        // 4.1 from the eye to their centres from every stand the room has, and failed their
        // op every try (cast.md wall 3).
        let by_box = box_dist(eye(body), target) <= REACH;
        let there = match stand {
            Some(s) => feet(body) == s && body.on_ground,
            None => by_box && body.on_ground,
        };
        if there && stand.is_none() && dist(eye(body), centre(target)) > REACH {
            self.box_reach += 1;
        }
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
                // No walk: the straight way dug toward it (clojurecraft's `toward-stand`), its
                // first block in reach, never the frame's rock or a cell the plan places (wall 22).
                if let Some(block) = self.way_block(w, body, stand.unwrap_or(target)) {
                    self.way_digs += 1;
                    self.work = Work::WayDig(Box::new(Dig::new(w, block, tick)));
                    return Err((Controls::default(), None, CastStatus::Running));
                }
                return Err(
                    if self
                        .try_again("no walk to the stand and no block of a way to dig".to_owned())
                    {
                        (Controls::default(), None, CastStatus::Running)
                    } else {
                        self.fail()
                    },
                );
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
            TaskStatus::Failed(f) => Err(
                if self.try_again(format!("walk to the stand failed {f:?}")) {
                    (Controls::default(), None, CastStatus::Running)
                } else {
                    self.fail()
                },
            ),
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
        // Never dig the floor the body stands on: step off it first, as a place steps out of its
        // cell (wall 35: place 0 of kitlava race 44236062785 dug the block under its feet at the
        // lava sea's level, fell three blocks and died in lava, in 13 of 16 lanes).
        if !matches!(self.work, Work::Dig(_)) && standing_on(body, pos) {
            if !matches!(self.work, Work::Walk(_)) {
                let mut walk = Walk::new(pos, Goal::Away { pos, range: 1.5 }, Arrive::Goal, tick);
                if !walk.route_now(w, tick) {
                    return if self.try_again("no step off the floor to be dug".to_owned()) {
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
            DigStatus::Failed(f) => {
                if holds(w, &op.done) {
                    self.work = Work::Idle;
                    return (c, act, CastStatus::Running);
                }
                if self.try_again(format!("dig failed {f:?}")) {
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
            if !self.try_again("a placed block not shown".to_owned()) {
                return self.fail();
            }
        }
        // Never place into the body: the server refuses a block whose cell the player's box
        // overlaps, so a place steps out of it first. The box, not the feet's cell: rs-l00-b06 of
        // race 44159281696 stood at (-17.53, -51, -65.86), its box 0.16 into the cell
        // (-18, -50, -67) on z, its feet in another cell, and placed into it four times for
        // nothing (wall 30).
        if overlaps(body, pos) {
            if !matches!(self.work, Work::Walk(_)) {
                let mut walk = Walk::new(pos, Goal::Away { pos, range: 1.5 }, Arrive::Goal, tick);
                if !walk.route_now(w, tick) {
                    return if self.try_again("no step out of the cell to place in".to_owned()) {
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
        // The op's block if held, else any filler held (wall 31: below y 0 the bot digs deepslate
        // and carries cobbled deepslate; once the kit's 16 cobblestone were laid every place op
        // failed with no click, rs-l05-b07 of race 44172568506 holding 55 cobbled deepslate).
        let item = op
            .item
            .filter(|i| crate::craft::count(w, *i) > 0)
            .or_else(|| filler(w))
            .unwrap_or(ItemKind::Cobblestone);
        match hold::toward(w, item) {
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
            return if self.try_again("no full block to place against".to_owned()) {
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
        if let Work::Refill(s) = &mut self.work {
            let uses = s.uses_filled();
            self.count_uses(uses);
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
                if self.try_again("a use not answered".to_owned()) {
                    (look, None, CastStatus::Running)
                } else {
                    self.fail()
                }
            }
        }
    }

    /// Fill a bucket for a pour: lava at the site's pool, water at a remembered surface source.
    /// The lava refill from the station (clojurecraft's `refill`): to the stand (walked, or the
    /// landing at its height first, or the way dug at its height), there the scoop of the nearest
    /// source with a clear face, else the block on the line to one dug. `None` when no station
    /// stands: the refill falls back to the nearest sources.
    fn station_refill(
        &mut self,
        w: &World,
        tick: u64,
    ) -> Option<(Controls, Option<Act>, CastStatus)> {
        use crate::station;
        let still = Controls::default();
        match &mut self.work {
            Work::Refill(_) | Work::Tunnel(_) => return None,
            Work::WayDig(d) => {
                let (c, act, status) = d.step(w, tick);
                return Some(match status {
                    DigStatus::Running => (c, act, CastStatus::Running),
                    DigStatus::Done => {
                        self.work = Work::Idle;
                        self.last_done = tick;
                        (c, act, CastStatus::Running)
                    }
                    DigStatus::Failed(e) => {
                        self.trail(tick, format!("station dig failed {e:?}"));
                        self.work = Work::Idle;
                        self.station_misses += 1;
                        (c, act, CastStatus::Running)
                    }
                });
            }
            Work::ToStation(walk) => {
                let (c, status) = walk.step(w, tick);
                return Some(match status {
                    TaskStatus::Running => (c, None, CastStatus::Running),
                    TaskStatus::Done => {
                        self.work = Work::Idle;
                        self.last_done = tick;
                        (still, None, CastStatus::Running)
                    }
                    TaskStatus::Failed(e) => {
                        // The goal and the feet, so a race says which stand no walk reached
                        // (places 2 and 12 failed two station walks NoPath a cast, wall 33).
                        let what = format!(
                            "station walk to {:?} failed {e:?} from {:?}",
                            walk.goal,
                            w.body.map(|b| feet(&b))
                        );
                        self.trail(tick, what);
                        self.work = Work::Idle;
                        self.station_misses += 1;
                        (still, None, CastStatus::Running)
                    }
                });
            }
            _ => {}
        }
        if self.station_misses >= STATION_MISSES {
            return None;
        }
        let body = w.body?;
        let own = cast_cells(&self.plan);
        let sources: Vec<Cell> = lava_sources(w)
            .into_iter()
            .filter(|c| !own.contains(c) && !self.refused.contains(c))
            .collect();
        let print = footprint(&self.plan);
        let Some(st) =
            station::station_among(w, &sources, self.site.frame, &print, Some(self.site.h))
        else {
            if self
                .refill_trail
                .last()
                .is_none_or(|t| !t.ends_with("no station"))
            {
                self.trail(tick, format!("{} sources, no station", sources.len()));
            }
            return None;
        };
        let f = feet(&body);
        if self
            .refill_trail
            .last()
            .is_none_or(|t| !t.contains("station at"))
        {
            self.trail(
                tick,
                format!(
                    "station at {:?} h {} with {} sources, feet {f:?}",
                    st.stand,
                    st.h,
                    st.sources.len()
                ),
            );
        }
        // The frame's rock and every cell the plan places, which a way never digs (clojurecraft's
        // `kept` and `placed?`): a way dug through the room's floor undid its place op, and the
        // cast went back to it 360 times (wall 20).
        let mut kept = station::kept(self.site.frame);
        kept.extend(
            self.plan
                .iter()
                .filter(|op| op.kind == OpKind::Place)
                .filter_map(|op| op.pos),
        );
        let from = eye(&body);
        // A station source already in reach with a clear face: scooped from here.
        if body.on_ground
            && let Some(src) = st
                .sources
                .iter()
                .find(|src| crate::scoop::aim_at_use(w, from, **src, crate::scoop::REACH).is_some())
        {
            self.work = Work::Refill(Box::new(Scoop::of(*src, memory::Fluid::Lava, tick)));
            self.last_done = tick;
            return Some((still, None, CastStatus::Running));
        }
        if f == st.stand && body.on_ground {
            if let Some(src) = st
                .sources
                .iter()
                .find(|src| crate::scoop::aim_at_use(w, from, **src, crate::scoop::REACH).is_some())
            {
                self.work = Work::Refill(Box::new(Scoop::of(*src, memory::Fluid::Lava, tick)));
                self.last_done = tick;
                return Some((still, None, CastStatus::Running));
            }
            let block = st
                .sources
                .iter()
                .find_map(|src| station::sight_block(w, from, *src, f.1, &kept))?;
            self.work = Work::WayDig(Box::new(Dig::new(w, block, tick)));
            return Some((still, None, CastStatus::Running));
        }
        // A found route only: a stand in the rock (andesite at (-34, 4, 9) and over it, race
        // 44252670652) had a partial route toward it, the walk failed NoPath one block off, and
        // the way was never dug (wall 38). With none, the landing at the stand's height first, or
        // at that height the straight way dug (`stance-block`).
        let found = |walk: &Walk| walk.route == Some(crate::path::Status::Found);
        let mut walk = Walk::new(st.stand, Goal::Block { pos: st.stand }, Arrive::Goal, tick);
        if walk.route_now(w, tick) && found(&walk) {
            self.work = Work::ToStation(walk);
            return Some((still, None, CastStatus::Running));
        }
        if f.1 != st.stand.1 {
            // Off the stand's height: to the landing at it first (the room's access steps).
            let landing = self.site.frame.at(crate::build::landing(st.h));
            if landing == f {
                return None;
            }
            let mut walk = Walk::new(landing, Goal::Block { pos: landing }, Arrive::Goal, tick);
            if !walk.route_now(w, tick) || !found(&walk) {
                return None;
            }
            self.work = Work::ToStation(walk);
            return Some((still, None, CastStatus::Running));
        }
        // At its height: the first block on the straight way, dug when in reach.
        let station::Way::Dig(block) = station::stance_block(w, f, st.stand, &kept) else {
            return None;
        };
        if dist(eye(&body), centre(block)) > REACH {
            return None;
        }
        self.work = Work::WayDig(Box::new(Dig::new(w, block, tick)));
        Some((still, None, CastStatus::Running))
    }

    fn refill(
        &mut self,
        w: &World,
        item: ItemKind,
        tick: u64,
    ) -> (Controls, Option<Act>, CastStatus) {
        // Stairs down end with the feet one over the source they head for: six stairs from a
        // floor three over the sea took the feet to y -58, under the lava's surface, and the next
        // refill found no route (race 45354357884, place 3).
        if let (Work::Tunnel(_), Some(floor), Some(body)) = (&self.work, self.tunnel_floor, w.body)
            && body.on_ground
            && feet(&body).1 <= floor
        {
            self.work = Work::Idle;
            self.tunnels += 1;
            self.last_done = tick;
            self.trail(tick, format!("tunnel down ended at y {}", feet(&body).1));
            return (Controls::default(), None, CastStatus::Running);
        }
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
        if item == ItemKind::LavaBucket
            && let Some(r) = self.station_refill(w, tick)
        {
            return r;
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
                    .filter(|c| !own.contains(c) && !self.refused.contains(c))
                    .filter(|c| dist(centre(*c), centre(self.site.lava)) <= POOL_RADIUS)
                    .collect(),
                // The cast's own poured water first (wall 28): a pour that made no obsidian left
                // its water in the room, the scoop back comes after the pour in the plan, and
                // memory keeps only surface water; b05 walked 364 times toward pools no walk
                // reached with its water a block away.
                memory::Fluid::Water => {
                    let mut near = own_water(w, self.site.frame);
                    if !near.is_empty() {
                        self.trail(tick, format!("own water {}", near.len()));
                    }
                    near.extend(crate::scoop::sources(w));
                    near
                }
            };
            targets.truncate(REFILL_TRIES);
            let nearest = targets.first().copied();
            let sight = targets.clone();
            let Some(s) = targets.into_iter().find_map(|t| {
                let mut s = Scoop::of(t, fluid, tick);
                s.route_now(w, tick).then_some(s)
            }) else {
                // A source in reach but out of sight: the first block on the eye's line to it dug,
                // under the feet, never the frame's rock or a placed cell (wall 24: the room's own
                // floor hid a lava sea two under it, and no stand could see it).
                if let (memory::Fluid::Lava, Some(body)) = (fluid, w.body) {
                    let from = eye(&body);
                    let mut kept = crate::station::kept(self.site.frame);
                    kept.extend(
                        self.plan
                            .iter()
                            .filter(|op| op.kind == OpKind::Place)
                            .filter_map(|op| op.pos),
                    );
                    if let Some(block) = sight
                        .iter()
                        .filter(|t| dist(from, centre(**t)) <= crate::scoop::REACH)
                        .find_map(|t| {
                            crate::station::sight_block(w, from, *t, feet(&body).1, &kept)
                        })
                        && self.sight_digs < SIGHT_DIGS
                    {
                        self.sight_digs += 1;
                        self.work = Work::WayDig(Box::new(Dig::new(w, block, tick)));
                        self.last_done = tick;
                        return (Controls::default(), None, CastStatus::Running);
                    }
                }
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
                        // A source two or more under the feet: stairs down toward it, not a level
                        // tunnel. A room floor three over the lava sea is out of a bucket's reach
                        // from every cell at its height, and a level tunnel never came nearer
                        // ("no route, 1 tunnels" with 168 and 286 sources seen: cast.md wall 2).
                        // The stairs' own refusal stops them short of lava.
                        let down = t.1 + 1 < f.1;
                        self.tunnel_floor = down.then_some(t.1 + 1);
                        self.trail(
                            tick,
                            format!(
                                "tunnel {} toward {t:?}",
                                if down { "down" } else { "level" }
                            ),
                        );
                        self.work = Work::Tunnel(Box::new(if down {
                            crate::stairs::Stairs::new(dir, tick)
                        } else {
                            crate::stairs::Stairs::level(dir, tick)
                        }));
                        self.last_done = tick;
                        return (Controls::default(), None, CastStatus::Running);
                    }
                }
                self.trail(
                    tick,
                    format!("no route to {:?}, {} tunnels", nearest, self.tunnels),
                );
                return (
                    Controls::default(),
                    None,
                    CastStatus::Failed(CastFailure::NoRefill),
                );
            };
            let how = match s.stand_exact {
                Some(true) => " (stand exact)",
                Some(false) => " (stand sampled)",
                None => "",
            };
            self.trail(
                tick,
                format!(
                    "scoop {:?} routed{how}, {} sampled skipped",
                    s.target, s.sampled_skipped
                ),
            );
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
                let uses = s.uses;
                self.count_uses(uses);
                self.work = Work::Idle;
                (c, act, CastStatus::Running)
            }
            ScoopStatus::Failed(f) => {
                let (target, uses) = (s.target, s.uses);
                self.count_uses(uses);
                // Never this source again in this cast: the next refill takes another.
                if !self.refused.contains(&target) {
                    self.refused.push(target);
                }
                self.trail(tick, format!("scoop {target:?} failed {f:?}, refused"));
                self.work = Work::Idle;
                if self.try_again(format!("scoop failed {f:?}")) {
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

pub fn lava_within(w: &World, radius: f64) -> Vec<Cell> {
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
    /// Stand-first candidates (wall 41): stands beside a source, and the sites built from them
    /// that passed.
    pub beside: usize,
    pub beside_passed: usize,
}

impl SiteReport {
    pub fn text(&self) -> String {
        format!(
            "sources {} landings {} refused/spent {} behind {} footprint {} blocks {} station {} passed {} beside {} beside-passed {}",
            self.sources,
            self.landings,
            self.refused_or_spent,
            self.lava_behind,
            self.footprint,
            self.blocks,
            self.no_station,
            self.passed,
            self.beside,
            self.beside_passed
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
            // The lava in front of the frame's plane, on the room's side: lava behind it is reached
            // only through the frame's own rock (wall 24: a pool two behind a south frame passed
            // the old distance test, and no stand saw it).
            let unit = frame.at((0, 0, 1));
            let (ux, uz) = (unit.0 - frame.origin.0, unit.2 - frame.origin.2);
            let ahead = (src.0 - frame.origin.0) * ux + (src.2 - frame.origin.2) * uz;
            if ahead < 1 {
                report.lava_behind += 1;
                continue;
            }
            // Access steps up to a pool above the room: its surface one over the step's feet.
            let h = (src.1 + 1 - landing.1).clamp(0, crate::build::MAX_ACCESS);
            let plan = frame_plan(frame, false, h);
            let foot = footprint(&plan);
            if !footprint_ok(w, &lava, &foot) {
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
    // No landing site passed: clojurecraft's `choose-site`, the site built from a lava stand
    // beside the cell over a source (`stands_beside`), the frame `STATION_REACH` behind it and h
    // under it, the room cut from the rock. Lava in rock has few open landings: 64 of the 67 bots
    // of kit race 44268348517 that never cast had seen lava, round 1 to 7 landings, none passing
    // (wall 41). Alone it lit fewer (wall 21, before the way digs of wall 22), so it is the second
    // choice here, never the first.
    if found.is_empty() {
        let mut stands: Vec<(f64, Cell, Cell)> = lava
            .iter()
            .filter(|c| !refused(**c))
            .take(MAX_SOURCES)
            .flat_map(|src| {
                crate::station::stands_beside(w, *src)
                    .into_iter()
                    .map(|st| (dist(here, centre(st)), st, *src))
            })
            .collect();
        stands.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
        report.beside = stands.len();
        for (d, stand, src) in stands {
            if found.len() >= SITE_CANDIDATES {
                break;
            }
            for (facing, h) in Facing::ALL
                .into_iter()
                .flat_map(|f| crate::station::STATION_HEIGHTS.map(|h| (f, h)))
            {
                let probe = Frame {
                    origin: (0, 0, 0),
                    facing,
                };
                let off = probe.at((crate::build::landing(h).0, h, crate::station::STATION_REACH));
                let frame = Frame {
                    origin: (stand.0 - off.0, stand.1 - off.1, stand.2 - off.2),
                    facing,
                };
                if refused(frame.origin) || spent(w, src, frame) {
                    continue;
                }
                let plan = frame_plan(frame, false, h);
                let foot = footprint(&plan);
                if !footprint_ok(w, &lava, &foot) {
                    continue;
                }
                let n = blocks_needed(w, &plan);
                if n > SITE_BLOCKS {
                    continue;
                }
                if crate::station::station_among(w, &lava, frame, &foot, Some(h)).is_none() {
                    continue;
                }
                report.beside_passed += 1;
                found.push((
                    (n, d),
                    Site {
                        frame,
                        lava: src,
                        h,
                    },
                    // Entered at h, along the level way from the lava stand, the room dug down
                    // from there (clojurecraft's `landing`): a bot routed to the room's floor
                    // stood 4 under its first op's stand and failed it in 51 ticks (wall 45).
                    frame.at(crate::build::landing(h)),
                ));
                break;
            }
        }
    }
    // The churn invariant: a site is taken only when a walk to its landing is found (one bot at
    // y -32 chose a landing at y -52 in another cave and failed its first dig: wall 4).
    // Low sites first: a bot routed to the room's floor reaches the first stands of a site with h
    // up to `LOW_SITE` by digging up from it, and fails a higher one's within a minute (wall 49:
    // `dig :landing` failures at h 3 and 4; routing to `landing(h)` and climbing both raced worse,
    // walls 46 and 48). Then the fewest blocks, then the nearest.
    found.sort_by(|a, b| {
        (a.1.h > LOW_SITE)
            .cmp(&(b.1.h > LOW_SITE))
            .then(a.0.0.cmp(&b.0.0))
            .then(a.0.1.total_cmp(&b.0.1))
    });
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

/// Every footprint cell loaded, none seen holding fluid, none sharing a face with a lava source.
fn footprint_ok(w: &World, lava: &[Cell], foot: &[Cell]) -> bool {
    foot.iter().all(|c| {
        kind(w, *c).is_some()
            && !matches!(seen(w, *c), Some(BlockKind::Water | BlockKind::Lava))
            // Lava flows into a cell through a face only: a source sharing one, not a diagonal
            // (wall 13: the diagonal rule refused 74 of 140 sites round one pool).
            && !lava
                .iter()
                .any(|l| (l.0 - c.0).abs() + (l.1 - c.1).abs() + (l.2 - c.2).abs() <= 1)
    })
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
