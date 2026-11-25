//! The heading (issue 17d, docs/problems/heading.md): a long way out into the Nether toward a warped
//! forest, on one compass axis held, a leg of 48 blocks at a time, never pacing old cells, its
//! spend counted per metre. clojurecraft's stage 18 lost its pickaxes and its filler to loops that
//! made no progress (tunnels turned on a 30 s clock dug squares, legs paced a 1-wide causeway,
//! filler spent sealing and unsealing: brain/oracle/handoff/cc-stage18-lessons.md); each of those
//! is an invariant here, not a patch:
//!
//! - **Holds direction**: the axis changes only by a quarter turn after a leg failed (toward the
//!   side with fewer seen columns, alternating on a tie), or by a re-choice, noted, after
//!   `IDLE_TURNS` turns without `RECHOOSE_GAIN` on any. Never on a clock.
//! - **Never paces**: `best`, the furthest projection of the feet on the axis, must grow
//!   `PACE_GAIN` every `PACE_TICKS` server ticks or the leg fails `NoProgress`. A leg's route never
//!   steps into a cell the heading cut or stood in at or behind the feet's projection (`cut`).
//! - **Spends in proportion**: digs, blocks placed and pickaxe uses per metre in every leg's end
//!   note; a leg over `DIGS_PER_M` or `PLACED_PER_M` ends.
//! - **Walk and bridge first, tunnel last**: pickaxe uses are the trip's scarce resource (two stone
//!   pickaxes, no wood), filler is not (about 620 blocks; `KEEP` held back, restocked from the
//!   wall when under it).
//!
//! What it may know (strict no X-ray): blocks seen, the biome at its own feet and the biomes that
//! blocks seen evidence; never a biome of an unseen column, never anything from RCON.

use std::collections::{BTreeMap, BTreeSet};

use data::BlockKind;
use world::{BlockPos, World};

use crate::path::{self, Cell, Goal};
use crate::tunnel;
use crate::walk::Walk;
use crate::{Plan, Task};

/// Blocks a leg runs along the axis: three chunks, so each leg loads a new row of columns ahead
/// (clojurecraft's legs were 64; issue 16's search tunnel's `EXPLORE_LEG` is 48).
pub const LEG: i32 = 48;
/// A leg's target height is clamped to this band: over the lava sea's level and its magma
/// (`tunnel::SAFE_Y`), under the Nether's high caverns (clojurecraft's 40 to 90).
pub const Y_LO: i32 = tunnel::SAFE_Y;
pub const Y_HI: i32 = 90;
/// A leg is done within this of its end on the axis: the tunnel's `REACH`.
pub const LEG_REACH: i32 = 5;
/// The projection invariant: `best` grows this much every `PACE_TICKS` server ticks, or the leg
/// fails `NoProgress` (the plan's numbers: a minute of game time for six blocks, where a walk
/// does 4 a second and a tunnel about 1 every 40 ticks).
pub const PACE_TICKS: u64 = 1200;
pub const PACE_GAIN: i32 = 6;
/// Turns without the turned-to axis gaining `RECHOOSE_GAIN` before the axis is chosen again from
/// scratch, and the choices made before the heading is turned out (the plan: 3 turns; 4 choices
/// are 16 legs failed in a row at most).
pub const IDLE_TURNS: u32 = 3;
pub const RECHOOSE_GAIN: i32 = 48;
pub const MAX_CHOICES: u32 = 4;
/// Filler kept unspent by a leg's bridges (the handoff's 32; `path::BRIDGE_RESERVE` is 16 for the
/// fortress walk), and a restock from the wall runs to `RESTOCK_TO`, at most `RESTOCK_DIGS` digs.
pub const KEEP: i64 = 32;
pub const RESTOCK_TO: i64 = 48;
pub const RESTOCK_DIGS: u32 = 24;
/// A wall cell is dug for filler from this near the eye (`walk::REACH`).
pub const RESTOCK_REACH: f64 = 4.0;
/// A leg's budget per metre of `best` gained, read only past `BUDGET_GRACE` metres of spend: a
/// level tunnel cuts 2 a metre and a stair 3 (docs/problems/nether-tunnel.md), a straight bridge
/// lays 1 a metre (the plan's starting numbers, to be refined from races). Blocks placed are let
/// run to 2 a metre: filler is plentiful (619 at the frontier start) and the first leg from the
/// frontier's leave point is a far walk of 78 cells, 66 of them bridged over the lava sea
/// (probe 45478841859); a leg ended mid-bridge stands over lava.
pub const DIGS_PER_M: f64 = 3.0;
pub const PLACED_PER_M: f64 = 2.0;
pub const BUDGET_GRACE: i32 = 8;
/// A tunnel leg's digs are estimated at this a metre against the uses left over the reserve.
pub const TUNNEL_DIGS_PER_M: i32 = 2;
/// A leg's hard dig budget: 144 pickaxe uses in all in the frontier kit (13 + 131, 17a's read of
/// the snapshot), 40 of them the reserve, so about four tunnelled legs' worth at 24 (a level
/// tunnel's 2 a metre over half a leg). Walk and bridge are the legs' ways; a tunnel is the
/// exception.
pub const LEG_DIGS: u32 = 24;
/// Seen columns counted ahead on an axis: chunks 7 to 16 out, 2 either side (clojurecraft's
/// `most-unseen-dir`, over seen columns, not loaded ones).
pub const AHEAD_NEAR: i32 = 7;
pub const AHEAD_FAR: i32 = 16;
pub const AHEAD_WIDE: i32 = 2;
/// The centroid of what is explored must be this far off before it decides the axis; nearer, the
/// four axes tie and the count ahead decides.
pub const AWAY_MIN: f64 = 16.0;
/// Crimson or basalt signs seen in the columns 1 to `SIGNS_AHEAD` chunks ahead (±1) that turn a
/// heading (hoglins and piglins; magma and broken ground): clojurecraft's forest threshold, 12.
pub const SIGNS_AHEAD: i32 = 4;
pub const SIGNS_TURN: u32 = 12;
/// Leg attempts one look may make: each is up to three searches (`walk::FAR_NODES` the dearest),
/// so a look that turns twice waits a tick for the next.
pub const ATTEMPTS_PER_LOOK: u32 = 2;
/// A leg whose target's column is not loaded waits this long for it, looking again every
/// `LOAD_EVERY` ticks (a far search is tens of milliseconds): chunks arrive over the first seconds
/// after a portal, and the race servers send 8 chunks (128 blocks), so a target 48 ahead is
/// loaded once the bot has stood a moment.
pub const LOAD_WAIT: u64 = 200;
pub const LOAD_EVERY: u64 = 20;
/// Tries of one way (walk, far walk) that gained under `PACE_GAIN` before the leg goes on to the
/// next way: one may be a walk cut by a fight or a meal.
pub const WAY_TRIES: u32 = 2;

