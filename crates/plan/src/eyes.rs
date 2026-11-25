//! Eyes of ender (issue 19c, `docs/problems/eyes-throw.md`): the eyes crafted in the 2x2 grid, a
//! throw, the eye's flight read into a line on the plane, the dropped eye picked up again, and the
//! lines crossed into a fix (`fix`, pure). The throw policy (`due`, `way`) spends as few eyes as it
//! can: every throw spends issue 20's margin (an eye breaks one throw in five).
//!
//! The eye's rules, read from the 26.1.2 jar (Q5, probe 45685968956): it spawns at the thrower's
//! (x, y + 0.9, z) with no velocity and is aimed once, at the located stronghold point (a chunk
//! corner) when that is within 12 blocks on the plane (it then flies to it and descends), else at a
//! point 12 blocks along the exact bearing and 8 up. It flies straight, through terrain, and after
//! 81 ticks drops as an item (four in five) or shatters (level event 2003). A use while the look's
//! block hit is an end portal frame throws nothing, so the throw looks up. Its moves reach the
//! client every few ticks; the line is fitted to every distinct position after the first few, and
//! its spread is set only by the positions' quantization: tiny.
//! A line's `from` is the throw's feet projected onto it; `dir` is a unit vector on the plane
//! ([x, z]); `spread` is its angular standard deviation in radians. A line with a zero `dir` is a
//! point: the near rule (the eye went down within 12 blocks), its `spread` an error in blocks.

use data::{BlockKind, EntityKind, ItemKind};
use physics::Controls;
use world::{Act, World};

use crate::hold::{self, Hold};
use crate::make::{self, Choice};
use crate::path::{self, Goal};
use crate::walk::{Arrive, TaskStatus, Walk};
use crate::{Plan, Task, craft};

/// `[eyes]` in `config/goals.toml`: every number with its source there.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct EyeRules {
    pub eyes_max: i32,
    pub throw_pitch: f32,
    pub hostile_clear: f64,
    pub night_throws: bool,
    pub settle_ticks: u64,
    pub seen_within: f64,
    pub answer_ticks: u64,
    pub flight_max: u64,
    pub fit_skip: usize,
    pub min_flight: f64,
    pub spread_floor_deg: f64,
    pub drop_radius: f64,
    pub drop_grace: u64,
    pub near_radius: f64,
    pub near_sink: f64,
    pub near_err: f64,
    pub pickup_radius: f64,
    pub pickup_rise: i32,
    pub pickup_ticks: u64,
    pub pickup_walks: u32,
    pub max_throws: u32,
    pub baseline: f64,
    pub baseline_across: f64,
    pub baseline_turn_deg: f64,
    pub refine_from: f64,
    pub refine_offset: f64,
    pub final_within: f64,
    pub respot: f64,
    pub frame_chunks: i32,
    pub note_every: u64,
}

impl Default for EyeRules {
    fn default() -> Self {
        Self {
            eyes_max: 14,
            throw_pitch: -10.0,
            hostile_clear: 16.0,
            night_throws: false,
            settle_ticks: 2,
            seen_within: 3.0,
            answer_ticks: 60,
            flight_max: 120,
            fit_skip: 2,
            min_flight: 1.0,
            spread_floor_deg: 0.2,
            drop_radius: 3.0,
            drop_grace: 20,
            near_radius: 12.0,
            near_sink: 1.0,
            near_err: 2.0,
            pickup_radius: 24.0,
            pickup_rise: 2,
            pickup_ticks: 600,
            pickup_walks: 3,
            max_throws: 4,
            baseline: 200.0,
            baseline_across: 140.0,
            baseline_turn_deg: 45.0,
            refine_from: 64.0,
            refine_offset: 0.0,
            final_within: 64.0,
            respot: 150.0,
            frame_chunks: 8,
            note_every: 600,
        }
    }
}

/// One eye's line on the plane (see the module's doc).
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Line {
    pub from: [f64; 2],
    pub dir: [f64; 2],
    pub spread: f64,
}

impl Line {
    /// A point sighting (the near rule): the eye went down here.
    pub fn is_point(&self) -> bool {
        unit(self.dir).is_none()
    }
}

/// Where the lines cross, with its one-sigma errors along the mean bearing and across it.
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Fix {
    pub point: [f64; 2],
    pub err_along: f64,
    pub err_across: f64,
}

/// Why lines make no fix.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Refusal {
    /// Fewer than two lines with a direction, and no point.
    TooFew,
    /// No two lines differ by `PARALLEL_DEG` or more: they say nothing of the distance.
    Parallel,
    /// The crossing lies behind a throw: an eye never flies away from its stronghold.
    Behind,
}

impl Refusal {
    pub fn name(self) -> &'static str {
        match self {
            Refusal::TooFew => "too few lines",
            Refusal::Parallel => "lines within 3 degrees of parallel",
            Refusal::Behind => "the crossing is behind a throw",
        }
    }
}

/// Lines closer than this to parallel are refused (clojurecraft refused 2; the plan, 19c.5: 3).
pub const PARALLEL_DEG: f64 = 3.0;
/// The least a line's spread is taken as: a perfect fit is never infinitely sure.
const MIN_SPREAD: f64 = 1e-4;
/// The least range a line's weight is computed at, in blocks.
const MIN_RANGE: f64 = 1.0;
/// Reweighting passes: each takes the ranges from the last crossing.
const PASSES: usize = 4;

fn dot(a: [f64; 2], b: [f64; 2]) -> f64 {
    a[0] * b[0] + a[1] * b[1]
}

fn cross(a: [f64; 2], b: [f64; 2]) -> f64 {
    a[0] * b[1] - a[1] * b[0]
}

fn sub(a: [f64; 2], b: [f64; 2]) -> [f64; 2] {
    [a[0] - b[0], a[1] - b[1]]
}

fn norm(a: [f64; 2]) -> f64 {
    a[0].hypot(a[1])
}

/// `a` scaled to length 1, or `None` for a zero (or not finite) vector.
pub fn unit(a: [f64; 2]) -> Option<[f64; 2]> {
    let n = norm(a);
    (n.is_finite() && n > 1e-9).then(|| [a[0] / n, a[1] / n])
}

/// `a` turned by `deg` degrees, counter-clockwise in (x, z).
pub fn turn(a: [f64; 2], deg: f64) -> [f64; 2] {
    let (s, c) = deg.to_radians().sin_cos();
    [a[0] * c - a[1] * s, a[0] * s + a[1] * c]
}

