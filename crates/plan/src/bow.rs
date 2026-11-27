//! The bow line (issue 20, docs/problems/end-bow.md): the End's open crystals shot from the ground
//! with a bow, the caged ones left to E4's pillar (`spikes`).
//!
//! **The pure part**: the arrow's flight from the jar (`AbstractArrow.tick`: a move, then the
//! velocity times 0.99, then 0.05 off its y), the pitch that carries it through a point (the low
//! arc, by bisection), the hit test against a crystal's box grown by the jar's margin, the
//! blocks along the flight, and the stands round a spike. No clock, no randomness: the spread
//! (`triangle(0, 0.0172275)` an axis) is the server's, and a miss is counted, not modelled.
//!
//! **The shot** (`Plan::bow_task`, `Plan::bow_step`): the bow to hand, standing still on the
//! ground, `use_item` along the aim, the draw held `DRAW_HOLD` ticks (full power at 20), then
//! `player_action` 5 (release) once the look the server last heard is the aim; then the flight
//! waited for and the crystal's removal read from the entities the server sends.

use data::{EntityKind, ItemKind};
use physics::Controls;
use world::{Act, World};

use crate::hold::{self, Hold};
use crate::path::{self, Cell, Goal};
use crate::walk::{Arrive, Walk};
use crate::{Plan, Task};

/// The arrow's speed at full power (`BowItem.releaseUsing`: power x 3.0).
pub const SPEED: f64 = 3.0;
/// Taken off the arrow's y velocity each tick, after the drag (`AbstractArrow.getDefaultGravity`).
pub const GRAVITY: f64 = 0.05;
/// The arrow's velocity is multiplied by this each tick in air (`AbstractArrow.INERTIA`).
pub const DRAG: f64 = 0.99;
/// Ticks of draw for full power (`BowItem.getPowerForTime`: (f^2 + 2f) / 3 with f = t / 20).
pub const FULL_DRAW: u64 = 20;
/// Ticks the draw is held before the release: full power, and 4 for the server hearing the use a
/// tick or two after we send it.
pub const DRAW_HOLD: u64 = FULL_DRAW + 4;
/// The arrow starts this far under the eye (`AbstractArrow(type, owner, ...)`: `getEyeY() - 0.1`).
pub const START_DROP: f64 = 0.1;
/// An entity's box is grown by this for an arrow 8 or more ticks old (`ProjectileUtil.computeMargin`:
/// `clamp((tickCount - 2) / 20, 0, 0.3)`); younger arrows get less, so the test uses the margin
/// for its own tick.
pub const MARGIN: f64 = 0.3;
/// An end crystal's box: 2 wide, 2 tall over its position (`EntityType.END_CRYSTAL`).
pub const CRYSTAL_HALF: f64 = 1.0;
pub const CRYSTAL_HEIGHT: f64 = 2.0;
/// No shot from nearer than this to the crystal: its blast (power 6) hurts within 12
/// (`ServerExplosion`: impact `1 - distance / 12`), and the crystal's blocks fly.
pub const SAFE_RANGE: f64 = 16.0;
/// No shot from farther than this: the spread's deviation is 0.0070 of the range an axis (0.34
/// at 48), against the box's half width 1.3 with the margin; and the entity is sent within 112.
pub const MAX_RANGE: f64 = 48.0;
/// The flight followed at most this many ticks (a low shot of 48 takes about 20; a high arc onto
/// a tall spike up to about 70).
pub const FLIGHT_TICKS: usize = 120;
/// Samples a block along each tick's segment when looking for a block in the way.
const STEP: f64 = 0.1;

/// A point, a cell.
type P = [f64; 3];

/// The arrow's starting point for a body whose feet are at `feet`.
pub fn start(feet: P) -> P {
    [feet[0], feet[1] + physics::EYE_HEIGHT - START_DROP, feet[2]]
}

/// The unit vector along a Notchian yaw and pitch, as `Projectile.shootFromRotation` builds it.
pub fn direction(yaw: f64, pitch: f64) -> P {
    let (y, p) = (yaw.to_radians(), pitch.to_radians());
    [-y.sin() * p.cos(), -p.sin(), y.cos() * p.cos()]
}

