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
/// The pitches tried, in order, for a look that would stare: straight down first (the feet, and a
/// dig or a place needs no look on the server, only reach), then less steep.
const SAFE_PITCHES: [f32; 7] = [90.0, 60.0, 30.0, 0.0, -30.0, -60.0, -90.0];

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

/// The controls with their look kept off every enderman: unchanged when the look provokes none;
/// else the same yaw (the walk's heading is the yaw) with the first of `SAFE_PITCHES` that
/// provokes none. With the enderman it would have provoked, when changed.
pub fn keep_off(w: &World, c: Controls) -> (Controls, Option<u32>) {
    let Some(body) = w.body else {
        return (c, None);
    };
    #[allow(clippy::cast_possible_truncation)]
    let yaw = c.yaw as f32;
    let pitch = c.pitch.unwrap_or(w.player.pitch);
    let eye = eye(body.pos, &c);
    let Some(id) = stares(w, eye, yaw, pitch) else {
        return (c, None);
    };
    let safe = SAFE_PITCHES
        .into_iter()
        .find(|p| stares(w, eye, yaw, *p).is_none())
        .unwrap_or(SAFE_PITCHES[0]);
    (
        Controls {
            pitch: Some(safe),
            ..c
        },
        Some(id),
    )
}