/// A compass axis: (dx, dz), one of them zero.
pub type Dir = (i32, i32);
/// The four, in the order a tie is settled.
pub const AXES: [Dir; 4] = [(1, 0), (0, 1), (-1, 0), (0, -1)];

/// How a leg is being made.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Way {
    /// A plain route (`path::plan`), walked.
    Walk,
    /// The far walk, bridging with `KEEP` held back.
    Far,
    /// The travel tunnel to the leg's target.
    Tunnel,
}

impl Way {
    fn name(self) -> &'static str {
        match self {
            Way::Walk => "walk",
            Way::Far => "far walk",
            Way::Tunnel => "tunnel",
        }
    }

    fn next(self) -> Option<Way> {
        match self {
            Way::Walk => Some(Way::Far),
            Way::Far => Some(Way::Tunnel),
            Way::Tunnel => None,
        }
    }
}

/// One leg in hand.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Leg {
    pub n: u32,
    /// Where the leg began (the feet) and its end: `LEG` ahead on the axis, or toward a forest.
    pub from: Cell,
    pub target: Cell,
    /// A known forest's column centre this leg runs toward, instead of the axis.
    #[serde(default)]
    pub forest: Option<Cell>,
    pub way: Way,
    pub tries: u32,
    pub began: u64,
    /// The leg's progress when its way was last handed out, and its best with the tick it last
    /// grew `PACE_GAIN` (the projection invariant).
    pub tried_at: i32,
    pub mark: (i32, u64),
    /// Spend at its start: pickaxe uses left and filler held.
    pub uses0: i32,
    pub filler0: i64,
    /// Digs seen done (cells handed to a stretch that are now open) and filler they gave.
    pub digs: u32,
    pub gained: i64,
    /// Cells a stretch was handed to open, and whether each drops filler; the walk's route, whose
    /// cells passed join `cut`.
    #[serde(default)]
    pub pending: Vec<(Cell, bool)>,
    #[serde(default)]
    pub planned: Vec<Cell>,
    /// While the target's column is not loaded yet: no search before this tick.
    #[serde(default)]
    pub wait_until: u64,
}

/// A restock in hand: filler and uses at its start, digs begun.
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Restock {
    pub filler0: i64,
    pub uses0: i32,
    pub digs: u32,
    pub began: u64,
}

/// The heading held (plan 17d). Every field defaults: a moment written before one was added loads.
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct Heading {
    pub dir: Dir,
    /// The feet where this axis began (a turn starts a fresh projection here).
    pub origin: Cell,
    /// The furthest projection of the feet on `dir` from `origin`.
    pub best: i32,
    /// Cells this heading cut or stood in (feet and head): kept across turns.
    pub cut: BTreeSet<Cell>,
    pub legs: u32,
    pub turns: u32,
    /// Legs failed, turns without `RECHOOSE_GAIN` since the last gain, axes so failed, choices.
    pub stalls: u32,
    pub idle_turns: u32,
    pub failed: Vec<Dir>,
    pub choices: u32,
    /// The side of the last turn: 1 left, -1 right, 0 none yet.
    pub side: i8,
    pub began: u64,
    /// Spent by legs ended so far.
    pub digs: u32,
    pub placed: i64,
    pub uses_spent: i32,
    pub reason: String,
    pub leg: Option<Leg>,
    pub restock: Option<Restock>,
    /// A restock that found no wall cell is not begun again before this tick.
    pub restock_after: u64,
    /// The feet biome a turn was last made for (crimson or basalt), so it turns once on entering.
    pub biome_turned: Option<String>,
    /// The column legs are running to (a warped forest, or a soul sand valley), instead of the axis.
    pub forest: Option<(i32, i32)>,
    /// Whether that column is a soul sand valley's (reached, the axis goes on) and not a forest's.
    pub valley: bool,
    /// Valleys reached: never run to again.
    pub valleys: Vec<(i32, i32)>,
    /// Set when the heading is over (a warped forest at the feet, or turned out).
    pub over: bool,
}

/// What a look knows of biomes (strict no X-ray: the feet's own, and blocks seen).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Signs {
    pub feet_biome: Option<&'static str>,
    /// A column with a warped forest's signs seen (`Memory::forest_known`).
    pub forest: Option<(i32, i32)>,
    /// A column with a soul sand valley's signs seen (soul sand and soul soil): the second
    /// choice, an enderman spawns there too (docs/problems/endermen.md, 17a).
    pub valley: Option<(i32, i32)>,
}

// What memory and the world know (E1, issue 17b): seen columns, the forest and valleys known from
// blocks seen, the signs in a column, the feet's own biome.

/// Chunk columns seen in this dimension (`Memory::seen_columns`: kept after they unload).
fn seen_columns(w: &World) -> BTreeSet<(i32, i32)> {
    w.dimension
        .as_ref()
        .map(|d| w.memory.seen_columns(&d.name).into_iter().collect())
        .unwrap_or_default()
}

/// Crimson and basalt signs seen in one column.
fn hostile_signs_in_column(w: &World, cx: i32, cz: i32) -> u32 {
    let s = w.memory.biome_signs_in_column(cx, cz);
    u32::try_from(s.crimson + s.basalt).unwrap_or(u32::MAX)
}