/// The Notchian yaw (south 0, west +90) facing a direction on the plane.
pub fn yaw_of(dir: [f64; 2]) -> f64 {
    (-dir[0]).atan2(dir[1]).to_degrees()
}

/// The crossing of the lines (`fix_why`), or `None`.
pub fn fix(lines: &[Line]) -> Option<Fix> {
    fix_why(lines).ok()
}

/// The lines crossed by weighted least squares, or why not.
///
/// A point line (the near rule) is the fix itself, the surest first. Otherwise each line weighs
/// `1 / (range * spread)^2`, its range the distance along it to the last crossing (reweighted a few
/// passes): the perpendicular miss of a line at range `r` is about `r * spread`. Refused when no
/// two lines differ by `PARALLEL_DEG` and when the crossing lies behind any throw. The errors are
/// the solution's covariance read along the mean bearing and across it: for two lines `b` apart at
/// distance `d` they come to about `d * spread / sqrt 2` across and `sqrt 2 * d^2 * spread / b`
/// along.
pub fn fix_why(lines: &[Line]) -> Result<Fix, Refusal> {
    if let Some(p) = lines
        .iter()
        .filter(|l| l.is_point())
        .min_by(|a, b| a.spread.total_cmp(&b.spread))
    {
        return Ok(Fix {
            point: p.from,
            err_along: p.spread,
            err_across: p.spread,
        });
    }
    let rays: Vec<([f64; 2], [f64; 2], f64)> = lines
        .iter()
        .filter_map(|l| unit(l.dir).map(|d| (l.from, d, l.spread.abs().max(MIN_SPREAD))))
        .collect();
    if rays.len() < 2 {
        return Err(Refusal::TooFew);
    }
    let min_sin = PARALLEL_DEG.to_radians().sin();
    let wide = rays.iter().enumerate().any(|(i, a)| {
        rays.iter()
            .skip(i + 1)
            .any(|b| cross(a.1, b.1).abs() >= min_sin)
    });
    if !wide {
        return Err(Refusal::Parallel);
    }
    let mut ranges = vec![MIN_RANGE; rays.len()];
    let mut point = [0.0; 2];
    let mut normal = [0.0; 3];
    for _ in 0..PASSES {
        // The normal equations: sum of w n n^T times x = sum of w n (n . p).
        let (mut a, mut b, mut c, mut r0, mut r1) = (0.0, 0.0, 0.0, 0.0, 0.0);
        for ((from, d, s), range) in rays.iter().zip(&ranges) {
            let n = [-d[1], d[0]];
            let w = 1.0 / (range * s).powi(2);
            let np = dot(n, *from);
            a += w * n[0] * n[0];
            b += w * n[0] * n[1];
            c += w * n[1] * n[1];
            r0 += w * n[0] * np;
            r1 += w * n[1] * np;
        }
        let det = a * c - b * b;
        if !(det.is_finite() && det > 1e-12 * (a + c).powi(2)) {
            return Err(Refusal::Parallel);
        }
        point = [(c * r0 - b * r1) / det, (a * r1 - b * r0) / det];
        normal = [a, b, c];
        ranges = rays
            .iter()
            .map(|(from, d, _)| dot(sub(point, *from), *d).max(MIN_RANGE))
            .collect();
    }
    if rays
        .iter()
        .any(|(from, d, _)| dot(sub(point, *from), *d) <= 0.0)
    {
        return Err(Refusal::Behind);
    }
    let [a, b, c] = normal;
    let det = a * c - b * b;
    // The covariance, the normal matrix's inverse.
    let cov = [c / det, -b / det, a / det];
    let var = |u: [f64; 2]| {
        (u[0] * u[0] * cov[0] + 2.0 * u[0] * u[1] * cov[1] + u[1] * u[1] * cov[2]).max(0.0)
    };
    let sum = rays
        .iter()
        .fold([0.0, 0.0], |s, (_, d, _)| [s[0] + d[0], s[1] + d[1]]);
    let along = unit(sum).unwrap_or(rays[0].1);
    let across = [-along[1], along[0]];
    Ok(Fix {
        point,
        err_along: var(along).sqrt(),
        err_across: var(across).sqrt(),
    })
}

/// An eye's flight: where it was thrown from and its distinct positions, as the server told them.
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Flight {
    pub id: i32,
    /// The thrower's feet at the throw.
    pub from: [f64; 3],
    pub samples: Vec<[f64; 3]>,
}

impl Flight {
    /// Keep a position unless it is the last one again (moves come every few ticks).
    pub fn push(&mut self, p: [f64; 3]) {
        if self.samples.last() != Some(&p) {
            self.samples.push(p);
        }
    }

    /// The near rule: the eye stayed within `near_radius` of the throw on the plane and sank
    /// `near_sink` or more under where it was first seen: the stronghold point is under it.
    pub fn near_point(&self, rules: &EyeRules) -> Option<Line> {
        let first = self.samples.first()?;
        let last = self.samples.last()?;
        let far = self
            .samples
            .iter()
            .map(|p| (p[0] - self.from[0]).hypot(p[2] - self.from[2]))
            .fold(0.0_f64, f64::max);
        (far <= rules.near_radius && last[1] <= first[1] - rules.near_sink).then_some(Line {
            from: [last[0], last[2]],
            dir: [0.0, 0.0],
            spread: rules.near_err,
        })
    }

    /// The flight's line on the plane: the principal axis of the positions after the first
    /// `fit_skip`, pointed from the first toward the last, through their centroid; `from` the
    /// throw's feet projected onto it. Its spread is the fit's: the residuals' spread over the
    /// length flown, never under `spread_floor_deg`. `None` when it flew under `min_flight`.
    pub fn line(&self, rules: &EyeRules) -> Option<Line> {
        let pts: Vec<[f64; 2]> = self
            .samples
            .iter()
            .skip(rules.fit_skip)
            .map(|p| [p[0], p[2]])
            .collect();
        let (first, last) = (*pts.first()?, *pts.last()?);
        let length = norm(sub(last, first));
        if pts.len() < 2 || length < rules.min_flight {
            return None;
        }
        let n = pts.len() as f64;
        let mean = pts
            .iter()
            .fold([0.0, 0.0], |s, p| [s[0] + p[0] / n, s[1] + p[1] / n]);
        let (mut sxx, mut sxz, mut szz) = (0.0, 0.0, 0.0);
        for p in &pts {
            let d = sub(*p, mean);
            sxx += d[0] * d[0];
            sxz += d[0] * d[1];
            szz += d[1] * d[1];
        }
        // The principal axis of the 2x2 scatter.
        let angle = 0.5 * (2.0 * sxz).atan2(sxx - szz);
        let mut dir = [angle.cos(), angle.sin()];
        if dot(dir, sub(last, first)) < 0.0 {
            dir = [-dir[0], -dir[1]];
        }
        let normal = [-dir[1], dir[0]];
        let rms = (pts
            .iter()
            .map(|p| dot(sub(*p, mean), normal).powi(2))
            .sum::<f64>()
            / n)
            .sqrt();
        let spread = (rms / length)
            .atan()
            .max(rules.spread_floor_deg.to_radians());
        let thrower = [self.from[0], self.from[2]];
        let t = dot(sub(thrower, mean), dir);
        Some(Line {
            from: [mean[0] + dir[0] * t, mean[1] + dir[1] * t],
            dir,
            spread,
        })
    }