/// The Notchian yaw from `a` toward `b` (south 0, west +90).
pub fn yaw_toward(a: P, b: P) -> f64 {
    (-(b[0] - a[0])).atan2(b[2] - a[2]).to_degrees()
}

/// The arrow's positions at the start of each tick from `from` with velocity `v`, `n` ticks.
pub fn fly(from: P, v: P, n: usize) -> Vec<P> {
    let mut out = Vec::with_capacity(n + 1);
    let (mut p, mut v) = (from, v);
    out.push(p);
    for _ in 0..n {
        p = [p[0] + v[0], p[1] + v[1], p[2] + v[2]];
        v = [v[0] * DRAG, v[1] * DRAG - GRAVITY, v[2] * DRAG];
        out.push(p);
    }
    out
}

/// Where along the segment `a` to `b` (0 to 1) it first meets the box `lo`..`hi` (the slab test),
/// if it does; 0 when `a` is inside.
pub fn segment_box(a: P, b: P, lo: P, hi: P) -> Option<f64> {
    let (mut t0, mut t1) = (0.0f64, 1.0f64);
    for i in 0..3 {
        let d = b[i] - a[i];
        if d.abs() < 1e-12 {
            if a[i] < lo[i] || a[i] > hi[i] {
                return None;
            }
            continue;
        }
        let (mut u, mut w) = ((lo[i] - a[i]) / d, (hi[i] - a[i]) / d);
        if u > w {
            std::mem::swap(&mut u, &mut w);
        }
        t0 = t0.max(u);
        t1 = t1.min(w);
        if t0 > t1 {
            return None;
        }
    }
    Some(t0)
}

/// A crystal's box at `pos` (its feet), grown by `margin`.
pub fn crystal_box(pos: P, margin: f64) -> (P, P) {
    let h = CRYSTAL_HALF + margin;
    (
        [pos[0] - h, pos[1] - margin, pos[2] - h],
        [pos[0] + h, pos[1] + CRYSTAL_HEIGHT + margin, pos[2] + h],
    )
}

/// The middle of a crystal's box, the point aimed at.
pub fn crystal_middle(pos: P) -> P {
    [pos[0], pos[1] + CRYSTAL_HEIGHT / 2.0, pos[2]]
}

/// The margin the jar grows a box by for an arrow `age` ticks old.
fn margin_at(age: usize) -> f64 {
    let a = f64::from(u32::try_from(age).unwrap_or(u32::MAX));
    ((a - 2.0) / 20.0).clamp(0.0, MARGIN)
}

/// The height the arrow has, in the vertical plane, when it has gone `flat` across, shot at
/// `pitch` (degrees, Notchian: down positive) at `SPEED`; `None` if it never goes that far in
/// `FLIGHT_TICKS`. Relative to its start.
pub fn height_at(flat: f64, pitch: f64) -> Option<f64> {
    let p = pitch.to_radians();
    let (mut h, mut y) = (0.0f64, 0.0f64);
    let (mut vh, mut vy) = (SPEED * p.cos(), -SPEED * p.sin());
    for _ in 0..FLIGHT_TICKS {
        let (nh, ny) = (h + vh, y + vy);
        if nh >= flat {
            let t = if vh > 0.0 { (flat - h) / vh } else { 0.0 };
            return Some(y + t * vy);
        }
        (h, y) = (nh, ny);
        vh *= DRAG;
        vy = vy * DRAG - GRAVITY;
    }
    None
}

/// The pitch (degrees, Notchian) whose flight passes `rise` over its start at `flat` across: the
/// low arc. Scanned from 60 down to 80 up a degree at a time for the first that reaches the
/// height, then bisected. `None` when no pitch reaches it.
pub fn solve_pitch(flat: f64, rise: f64) -> Option<f64> {
    let reaches = |p: f64| height_at(flat, p).is_some_and(|y| y >= rise);
    let mut below = 60.0f64;
    if reaches(below) {
        return Some(below);
    }
    let mut p = below;
    while p > -80.0 {
        let next = p - 1.0;
        if reaches(next) {
            let mut above = next;
            for _ in 0..40 {
                let mid = (below + above) / 2.0;
                if reaches(mid) {
                    above = mid;
                } else {
                    below = mid;
                }
            }
            return Some(above);
        }
        below = next;
        p = next;
    }
    None
}