/// Soul signs seen in one column.
fn soul_signs_in_column(w: &World, cx: i32, cz: i32) -> u32 {
    u32::try_from(w.memory.biome_signs_in_column(cx, cz).soul).unwrap_or(u32::MAX)
}

// E3's (issue 17c): the pickaxe uses held, and the reserve kept for issue 18's burrow.
fn pickaxe_uses_left(w: &World) -> i32 {
    crate::endermen::pickaxe_uses_left(w)
}

fn pickaxe_reserve(p: &Plan) -> i32 {
    p.table.as_ref().map_or(0, |t| t.survival.pickaxe_reserve)
}

/// The seen columns with soul signs, and how many.
fn soul_columns(w: &World, seen: &BTreeSet<(i32, i32)>) -> BTreeMap<(i32, i32), u32> {
    seen.iter()
        .map(|c| (*c, soul_signs_in_column(w, c.0, c.1)))
        .filter(|(_, n)| *n > 0)
        .collect()
}

/// Seen columns `AHEAD_NEAR` to `AHEAD_FAR` chunks ahead of `chunk` on `dir`, `AHEAD_WIDE` aside.
pub fn seen_ahead(seen: &BTreeSet<(i32, i32)>, chunk: (i32, i32), (dx, dz): Dir) -> usize {
    (AHEAD_NEAR..=AHEAD_FAR)
        .flat_map(|d| (-AHEAD_WIDE..=AHEAD_WIDE).map(move |s| (d, s)))
        .filter(|(d, s)| seen.contains(&(chunk.0 + dx * d + dz * s, chunk.1 + dz * d + dx * s)))
        .count()
}

/// Soul signs seen in the columns on `dir`'s side of `chunk` (ahead more than aside): a soul
/// sand valley that way, where endermen spawn too.
pub fn soul_ahead(soul: &BTreeMap<(i32, i32), u32>, chunk: (i32, i32), (dx, dz): Dir) -> u32 {
    soul.iter()
        .filter(|((cx, cz), _)| {
            let (x, z) = (cx - chunk.0, cz - chunk.1);
            let on = x * dx + z * dz;
            on > 0 && on >= (x * dz - z * dx).abs()
        })
        .map(|(_, n)| *n)
        .sum()
}

/// The axis to hold from `feet`, with why, never one of `avoid`: the one with the most soul signs
/// seen on its side (`soul_ahead`), then the one pointing most away from the centroid of the
/// columns seen (the fortress and the portal trail in them); ties (the centroid within
/// `AWAY_MIN`, or two axes level) to the fewest seen columns ahead, then `AXES`' order. `None`
/// when every axis is avoided. Pure.
pub fn choose_dir(
    feet: Cell,
    seen: &BTreeSet<(i32, i32)>,
    soul: &BTreeMap<(i32, i32), u32>,
    avoid: &[Dir],
) -> Option<(Dir, String)> {
    let n = seen.len().max(1) as f64;
    let (sx, sz) = seen.iter().fold((0.0, 0.0), |(x, z), (cx, cz)| {
        (x + f64::from(cx * 16 + 8), z + f64::from(cz * 16 + 8))
    });
    let centre = (sx / n, sz / n);
    let away = (
        f64::from(feet.0) + 0.5 - centre.0,
        f64::from(feet.2) + 0.5 - centre.1,
    );
    let far = away.0.hypot(away.1) >= AWAY_MIN && !seen.is_empty();
    let chunk = (feet.0 >> 4, feet.2 >> 4);
    // Each axis: how far it points away (in whole blocks, so near-ties tie), then fewest ahead.
    let score = |d: &Dir| {
        let dot = if far {
            (away.0 * f64::from(d.0) + away.1 * f64::from(d.1)).round() as i64
        } else {
            0
        };
        (
            std::cmp::Reverse(soul_ahead(soul, chunk, *d)),
            std::cmp::Reverse(dot),
            seen_ahead(seen, chunk, *d),
        )
    };
    let best = AXES
        .iter()
        .filter(|d| !avoid.contains(d))
        .min_by_key(|d| score(d))?;
    let (souls, dot, ahead) = score(best);
    let why = if souls.0 > 0 {
        format!(
            "{} soul signs seen that way (a soul sand valley); {ahead} seen columns ahead",
            souls.0
        )
    } else if far {
        format!(
            "{} blocks away from the centroid of {} seen columns at ({:.0}, {:.0}); {ahead} seen \
             columns ahead",
            dot.0,
            seen.len(),
            centre.0,
            centre.1
        )
    } else {
        format!(
            "the centroid of {} seen columns is under {AWAY_MIN} blocks off: the fewest seen \
             columns ahead ({ahead})",
            seen.len()
        )
    };
    Some((*best, why))
}

/// The quarter turn from `dir`: toward the side with fewer seen columns ahead, a tie to the side
/// other than `last` (alternating). Returns the new axis and its side (1 left, -1 right). Pure.
pub fn turn_dir(dir: Dir, chunk: (i32, i32), seen: &BTreeSet<(i32, i32)>, last: i8) -> (Dir, i8) {
    let left = (dir.1, -dir.0);
    let right = (-dir.1, dir.0);
    let (l, r) = (
        seen_ahead(seen, chunk, left),
        seen_ahead(seen, chunk, right),
    );
    match l.cmp(&r) {
        std::cmp::Ordering::Less => (left, 1),
        std::cmp::Ordering::Greater => (right, -1),
        std::cmp::Ordering::Equal if last == 1 => (right, -1),
        std::cmp::Ordering::Equal => (left, 1),
    }
}

/// The projection of `c` on `dir` from `origin`.
pub fn projection(origin: Cell, dir: Dir, c: Cell) -> i32 {
    path::along(origin, dir, c).0
}

