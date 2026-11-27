//! Never stare (issue 17e, `docs/problems/endermen-leave.md`): an enderman a player looks in the
//! face turns on the player, keeps its anger for ever and teleports in (mc-mob-facts). Every look
//! the planner sends goes out in its `Controls` (the movement packets and the use-item packet take
//! the controls' yaw and pitch, `world::World::step`), so `Plan::step` passes each tick's controls
//! through `keep_off` and nothing bypasses it.
//!
//! The test is the 26.1.2 jar's `EnderMan.isBeingStaredBy`, read by javap (probe 45472254333):
//! the unit vector from the player's eye to the enderman's eye point (feet + 2.55; no legs or body
//! test), dotted with the player's view vector, over `1 - 0.025 / distance` (about 6.4 degrees at
//! 4 blocks, 1.6 at 64), with a clip between the two that misses every block's collision shape,
//! and no gaze-disguise item (the carved pumpkin) on the head. The target goal looks within the
//! follow range, 64, every 2 server ticks; a held stare angers it after 3 goal runs (4 to 6
//! ticks), but no staring look is ever sent, not for one tick. Here only full blocks seen exposed
//! stop the line: a slab or an unseen wall that would stop it is assumed not to, the safe side.

use data::{EntityKind, ItemKind};
use physics::Controls;
use world::World;

use crate::path::Cell;

/// The dot product's tolerance, divided by the distance (the jar's `isLookingAtMe(player, 0.025,
/// true, ...)`, probe 45472254333).
pub const STARE_TOLERANCE: f64 = 0.025;
/// The enderman's eye over its feet, the only point the stare tests (probe 45472254333).
pub const ENDERMAN_EYE: f64 = 2.55;
/// A player farther than this from an enderman is never its staring target: the goal's targeting
/// range is the enderman's follow range, 64 (probe 45472254333).
pub const STARE_RANGE: f64 = 64.0;
/// A crouching player's eye height (`Pose.CROUCHING`: 1.27).
pub const SNEAK_EYE: f64 = 1.27;

/// The view vector of a Notchian yaw and pitch, in degrees (vanilla `Entity.calculateViewVector`).
pub fn view(yaw: f32, pitch: f32) -> [f64; 3] {
    let (y, p) = (f64::from(yaw).to_radians(), f64::from(pitch).to_radians());
    [-y.sin() * p.cos(), -p.sin(), y.cos() * p.cos()]
}

/// Whether a cell stops the stare's line: a full block seen exposed. A cell never seen counts as
/// open (strict no X-ray, and the safe side here: an unseen wall is assumed not to be there).
fn stops(w: &World, c: Cell) -> bool {
    crate::cast::seen(w, c).is_some_and(|k| {
        data::generated::shapes::collision(k) == Some(data::types::Collision::Full)
    })
}

/// Whether the line between two points crosses no sight-stopping cell.
pub fn clear(w: &World, from: [f64; 3], to: [f64; 3]) -> bool {
    !crate::scoop::ray_cells(from, to)
        .into_iter()
        .any(|c| stops(w, c))
}

/// Whether a carved pumpkin is worn (menu slot 5, the head): no enderman is provoked by a stare.
fn pumpkin_worn(w: &World) -> bool {
    w.inventory
        .slots
        .get(&5)
        .is_some_and(|(k, _)| *k == ItemKind::CarvedPumpkin)
}

/// The entity id of an enderman that a look from `eye` along `yaw` and `pitch` would provoke, the
/// nearest first; `None` when the look is safe.
pub fn stares(w: &World, eye: [f64; 3], yaw: f32, pitch: f32) -> Option<u32> {
    stares_except(w, eye, yaw, pitch, None)
}