    /// The flight's reading: the near rule's point, else its line.
    pub fn reading(&self, rules: &EyeRules) -> Option<Line> {
        self.near_point(rules).or_else(|| self.line(rules))
    }
}

/// Why a throw is made: the policy's step.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Why {
    /// On arrival on the surface: a bearing.
    First,
    /// After a baseline off the first line: a crossing.
    Baseline,
    /// Within `refine_from` of the fix with no frame seen.
    Refine,
    /// At the fix with no frame seen: the near rule or a last crossing.
    Final,
}

impl Why {
    pub fn name(self) -> &'static str {
        match self {
            Why::First => "first",
            Why::Baseline => "baseline",
            Why::Refine => "refine",
            Why::Final => "final",
        }
    }
}

/// What the policy knows: the lines, the fix, the spots thrown from, the eyes thrown.
#[derive(Clone, Copy, Debug)]
pub struct Known<'a> {
    pub lines: &'a [Line],
    pub fix: Option<&'a Fix>,
    pub spots: &'a [[f64; 2]],
    pub thrown: u32,
    pub frames_seen: bool,
}

/// The throw due where the feet stand, if any (the plan, 19c.6): never past `max_throws` or once a
/// frame is seen. The first on the surface; with a line and no fix, once `baseline` from the last
/// line's throw and `baseline_across` off it; with a fix, within `refine_from` of it from a spot
/// `respot` from every earlier one, once out there and once more within `final_within`.
pub fn due(rules: &EyeRules, k: &Known<'_>, feet: [f64; 2]) -> Option<Why> {
    if k.thrown >= rules.max_throws || k.frames_seen {
        return None;
    }
    let Some(fix) = k.fix else {
        let Some(last) = k.lines.iter().rev().find(|l| !l.is_point()) else {
            return Some(Why::First);
        };
        let d = sub(feet, last.from);
        let across = cross(last.dir, d).abs();
        return (norm(d) >= rules.baseline && across >= rules.baseline_across)
            .then_some(Why::Baseline);
    };
    let to_fix = norm(sub(fix.point, feet));
    let fresh = k.spots.iter().all(|s| norm(sub(*s, feet)) >= rules.respot);
    if !fresh || to_fix > rules.refine_from {
        return None;
    }
    let refined = k
        .spots
        .iter()
        .any(|s| norm(sub(*s, fix.point)) <= rules.refine_from);
    match (refined, to_fix <= rules.final_within) {
        (false, _) => Some(Why::Refine),
        (true, true) => Some(Why::Final),
        (true, false) => None,
    }
}

/// Where the bot should walk for the next reading (for the overland leg): nothing before the
/// first line; with a line and no fix, the baseline spot `baseline` + 32 out from the last line's
/// throw along the bearing turned `baseline_turn_deg` to `side` (+1 counter-clockwise, -1
/// clockwise); with a fix not yet refined, the refine spot `refine_offset` from the fix square to
/// the mean bearing, on the side nearer the feet (the refine line then crosses the others near a
/// right angle); after, the fix.
pub fn way(rules: &EyeRules, k: &Known<'_>, feet: [f64; 2], side: f64) -> Option<[f64; 2]> {
    let Some(fix) = k.fix else {
        let last = k.lines.iter().rev().find(|l| !l.is_point())?;
        let d = turn(last.dir, side.signum() * rules.baseline_turn_deg);
        let reach = rules.baseline + 32.0;
        return Some([last.from[0] + d[0] * reach, last.from[1] + d[1] * reach]);
    };
    let refined = k
        .spots
        .iter()
        .any(|s| norm(sub(*s, fix.point)) <= rules.refine_from);
    if refined || k.frames_seen || k.thrown >= rules.max_throws {
        return Some(fix.point);
    }
    let sum = k
        .lines
        .iter()
        .filter_map(|l| unit(l.dir))
        .fold([0.0, 0.0], |s, d| [s[0] + d[0], s[1] + d[1]]);
    let Some(bearing) = unit(sum) else {
        return Some(fix.point);
    };
    let perp = [-bearing[1], bearing[0]];
    let spot = |s: f64| {
        [
            fix.point[0] + perp[0] * s * rules.refine_offset,
            fix.point[1] + perp[1] * s * rules.refine_offset,
        ]
    };
    let (a, b) = (spot(1.0), spot(-1.0));
    Some(if norm(sub(a, feet)) <= norm(sub(b, feet)) {
        a
    } else {
        b
    })
}

/// The eyes the held pearls, powder and rods make, up to `eyes_max`, with the eyes held; `None`
/// when no more can be made. A rod makes two powder (the generated `blaze_powder` recipe).
pub fn eyes_target(rules: &EyeRules, counts: &make::Counts) -> Option<i32> {
    let held = |k| counts.get(&k).copied().unwrap_or(0);
    let eyes = held(ItemKind::EnderEye);
    let pearls = held(ItemKind::EnderPearl);
    let powder = held(ItemKind::BlazePowder) + 2 * held(ItemKind::BlazeRod);
    let target = (eyes + pearls.min(powder)).min(rules.eyes_max);
    (target > eyes).then_some(target)
}

/// The `eyes` goal row with its count set to `eyes_target`: the needs planner then chains powder
/// from rods, a rod at a time, so a rod not needed is kept.
pub fn eyes_row(table: &make::Table, counts: &make::Counts) -> Option<make::Row> {
    let target = eyes_target(&table.eyes, counts)?;
    let mut row = table.until(ROW)?.clone();
    let first = row.provides.first_mut()?;
    first.1 = target;
    Some(row)
}

/// The goal row's `until` name.
pub const ROW: &str = "eyes";