/// The high arc's pitch through the same point: scanned from 88 up down a degree at a time for
/// the first that reaches the height, then bisected toward the steep side. It comes down onto
/// the crystal from above, over its spike's top (race 46078921264: every low arc from the ground
/// at a tall spike met the obsidian's edge under the box). `None` when none reaches it.
pub fn solve_pitch_high(flat: f64, rise: f64) -> Option<f64> {
    let reaches = |p: f64| height_at(flat, p).is_some_and(|y| y >= rise);
    let mut steep = -88.0f64;
    if reaches(steep) {
        return Some(steep);
    }
    let mut p = steep;
    while p < 60.0 {
        let next = p + 1.0;
        if reaches(next) {
            let mut lower = next;
            for _ in 0..40 {
                let mid = (steep + lower) / 2.0;
                if reaches(mid) {
                    lower = mid;
                } else {
                    steep = mid;
                }
            }
            return Some(lower);
        }
        steep = next;
        p = next;
    }
    None
}

/// Why no shot.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Miss {
    /// Nearer than `SAFE_RANGE`.
    Near,
    /// Farther than `MAX_RANGE`.
    Far,
    /// No pitch reaches it.
    NoArc,
    /// A block in the flight's way, at this cell.
    Blocked(Cell),
    /// The flight passes the box (the solver's interpolation off).
    Wide,
}

/// A shot's aim.
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Aim {
    pub yaw: f64,
    pub pitch: f32,
    /// The tick of the flight that meets the box.
    pub ticks: u32,
    /// Feet to the crystal's position.
    pub range: f64,
}

/// The aim from feet at `feet` at the crystal whose position is `crystal`: the yaw to its middle,
/// the low arc's pitch, then the whole flight checked: it meets the box (grown by the margin for
/// each tick's age) before any cell `blocked` says no arrow passes. Pure.
pub fn aim(feet: P, crystal: P, blocked: &mut impl FnMut(Cell) -> bool) -> Result<Aim, Miss> {
    let from = start(feet);
    let mid = crystal_middle(crystal);
    let range = (0..3)
        .map(|i| (crystal[i] - feet[i]).powi(2))
        .sum::<f64>()
        .sqrt();
    if range < SAFE_RANGE {
        return Err(Miss::Near);
    }
    if range > MAX_RANGE {
        return Err(Miss::Far);
    }
    let flat = (mid[0] - from[0]).hypot(mid[2] - from[2]);
    let low = solve_pitch(flat, mid[1] - from[1]).ok_or(Miss::NoArc)?;
    match fly_check(from, crystal, flat, range, low, blocked) {
        Err(Miss::Blocked(c)) => match solve_pitch_high(flat, mid[1] - from[1]) {
            Some(high) if (high - low).abs() > 1.0 => {
                fly_check(from, crystal, flat, range, high, blocked).map_err(|_| Miss::Blocked(c))
            }
            _ => Err(Miss::Blocked(c)),
        },
        other => other,
    }
}

/// The flight along `pitch` from `from` toward the crystal's middle, checked against its box
/// and the cells `blocked` says no arrow passes.
fn fly_check(
    from: P,
    crystal: P,
    flat: f64,
    range: f64,
    pitch: f64,
    blocked: &mut impl FnMut(Cell) -> bool,
) -> Result<Aim, Miss> {
    let mid = crystal_middle(crystal);
    let yaw = yaw_toward(from, mid);
    // The pitch as it is sent (f32), so the check flies what the server will.
    #[allow(clippy::cast_possible_truncation)]
    let sent = pitch as f32;
    let d = direction(yaw, f64::from(sent));
    let v = [d[0] * SPEED, d[1] * SPEED, d[2] * SPEED];
    let points = fly(from, v, FLIGHT_TICKS);
    for (i, pair) in points.windows(2).enumerate() {
        let (a, b) = (pair[0], pair[1]);
        let (lo, hi) = crystal_box(crystal, margin_at(i));
        let hit = segment_box(a, b, lo, hi);
        let end = hit.unwrap_or(1.0);
        let len = (0..3).map(|k| (b[k] - a[k]).powi(2)).sum::<f64>().sqrt();
        let samples = (len / STEP).ceil().max(1.0);
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let n = samples as u32;
        for s in 0..n {
            let t = f64::from(s) / samples;
            if t >= end {
                break;
            }
            let q = [
                a[0] + (b[0] - a[0]) * t,
                a[1] + (b[1] - a[1]) * t,
                a[2] + (b[2] - a[2]) * t,
            ];
            let c = path::feet_cell(q);
            if blocked(c) {
                return Err(Miss::Blocked(c));
            }
        }
        if hit.is_some() {
            return Ok(Aim {
                yaw,
                pitch: sent,
                ticks: u32::try_from(i + 1).unwrap_or(u32::MAX),
                range,
            });
        }
        // Past the crystal across: it went wide (never by height alone: a shot from the
        // ground starts under the crystal).
        if (b[0] - from[0]).hypot(b[2] - from[2]) > flat + 2.0 * (CRYSTAL_HALF + MARGIN) {
            break;
        }
    }
    Err(Miss::Wide)
}