/// `stares`, leaving `except` out: the one enderman the hunt's provoke row stares at on purpose
/// (issue 18f), and only while that row runs.
pub fn stares_except(
    w: &World,
    eye: [f64; 3],
    yaw: f32,
    pitch: f32,
    except: Option<u32>,
) -> Option<u32> {
    if pumpkin_worn(w) {
        return None;
    }
    let feet = w
        .body
        .map_or([eye[0], eye[1] - physics::EYE_HEIGHT, eye[2]], |b| b.pos);
    let v = view(yaw, pitch);
    w.entities
        .iter()
        .filter(|(_, e)| e.kind == EntityKind::Enderman && !e.dead)
        .filter(|(id, _)| except.is_none_or(|x| u32::try_from(**id).ok() != Some(x)))
        .filter_map(|(id, e)| {
            let range = (0..3)
                .map(|i| (e.pos[i] - feet[i]).powi(2))
                .sum::<f64>()
                .sqrt();
            if range > STARE_RANGE {
                return None;
            }
            let target = [e.pos[0], e.pos[1] + ENDERMAN_EYE, e.pos[2]];
            let d = [target[0] - eye[0], target[1] - eye[1], target[2] - eye[2]];
            let len = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
            if len <= f64::EPSILON {
                return u32::try_from(*id).ok().map(|i| (range, i));
            }
            let dot = (v[0] * d[0] + v[1] * d[1] + v[2] * d[2]) / len;
            (dot > 1.0 - STARE_TOLERANCE / len && clear(w, eye, target))
                .then(|| u32::try_from(*id).ok().map(|i| (range, i)))
                .flatten()
        })
        .min_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)))
        .map(|(_, id)| id)
}

/// The eye of the body under these controls: lower when crouching.
pub fn eye(pos: [f64; 3], c: &Controls) -> [f64; 3] {
    let h = if c.sneak {
        SNEAK_EYE
    } else {
        physics::EYE_HEIGHT
    };
    [pos[0], pos[1] + h, pos[2]]
}

/// How close a look from `eye` comes to an enderman's eyes over `feet`, in stare tolerances
/// (`1 - dot` over `STARE_TOLERANCE / distance`: under 1 is inside the jar's cone), with the
/// distance; `None` at the eye itself.
pub fn tolerances(eye: [f64; 3], yaw: f32, pitch: f32, feet: [f64; 3]) -> Option<(f64, f64)> {
    let t = [feet[0], feet[1] + ENDERMAN_EYE, feet[2]];
    let d = [t[0] - eye[0], t[1] - eye[1], t[2] - eye[2]];
    let len = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
    if len <= f64::EPSILON {
        return None;
    }
    let v = view(yaw, pitch);
    let dot = (v[0] * d[0] + v[1] * d[1] + v[2] * d[2]) / len;
    Some(((1.0 - dot) * len / STARE_TOLERANCE, len))
}

/// The guard (issue 20, `docs/problems/end-gaze.md`): a look is kept this many stare tolerances
/// off every enderman's eyes, not one. The jar's cone is judged on the server's copy of both
/// bodies, and ours lags it: the enderman's place is the last one sent (a walk of up to 0.3 a
/// tick, a teleport of up to 32), the look goes out with the feet after the tick's move (a fall
/// of 2 a tick on a clutch drop), and the pose (crouching or not) changes a tick late. Four is
/// the race summary's own near miss (`observe::NEAR_MISS`), so a look kept outside it is neither
/// a stare nor a near miss there: twice the cone's angle (about 3.2 degrees at 64, 12.8 at 4).
pub const GUARD: f64 = 4.0;

/// The pitches a kept look may take, every `PITCH_STEP` degrees: the one nearest the asked-for
/// pitch that clears the guard is sent, so a clutch's bucket still points near its landing and a
/// look down stays near the feet.
const PITCH_STEP: f32 = 1.0;

/// Why a look was kept off an enderman.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kept {
    /// Inside the jar's own cone, from the eye the planner holds now.
    Stare,
    /// Outside it, but inside the guard, from some eye the server may judge it from.
    Guard,
}

/// The eyes the server may judge a look from this tick: standing and crouching (the pose
/// changes a tick late), at the feet now, after this tick's move and after the next
/// (`pos + vel`, `pos + 2 vel`: the feet the look's packet carries).
pub fn eyes(pos: [f64; 3], vel: [f64; 3]) -> Vec<[f64; 3]> {
    (0..3)
        .flat_map(|n| {
            let k = f64::from(n);
            let p = [
                pos[0] + k * vel[0],
                pos[1] + k * vel[1],
                pos[2] + k * vel[2],
            ];
            [
                [p[0], p[1] + physics::EYE_HEIGHT, p[2]],
                [p[0], p[1] + SNEAK_EYE, p[2]],
            ]
        })
        .collect()
}