/// Whether the dimension is the overworld (eyes fly only there: `EnderEyeItem.use` finds no
/// stronghold elsewhere).
pub fn in_overworld(w: &World) -> bool {
    w.dimension
        .as_ref()
        .is_some_and(|d| d.name == "minecraft:overworld")
}

/// Why a throw due cannot be made here and now, or `None` when it can.
pub fn blocked(w: &World, rules: &EyeRules) -> Option<String> {
    if !in_overworld(w) {
        return Some("not in the overworld".to_owned());
    }
    if craft::count(w, ItemKind::EnderEye) == 0 {
        return Some("no eye held".to_owned());
    }
    if !world::stage::open_sky(w) {
        return Some("no open sky over the feet".to_owned());
    }
    if let Some((id, kind)) = crate::defend::nearest_foe(w, rules.hostile_clear) {
        return Some(format!("a {kind:?} ({id}) within {}", rules.hostile_clear));
    }
    if !rules.night_throws && w.is_night() == Some(true) {
        return Some("night".to_owned());
    }
    None
}

/// Whether an end portal frame is on record within `frame_chunks` (the eyes' work is over).
pub fn frames_seen(w: &World, rules: &EyeRules) -> bool {
    !world::stage::kind_near(w, BlockKind::EndPortalFrame, rules.frame_chunks).is_empty()
}

/// Where a throw stands.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum Phase {
    /// The eye being put in hand and the look settled.
    Aim { since: u64 },
    /// `use_item` sent: waiting for the eye entity.
    Sent { at: u64 },
    /// Following the eye.
    Flying { since: u64 },
    /// The eye gone at `pos`: waiting for its dropped item.
    Ended { at: u64, pos: [f64; 3] },
    /// Walking to the dropped eye.
    Pickup {
        item: i32,
        since: u64,
        walks: u32,
        walk: Box<Walk>,
    },
}

/// Why a throw failed.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ThrowFailure {
    /// No eye held.
    NoEye,
    /// No eye entity came within `answer_ticks` of the use.
    NotSeen,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ThrowStatus {
    Running,
    Done,
    Failed(ThrowFailure),
}

/// How a throw's eye ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum End {
    /// Dropped and picked up again.
    Kept,
    /// Dropped and not picked up (out of reach, no way, or the time ran out).
    Lost,
    /// Broken in the air.
    Shattered,
}

/// A throw: aim, use, follow, read, pick up (`Task::Throw`).
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Throw {
    pub why: Why,
    pub yaw: f64,
    pub pitch: f32,
    pub started: u64,
    pub phase: Phase,
    /// Eyes held when the use went.
    pub eyes_before: i32,
    /// Eye entities and dropped eyes there before the throw: never the throw's own.
    pub old_eyes: Vec<i32>,
    pub old_items: Vec<i32>,
    pub flight: Flight,
    /// The flight's reading, once taken.
    pub read: bool,
    pub end: Option<End>,
}

/// One tick of a throw: the controls, the act, how it stands, a reading when the flight ended,
/// and the notes to write.
#[derive(Clone, Debug)]
pub struct ThrowOut {
    pub controls: Controls,
    pub acts: Vec<Act>,
    pub status: ThrowStatus,
    pub reading: Option<Line>,
    pub said: Vec<(String, String)>,
}

fn ids_of(w: &World, f: impl Fn(&world::Entity) -> bool) -> Vec<i32> {
    w.entities
        .iter()
        .filter(|(_, e)| f(e))
        .map(|(id, _)| *id)
        .collect()
}

fn dropped_eye(e: &world::Entity) -> bool {
    e.kind == EntityKind::Item && e.item().is_some_and(|(k, _)| k == ItemKind::EnderEye)
}

fn flat(a: [f64; 3], b: [f64; 3]) -> f64 {
    (a[0] - b[0]).hypot(a[2] - b[2])
}

/// A walk to a dropped item's cell: where it lies within the pickup box.
fn pickup_walk(pos: [f64; 3], rise: i32, tick: u64) -> Walk {
    let cell = path::feet_cell(pos);
    Walk::new(
        cell,
        Goal::Pickup {
            pos: cell,
            range: path::DIAGONAL_COST,
            rise,
        },
        Arrive::Goal,
        tick,
    )
}

impl Throw {
    pub fn new(
        w: &World,
        why: Why,
        bearing: Option<[f64; 2]>,
        rules: &EyeRules,
        tick: u64,
    ) -> Self {
        let yaw = bearing.map_or_else(|| w.controls.yaw, yaw_of);
        Self {
            why,
            yaw,
            pitch: rules.throw_pitch,
            started: tick,
            phase: Phase::Aim { since: tick },
            eyes_before: craft::count(w, ItemKind::EnderEye),
            old_eyes: ids_of(w, |e| e.kind == EntityKind::EyeOfEnder),
            old_items: ids_of(w, dropped_eye),
            flight: Flight {
                from: w.body.map_or([0.0; 3], |b| b.pos),
                ..Flight::default()
            },
            read: false,
            end: None,
        }
    }

    fn look(&self) -> Controls {
        Controls {
            yaw: self.yaw,
            pitch: Some(self.pitch),
            ..Controls::default()
        }
    }