/// Whether a dig of `c` is a safe restock: exposed netherrack in a wall (at the feet's height to
/// `WALL_RISE` over them, never the floor), with no lava beside it (read as the staircase's
/// refusal reads the cells a cut opens: docs/problems/nether-tunnel.md), nothing over it that
/// falls, and at the feet's height a floor under it, so no opening over a drop is made.
fn restock_cell(w: &World, feet: Cell, c: Cell) -> bool {
    let kind = |c: Cell| w.block_at(BlockPos::new(c.0, c.1, c.2));
    if kind(c) != Some(BlockKind::Netherrack)
        || !crate::escape::exposed(w, c)
        || c.1 < feet.1
        || c.1 > feet.1 + WALL_RISE
    {
        return false;
    }
    let lava = memory::neighbours(c)
        .iter()
        .any(|n| matches!(kind(*n), Some(BlockKind::Lava)));
    let falls_over = crate::stairs::falls(kind((c.0, c.1 + 1, c.2)));
    let floor_under =
        path::classify(w.state_at(BlockPos::new(c.0, c.1 - 1, c.2))) == path::Class::Solid;
    !lava && !falls_over && (c.1 > feet.1 || floor_under)
}

/// Wall cells dug for filler are at most this over the feet (reach from the eye bounds it too).
pub const WALL_RISE: i32 = 3;

/// Exposed netherrack within `RESTOCK_REACH` of the eye that `restock_cell` allows, nearest
/// first.
pub fn restock_cells(w: &World) -> Vec<Cell> {
    let Some(body) = w.body else {
        return Vec::new();
    };
    let feet = path::stand_cell(w, body.pos);
    let eye = [body.pos[0], body.pos[1] + physics::EYE_HEIGHT, body.pos[2]];
    let r = RESTOCK_REACH.ceil() as i32;
    let mut out: Vec<(i64, Cell)> = (-r..=r)
        .flat_map(|dx| (-r..=r).flat_map(move |dy| (-r..=r).map(move |dz| (dx, dy, dz))))
        .map(|(dx, dy, dz)| (feet.0 + dx, feet.1 + dy, feet.2 + dz))
        .filter_map(|c| {
            let d = (f64::from(c.0) + 0.5 - eye[0]).powi(2)
                + (f64::from(c.1) + 0.5 - eye[1]).powi(2)
                + (f64::from(c.2) + 0.5 - eye[2]).powi(2);
            (d.sqrt() <= RESTOCK_REACH && restock_cell(w, feet, c))
                .then_some(((d * 100.0) as i64, c))
        })
        .collect();
    out.sort();
    out.into_iter().map(|(_, c)| c).collect()
}

/// Blocks whose dig drops filler (`path::BRIDGE_BLOCKS`).
fn drops_filler(k: Option<BlockKind>) -> bool {
    matches!(
        k,
        Some(
            BlockKind::Netherrack
                | BlockKind::Stone
                | BlockKind::Cobblestone
                | BlockKind::Deepslate
                | BlockKind::CobbledDeepslate
                | BlockKind::Granite
                | BlockKind::Diorite
                | BlockKind::Andesite
                | BlockKind::Tuff
                | BlockKind::Dirt
        )
    )
}

fn open(w: &World, c: Cell) -> bool {
    matches!(
        w.block_at(BlockPos::new(c.0, c.1, c.2)),
        Some(BlockKind::Air | BlockKind::CaveAir | BlockKind::VoidAir)
    )
}

/// Whether the feet are within `LEG_REACH` of a column's centre on the plane.
fn feet_at_column(feet: Cell, col: Option<(i32, i32)>) -> bool {
    col.is_some_and(|(cx, cz)| {
        (cx * 16 + 8 - feet.0).abs() + (cz * 16 + 8 - feet.2).abs() <= LEG_REACH
    })
}

fn per_m(n: f64, gain: i32) -> f64 {
    n / f64::from(gain.max(1))
}

impl Heading {
    /// A heading on `dir` from `feet`, chosen for `reason`.
    pub fn new(dir: Dir, feet: Cell, tick: u64, reason: String) -> Self {
        Self {
            dir,
            origin: feet,
            began: tick,
            reason,
            ..Self::default()
        }
    }

    /// The axis chosen once from what is seen (plan 17d): see `choose_dir`.
    pub fn choose(w: &World, feet: Cell, tick: u64) -> Self {
        let seen = seen_columns(w);
        let (dir, why) = choose_dir(feet, &seen, &soul_columns(w, &seen), &[])
            .unwrap_or(((1, 0), "no axis scored".to_owned()));
        Self::new(dir, feet, tick, why)
    }

    /// Cut cells at or behind the projection of `feet` on the axis, but the feet's own and the
    /// head's: what a leg's route may not step into (it would pace old ground).
    pub fn behind(&self, feet: Cell) -> BTreeSet<Cell> {
        let on = projection(self.origin, self.dir, feet);
        self.cut
            .iter()
            .filter(|c| **c != feet && **c != (feet.0, feet.1 + 1, feet.2))
            .filter(|c| projection(self.origin, self.dir, **c) <= on)
            .copied()
            .collect()
    }

    /// The feet (and head) stood in join `cut`.
    fn stand(&mut self, feet: Cell) {
        self.cut.insert(feet);
        self.cut.insert((feet.0, feet.1 + 1, feet.2));
    }
}

impl Leg {
    /// The leg's progress at `feet`: the projection on the axis from the leg's start, or the
    /// nearness to the forest (minus the distance on the plane).
    fn progress(&self, dir: Dir, feet: Cell) -> i32 {
        match self.forest {
            Some(f) => -((f.0 - feet.0).abs() + (f.2 - feet.2).abs()),
            None => projection(self.from, dir, feet),
        }
    }

    fn done(&self, dir: Dir, feet: Cell) -> bool {
        match self.forest {
            Some(f) => (f.0 - feet.0).abs() + (f.2 - feet.2).abs() <= LEG_REACH,
            None => projection(self.from, dir, feet) >= LEG - LEG_REACH,
        }
    }

    fn goal(&self, dir: Dir) -> Goal {
        match self.forest {
            Some(f) => Goal::Near {
                pos: f,
                range: f64::from(LEG_REACH - 1),
            },
            None => Goal::Ahead {
                from: self.from,
                dir,
                gain: LEG,
            },
        }
    }
}

/// How a leg stands when the planner looks again.
enum Look {
    Done,
    Failed(String),
    Going,
}