/// An enderman a look could provoke: its id, its feet, and the eyes it can be stared at from.
type Watcher = (u32, [f64; 3], Vec<[f64; 3]>);

/// The endermen a look could provoke, nearest first: within `STARE_RANGE` (and the guard) of
/// the feet, all but `except`, each with the eyes of `eyes` it can be stared at from. In the End
/// no line of sight is trusted (the island is open, a pillar's own blocks under the feet are the
/// only walls, and the race counts a stare with no line test); elsewhere a seen full block
/// between stops it, as the jar's clip does. The lines are tested once here, not for each pitch.
fn watchers(w: &World, eyes: &[[f64; 3]], except: Option<u32>) -> Vec<Watcher> {
    if pumpkin_worn(w) {
        return Vec::new();
    }
    let Some(feet) = w.body.map(|b| b.pos) else {
        return Vec::new();
    };
    let end = crate::end::in_end(w);
    let mut out: Vec<(f64, Watcher)> = w
        .entities
        .iter()
        .filter(|(_, e)| e.kind == EntityKind::Enderman && !e.dead)
        .filter_map(|(id, e)| {
            let id = u32::try_from(*id).ok()?;
            if except == Some(id) {
                return None;
            }
            let range = (0..3)
                .map(|i| (e.pos[i] - feet[i]).powi(2))
                .sum::<f64>()
                .sqrt();
            if range > STARE_RANGE + GUARD {
                return None;
            }
            let target = [e.pos[0], e.pos[1] + ENDERMAN_EYE, e.pos[2]];
            let from: Vec<[f64; 3]> = eyes
                .iter()
                .copied()
                .filter(|eye| end || clear(w, *eye, target))
                .collect();
            (!from.is_empty()).then_some((range, (id, e.pos, from)))
        })
        .collect();
    out.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.0.cmp(&b.1.0)));
    out.into_iter().map(|(_, x)| x).collect()
}

/// The first of `watchers` (the nearest) that a look comes within `GUARD` tolerances of, with
/// how close it came, in tolerances.
fn within_guard(watchers: &[Watcher], yaw: f32, pitch: f32) -> Option<(u32, f64)> {
    watchers.iter().find_map(|(id, pos, from)| {
        let k = from
            .iter()
            .map(|eye| tolerances(*eye, yaw, pitch, *pos).map_or(0.0, |(k, _)| k))
            .fold(f64::INFINITY, f64::min);
        (k < GUARD).then_some((*id, k))
    })
}

/// The enderman nearest the bot that a look from any of `eyes` comes within `GUARD` tolerances
/// of (`watchers`), with how close it came, in tolerances.
pub fn guarded(
    w: &World,
    eyes: &[[f64; 3]],
    yaw: f32,
    pitch: f32,
    except: Option<u32>,
) -> Option<(u32, f64)> {
    within_guard(&watchers(w, eyes, except), yaw, pitch)
}

/// The pitch nearest `pitch` (along the same yaw: a walk's heading) whose look clears the guard
/// from every one of `eyes`, the lower first on a tie; with none clear (an enderman above and one
/// below, both near), the one that comes least close to any.
pub fn safe_pitch(w: &World, eyes: &[[f64; 3]], yaw: f32, pitch: f32, except: Option<u32>) -> f32 {
    let seen = watchers(w, eyes, except);
    #[allow(clippy::cast_possible_truncation)]
    let steps = (180.0 / PITCH_STEP) as i32;
    let mut all: Vec<f32> = (0..=steps)
        .map(|i| {
            #[allow(clippy::cast_precision_loss)]
            let p = -90.0 + i as f32 * PITCH_STEP;
            p
        })
        .collect();
    all.sort_by(|a, b| {
        (a - pitch)
            .abs()
            .total_cmp(&(b - pitch).abs())
            .then(b.total_cmp(a))
    });
    all.iter()
        .copied()
        .find(|p| within_guard(&seen, yaw, *p).is_none())
        .unwrap_or_else(|| {
            all.iter()
                .copied()
                .map(|p| (p, within_guard(&seen, yaw, p).map_or(GUARD, |g| g.1)))
                .fold((pitch, -1.0), |best, x| if x.1 > best.1 { x } else { best })
                .0
        })
}