    pub fn step(&mut self, w: &World, rules: &EyeRules, tick: u64) -> ThrowOut {
        let mut out = ThrowOut {
            controls: self.look(),
            acts: Vec::new(),
            status: ThrowStatus::Running,
            reading: None,
            said: Vec::new(),
        };
        let feet = w.body.map_or(self.flight.from, |b| b.pos);
        match &mut self.phase {
            Phase::Aim { since } => {
                match hold::toward(w, ItemKind::EnderEye) {
                    Hold::Held => {}
                    Hold::Act(a) => out.acts.push(a),
                    Hold::Wait => {}
                    Hold::Missing => out.status = ThrowStatus::Failed(ThrowFailure::NoEye),
                }
                if hold::held_item(w) == Some(ItemKind::EnderEye)
                    && w.inventory.carried.is_none()
                    && out.acts.is_empty()
                    && tick.saturating_sub(*since) >= rules.settle_ticks
                {
                    self.eyes_before = craft::count(w, ItemKind::EnderEye);
                    self.old_eyes = ids_of(w, |e| e.kind == EntityKind::EyeOfEnder);
                    self.old_items = ids_of(w, dropped_eye);
                    self.flight.from = feet;
                    self.phase = Phase::Sent { at: tick };
                    out.acts.push(Act::UseItem);
                }
            }
            Phase::Sent { at } => {
                let middle = [feet[0], feet[1] + 0.9, feet[2]];
                let own = w
                    .entities
                    .iter()
                    .filter(|(id, e)| {
                        e.kind == EntityKind::EyeOfEnder && !e.dead && !self.old_eyes.contains(id)
                    })
                    .map(|(id, e)| {
                        let d = (0..3).map(|i| (e.pos[i] - middle[i]).powi(2)).sum::<f64>();
                        (d.sqrt(), *id, e.pos)
                    })
                    .filter(|(d, _, _)| *d <= rules.seen_within)
                    .min_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
                if let Some((_, id, pos)) = own {
                    self.flight.id = id;
                    self.flight.push(pos);
                    self.phase = Phase::Flying { since: tick };
                } else if tick.saturating_sub(*at) > rules.answer_ticks {
                    let now = craft::count(w, ItemKind::EnderEye);
                    out.said.push((
                        "eye not seen".to_owned(),
                        format!(
                            "no eye entity within {} of the feet {} ticks after the use; eyes held {} (before {})",
                            rules.seen_within, rules.answer_ticks, now, self.eyes_before
                        ),
                    ));
                    out.status = ThrowStatus::Failed(ThrowFailure::NotSeen);
                }
            }
            Phase::Flying { since } => {
                let live = w
                    .entities
                    .get(&self.flight.id)
                    .filter(|e| !e.dead)
                    .map(|e| e.pos);
                if let Some(p) = live {
                    self.flight.push(p);
                }
                if live.is_none() || tick.saturating_sub(*since) > rules.flight_max {
                    let pos = self.flight.samples.last().copied().unwrap_or(feet);
                    out.reading = self.flight.reading(rules);
                    self.read = true;
                    let first = self.flight.samples.first().copied().unwrap_or(pos);
                    let what = match out.reading {
                        Some(l) if l.is_point() => format!(
                            "eye flight: sank within {} at ({:.1}, {:.1}): the stronghold is under it",
                            rules.near_radius, l.from[0], l.from[1]
                        ),
                        Some(l) => format!(
                            "eye flight: heading {:.2} (yaw), spread {:.2} deg",
                            yaw_of(l.dir),
                            l.spread.to_degrees()
                        ),
                        None => "eye flight: no line".to_owned(),
                    };
                    out.said.push((
                        what,
                        format!(
                            "{} positions over {} ticks, ({:.1}, {:.1}, {:.1}) to ({:.1}, {:.1}, {:.1}){}",
                            self.flight.samples.len(),
                            tick.saturating_sub(*since),
                            first[0],
                            first[1],
                            first[2],
                            pos[0],
                            pos[1],
                            pos[2],
                            if live.is_some() { "; still flying at the cap" } else { "" }
                        ),
                    ));
                    self.phase = Phase::Ended { at: tick, pos };
                }
            }
            Phase::Ended { at, pos } => {
                let drop = w
                    .entities
                    .iter()
                    .filter(|(id, e)| dropped_eye(e) && !self.old_items.contains(id))
                    .map(|(id, e)| (flat(e.pos, *pos), *id, e.pos))
                    .filter(|(d, _, _)| *d <= rules.drop_radius)
                    .min_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
                if let Some((_, item, at_pos)) = drop {
                    let reach = flat(at_pos, feet);
                    let rise = (at_pos[1] - feet[1]).abs();
                    if reach <= rules.pickup_radius {
                        out.said.push((
                            format!(
                                "eye ended: dropped at ({:.1}, {:.1}, {:.1})",
                                at_pos[0], at_pos[1], at_pos[2]
                            ),
                            format!("{reach:.1} off on the plane, {rise:.1} in height: walk to it"),
                        ));
                        self.phase = Phase::Pickup {
                            item,
                            since: tick,
                            walks: 1,
                            walk: Box::new(pickup_walk(at_pos, rules.pickup_rise, tick)),
                        };
                    } else {
                        out.said.push((
                            "eye ended: dropped out of reach".to_owned(),
                            format!(
                                "{reach:.1} off, over the {} pickup radius: lost",
                                rules.pickup_radius
                            ),
                        ));
                        self.end = Some(End::Lost);
                        out.status = ThrowStatus::Done;
                    }
                } else if tick.saturating_sub(*at) > rules.drop_grace {
                    out.said.push((
                        format!(
                            "eye ended: shattered at ({:.1}, {:.1}, {:.1})",
                            pos[0], pos[1], pos[2]
                        ),
                        format!(
                            "no dropped eye within {} in {} ticks",
                            rules.drop_radius, rules.drop_grace
                        ),
                    ));
                    self.end = Some(End::Shattered);
                    out.status = ThrowStatus::Done;
                }
            }
            Phase::Pickup {
                item,
                since,
                walks,
                walk,
            } => {
                let picked = w.pickups.iter().any(|p| p.item_entity == *item);
                let lying = w.entities.get(item).map(|e| e.pos);
                let back = craft::count(w, ItemKind::EnderEye) >= self.eyes_before;
                if picked || (lying.is_none() && back) {
                    out.said.push((
                        "eye picked up".to_owned(),
                        format!(
                            "{} ticks after it dropped; eyes held {}",
                            tick.saturating_sub(*since),
                            craft::count(w, ItemKind::EnderEye)
                        ),
                    ));
                    self.end = Some(End::Kept);
                    out.status = ThrowStatus::Done;
                } else if lying.is_none() || tick.saturating_sub(*since) > rules.pickup_ticks {
                    out.said.push((
                        "eye pickup given up".to_owned(),
                        if lying.is_none() {
                            "the dropped eye is gone and none came back".to_owned()
                        } else {
                            format!("not picked up in {} ticks", rules.pickup_ticks)
                        },
                    ));
                    self.end = Some(End::Lost);
                    out.status = ThrowStatus::Done;
                } else {
                    let (c, status) = walk.step(w, tick);
                    let (acts, said) = walk.take_out();
                    out.acts.extend(acts);
                    out.said.extend(said);
                    out.controls = c;
                    match status {
                        TaskStatus::Running => {}
                        TaskStatus::Done | TaskStatus::Failed(_) if *walks < rules.pickup_walks => {
                            // At the cell and not picked up (it rolled), or no way yet: walk again
                            // to where it lies now.
                            *walks += 1;
                            if let Some(p) = lying {
                                **walk = pickup_walk(p, rules.pickup_rise, tick);
                            }
                        }
                        TaskStatus::Done | TaskStatus::Failed(_) => {
                            out.said.push((
                                "eye pickup given up".to_owned(),
                                format!("{walks} walks to it ended without it ({status:?})"),
                            ));
                            self.end = Some(End::Lost);
                            out.status = ThrowStatus::Done;
                        }
                    }
                }
            }
        }
        out
    }
}