impl Plan {
    // >>> heading (issue 17d, docs/problems/heading.md): the planner's one entry to the heading.
    /// The next step of the held heading: a leg's walk, far walk or tunnel stretch, a restock dig,
    /// or `Task::Idle` to look again next tick. `None` when the heading is over: a warped forest
    /// at the feet (the caller's endermen task takes over), or no way on any axis after
    /// `MAX_CHOICES` choices (turned out). The ledger says which.
    pub fn heading_task(&mut self, w: &World, tick: u64) -> Option<Task> {
        // The first valley known not yet crossed.
        let crossed = self
            .headed
            .as_ref()
            .map(|h| h.valleys.clone())
            .unwrap_or_default();
        let signs = Signs {
            feet_biome: w.feet_biome(),
            forest: w.memory.forest_known(),
            // Issue 18c: past the endermen stage a valley is no longer a target.
            valley: (self.ender.phase == crate::endermen::EnderPhase::Reach)
                .then(|| {
                    w.memory
                        .soul_valleys()
                        .iter()
                        .find(|c| !crossed.contains(c))
                        .copied()
                })
                .flatten(),
        };
        self.heading_with(w, signs, tick)
    }

    /// `heading_task` with what is known of biomes given: the tests' entry (the feet biome is
    /// E1's to decode, the model's to paint).
    pub fn heading_with(&mut self, w: &World, signs: Signs, tick: u64) -> Option<Task> {
        let body = w.body?;
        let feet = path::stand_cell(w, body.pos);
        let mut h = match self.headed.take() {
            Some(h) if !h.over => h,
            Some(h) => {
                self.headed = Some(h);
                return None;
            }
            None => {
                let h = Heading::choose(w, feet, tick);
                self.note(
                    tick,
                    format!("heading chosen: ({}, {})", h.dir.0, h.dir.1),
                    format!(
                        "{}; from {feet:?}, pickaxe uses {}, filler {}",
                        h.reason,
                        pickaxe_uses_left(w),
                        path::bridge_blocks(w)
                    ),
                );
                h
            }
        };
        let task = self.heading_look(w, &mut h, signs, feet, tick);
        self.headed = Some(h);
        task
    }
    // <<< heading

    fn heading_look(
        &mut self,
        w: &World,
        h: &mut Heading,
        signs: Signs,
        feet: Cell,
        tick: u64,
    ) -> Option<Task> {
        h.stand(feet);
        let on = projection(h.origin, h.dir, feet);
        h.best = h.best.max(on);
        // A warped forest at the feet ends the heading: the endermen task takes over.
        if signs.feet_biome == Some("warped_forest") {
            self.heading_leg_end(w, h, feet, tick, "ended: a warped forest at the feet");
            h.over = true;
            self.note(
                tick,
                "heading ended: warped forest".to_owned(),
                format!(
                    "the feet's biome at {feet:?} after {} legs, {} turns; digs {}, placed {}, \
                     pickaxe uses {}",
                    h.legs, h.turns, h.digs, h.placed, h.uses_spent
                ),
            );
            return None;
        }
        // A forest known from blocks seen replaces the axis (and a valley): legs run to it.
        if let Some(col) = signs.forest
            && (h.forest != Some(col) || h.valley)
        {
            h.forest = Some(col);
            h.valley = false;
            self.heading_leg_end(w, h, feet, tick, "replaced: a warped forest is known");
            self.note(
                tick,
                format!("heading to the warped forest at column {col:?}"),
                "a forest known from warped blocks seen replaces the axis".to_owned(),
            );
        }
        // Else a soul sand valley known from soul blocks seen, once: endermen spawn there too.
        if h.forest.is_none()
            && let Some(col) = signs.valley.filter(|c| !h.valleys.contains(c))
        {
            h.forest = Some(col);
            h.valley = true;
            self.heading_leg_end(w, h, feet, tick, "replaced: a soul sand valley is known");
            self.note(
                tick,
                format!("heading to the soul sand valley at column {col:?}"),
                "soul blocks seen there; no warped forest known; the axis waits".to_owned(),
            );
        }
        // Restock in hand: on until enough, or no wall cell to dig.
        if let Some(t) = self.heading_restock(w, h, tick) {
            return Some(t);
        }
        // The leg in hand.
        if h.leg.is_some() {
            match self.heading_leg_look(w, h, feet, tick) {
                Look::Going => {
                    if let Some(t) = self.heading_way(w, h, feet, tick) {
                        return Some(t);
                    }
                    self.heading_fail(w, h, feet, tick, "no way on")?;
                }
                Look::Done => {
                    self.heading_leg_end(w, h, feet, tick, "done");
                    if let Some(col) = h
                        .forest
                        .filter(|_| h.leg.is_none() && feet_at_column(feet, h.forest))
                    {
                        if h.valley {
                            // The valley is crossed, not waited in: the axis goes on.
                            h.valleys.push(col);
                            h.forest = None;
                            h.valley = false;
                            self.note(
                                tick,
                                format!("soul sand valley reached at column {col:?}"),
                                "the axis goes on from here".to_owned(),
                            );
                        } else {
                            h.over = true;
                            self.note(
                                tick,
                                format!("heading ended: the warped forest at column {col:?}"),
                                format!(
                                    "its column's centre reached at {feet:?}; the endermen task takes                                      over"
                                ),
                            );
                            return None;
                        }
                    }
                }
                Look::Failed(why) => {
                    self.heading_fail(w, h, feet, tick, &why)?;
                }
            }
        }
        // Biome signs: crimson or basalt at the feet, or seen ahead, turn the axis once.
        if h.forest.is_none() {
            let feet_bad = signs
                .feet_biome
                .filter(|b| matches!(*b, "crimson_forest" | "basalt_deltas"));
            if let Some(b) = feet_bad
                && h.biome_turned.as_deref() != Some(b)
            {
                h.biome_turned = Some(b.to_owned());
                self.heading_turn(w, h, feet, tick, &format!("the feet's biome is {b}"))?;
            } else {
                let chunk = (feet.0 >> 4, feet.2 >> 4);
                let ahead: u32 = (1..=SIGNS_AHEAD)
                    .flat_map(|d| (-1..=1).map(move |s| (d, s)))
                    .map(|(d, s)| {
                        hostile_signs_in_column(
                            w,
                            chunk.0 + h.dir.0 * d + h.dir.1 * s,
                            chunk.1 + h.dir.1 * d + h.dir.0 * s,
                        )
                    })
                    .sum();
                if ahead >= SIGNS_TURN {
                    self.heading_turn(
                        w,
                        h,
                        feet,
                        tick,
                        &format!("{ahead} crimson or basalt signs seen ahead"),
                    )?;
                }
            }
        }
        // Under the keep: restock from the wall before the leg.
        if path::bridge_blocks(w) < KEEP && h.restock.is_none() && tick >= h.restock_after {
            h.restock = Some(Restock {
                filler0: path::bridge_blocks(w),
                uses0: pickaxe_uses_left(w),
                digs: 0,
                began: tick,
            });
            if let Some(t) = self.heading_restock(w, h, tick) {
                return Some(t);
            }
        }
        // New legs, a turn between each that has no way.
        for _ in 0..ATTEMPTS_PER_LOOK {
            self.heading_leg_start(w, h, feet, tick);
            if let Some(t) = self.heading_way(w, h, feet, tick) {
                return Some(t);
            }
            self.heading_fail(w, h, feet, tick, "no way on")?;
        }
        Some(Task::Idle)
    }