/// The standing columns (x, z) round a spike for its crystal: on rings `SAFE_RANGE` + 2 to 40
/// from the spike's centre, every 15 degrees within 90 of the way toward `toward` (the island's
/// middle), the way toward first, then nearer rings first. Pure; the caller finds the ground.
pub fn stand_columns(centre: [i32; 2], toward: [f64; 2]) -> Vec<[i32; 2]> {
    let (cx, cz) = (f64::from(centre[0]) + 0.5, f64::from(centre[1]) + 0.5);
    let base = (toward[1] - cz).atan2(toward[0] - cx);
    let mut out: Vec<[i32; 2]> = Vec::new();
    for k in 0..13 {
        // 0, +15, -15, +30, -30, ... up to 90.
        let step = f64::from((k + 1) / 2) * 15.0;
        let off = if k % 2 == 1 { step } else { -step };
        for r in [18.0, 22.0, 26.0, 30.0, 34.0, 40.0] {
            let a = base + f64::to_radians(off);
            #[allow(clippy::cast_possible_truncation)]
            let c = [
                (cx + r * a.cos()).floor() as i32,
                (cz + r * a.sin()).floor() as i32,
            ];
            if !out.contains(&c) {
                out.push(c);
            }
        }
    }
    out
}

/// The angle in degrees between two yaws, 0 to 180.
pub fn yaw_gap(a: f64, b: f64) -> f64 {
    let d = (a - b).rem_euclid(360.0);
    d.min(360.0 - d)
}

// ------------------------------------------------------------------ the shot

/// Where a shot stands.
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum Stage {
    /// The bow to hand.
    Hold,
    /// Standing still on the ground, then the use.
    Settle,
    /// The use sent at `since`: drawing.
    Draw { since: u64 },
    /// Released at `since`: the flight waited for.
    Flight { since: u64 },
}

/// One shot at one crystal.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Shot {
    pub target: i32,
    pub at: [f64; 3],
    pub aim: Aim,
    pub stage: Stage,
    pub begun: u64,
    /// The server's tick at the release, and our tick when the crystal left the entities after
    /// it: a hit is its blast at its spot after the release, never an untracking.
    #[serde(default)]
    pub released_server: u64,
    #[serde(default)]
    pub gone_at: Option<u64>,
}

/// The bow line's state, one field of `Plan` (`Plan::bow`).
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct BowState {
    pub doing: Option<Shot>,
    /// The last tick `bow_step` drove the shot (a gap means another row took the controls).
    pub driven_at: u64,
    pub shots: u32,
    pub hits: u32,
    /// Per crystal, the shots that missed and the walks to stands for it.
    pub misses: Vec<(i32, u32)>,
    pub walks: Vec<(i32, u32)>,
    /// Crystals left to the pillar: missed `MISSES` times, no stand, or caged.
    pub refused: Vec<i32>,
    /// The stand a walk last went to, and for which crystal.
    pub stand: Option<(Cell, i32)>,
    pub noted_caged: Vec<i32>,
    /// Crystals with no stand walked to, and when they are asked again.
    #[serde(default)]
    pub no_stand: Vec<(i32, u64)>,
}

/// Shots that miss one crystal before it is left to the pillar.
pub const MISSES: u32 = 4;
/// A crystal with no stand to walk to is asked again after this.
const NO_STAND_WAIT: u64 = 400;
/// The feet within this of the fountain (flat) are on the island for the bow (the island's
/// radius is about 64 on three axes, 72 on +x: E1's probe; the spikes stand 42 out).
const ON_ISLAND: f64 = 60.0;

