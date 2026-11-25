//! Eyes of ender (issue 19, docs/problems/eyes-model.md): a thrown eye's flight, its drop or
//! shatter, read from the 26.1.2 jar (probe 45685968956: `javap -c -p` of `EnderEyeItem` and
//! `EyeOfEnder`).
//!
//! - `EnderEyeItem.use`: a look whose block hit is an end portal frame passes (the frame is
//!   filled by `useOn` instead, not modelled). Else the target is
//!   `findNearestMapStructure(#eye_of_ender_located, player.blockPosition(), 100, false)` as a
//!   lower-corner point; none (the Nether: strongholds are overworld structures) consumes the use
//!   and throws nothing, the eye kept. Else an `EyeOfEnder` at the player's `getY(0.5)`, its
//!   delta zero, `signalTo` the point, one eye consumed.
//! - `signalTo`: beyond 12 on the plane the eye's own target is 12 along the line and 8 up;
//!   within 12 it is the structure's point. `surviveAfterDeath = nextInt(5) > 0`.
//! - `tick`: the position moves by the old delta with no collision (`setPos`), then
//!   `updateDeltaMovement`: the horizontal speed lerps 0.0025 of the way to the distance left,
//!   along the line to the target (both it and the vertical speed times 0.8 within 1 of it); the
//!   vertical speed moves 0.015 of the way to +1 (below the target) or -1. At life 81 it is
//!   discarded: an item entity of the eye where it was (4 in 5), or level event 2003 (shatter).
//!
//! The model has no structure generator: the stronghold's point is the model's to be told
//! (`Sim::stronghold`, from RCON's `locate` in a parity row, or a test's synthetic one).

use data::{EntityKind, ItemKind};
use protocol::metadata::MetaValue;
use protocol::{EncodeError, S2c, State};

use crate::{Sim, Wire, wire};

/// Beyond this on the plane the eye flies to a point this far along the line
/// (`EyeOfEnder.TOO_FAR_DISTANCE`).
pub const TOO_FAR: f64 = 12.0;
/// ... and this high over its spawn (`TOO_FAR_SIGNAL_HEIGHT`).
pub const TOO_FAR_RISE: f64 = 8.0;
/// The horizontal speed's lerp toward the distance left (`updateDeltaMovement`'s 0.0025).
const SPEED_LERP: f64 = 0.0025;
/// Within this of the target on the plane both speeds are cut (`updateDeltaMovement`'s 1.0).
const NEAR: f64 = 1.0;
/// ... by this (0.8).
const NEAR_CUT: f64 = 0.8;
/// The vertical speed's lerp toward +1 or -1 (0.015).
const CLIMB_LERP: f64 = 0.015;
/// Ticks of life: it is discarded once `life > 80`.
pub const LIFE: u32 = 80;
/// `surviveAfterDeath = nextInt(5) > 0`.
const SURVIVE_ONE_IN: f64 = 5.0;
/// The eye spawns at the player's `getY(0.5)`: half its 1.8 box.
const SPAWN_HEIGHT: f64 = 0.9;
/// `EntityType.EYE_OF_ENDER`'s update interval: the tracker sends where it is every 4 ticks.
const UPDATE_EVERY: u32 = 4;
/// The shatter (`LevelEvent.PARTICLES_EYE_OF_ENDER_DEATH`).
pub const SHATTER_EVENT: i32 = 2003;
/// The eye's item stack is its synced data 8 (after `Entity`'s eight).
const EYE_ITEM_INDEX: u8 = 8;
/// A dropped eye's kick, as `new ItemEntity(level, x, y, z, stack)` gives one: sideways
/// `nextDouble() * 0.2 - 0.1` on x and z, 0.2 up.
const DROP_SIDEWAYS: f64 = 0.2;
const DROP_UP: f64 = 0.2;

/// A flying eye of ender.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Eye {
    pub pos: [f64; 3],
    pub delta: [f64; 3],
    pub target: [f64; 3],
    pub life: u32,
    pub survive: bool,
}

/// `signalTo`: where an eye spawned at `from` flies, toward a structure at `point`.
pub fn eye_target(from: [f64; 3], point: [f64; 3]) -> [f64; 3] {
    let (dx, dz) = (point[0] - from[0], point[2] - from[2]);
    let d = dx.hypot(dz);
    if d > TOO_FAR {
        [
            from[0] + dx / d * TOO_FAR,
            from[1] + TOO_FAR_RISE,
            from[2] + dz / d * TOO_FAR,
        ]
    } else {
        point
    }
}

/// One tick of an eye's flight (`EyeOfEnder.tick` on the server): the move by the old delta,
/// then the new delta. Pure: the model's flight is this function folded.
pub fn eye_step(e: &mut Eye) {
    let pos = [
        e.pos[0] + e.delta[0],
        e.pos[1] + e.delta[1],
        e.pos[2] + e.delta[2],
    ];
    let (hx, hz) = (e.target[0] - pos[0], e.target[2] - pos[2]);
    let dist = hx.hypot(hz);
    let mut speed = e.delta[0].hypot(e.delta[2]);
    speed += SPEED_LERP * (dist - speed);
    let mut vy = e.delta[1];
    if dist < NEAR {
        speed *= NEAR_CUT;
        vy *= NEAR_CUT;
    }
    let toward = if pos[1] - e.delta[1] < e.target[1] {
        1.0
    } else {
        -1.0
    };
    // `vec34.scale(e / d)`: at the target itself the division is 0/0 (NaN in Java too); the
    // eye then stays on the plane, as no finite heading can be taken from it.
    let k = if dist > 0.0 { speed / dist } else { 0.0 };
    e.delta = [hx * k, vy + (toward - vy) * CLIMB_LERP, hz * k];
    e.pos = pos;
    e.life += 1;
}