    /// The leg in hand as the planner looks again: its spend counted, its cells cut, and whether
    /// it is done, failed (the projection invariant, the budget) or going.
    fn heading_leg_look(&mut self, w: &World, h: &mut Heading, feet: Cell, tick: u64) -> Look {
        let dir = h.dir;
        let Some(leg) = h.leg.as_mut() else {
            return Look::Going;
        };
        // Digs: cells a stretch was handed that are open now.
        for (c, filler) in std::mem::take(&mut leg.pending) {
            if open(w, c) {
                leg.digs += 1;
                leg.gained += i64::from(filler);
            }
        }
        // The cells passed join the cut: a walk's route up to the feet, the tunnel's route to here.
        let here = leg.progress(dir, feet);
        let passed: Vec<Cell> = match leg.way {
            Way::Walk | Way::Far => leg
                .planned
                .iter()
                .filter(|c| leg.progress(dir, **c) <= here)
                .copied()
                .collect(),
            Way::Tunnel => match self.travel.route.iter().position(|c| *c == feet) {
                Some(i) => self.travel.route.iter().take(i + 1).copied().collect(),
                None => Vec::new(),
            },
        };
        for c in passed {
            h.cut.insert(c);
            h.cut.insert((c.0, c.1 + 1, c.2));
        }
        if here >= leg.mark.0 + PACE_GAIN {
            leg.mark = (here, tick);
        }
        if leg.done(dir, feet) {
            return Look::Done;
        }
        if tick.saturating_sub(leg.mark.1) > PACE_TICKS {
            return Look::Failed(format!(
                "NoProgress: under {PACE_GAIN} blocks gained in {PACE_TICKS} ticks"
            ));
        }
        if leg.digs > LEG_DIGS {
            return Look::Failed(format!(
                "over budget: {} digs, the leg's budget is {LEG_DIGS}",
                leg.digs
            ));
        }
        let gain = here - leg.progress(dir, leg.from);
        let placed = (leg.filler0 + leg.gained - path::bridge_blocks(w)).max(0);
        let spent = BUDGET_GRACE.max(gain);
        if f64::from(leg.digs) > DIGS_PER_M * f64::from(spent) {
            return Look::Failed(format!(
                "over budget: {} digs in {gain} m (over {DIGS_PER_M} a metre)",
                leg.digs
            ));
        }
        if placed as f64 > PLACED_PER_M * f64::from(spent) {
            return Look::Failed(format!(
                "over budget: {placed} blocks placed in {gain} m (over {PLACED_PER_M} a metre)"
            ));
        }
        Look::Going
    }

    /// A new leg from the feet: `LEG` ahead on the axis (its height clamped to `Y_LO`..`Y_HI`), or
    /// toward the forest known.
    fn heading_leg_start(&mut self, w: &World, h: &mut Heading, feet: Cell, tick: u64) {
        h.legs += 1;
        let y = feet.1.clamp(Y_LO, Y_HI);
        let forest = h.forest.map(|(cx, cz)| (cx * 16 + 8, y, cz * 16 + 8));
        let target = match forest {
            Some(f) => {
                let (dx, dz) = (f.0 - feet.0, f.2 - feet.2);
                let d = f64::from(dx).hypot(f64::from(dz));
                if d <= f64::from(LEG) {
                    f
                } else {
                    let k = f64::from(LEG) / d;
                    (
                        feet.0 + (f64::from(dx) * k).round() as i32,
                        y,
                        feet.2 + (f64::from(dz) * k).round() as i32,
                    )
                }
            }
            None => (feet.0 + h.dir.0 * LEG, y, feet.2 + h.dir.1 * LEG),
        };
        let mut leg = Leg {
            n: h.legs,
            from: feet,
            target,
            forest: forest.map(|_| target),
            way: Way::Walk,
            tries: 0,
            began: tick,
            tried_at: 0,
            mark: (0, tick),
            uses0: pickaxe_uses_left(w),
            filler0: path::bridge_blocks(w),
            digs: 0,
            gained: 0,
            pending: Vec::new(),
            planned: Vec::new(),
            wait_until: 0,
        };
        leg.mark = (leg.progress(h.dir, feet), tick);
        self.note(
            tick,
            format!("heading leg {} toward {target:?}", h.legs),
            format!(
                "on ({}, {}), best {} from {:?}; pickaxe uses {}, filler {}, {} cells cut; \
                 health {:?}, food {:?}",
                h.dir.0,
                h.dir.1,
                h.best,
                h.origin,
                leg.uses0,
                leg.filler0,
                h.cut.len(),
                w.player.health,
                w.player.food
            ),
        );
        h.leg = Some(leg);
    }