fn flat_to_fountain(p: P) -> f64 {
    p[0].hypot(p[2])
}

/// Walks to stands for one crystal before it is left to the pillar.
pub const STAND_WALKS: u32 = 6;
/// Stand columns tried for a route each choice.
const STAND_TRIES: usize = 4;
/// Ticks a draw waits for the look the server heard to be the aim before it lets go anyway.
const LOOK_WAIT: u64 = 60;
/// The look the server heard is the aim within this (degrees).
const LOOK_NEAR: f64 = 0.25;
/// The flight is waited for this many ticks past its predicted hit.
const FLIGHT_SPARE: u64 = 20;
/// Ticks a crystal gone from the entities waits for its blast's packet (parity e07: removed at
/// t 98, the blast heard at t 100).
const BLAST_WAIT: u64 = 10;
/// Standing still: the horizontal speed under this a tick.
const STILL: f64 = 0.003;

fn count(v: &[(i32, u32)], id: i32) -> u32 {
    v.iter().find(|x| x.0 == id).map_or(0, |x| x.1)
}

fn bump(v: &mut Vec<(i32, u32)>, id: i32) -> u32 {
    if let Some(x) = v.iter_mut().find(|x| x.0 == id) {
        x.1 += 1;
        x.1
    } else {
        v.push((id, 1));
        1
    }
}

/// Arrows held.
pub fn arrows(w: &World) -> i32 {
    crate::craft::count(w, ItemKind::Arrow)
}

/// A bow and an arrow held.
pub fn armed(w: &World) -> bool {
    crate::craft::count(w, ItemKind::Bow) >= 1 && arrows(w) >= 1
}

fn in_end(w: &World) -> bool {
    w.dimension
        .as_ref()
        .is_some_and(|d| d.name == crate::dragon::THE_END)
}

/// Whether an arrow cannot pass a cell: a full or partial collision box (iron bars among them),
/// water, or not loaded.
fn blocks_arrow(cells: &mut path::Cells, c: Cell) -> bool {
    !matches!(cells.at(c), path::Class::Clear | path::Class::Lava)
}

/// The ground's standing cell in a column (x, z) on the island: the highest standable cell
/// from y 75 down to 45 with end stone under it.
fn ground_in(w: &World, cells: &mut path::Cells, x: i32, z: i32) -> Option<Cell> {
    (45..=75).rev().map(|y| (x, y, z)).find(|c| {
        cells.standable(*c)
            && w.block_at(world::BlockPos::new(c.0, c.1 - 1, c.2))
                == Some(data::BlockKind::EndStone)
    })
}

fn centre_of(c: Cell) -> P {
    [f64::from(c.0) + 0.5, f64::from(c.1), f64::from(c.2) + 0.5]
}

impl Plan {
    fn bow_note_miss(&mut self, tick: u64, id: i32, why: String) {
        let n = bump(&mut self.bow.misses, id);
        if n >= MISSES && !self.bow.refused.contains(&id) {
            self.bow.refused.push(id);
            self.note(
                tick,
                format!("bow: crystal {id} left to the pillar"),
                format!("{n} shots missed; {why}"),
            );
        }
    }

    /// The crystals the bow may take, nearest first: alive, sent by the server (no X-ray), not on
    /// a caged spike (iron bars stop arrows), not refused.
    fn bow_targets(&mut self, w: &World, tick: u64) -> Vec<(i32, P)> {
        let Some(body) = w.body else {
            return Vec::new();
        };
        let mut v: Vec<(f64, i32, P)> = Vec::new();
        let ids: Vec<(i32, P)> = w
            .entities
            .iter()
            .filter(|(_, e)| !e.dead && e.kind == EntityKind::EndCrystal)
            .map(|(id, e)| (*id, e.pos))
            .collect();
        for (id, pos) in ids {
            if self.bow.refused.contains(&id) {
                continue;
            }
            let spike = crate::spikes::spike_for(w, &self.spikes.geography, pos);
            if spike.as_ref().is_some_and(|s| s.caged) {
                if !self.bow.noted_caged.contains(&id) {
                    self.bow.noted_caged.push(id);
                    self.note(
                        tick,
                        format!("bow: crystal {id} caged, left to the pillar"),
                        format!("at {pos:?}; iron bars stop arrows"),
                    );
                }
                continue;
            }
            let d = (0..3).map(|i| (pos[i] - body.pos[i]).powi(2)).sum::<f64>();
            v.push((d, id, pos));
        }
        v.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
        v.into_iter().map(|(_, id, p)| (id, p)).collect()
    }