impl Sim {
    /// The stronghold an eye thrown in this world flies to, if any: the overworld's, as the model
    /// was told it (`with_stronghold`).
    fn eye_point(&self) -> Option<[f64; 3]> {
        (self.world().dimension == "minecraft:overworld")
            .then_some(self.stronghold)
            .flatten()
    }

    /// Tell the model where the overworld's nearest stronghold is (a lower-corner point: RCON's
    /// `locate` names x and z; y is the structure's, 0 for a chunk's corner).
    pub fn with_stronghold(mut self, point: [f64; 3]) -> Self {
        self.stronghold = Some(point);
        self
    }

    /// `EnderEyeItem.use` with an eye held: `Some` when the use was the eye's (a throw, or
    /// nothing at all in a world with no stronghold), `None` when the held item is not an eye.
    pub(crate) fn use_eye(&mut self) -> Option<Result<Vec<Wire>, EncodeError>> {
        let (item, count) = self.survival.held_stack()?;
        if item != ItemKind::EnderEye {
            return None;
        }
        // A look at an end portal frame is the frame's (`useOn`), never a throw.
        if self
            .look_hit(false)
            .and_then(|((x, y, z), _)| self.state_at(x, y, z))
            .and_then(data::block_of_state)
            .is_some_and(|k| k == data::BlockKind::EndPortalFrame)
        {
            return Some(Ok(Vec::new()));
        }
        let Some(point) = self.eye_point() else {
            return Some(Ok(Vec::new()));
        };
        let from = [self.pos[0], self.pos[1] + SPAWN_HEIGHT, self.pos[2]];
        let survive = (self.survival.next_double() * SURVIVE_ONE_IN).floor() > 0.0;
        let eye = Eye {
            pos: from,
            delta: [0.0; 3],
            target: eye_target(from, point),
            life: 0,
            survive,
        };
        let held = self.survival.held_slot_index();
        if let Some(s) = self.survival.slots.get_mut(held) {
            *s = (count > 1).then_some((item, count - 1));
        }
        let id = self.survival.next_id();
        self.eyes.insert(id, eye);
        self.eyes_thrown += 1;
        Some(self.eye_added(id, from))
    }

    fn eye_added(&mut self, id: i32, at: [f64; 3]) -> Result<Vec<Wire>, EncodeError> {
        let mut uuid = [0u8; 16];
        uuid[12..].copy_from_slice(&id.to_be_bytes());
        uuid[0] = 0xe7;
        Ok(vec![
            wire(
                State::Play,
                &S2c::AddEntity {
                    id,
                    uuid,
                    kind: EntityKind::EyeOfEnder.id(),
                    x: at[0],
                    y: at[1],
                    z: at[2],
                },
            )?,
            wire(
                State::Play,
                &S2c::SetEntityData {
                    id,
                    entries: vec![(
                        EYE_ITEM_INDEX,
                        MetaValue::Item(crate::survival::slot_of(Some((ItemKind::EnderEye, 1)))),
                    )],
                },
            )?,
        ])
    }

    /// Every flying eye's tick: its move, its place told every `UPDATE_EVERY` ticks, its end.
    pub(crate) fn tick_eyes(&mut self) -> Result<Vec<Wire>, EncodeError> {
        let mut out = Vec::new();
        let ids: Vec<i32> = self.eyes.keys().copied().collect();
        for id in ids {
            let Some(mut e) = self.eyes.get(&id).copied() else {
                continue;
            };
            eye_step(&mut e);
            if e.life > LIFE {
                self.eyes.remove(&id);
                out.push(wire(State::Play, &S2c::RemoveEntities { ids: vec![id] })?);
                if e.survive {
                    self.eyes_dropped += 1;
                    let vx = self.survival.next_double() * DROP_SIDEWAYS - DROP_SIDEWAYS / 2.0;
                    let vz = self.survival.next_double() * DROP_SIDEWAYS - DROP_SIDEWAYS / 2.0;
                    out.extend(self.spawn_item(
                        (ItemKind::EnderEye, 1),
                        e.pos,
                        [vx, DROP_UP, vz],
                        0,
                    )?);
                } else {
                    self.eyes_shattered += 1;
                    let cell = (
                        e.pos[0].floor() as i32,
                        e.pos[1].floor() as i32,
                        e.pos[2].floor() as i32,
                    );
                    out.push(wire(
                        State::Play,
                        &S2c::LevelEvent {
                            event: SHATTER_EVENT,
                            pos: cell,
                            data: 0,
                            global: false,
                        },
                    )?);
                }
                continue;
            }
            self.eyes.insert(id, e);
            if e.life.is_multiple_of(UPDATE_EVERY) {
                out.push(wire(
                    State::Play,
                    &S2c::EntityPositionSync {
                        id,
                        x: e.pos[0],
                        y: e.pos[1],
                        z: e.pos[2],
                    },
                )?);
            }
        }
        Ok(out)
    }
}