    /// The leg's way, from the one in hand on: a walk, the far walk, the tunnel. `None` when none
    /// has a way (the caller turns).
    fn heading_way(&mut self, w: &World, h: &mut Heading, feet: Cell, tick: u64) -> Option<Task> {
        let dir = h.dir;
        let blocked = h.behind(feet);
        let mut leg = h.leg.as_mut()?;
        let here = leg.progress(dir, feet);
        // A walk that ended short: tried again while it gains, else the next way.
        if matches!(leg.way, Way::Walk | Way::Far)
            && leg.tries >= WAY_TRIES
            && here < leg.tried_at + PACE_GAIN
        {
            leg.way = leg.way.next()?;
            leg.tries = 0;
        }
        if tick < leg.wait_until {
            return Some(Task::Idle);
        }
        let mut refusals: Vec<String> = Vec::new();
        loop {
            leg.tries += 1;
            leg.tried_at = here;
            let goal = leg.goal(dir);
            match leg.way {
                Way::Walk | Way::Far => {
                    let far = leg.way == Way::Far;
                    let mut walk = if far {
                        let mut walk = Walk::far_to(leg.target, goal, tick);
                        walk.more.keep = Some(KEEP);
                        walk
                    } else {
                        Walk::new(leg.target, goal, crate::walk::Arrive::Goal, tick)
                    };
                    walk.more.blocked = blocked.clone();
                    let found = walk.route_now(w, tick) && walk.route == Some(path::Status::Found);
                    self.expanded += walk.expanded;
                    let bridges = i64::try_from(walk.bridges_ahead()).unwrap_or(i64::MAX);
                    let spare = path::bridge_blocks(w) - KEEP;
                    // A route with no bridge is taken whatever is held; one that bridges keeps
                    // the keep.
                    if found && (bridges == 0 || bridges <= spare) {
                        leg.planned = walk.waypoints.clone().unwrap_or_default();
                        let cells = leg.planned.len();
                        let n = leg.n;
                        self.walks += 1;
                        self.note(
                            tick,
                            format!("heading leg {n} by {}", leg.way.name()),
                            format!(
                                "a route of {cells} cells, {bridges} to bridge, {} expansions, \
                                 {} cut cells refused",
                                walk.expanded,
                                blocked.len()
                            ),
                        );
                        return Some(Task::Explore(walk));
                    }
                    refusals.push(if found {
                        format!(
                            "{}: {bridges} to bridge over the keep ({spare} spare)",
                            leg.way.name()
                        )
                    } else {
                        format!("{}: {:?}", leg.way.name(), walk.route)
                    });
                }
                Way::Tunnel => {
                    let left = pickaxe_uses_left(w);
                    let reserve = pickaxe_reserve(self);
                    let need = (TUNNEL_DIGS_PER_M * (LEG - here.max(0)).max(LEG_REACH))
                        .min(i32::try_from(LEG_DIGS).unwrap_or(i32::MAX));
                    if left > reserve + need {
                        let target = leg.target;
                        let n = leg.n;
                        let task = self.heading_tunnel(
                            w,
                            target,
                            blocked.clone(),
                            ("the heading", &format!("heading leg {n}")),
                            tick,
                        );
                        if let Some(Task::Stairs(st)) = &task
                            && let Some(leg) = h.leg.as_mut()
                        {
                            leg.pending = st
                                .route
                                .windows(2)
                                .filter_map(|p| tunnel::opened(p[0], p[1]))
                                .flat_map(|(cells, _)| cells)
                                .filter(|c| !open(w, *c))
                                .map(|c| {
                                    (c, drops_filler(w.block_at(BlockPos::new(c.0, c.1, c.2))))
                                })
                                .collect();
                        }
                        if task.is_some() {
                            return task;
                        }
                        refusals.push("tunnel: no route".to_owned());
                    } else {
                        refusals.push(format!(
                            "tunnel: {left} pickaxe uses, the reserve {reserve} and {need} for \
                             the leg"
                        ));
                    }
                }
            }
            let leg_now = h.leg.as_mut()?;
            match leg_now.way.next() {
                Some(next) => {
                    leg_now.way = next;
                    leg_now.tries = 0;
                }
                None => {
                    let n = leg_now.n;
                    // The target's column not loaded yet (a bot just through a portal, a turn
                    // toward columns still arriving): no way is not known yet. Wait, with the
                    // ways from the first, for at most `LOAD_WAIT` ticks of the leg.
                    let t = leg_now.target;
                    let loaded = w
                        .columns
                        .contains_key(&BlockPos::new(t.0, t.1, t.2).chunk());
                    if !loaded && tick.saturating_sub(leg_now.began) < LOAD_WAIT {
                        if leg_now.wait_until == 0 {
                            self.note(
                                tick,
                                format!("heading leg {n} waits for its target's column"),
                                format!("{t:?} not loaded; {}", refusals.join("; ")),
                            );
                        }
                        leg_now.way = Way::Walk;
                        leg_now.tries = 0;
                        leg_now.wait_until = tick + LOAD_EVERY;
                        return Some(Task::Idle);
                    }
                    // Paced: was there a way only back over the cut? One far search, laying
                    // nothing (a walk back is what the cut refuses), to say so.
                    if !blocked.is_empty() {
                        let goal = leg_now.goal(dir);
                        let mut walk = Walk::far_to(leg_now.target, goal, tick);
                        walk.more.keep = Some(i64::MAX);
                        let found =
                            walk.route_now(w, tick) && walk.route == Some(path::Status::Found);
                        let back = walk
                            .waypoints
                            .as_ref()
                            .map_or(0, |wps| wps.iter().filter(|c| blocked.contains(c)).count());
                        // Only a route the cut alone refused: over cut cells.
                        if found && back > 0 {
                            self.note(
                                tick,
                                format!("heading leg {n} refused for cut (paced)"),
                                format!(
                                    "the only route found runs back over {back} cells this \
                                     heading already cut"
                                ),
                            );
                        }
                        self.expanded += walk.expanded;
                    }
                    self.note(
                        tick,
                        format!("heading leg {n} has no way"),
                        refusals.join("; "),
                    );
                    return None;
                }
            }
            leg = h.leg.as_mut()?;
        }
    }