    // >>> E8 (issue 20, the bow line): the crystals' hand-off asks the bow first.
    /// In the End with a bow and arrows: a shot at the nearest open crystal from where the bot
    /// stands, else a walk to a stand that has one. `None` when the bow has nothing to do (no
    /// bow, no arrows, every open crystal gone or left to the pillar): the pillar's turn.
    pub fn bow_task(&mut self, w: &World, tick: u64) -> Option<Task> {
        if !in_end(w) || !armed(w) {
            self.spikes.bow_keep.clear();
            return None;
        }
        if self.bow.doing.is_some() {
            self.retry_at = tick + 1;
            return Some(Task::Idle);
        }
        let body = w.body?;
        if !body.on_ground || body.in_water {
            return None;
        }
        let danger = self
            .table
            .as_ref()
            .map_or(8.0, |t| t.survival.danger_health);
        if w.player.health.is_some_and(|h| h <= danger) {
            return None;
        }
        let targets = self.bow_targets(w, tick);
        // The pillar keeps off the crystals the bow may still take (a fenced filter in
        // `spikes::next`): set every ask, emptied when the bow is out of arrows.
        self.spikes.bow_keep = targets.iter().map(|t| t.0).collect();
        if targets.is_empty() {
            return None;
        }
        let mut cells = path::Cells::new(w);
        let mut blocked = |c: Cell| blocks_arrow(&mut cells, c);
        // A shot from here, at any of them.
        let mut first_miss = None;
        for (id, pos) in &targets {
            match aim(body.pos, *pos, &mut blocked) {
                Ok(a) => {
                    self.bow.doing = Some(Shot {
                        target: *id,
                        at: *pos,
                        aim: a,
                        stage: Stage::Hold,
                        begun: tick,
                        released_server: 0,
                        gone_at: None,
                    });
                    self.bow.driven_at = tick;
                    self.note(
                        tick,
                        format!("bow: aim at crystal {id}"),
                        format!(
                            "from {:?}, {:.1} off, pitch {:.1}, yaw {:.1}, {} ticks of flight; {} arrows",
                            path::feet_cell(body.pos),
                            a.range,
                            a.pitch,
                            a.yaw,
                            a.ticks,
                            arrows(w)
                        ),
                    );
                    self.retry_at = tick + 1;
                    return Some(Task::Idle);
                }
                Err(m) => {
                    first_miss.get_or_insert((*id, m));
                }
            }
        }
        // Off the island (the platform, the bridge: race 46072716511 lane 3 asked for stands from
        // (77, 50, 0) and gave six crystals up in 2,000 ticks): E3's way onto it first.
        if flat_to_fountain(body.pos) > ON_ISLAND {
            return None;
        }
        // A walk to a stand, for the nearest crystals still to be shot that have one.
        let feet = path::feet_cell(body.pos);
        for (id, pos) in targets.iter().copied().take(3) {
            if count(&self.bow.walks, id) >= STAND_WALKS {
                if !self.bow.refused.contains(&id) {
                    self.bow.refused.push(id);
                    self.note(
                        tick,
                        format!("bow: crystal {id} left to the pillar"),
                        format!("{STAND_WALKS} walks to stands; from here {first_miss:?}"),
                    );
                }
                continue;
            }
            if self
                .bow
                .no_stand
                .iter()
                .any(|(c, until)| *c == id && tick < *until)
            {
                continue;
            }
            let spike = crate::spikes::spike_for(w, &self.spikes.geography, pos);
            #[allow(clippy::cast_possible_truncation)]
            let centre = spike.map_or([pos[0].floor() as i32, pos[2].floor() as i32], |s| s.centre);
            let mut stands: Vec<(i32, Cell)> = Vec::new();
            let mut cells = path::Cells::new(w);
            for col in stand_columns(centre, [0.0, 0.0]) {
                let Some(c) = ground_in(w, &mut cells, col[0], col[1]) else {
                    continue;
                };
                let mut blocked = |q: Cell| blocks_arrow(&mut cells, q);
                if aim(centre_of(c), pos, &mut blocked).is_err() {
                    continue;
                }
                let d = (c.0 - feet.0).abs() + (c.2 - feet.2).abs() + 2 * (c.1 - feet.1).abs();
                stands.push((d, c));
            }
            stands.sort();
            let found = stands.len();
            for (_, c) in stands.into_iter().take(STAND_TRIES) {
                let mut walk = Walk::new(c, Goal::Block { pos: c }, Arrive::Goal, tick);
                if walk.route_now(w, tick) && walk.route == Some(path::Status::Found) {
                    let n = bump(&mut self.bow.walks, id);
                    self.bow.stand = Some((c, id));
                    self.walks += 1;
                    self.note(
                        tick,
                        format!("bow: to the stand at {c:?} for crystal {id}"),
                        format!(
                            "from {feet:?}; walk {n} of {STAND_WALKS}; from here {first_miss:?}"
                        ),
                    );
                    return Some(Task::Explore(walk));
                }
                self.expanded += walk.expanded;
            }
            // None now: asked again later, the pillar kept off it meanwhile.
            self.bow
                .no_stand
                .retain(|(c, until)| *c != id && tick < *until);
            self.bow.no_stand.push((id, tick + NO_STAND_WAIT));
            self.note(
                tick,
                format!("bow: no stand for crystal {id}"),
                format!(
                    "from {feet:?}; {found} stands with a shot, none walked to; asked again in {NO_STAND_WAIT}; from here {first_miss:?}"
                ),
            );
        }
        None
    }