/// The eyes' running record on the plan (beside `Plan.fix` and `Plan.throws`).
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct EyeState {
    /// Eyes thrown (a use sent with an eye in hand), and how each ended.
    pub thrown: u32,
    pub kept: u32,
    pub lost: u32,
    pub shattered: u32,
    /// Where each throw was made from, on the plane.
    pub spots: Vec<[f64; 2]>,
    /// When a throw that is due and blocked, or a craft that cannot go on, was last noted.
    pub noted_at: Option<u64>,
}

/// What survives a resume: written beside the snapshot and read back (`Plan::eyes_kept`,
/// `Plan::restore_eyes`).
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Kept {
    #[serde(default)]
    pub fix: Option<Fix>,
    #[serde(default)]
    pub throws: Vec<Line>,
    #[serde(default)]
    pub state: EyeState,
}

impl Plan {
    fn eye_rules(&self) -> EyeRules {
        self.table
            .as_ref()
            .map_or_else(EyeRules::default, |t| t.eyes.clone())
    }

    fn eyes_known<'a>(&'a self, w: &World, rules: &EyeRules) -> Known<'a> {
        Known {
            lines: &self.throws,
            fix: self.fix.as_ref(),
            spots: &self.eyes.spots,
            thrown: self.eyes.thrown,
            frames_seen: frames_seen(w, rules),
        }
    }

    fn eyes_note_due(&self, tick: u64, every: u64) -> bool {
        self.eyes
            .noted_at
            .is_none_or(|t| tick.saturating_sub(t) >= every)
    }

    /// The eyes' state to keep across a resume.
    pub fn eyes_kept(&self) -> Kept {
        Kept {
            fix: self.fix,
            throws: self.throws.clone(),
            state: self.eyes.clone(),
        }
    }

    /// The eyes' state read back on a resume.
    pub fn restore_eyes(&mut self, kept: Kept) {
        self.fix = kept.fix;
        self.throws = kept.throws;
        self.eyes = kept.state;
    }

    /// Where the overland leg should walk for the next reading (`way`), with the baseline turned
    /// counter-clockwise; `None` before the first line.
    pub fn eyes_way(&self, w: &World) -> Option<[f64; 2]> {
        let rules = self.eye_rules();
        let feet = w.body?.pos;
        way(&rules, &self.eyes_known(w, &rules), [feet[0], feet[2]], 1.0)
    }

    /// The eyes craft (19c.1), where the bot stands: the next 2x2 craft toward `eyes_target`, or
    /// `None` when no more eyes can be made (or the planner cannot go on: noted).
    pub fn eyes_craft_task(&mut self, w: &World, tick: u64) -> Option<Task> {
        let table = self.table.clone()?;
        let counts = crate::held(w);
        let row = eyes_row(&table, &counts)?;
        let target = row.provides.first().map_or(0, |(_, n)| *n);
        let wear = make::Wear::default();
        let next = match make::make_with_wear(
            &table,
            &counts,
            &|_: BlockKind| false,
            false,
            &wear,
            &row,
        ) {
            Choice::Act(r) => match r.act {
                Some(make::Act::Craft(rec)) if make::fits_grid(rec) => Ok(rec),
                Some(_) | None => Err(format!("{} is not a 2x2 craft", r.id)),
            },
            Choice::Done => return None,
            Choice::Stuck => Err("not makeable from what is held".to_owned()),
        };
        match next {
            Ok(rec) => {
                self.note(
                    tick,
                    format!("eyes: craft {}", rec.id),
                    format!(
                        "toward {target} eyes: eyes {} pearls {} powder {} rods {} held",
                        craft::count(w, ItemKind::EnderEye),
                        craft::count(w, ItemKind::EnderPearl),
                        craft::count(w, ItemKind::BlazePowder),
                        craft::count(w, ItemKind::BlazeRod)
                    ),
                );
                Some(Task::Craft(crate::craft::Craft::new(rec, tick)))
            }
            Err(why) => {
                if self.eyes_note_due(tick, table.eyes.note_every) {
                    self.eyes.noted_at = Some(tick);
                    self.note(tick, "eyes: craft cannot go on".to_owned(), why);
                }
                None
            }
        }
    }

    /// A throw, when the policy has one due here (`due`) and nothing blocks it (`blocked`); a due
    /// throw that is blocked is noted every `note_every`. Its yaw is along the fix or the last
    /// line's bearing.
    pub fn eyes_task(&mut self, w: &World, tick: u64) -> Option<Task> {
        let rules = self.eye_rules();
        let feet = w.body?.pos;
        let flat = [feet[0], feet[2]];
        let why = due(&rules, &self.eyes_known(w, &rules), flat)?;
        if let Some(block) = blocked(w, &rules) {
            if self.eyes_note_due(tick, rules.note_every) {
                self.eyes.noted_at = Some(tick);
                self.note(
                    tick,
                    format!("eye throw {} ({}) waits", self.eyes.thrown + 1, why.name()),
                    block,
                );
            }
            return None;
        }
        let bearing = self
            .fix
            .and_then(|f| unit(sub(f.point, flat)))
            .or_else(|| self.throws.iter().rev().find_map(|l| unit(l.dir)));
        let t = Throw::new(w, why, bearing, &rules, tick);
        self.note(
            tick,
            format!("eye throw {} ({})", self.eyes.thrown + 1, why.name()),
            format!(
                "from ({:.1}, {:.1}), yaw {:.1} pitch {:.1}; eyes held {}; {} lines, fix {}",
                flat[0],
                flat[1],
                t.yaw,
                t.pitch,
                t.eyes_before,
                self.throws.len(),
                self.fix.map_or_else(
                    || "none".to_owned(),
                    |f| format!("({:.0}, {:.0})", f.point[0], f.point[1])
                )
            ),
        );
        Some(Task::Throw(Box::new(t)))
    }

    /// One tick of `Task::Throw`: the step's acts sent, its notes written, a reading added to the
    /// lines and the fix crossed again, and its end counted.
    pub(crate) fn throw_step(&mut self, w: &World, tick: u64) -> Controls {
        let rules = self.eye_rules();
        let Task::Throw(t) = &mut self.task else {
            return Controls::default();
        };
        let out = t.step(w, &rules, tick);
        let end = t.end;
        let from = t.flight.from;
        let used = out.acts.contains(&Act::UseItem);
        self.acts.extend(out.acts);
        for (what, why) in out.said {
            self.note(tick, what, why);
        }
        if used {
            self.eyes.thrown += 1;
            self.eyes.spots.push([from[0], from[2]]);
        }
        if let Some(line) = out.reading {
            self.throws.push(line);
            match fix_why(&self.throws) {
                Ok(f) => {
                    self.fix = Some(f);
                    let feet = w.body.map_or(from, |b| b.pos);
                    self.note(
                        tick,
                        format!(
                            "fix ({:.0}, {:.0}) err along {:.0} across {:.0}",
                            f.point[0], f.point[1], f.err_along, f.err_across
                        ),
                        format!(
                            "{} lines; {:.0} blocks off on the plane",
                            self.throws.len(),
                            norm(sub(f.point, [feet[0], feet[2]]))
                        ),
                    );
                }
                Err(r) => self.note(
                    tick,
                    format!("no fix from {} lines", self.throws.len()),
                    format!(
                        "{}; the fix held: {}",
                        r.name(),
                        self.fix.map_or_else(
                            || "none".to_owned(),
                            |f| format!("({:.0}, {:.0})", f.point[0], f.point[1])
                        )
                    ),
                ),
            }
        }
        match out.status {
            ThrowStatus::Running => out.controls,
            ThrowStatus::Done => {
                match end {
                    Some(End::Kept) => self.eyes.kept += 1,
                    Some(End::Lost) => self.eyes.lost += 1,
                    Some(End::Shattered) => self.eyes.shattered += 1,
                    None => {}
                }
                self.task = Task::Idle;
                self.retry_at = tick;
                self.note(
                    tick,
                    "eye throw over".to_owned(),
                    format!(
                        "thrown {}, kept {}, lost {}, shattered {}; eyes held {}",
                        self.eyes.thrown,
                        self.eyes.kept,
                        self.eyes.lost,
                        self.eyes.shattered,
                        craft::count(w, ItemKind::EnderEye)
                    ),
                );
                Controls::default()
            }
            ThrowStatus::Failed(f) => {
                self.task = Task::Idle;
                self.retry_at = tick + crate::RETRY_TICKS;
                *self.failures.entry(format!("{f:?}")).or_default() += 1;
                self.note(
                    tick,
                    format!("eye throw failed: {f:?}"),
                    format!("thrown {}; choose again", self.eyes.thrown),
                );
                Controls::default()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DEG: f64 = std::f64::consts::PI / 180.0;

    fn toward(from: [f64; 2], to: [f64; 2], off_deg: f64, spread_deg: f64) -> Line {
        let d = unit(sub(to, from)).unwrap();
        Line {
            from,
            dir: turn(d, off_deg),
            spread: spread_deg * DEG,
        }
    }

    fn miss(f: &Fix, t: [f64; 2]) -> f64 {
        norm(sub(f.point, t))
    }

    #[test]
    fn two_clean_lines_cross_at_the_target_with_the_baseline_error_along() {
        let t = [1000.0, 2000.0];
        let lines = [
            toward([0.0, 0.0], t, 0.0, 1.0),
            toward([300.0, 0.0], t, 0.0, 1.0),
        ];
        let f = fix(&lines).unwrap();
        assert!(miss(&f, t) < 1e-6, "{f:?}");
        // About d * spread / sqrt 2 across (d about 2,200) and sqrt 2 * d^2 * spread / b along
        // (b about 270 square to the bearing).
        assert!(f.err_across > 15.0 && f.err_across < 45.0, "{f:?}");
        assert!(f.err_along > 250.0 && f.err_along < 900.0, "{f:?}");
        assert!(f.err_along > 5.0 * f.err_across, "{f:?}");
    }

    #[test]
    fn three_noisy_lines_with_a_side_refine_land_near_the_target() {
        let t = [-1500.0, 1200.0];
        // The refine spot: 350 from the target square to the bearing.
        let b = unit(t).unwrap();
        let refine = [t[0] - b[1] * 350.0, t[1] + b[0] * 350.0];
        let lines = [
            toward([0.0, 0.0], t, 0.5, 1.0),
            toward([190.0, 240.0], t, -0.6, 1.0),
            toward(refine, t, 0.4, 1.0),
        ];
        let f = fix(&lines).unwrap();
        let m = miss(&f, t);
        assert!(m < 40.0, "{f:?} missed by {m}");
        assert!(
            m <= 3.0 * f.err_along.hypot(f.err_across),
            "{f:?} missed by {m}"
        );
        assert!(f.err_along < 60.0 && f.err_across < 60.0, "{f:?}");
    }

    #[test]
    fn at_the_jars_spread_a_140_baseline_fixes_within_the_frame_view() {
        // The heading is exact (the jar): only the configured floor's spread; 1,500 off.
        let r = rules();
        let t = [1500.0, 0.0];
        let lines = [
            toward([0.0, 0.0], t, 0.1, r.spread_floor_deg),
            toward([100.0, 140.0], t, -0.1, r.spread_floor_deg),
        ];
        let f = fix(&lines).unwrap();
        assert!(f.err_along < 128.0, "{f:?}");
        assert!(miss(&f, t) < 128.0, "{f:?}");
    }

    #[test]
    fn lines_within_three_degrees_are_refused_as_parallel() {
        let a = Line {
            from: [0.0, 0.0],
            dir: [0.0, 1.0],
            spread: DEG,
        };
        let b = Line {
            from: [100.0, 0.0],
            dir: turn([0.0, 1.0], 1.0),
            spread: DEG,
        };
        assert_eq!(fix_why(&[a, b]), Err(Refusal::Parallel));
        assert_eq!(fix_why(&[a]), Err(Refusal::TooFew));
        assert_eq!(fix_why(&[]), Err(Refusal::TooFew));
    }

    #[test]
    fn a_crossing_behind_a_throw_is_refused() {
        let a = Line {
            from: [0.0, 0.0],
            dir: [0.0, 1.0],
            spread: DEG,
        };
        let b = Line {
            from: [100.0, 0.0],
            dir: unit([1.0, 1.0]).unwrap(),
            spread: DEG,
        };
        // They cross at (0, -100): behind both.
        assert_eq!(fix_why(&[a, b]), Err(Refusal::Behind));
    }

    #[test]
    fn a_near_line_is_the_fix() {
        let far = toward([0.0, 0.0], [800.0, 800.0], 0.0, 1.0);
        let near = Line {
            from: [812.5, 790.5],
            dir: [0.0, 0.0],
            spread: 2.0,
        };
        let f = fix(&[far, near]).unwrap();
        assert_eq!(f.point, [812.5, 790.5]);
        assert_eq!((f.err_along, f.err_across), (2.0, 2.0));
    }

    fn rules() -> EyeRules {
        EyeRules::default()
    }

    #[test]
    fn a_flight_toward_a_far_point_reads_its_bearing() {
        let to = unit([3.0, -4.0]).unwrap();
        let mut f = Flight {
            id: 7,
            from: [10.0, 64.0, 20.0],
            samples: Vec::new(),
        };
        // Moves every 4 ticks, rising toward 8 up over 12 blocks, each told twice.
        for i in 0..20 {
            let s = 0.6 * f64::from(i);
            let p = [
                10.0 + to[0] * s,
                65.0 + 0.4 * f64::from(i),
                20.0 + to[1] * s,
            ];
            f.push(p);
            f.push(p);
        }
        assert_eq!(f.samples.len(), 20);
        let l = f.reading(&rules()).unwrap();
        assert!(!l.is_point());
        assert!(dot(l.dir, to) > (0.1 * DEG).cos(), "{l:?}");
        assert!(
            (l.spread - rules().spread_floor_deg * DEG).abs() < 1e-9,
            "a perfect fit takes the floor"
        );
        assert!(norm(sub(l.from, [10.0, 20.0])) < 1e-6, "{l:?}");
    }

    #[test]
    fn an_eye_that_sinks_near_is_a_point() {
        let mut f = Flight {
            id: 7,
            from: [0.0, 70.0, 0.0],
            samples: Vec::new(),
        };
        for i in 0..20 {
            let s = 0.3 * f64::from(i);
            f.push([s, 71.0 - 0.3 * f64::from(i), -s]);
        }
        let l = f.reading(&rules()).unwrap();
        assert!(l.is_point());
        assert!((l.from[0] - 5.7).abs() < 1e-9 && (l.from[1] + 5.7).abs() < 1e-9);
        // A flight that did not move gives no line.
        let still = Flight {
            id: 1,
            from: [0.0, 70.0, 0.0],
            samples: vec![[0.0, 71.0, 0.0]; 3],
        };
        assert_eq!(still.line(&rules()), None);
    }

    #[test]
    fn the_policy_throws_first_then_after_a_baseline_then_to_refine() {
        // The plan's first numbers (a 1 degree spread): the mechanism, whatever the config says.
        let r = EyeRules {
            baseline: 300.0,
            baseline_across: 150.0,
            baseline_turn_deg: 35.0,
            refine_from: 400.0,
            refine_offset: 350.0,
            ..rules()
        };
        let none = Known {
            lines: &[],
            fix: None,
            spots: &[],
            thrown: 0,
            frames_seen: false,
        };
        assert_eq!(due(&r, &none, [0.0, 0.0]), Some(Why::First));
        let first = [Line {
            from: [0.0, 0.0],
            dir: [0.0, 1.0],
            spread: DEG,
        }];
        let one = Known {
            lines: &first,
            spots: &[[0.0, 0.0]],
            thrown: 1,
            ..none
        };
        // On the line, or too near: no throw.
        assert_eq!(due(&r, &one, [0.0, 500.0]), None);
        assert_eq!(due(&r, &one, [120.0, 120.0]), None);
        // 300 out at 35 degrees: across 172.
        let spot = way(&r, &one, [0.0, 0.0], 1.0).unwrap();
        assert!((norm(spot) - 332.0).abs() < 1e-6);
        let at = [-(35.0 * DEG).sin() * 310.0, (35.0 * DEG).cos() * 310.0];
        assert!(norm(sub(spot, [at[0] * 332.0 / 310.0, at[1] * 332.0 / 310.0])) < 1e-6);
        assert_eq!(due(&r, &one, at), Some(Why::Baseline));
        // With a fix: a refine within 400, from a fresh spot, once; then the final within 64.
        let f = Fix {
            point: [0.0, 2000.0],
            err_along: 230.0,
            err_across: 25.0,
        };
        let spots = [[0.0, 0.0], at];
        let fixed = Known {
            fix: Some(&f),
            spots: &spots,
            thrown: 2,
            ..one
        };
        assert_eq!(due(&r, &fixed, [0.0, 1500.0]), None);
        let side = way(&r, &fixed, [100.0, 1000.0], 1.0).unwrap();
        assert!((norm(sub(side, f.point)) - 350.0).abs() < 1e-6, "{side:?}");
        assert_eq!(due(&r, &fixed, side), Some(Why::Refine));
        let spots3 = [[0.0, 0.0], at, side];
        let refined = Known {
            spots: &spots3,
            thrown: 3,
            ..fixed
        };
        assert_eq!(way(&r, &refined, side, 1.0), Some(f.point));
        assert_eq!(due(&r, &refined, [0.0, 1700.0]), None);
        assert_eq!(due(&r, &refined, [10.0, 1990.0]), Some(Why::Final));
        // Four thrown, or a frame seen: no more.
        let spent = Known {
            thrown: 4,
            ..refined
        };
        assert_eq!(due(&r, &spent, [10.0, 1990.0]), None);
        let seen = Known {
            frames_seen: true,
            ..refined
        };
        assert_eq!(due(&r, &seen, [10.0, 1990.0]), None);
    }

    #[test]
    fn the_eyes_made_are_the_pearls_up_to_the_powder_and_the_cap() {
        let r = rules();
        let c = |v: &[(ItemKind, i32)]| v.iter().copied().collect::<make::Counts>();
        use ItemKind::{BlazePowder, BlazeRod, EnderEye, EnderPearl};
        assert_eq!(
            eyes_target(&r, &c(&[(EnderPearl, 12), (BlazeRod, 7)])),
            Some(12)
        );
        assert_eq!(
            eyes_target(&r, &c(&[(EnderPearl, 14), (BlazeRod, 7)])),
            Some(14)
        );
        assert_eq!(
            eyes_target(&r, &c(&[(EnderPearl, 16), (BlazeRod, 9)])),
            Some(14)
        );
        assert_eq!(
            eyes_target(&r, &c(&[(EnderPearl, 12), (BlazeRod, 5)])),
            Some(10)
        );
        assert_eq!(
            eyes_target(&r, &c(&[(EnderPearl, 2), (BlazePowder, 1), (EnderEye, 10)])),
            Some(11)
        );
        assert_eq!(eyes_target(&r, &c(&[(EnderEye, 12)])), None);
        assert_eq!(eyes_target(&r, &c(&[(BlazeRod, 7)])), None);
    }
}