    /// The leg in hand ends (done, failed or cut short): its spend per metre in the ledger, and
    /// into the heading's totals.
    fn heading_leg_end(&mut self, w: &World, h: &mut Heading, feet: Cell, tick: u64, how: &str) {
        let Some(leg) = h.leg.take() else {
            return;
        };
        let gain = leg.progress(h.dir, feet) - leg.progress(h.dir, leg.from);
        let uses = (leg.uses0 - pickaxe_uses_left(w)).max(0);
        let placed = (leg.filler0 + leg.gained - path::bridge_blocks(w)).max(0);
        h.digs += leg.digs;
        h.placed += placed;
        h.uses_spent += uses;
        if gain >= RECHOOSE_GAIN || h.best >= RECHOOSE_GAIN {
            h.idle_turns = 0;
            h.failed.clear();
        }
        self.note(
            tick,
            format!("heading leg {} {how}", leg.n),
            format!(
                "by {} in {} ticks: {gain} m; digs {} ({:.2}/m), placed {placed} ({:.2}/m), \
                 pickaxe uses {uses} ({:.2}/m); uses left {}, filler {}, best {} on ({}, {})",
                leg.way.name(),
                tick.saturating_sub(leg.began),
                leg.digs,
                per_m(f64::from(leg.digs), gain),
                per_m(placed as f64, gain),
                per_m(f64::from(uses), gain),
                pickaxe_uses_left(w),
                path::bridge_blocks(w),
                h.best,
                h.dir.0,
                h.dir.1
            ),
        );
    }

    /// A leg failed: its end noted, and a quarter turn. `None` when the heading turned out.
    fn heading_fail(
        &mut self,
        w: &World,
        h: &mut Heading,
        feet: Cell,
        tick: u64,
        why: &str,
    ) -> Option<()> {
        h.stalls += 1;
        self.heading_leg_end(w, h, feet, tick, &format!("failed ({why})"));
        self.heading_turn(w, h, feet, tick, why)
    }

    /// A quarter turn (or, after `IDLE_TURNS` without a gain, a fresh choice): a fresh projection
    /// from the feet, the cut kept. `None`, with the heading over, when it has turned out.
    fn heading_turn(
        &mut self,
        w: &World,
        h: &mut Heading,
        feet: Cell,
        tick: u64,
        why: &str,
    ) -> Option<()> {
        if h.leg.is_some() {
            self.heading_leg_end(w, h, feet, tick, &format!("cut short ({why})"));
        }
        let was = h.dir;
        if h.best < RECHOOSE_GAIN {
            h.idle_turns += 1;
            if !h.failed.contains(&was) {
                h.failed.push(was);
            }
        } else {
            h.idle_turns = 0;
            h.failed.clear();
        }
        h.turns += 1;
        let seen = seen_columns(w);
        if h.idle_turns > IDLE_TURNS {
            h.choices += 1;
            let avoid = std::mem::take(&mut h.failed);
            let chosen = (h.choices < MAX_CHOICES)
                .then(|| choose_dir(feet, &seen, &soul_columns(w, &seen), &[was]))
                .flatten();
            let Some((dir, reason)) = chosen else {
                h.over = true;
                self.note(
                    tick,
                    "heading turned out".to_owned(),
                    format!(
                        "{} choices, {} turns, {} legs; no way on {avoid:?} ({why}); digs {}, \
                         placed {}, pickaxe uses {}",
                        h.choices, h.turns, h.legs, h.digs, h.placed, h.uses_spent
                    ),
                );
                return None;
            };
            h.idle_turns = 0;
            h.dir = dir;
            h.reason = reason.clone();
            self.note(
                tick,
                format!("heading re-chosen: ({}, {})", dir.0, dir.1),
                format!(
                    "{IDLE_TURNS} turns with none gaining {RECHOOSE_GAIN} ({avoid:?} failed); \
                     {reason}"
                ),
            );
        } else {
            let (dir, side) = turn_dir(was, (feet.0 >> 4, feet.2 >> 4), &seen, h.side);
            h.dir = dir;
            h.side = side;
            self.note(
                tick,
                format!("heading turned to ({}, {})", dir.0, dir.1),
                format!(
                    "from ({}, {}) after {why}; best there {}; {} turns without {RECHOOSE_GAIN} \
                     gained",
                    was.0, was.1, h.best, h.idle_turns
                ),
            );
        }
        h.origin = feet;
        h.best = 0;
        Some(())
    }

    /// The restock in hand: the next wall cell dug, or its end noted (filler back to `RESTOCK_TO`,
    /// `RESTOCK_DIGS` dug, or no cell to dig).
    fn heading_restock(&mut self, w: &World, h: &mut Heading, tick: u64) -> Option<Task> {
        let r = h.restock.as_mut()?;
        let filler = path::bridge_blocks(w);
        let cell = (filler < RESTOCK_TO && r.digs < RESTOCK_DIGS)
            .then(|| restock_cells(w).into_iter().next())
            .flatten();
        if let Some(c) = cell {
            r.digs += 1;
            let n = r.digs;
            self.note(
                tick,
                format!("restock dig {c:?}"),
                format!("filler {filler} under the keep {KEEP}; dig {n} of this restock"),
            );
            return Some(Task::Dig(crate::dig::Dig::new(w, c, tick)));
        }
        let r = h.restock.take()?;
        if filler < KEEP {
            h.restock_after = tick + PACE_TICKS;
        }
        self.note(
            tick,
            "restock over".to_owned(),
            format!(
                "{} blocks gained in {} digs, {} pickaxe uses spent, {} ticks; filler {filler}{}",
                filler - r.filler0,
                r.digs,
                (r.uses0 - pickaxe_uses_left(w)).max(0),
                tick.saturating_sub(r.began),
                if filler < KEEP {
                    " (no wall cell to dig: still under the keep)"
                } else {
                    ""
                }
            ),
        );
        None
    }
}