/// The controls with their look kept off every enderman: unchanged when the look provokes none;
/// else the same yaw (the walk's heading is the yaw) with the nearest pitch that clears the
/// guard. With the enderman it would have provoked, when changed.
pub fn keep_off(w: &World, c: Controls) -> (Controls, Option<u32>) {
    keep_off_controls(w, c, None)
}

/// A look from `eye` kept off every enderman but `except` (issue 18f: the provoke row stares at
/// its one target on purpose; every other look stays filtered): the same yaw (a walk's heading)
/// and the nearest pitch that clears the guard from this eye, or the look unchanged when it
/// does.
pub fn keep_off_except(
    w: &World,
    eye: [f64; 3],
    yaw: f32,
    pitch: f32,
    except: Option<u32>,
) -> (f32, f32) {
    if guarded(w, &[eye], yaw, pitch, except).is_none() {
        return (yaw, pitch);
    }
    (yaw, safe_pitch(w, &[eye], yaw, pitch, except))
}

/// `keep_off` with one enderman exempt (`keep_off_except`): the provoke row's target. With the
/// enderman the look would have provoked, when changed.
pub fn keep_off_controls(w: &World, c: Controls, except: Option<u32>) -> (Controls, Option<u32>) {
    let (c, why) = keep_off_why(w, c, except);
    (c, why.map(|(id, _)| id))
}

/// `keep_off_controls`, saying why the look was changed: inside the jar's cone from the eye
/// held now (`Kept::Stare`), or only inside the guard (`Kept::Guard`).
pub fn keep_off_why(
    w: &World,
    c: Controls,
    except: Option<u32>,
) -> (Controls, Option<(u32, Kept)>) {
    let Some(body) = w.body else {
        return (c, None);
    };
    #[allow(clippy::cast_possible_truncation)]
    let yaw = c.yaw as f32;
    let pitch = c.pitch.unwrap_or(w.player.pitch);
    let all = eyes(body.pos, body.vel);
    let Some((id, _)) = guarded(w, &all, yaw, pitch, except) else {
        return (c, None);
    };
    let why = if stares_except(w, eye(body.pos, &c), yaw, pitch, except).is_some() {
        Kept::Stare
    } else {
        Kept::Guard
    };
    (
        Controls {
            pitch: Some(safe_pitch(w, &all, yaw, pitch, except)),
            ..c
        },
        Some((id, why)),
    )
}

/// Whether this tick's acts pour a water bucket while falling: a clutch's pour (E4's spikes),
/// whose look is the bucket's aim (`Act::UseItem` carries the controls' look, and the server
/// snaps to it before the use, `handleUseItem`).
pub fn pouring(w: &World, acts: &[world::Act]) -> bool {
    w.body.is_some_and(|b| !b.on_ground)
        && crate::dig::held_item(w) == Some(ItemKind::WaterBucket)
        && acts.contains(&world::Act::UseItem)
}

/// What `keep_look` sends (issue 20, `docs/problems/end-gaze.md`): the look kept off every
/// enderman (`keep_off_why`), except a clutch's pour (`pouring`), which is sent as asked: the
/// water must land under the feet, and the fall is the bigger danger (race 46038776204 lane 5 at
/// 34,040: a pour bent from 90 to 60 stared at the enderman the old bend did not test). With the
/// enderman the look comes near and why, in both cases, and whether it was sent unbent.
pub fn keep_off_act(
    w: &World,
    c: Controls,
    except: Option<u32>,
    pour: bool,
) -> (Controls, Option<(u32, Kept)>, bool) {
    let (kept, why) = keep_off_why(w, c, except);
    if pour && why.is_some() {
        return (c, why, true);
    }
    (kept, why, false)
}