    /// The shot in hand, a tick at a time while no task runs: the bow to hand, still, the use,
    /// the draw, the release on the aim, the flight. `None` with no shot (or the shot over).
    pub fn bow_step(&mut self, w: &World, tick: u64) -> Option<Controls> {
        let mut shot = self.bow.doing.take()?;
        let gap = tick.saturating_sub(self.bow.driven_at) > 1;
        self.bow.driven_at = tick;
        let drawing = matches!(shot.stage, Stage::Draw { .. });
        // Another row took the controls (a dodge, an eat), or a task began: the draw let go and
        // the shot begun again from the bow in hand.
        if !matches!(self.task, Task::Idle) || gap {
            if drawing {
                self.acts.push(Act::ReleaseUse);
                self.note(
                    tick,
                    format!("bow: draw on crystal {} let go", shot.target),
                    format!(
                        "another row had the controls ({}); the arrow may be loosed off the aim",
                        if gap { "a gap" } else { self.task.name() }
                    ),
                );
                shot.stage = Stage::Hold;
            }
            if !matches!(self.task, Task::Idle) {
                return None;
            }
        }
        let body = w.body?;
        let alive = w
            .entities
            .get(&shot.target)
            .is_some_and(|e| !e.dead && e.kind == EntityKind::EndCrystal);
        if !alive {
            if matches!(shot.stage, Stage::Draw { .. }) {
                self.acts.push(Act::ReleaseUse);
            }
            if let Stage::Flight { since } = shot.stage {
                // Its blast at its spot after the release is the hit (the server untracks and
                // sends crystals again on a resume: E9's false crystals_5 at tick 40).
                let blast = w.explosions.iter().any(|b| {
                    b.at_tick >= shot.released_server
                        && (0..3)
                            .map(|i| (b.center[i] - shot.at[i]).powi(2))
                            .sum::<f64>()
                            <= 4.0
                });
                let gone = *shot.gone_at.get_or_insert(tick);
                if !blast {
                    if tick.saturating_sub(gone) <= BLAST_WAIT {
                        self.bow.doing = Some(shot.clone());
                        return Some(Controls {
                            yaw: shot.aim.yaw,
                            pitch: Some(shot.aim.pitch),
                            ..Controls::default()
                        });
                    }
                    self.note(
                        tick,
                        format!("bow: crystal {} out of sight after the shot", shot.target),
                        format!(
                            "no blast at {:?} within {BLAST_WAIT} ticks of its leaving: not counted",
                            shot.at
                        ),
                    );
                    self.retry_at = tick;
                    return None;
                }
                self.bow.hits += 1;
                self.note(
                    tick,
                    format!("bow: crystal {} gone by arrow", shot.target),
                    format!(
                        "{} ticks after the release, {:.1} off; {} hits of {} shots; {} arrows left",
                        tick.saturating_sub(since),
                        shot.aim.range,
                        self.bow.hits,
                        self.bow.shots,
                        arrows(w)
                    ),
                );
            } else {
                self.note(
                    tick,
                    format!("bow: crystal {} gone before the shot", shot.target),
                    "another's doing".to_owned(),
                );
            }
            self.retry_at = tick;
            return None;
        }
        let still = Controls {
            yaw: shot.aim.yaw,
            pitch: Some(shot.aim.pitch),
            ..Controls::default()
        };
        let danger = self
            .table
            .as_ref()
            .map_or(8.0, |t| t.survival.danger_health);
        let hurt = w.player.health.is_some_and(|h| h <= danger);
        match shot.stage {
            Stage::Hold | Stage::Settle if hurt => {
                self.note(
                    tick,
                    "bow: shot dropped under danger health".to_owned(),
                    format!("health {:?}, danger {danger}", w.player.health),
                );
                self.retry_at = tick;
                return None;
            }
            Stage::Hold => match hold::toward(w, ItemKind::Bow) {
                Hold::Held => shot.stage = Stage::Settle,
                Hold::Act(a) => self.acts.push(a),
                Hold::Wait => {}
                Hold::Missing => {
                    self.note(
                        tick,
                        "bow: no bow in the inventory".to_owned(),
                        String::new(),
                    );
                    return None;
                }
            },
            Stage::Settle => {
                let speed = body.vel[0].hypot(body.vel[2]);
                // The aim again from where the feet are now.
                let mut cells = path::Cells::new(w);
                let mut blocked = |c: Cell| blocks_arrow(&mut cells, c);
                match aim(body.pos, shot.at, &mut blocked) {
                    Ok(a) => shot.aim = a,
                    Err(m) => {
                        self.note(
                            tick,
                            format!("bow: no shot at crystal {} from here", shot.target),
                            format!("{m:?} from {:?}", path::feet_cell(body.pos)),
                        );
                        self.retry_at = tick;
                        return None;
                    }
                }
                if body.on_ground && speed < STILL && self.acts.is_empty() {
                    self.acts.push(Act::UseItem);
                    shot.stage = Stage::Draw { since: tick };
                }
            }
            Stage::Draw { since } => {
                let held = tick.saturating_sub(since);
                let heard = yaw_gap(f64::from(w.player.yaw), shot.aim.yaw) <= LOOK_NEAR
                    && (f64::from(w.player.pitch) - f64::from(shot.aim.pitch)).abs() <= LOOK_NEAR;
                if held >= DRAW_HOLD && (heard || held >= DRAW_HOLD + LOOK_WAIT) {
                    self.acts.push(Act::ReleaseUse);
                    self.bow.shots += 1;
                    self.note(
                        tick,
                        format!("bow: shot at crystal {}", shot.target),
                        format!(
                            "from {:?}, {:.1} off, drawn {held} ticks, pitch {:.2} (heard {:.2}), yaw {:.2} (heard {:.2}){}; shot {}",
                            path::feet_cell(body.pos),
                            shot.aim.range,
                            shot.aim.pitch,
                            w.player.pitch,
                            shot.aim.yaw,
                            w.player.yaw,
                            if heard { "" } else { ", the look never matched" },
                            self.bow.shots
                        ),
                    );
                    shot.stage = Stage::Flight { since: tick };
                    shot.released_server = w.session.server_ticks;
                }
            }
            Stage::Flight { since } => {
                if tick.saturating_sub(since) > u64::from(shot.aim.ticks) + FLIGHT_SPARE {
                    let why = format!(
                        "still there {} ticks after the release ({} predicted), {:.1} off",
                        tick.saturating_sub(since),
                        shot.aim.ticks,
                        shot.aim.range
                    );
                    self.note(
                        tick,
                        format!("bow: shot at crystal {} missed", shot.target),
                        why.clone(),
                    );
                    self.bow_note_miss(tick, shot.target, why);
                    self.retry_at = tick;
                    return None;
                }
            }
        }
        self.bow.doing = Some(shot);
        Some(still)
    }
    // <<< E8
}
