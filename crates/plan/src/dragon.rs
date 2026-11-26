//! The dragon and the win (issue 20g, docs/problems/end-dragon.md): the port of the fight half of
//! clojurecraft's `dragon.clj`. With every crystal gone the bot waits while the dragon flies, and
//! once it has sat on the exit portal's pillar (the fountain) it stands on the basin's floor behind
//! the body, out of the head's and neck's way, and strikes the body while its health is over the
//! line. A breath cloud near is left by the nearest cell out of its reach, never under the head; a
//! blow of the dragon's sends the bot out of reach for `HIT_HOLD_TICKS`; in a lull it eats. Once
//! the dragon is dead the bot walks into the exit portal, and the credits (`win_game`, `w.won`) are
//! answered with the respawn the client sends after them (`Act::Respawn`, `client_command` 0).
//!
//! The model is pure and read from the world each tick: the phase from the dragon's entity data,
//! the yaw from its moves, the clouds' radii from theirs. Every decision that changes what the bot
//! does writes a ledger note.

use data::{BlockKind, EntityKind, ItemKind};
use physics::Controls;
use protocol::metadata::MetaValue;
use world::{Act, World};

use crate::hold::{self, Hold};
use crate::path::{Cell, Goal};
use crate::walk::{Arrive, TaskStatus, Walk};
use crate::{Plan, Task, eat};

/// The exit portal's pillar on the plane, where the dragon perches: `EndPodiumFeature` at the
/// End's 0 0 (clojurecraft `dragon/fountain`).
pub const FOUNTAIN: [f64; 2] = [0.5, 0.5];
/// Blocks on the plane from the fountain the dragon perches within (`dragon/perch-radius`).
pub const PERCH_RADIUS: f64 = 6.0;
/// ... and under this height (`dragon/perch-below`; the island's surface is about 60 to 65).
pub const PERCH_BELOW: f64 = 70.0;
/// Ticks it holds there before it counts as perched (`dragon/perch-ticks`).
pub const PERCH_TICKS: u32 = 10;
/// The dragon's phase in its entity data: `EnderDragon.DATA_PHASE`, an INT after the living
/// entity's 8 to 14 and the mob's 15 (clojurecraft `entity/phase-index` 16).
pub const PHASE_INDEX: u8 = 16;
/// A cloud's radius in its entity data: `AreaEffectCloud.DATA_RADIUS`, a FLOAT after the
/// entity's 0 to 7 (clojurecraft `entity/radius-index` 8).
pub const RADIUS_INDEX: u8 = 8;
/// The head part's distance from the body along the facing (`EnderDragon.aiStep`: the head at
/// sin yaw · 6.5, −cos yaw · 6.5), and the neck's.
pub const HEAD_OUT: f64 = 6.5;
pub const NECK_OUT: f64 = 5.5;
/// The head part's middle over the dragon's feet while it sits: `getHeadYOffset` is −1 for a
/// sitting phase and the part is a 1 by 1 box, so its middle is half a block over that. Parity
/// e05 checks it against the server's part boxes.
pub const HEAD_RISE: f64 = -0.5;
/// Half the head part's box (1 by 1).
pub const HEAD_HALF: f64 = 0.5;
/// The dragon's own box, 16 by 8 (`EntityType.ENDER_DRAGON`): an attack on its id is judged by
/// reach to it and lands on the body part (a quarter of the blow and one).
pub const BODY_HALF: f64 = 8.0;
pub const BODY_HEIGHT: f64 = 8.0;
/// Blocks along its facing the sitting dragon's head reaches: the head 6.5 out, its box inflated
/// 1.5 more (clojurecraft `mob/bite-out`).
pub const BITE_OUT: f64 = 8.0;
/// Blocks across its facing a way must keep: the neck's box, the body's half width and a half
/// block (`dragon/clear-across`).
pub const CLEAR_ACROSS: f64 = 3.3;
/// Blocks behind the body (along its facing, negative) a stand must be (`dragon/behind-by`).
pub const BEHIND_BY: f64 = 1.0;
/// Blocks less behind the body a stand already held may be than one chosen anew: the hysteresis
/// that keeps a stand while the dragon turns a little (`dragon/stand-slack`; gym chain-w13 swapped
/// two stands every 0.8 s).
pub const STAND_SLACK: f64 = 1.0;
/// Blocks behind it the feet may come to, the dragon turning, before the head counts as on them
/// (`dragon/leave-by`).
pub const LEAVE_BY: f64 = 0.5;
/// Blocks between the samples of a way tested for the head (`dragon/way-step`).
pub const WAY_STEP: f64 = 0.5;
/// Blocks across its facing a way round the head goes out to, the feet must be at before it goes
/// along, and past the head's reach it turns at (`dragon/side-out`, `side-clear`, `side-margin`).
pub const SIDE_OUT: f64 = 5.0;
pub const SIDE_CLEAR: f64 = 3.8;
pub const SIDE_MARGIN: f64 = 1.0;
/// A breath cloud's radius when its data has not said: the sitting dragon's flame cloud is 5
/// (`DragonSittingFlamingPhase`), a fireball's 3 growing to 7.
pub const BREATH_RADIUS: f64 = 5.0;
/// A fireball's cloud: radius 3.0 at first, growing to 7.0 over 600 ticks (E1, `DragonFireball`):
/// a cloud's radius is never taken under that, whatever its data says.
pub const CLOUD_MIN: f64 = 3.0;
pub const CLOUD_MAX: f64 = 7.0;
pub const CLOUD_GROWTH: f64 = 4.0 / 600.0;
/// Blocks beyond a cloud's radius the bot keeps (`dragon/breath-margin`).
pub const BREATH_MARGIN: f64 = 1.5;
/// More than that the way back to a stand keeps, so a cloud growing as the bot walks does not
/// turn it back each tick (`dragon/breath-hold`).
pub const BREATH_HOLD: f64 = 2.0;
/// Eye to a target's box within which a strike attacks: the server's entity reach.
pub const STRIKE_REACH: f64 = 3.0;
/// Health one blow of the head or neck takes through the iron set with boots: 10 before armour
/// (`EnderDragon.hurt`), armour 15 and no toughness take 10/25 of it (`CombatRules`), so 6.0.
/// clojurecraft measured 6.8 without boots (armour 13), which the same rule gives. E1/E6 measure
/// it on a real server (parity e05).
pub const HEAD_BLOW: f32 = 6.0;
/// The player's full health.
pub const FULL_HEALTH: f32 = 20.0;
/// Server ticks after the dragon's own blow that the bot stays out of its reach and strikes no
/// more (`dragon/hit-hold-ticks`).
pub const HIT_HOLD_TICKS: u64 = 100;
/// Blocks from the fountain a step out of the head's reach goes to (`dragon/step-out-radius`).
pub const STEP_OUT_RADIUS: f64 = 8.0;
/// The ring out of a breath's reach is looked for from and to, in this many directions
/// (`dragon/escape-from`, `escape-to`, `escape-ways`).
pub const ESCAPE_FROM: i32 = 4;
pub const ESCAPE_TO: i32 = 12;
pub const ESCAPE_WAYS: u32 = 16;
/// The basin's floor: cells whose middle is this near the fountain on the plane, past the pillar
/// (`EndPodiumFeature`: the floor within 2.5, the rim to 3.5; clojurecraft stood 2 out).
pub const BASIN_MIN: f64 = 1.4;
pub const BASIN_MAX: f64 = 2.6;
/// Blocks from the fountain on the plane the feet count as in the basin (`dragon/walk-in-to`).
pub const IN_BASIN: f64 = 3.0;
/// The rim's ring a walk into the exit portal starts from: within this of the fountain the bot
/// steers straight in (the rim is 2.5 to 3.5 out).
pub const RIM: f64 = 3.6;
/// Blocks on the plane from a stand's middle the feet count as at it (`dragon/stand-near`).
pub const STAND_NEAR: f64 = 1.0;
/// Blocks round the feet no hostile may be for a lull (`dragon/calm-radius`).
pub const CALM_RADIUS: f64 = 8.0;
/// Heights over and under the feet a stand's ground is looked for in.
pub const GROUND_UP: i32 = 3;
pub const GROUND_DOWN: i32 = 8;
/// A fight hands back to the planner this often, so the End's order is asked again
/// (`dragon/perch-hold-ms`, 60 s).
pub const FIGHT_TICKS: u64 = 1200;
/// Server ticks the credits' respawn waits for the overworld before it is asked again, and how
/// many times.
pub const RESPAWN_WAIT: u64 = 200;
pub const RESPAWN_TRIES: u32 = 5;
/// Ticks between the notes of a pocket passed over for the dragon.
pub const FIRST_NOTE_TICKS: u64 = 200;
/// The End's dimension name.
pub const THE_END: &str = "minecraft:the_end";

/// The dragon's phases (`EnderDragonPhase`), by id.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Phase {
    HoldingPattern,
    Strafe,
    LandingApproach,
    Landing,
    Takeoff,
    SittingFlaming,
    SittingScanning,
    SittingAttacking,
    ChargingPlayer,
    Dying,
    Hover,
    Unknown(i32),
}

impl Phase {
    pub fn of(id: i32) -> Phase {
        match id {
            0 => Phase::HoldingPattern,
            1 => Phase::Strafe,
            2 => Phase::LandingApproach,
            3 => Phase::Landing,
            4 => Phase::Takeoff,
            5 => Phase::SittingFlaming,
            6 => Phase::SittingScanning,
            7 => Phase::SittingAttacking,
            8 => Phase::ChargingPlayer,
            9 => Phase::Dying,
            10 => Phase::Hover,
            n => Phase::Unknown(n),
        }
    }

    /// On its perch (sitting flaming, scanning, attacking).
    pub fn sitting(self) -> bool {
        matches!(
            self,
            Phase::SittingFlaming | Phase::SittingScanning | Phase::SittingAttacking
        )
    }

    pub fn name(self) -> String {
        match self {
            Phase::HoldingPattern => "holding".to_owned(),
            Phase::Strafe => "strafe".to_owned(),
            Phase::LandingApproach => "landing-approach".to_owned(),
            Phase::Landing => "landing".to_owned(),
            Phase::Takeoff => "takeoff".to_owned(),
            Phase::SittingFlaming => "sitting-flaming".to_owned(),
            Phase::SittingScanning => "sitting-scanning".to_owned(),
            Phase::SittingAttacking => "sitting-attacking".to_owned(),
            Phase::ChargingPlayer => "charging".to_owned(),
            Phase::Dying => "dying".to_owned(),
            Phase::Hover => "hover".to_owned(),
            Phase::Unknown(n) => format!("phase-{n}"),
        }
    }
}

/// The dragon as the world shows it. `phase` is −1 until its data says; `yaw` 0 until a move
/// says (`yaw_seen`); `health` its entity data's (200 full).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DragonView {
    pub pos: [f64; 3],
    pub yaw: f32,
    pub phase: i32,
    pub health: f32,
}

impl DragonView {
    pub fn phase(&self) -> Option<Phase> {
        (self.phase >= 0).then(|| Phase::of(self.phase))
    }
}

/// The live dragon the server shows: its id and entity, never one it said died.
pub fn dragon(w: &World) -> Option<(i32, &world::Entity)> {
    w.entities
        .iter()
        .find(|(_, e)| e.kind == EntityKind::EnderDragon && !died(e))
        .map(|(id, e)| (*id, e))
}

fn died(e: &world::Entity) -> bool {
    e.dead || e.statuses.iter().any(|(_, s)| *s == 3)
}

/// The dragon's view and id, or `None` when none is shown alive.
pub fn view(w: &World) -> Option<(i32, DragonView)> {
    let (id, e) = dragon(w)?;
    let phase = match e.metadata.get(&PHASE_INDEX) {
        Some(MetaValue::Int(p)) => *p,
        Some(_) | None => -1,
    };
    Some((
        id,
        DragonView {
            pos: e.pos,
            yaw: e.yaw.unwrap_or(0.0),
            phase,
            health: e.health().unwrap_or(200.0),
        },
    ))
}

/// Has a move told the dragon's yaw?
pub fn yaw_seen(w: &World) -> bool {
    dragon(w).is_some_and(|(_, e)| e.yaw.is_some())
}

/// Is a dragon shown that the server said died (entity event 3)?
pub fn died_in_view(w: &World) -> bool {
    w.entities
        .values()
        .any(|e| e.kind == EntityKind::EnderDragon && died(e))
}

/// Within `PERCH_RADIUS` of the fountain on the plane and under `PERCH_BELOW`.
pub fn over_fountain(v: &DragonView) -> bool {
    flat(v.pos, [FOUNTAIN[0], 0.0, FOUNTAIN[1]]) <= PERCH_RADIUS && v.pos[1] < PERCH_BELOW
}

/// Does the dragon sit on the fountain: over it for `PERCH_TICKS`, and in a sitting phase once
/// its data has said a phase (landing over the pillar is not yet sat)?
pub fn perched(v: &DragonView, perched_ticks: u32) -> bool {
    over_fountain(v) && perched_ticks >= PERCH_TICKS && v.phase().is_none_or(Phase::sitting)
}

/// Is it on its perch or coming down onto it: its head and neck are then over the basin and the
/// rim (clojurecraft `grounded?`: a landing dragon's head touched a bot on the rim).
pub fn grounded(v: &DragonView) -> bool {
    match v.phase() {
        Some(p) => p.sitting() || p == Phase::Landing,
        None => over_fountain(v),
    }
}

/// The unit vector on the plane from the body toward the head for a yaw: `EnderDragon.aiStep`
/// puts the head at (sin yaw, −cos yaw), the opposite of any other mob's look.
pub fn facing(yaw: f32) -> [f64; 2] {
    let r = f64::from(yaw).to_radians();
    [r.sin(), -r.cos()]
}

/// (along, across) of point `p` from the dragon on the plane: along its facing (toward the head,
/// negative behind), and across it (positive to the facing's left).
pub fn ahead(v: &DragonView, p: [f64; 3]) -> (f64, f64) {
    let [fx, fz] = facing(v.yaw);
    let (dx, dz) = (p[0] - v.pos[0], p[2] - v.pos[2]);
    (dx * fx + dz * fz, dz * fx - dx * fz)
}

/// The point on the plane `along` and `across` (`ahead`'s frame) from the dragon.
pub fn beside(v: &DragonView, along: f64, across: f64) -> [f64; 2] {
    let [fx, fz] = facing(v.yaw);
    [
        v.pos[0] + along * fx - across * fz,
        v.pos[2] + along * fz + across * fx,
    ]
}

/// The middle of the sitting dragon's head part.
pub fn head_point(v: &DragonView) -> [f64; 3] {
    let [x, z] = beside(v, HEAD_OUT, 0.0);
    [x, v.pos[1] + HEAD_RISE, z]
}

/// Is `p` at least `by` behind the body along its facing? Always while it is not grounded: a
/// flying dragon's head is not at its perch.
pub fn behind(v: &DragonView, p: [f64; 3], by: f64) -> bool {
    !grounded(v) || ahead(v, p).0 <= -by
}

/// Is `p` where the grounded dragon's head and neck may come: along its facing from `LEAVE_BY`
/// behind the body out to `BITE_OUT`, within `CLEAR_ACROSS` of it?
pub fn before_head(v: &DragonView, p: [f64; 3]) -> bool {
    if !grounded(v) {
        return false;
    }
    let (along, across) = ahead(v, p);
    (-LEAVE_BY..=BITE_OUT).contains(&along) && across.abs() <= CLEAR_ACROSS
}

/// Has the head come onto `p`: not `LEAVE_BY` behind the body, within `BITE_OUT` on the plane?
pub fn in_bite(v: &DragonView, p: [f64; 3]) -> bool {
    grounded(v) && !behind(v, p, LEAVE_BY) && flat(v.pos, p) < BITE_OUT
}

/// Does the straight way on the plane from `a` to `b` pass before the head (sampled every
/// `WAY_STEP`)?
pub fn bite_on_way(v: &DragonView, a: [f64; 3], b: [f64; 3]) -> bool {
    if !grounded(v) {
        return false;
    }
    let n = (flat(a, b) / WAY_STEP).ceil().max(1.0) as u32;
    (0..=n).any(|k| {
        let t = f64::from(k) / f64::from(n);
        before_head(v, lerp(a, b, t))
    })
}

/// Blocks the way out from before the head keeps from the head part's middle and the neck's:
/// the head's 1 by 1 box and the neck's 3 by 3, each inflated 1 (`EnderDragon.aiStep`'s
/// `hurt` boxes), with the player's half width and a margin.
pub const HEAD_CLEAR: f64 = 2.5;
pub const NECK_CLEAR: f64 = 3.0;

/// Is the straight way from `a` to `b` clear of the head: never before it from outside; from
/// feet already before it (no way out of there passes no sample before it), a way that comes no
/// nearer the head than `HEAD_CLEAR` nor the neck than `NECK_CLEAR`, or than the feet already
/// are when they are nearer: a way out that moves away.
pub fn way_clear(v: &DragonView, a: [f64; 3], b: [f64; 3]) -> bool {
    if !before_head(v, a) {
        return !bite_on_way(v, a, b);
    }
    let keeps = |out: f64, clear: f64| {
        let [px, pz] = beside(v, out, 0.0);
        let p = [px, 0.0, pz];
        segment_distance(p, a, b) >= clear.min(flat(p, a)) - 1e-9
    };
    keeps(HEAD_OUT, HEAD_CLEAR) && keeps(NECK_OUT, NECK_CLEAR)
}

/// The point on the plane a walk from `pos` to `b` goes by first while the straight way passes
/// before the head, or `None`: with the feet less than `SIDE_CLEAR` beside its facing, out to
/// `SIDE_OUT` on their side short of its reach; else along beside it to `b`'s place along it
/// (behind the stands' line for a stand behind, past the head for the head stand; clojurecraft
/// `around`).
pub fn around(v: &DragonView, pos: [f64; 3], b: [f64; 3]) -> Option<[f64; 2]> {
    if !way_clear(v, pos, b) {
        let (along, across) = ahead(v, pos);
        let side = if across < 0.0 { -SIDE_OUT } else { SIDE_OUT };
        let to = ahead(v, b).0;
        let to = if to < 0.0 {
            to.min(-(BEHIND_BY + SIDE_MARGIN))
        } else {
            to.max(BITE_OUT + SIDE_MARGIN)
        };
        return Some(if across.abs() < SIDE_CLEAR {
            beside(
                v,
                along.clamp(-(BEHIND_BY + SIDE_MARGIN), BITE_OUT + SIDE_MARGIN),
                side,
            )
        } else {
            beside(v, to, side)
        });
    }
    None
}

/// A breath cloud or fireball: where, and its radius.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Breath {
    pub id: i32,
    pub pos: [f64; 3],
    pub radius: f64,
}

impl Breath {
    /// Blocks on the plane the bot keeps from its middle: the radius and `BREATH_MARGIN`.
    pub fn reach(&self) -> f64 {
        self.radius + BREATH_MARGIN
    }
}

/// The dragon's breath clouds and fireballs the server shows, with their radii (a cloud's from
/// its data; a fireball's the cloud it makes, 3 at first).
pub fn breaths(w: &World) -> Vec<Breath> {
    w.entities
        .iter()
        .filter_map(|(id, e)| {
            let radius = match e.kind {
                // Never under a fireball's cloud as it grows (race 45873410249 lane 0: a cloud
                // shown with radius 0.0 on the feet was stood in for 80 ticks, 20 to dead).
                EntityKind::AreaEffectCloud => {
                    let age = w.session.server_ticks.saturating_sub(e.first_seen) as f64;
                    let grown = (CLOUD_MIN + age * CLOUD_GROWTH).min(CLOUD_MAX);
                    match e.metadata.get(&RADIUS_INDEX) {
                        Some(MetaValue::Float(r)) if r.is_finite() => f64::from(*r).max(grown),
                        Some(_) | None => BREATH_RADIUS.max(grown),
                    }
                }
                EntityKind::DragonFireball => 3.0,
                _ => return None,
            };
            Some(Breath {
                id: *id,
                pos: e.pos,
                radius,
            })
        })
        .collect()
}

/// Blocks over a cloud's floor the feet may be and still be in it: its box is 0.5 tall
/// (`AreaEffectCloud`), and a margin of 1. And blocks under it the head may be: the bunker three
/// down and a pillar two up are out of it, a body stood on its floor is in it.
pub const BREATH_OVER: f64 = 1.5;
pub const BREATH_UNDER: f64 = 1.0;
/// Columns round the feet the cell out of a breath is looked for in.
pub const OUT_SEARCH: i32 = 9;
/// Blocks ahead of the feet along a walk's heading tested for a breath (`breath_ahead`).
pub const AHEAD: f64 = 1.5;
/// Ticks between the notes of a walk held at a breath's edge.
pub const AHEAD_NOTE_TICKS: u64 = 100;

/// The nearest breath whose reach takes in a body with its feet at `feet`: within its reach on
/// the plane, and the body's height meeting the cloud's (`BREATH_OVER`, `BREATH_UNDER`).
pub fn breath_on(breaths: &[Breath], feet: [f64; 3]) -> Option<Breath> {
    breaths
        .iter()
        .filter(|b| {
            flat(b.pos, feet) <= b.reach()
                && feet[1] <= b.pos[1] + BREATH_OVER
                && feet[1] + physics::HEIGHT + BREATH_UNDER >= b.pos[1]
        })
        .min_by(|a, b| flat(a.pos, feet).total_cmp(&flat(b.pos, feet)))
        .copied()
}

/// The ground cell nearest the feet (within `OUT_SEARCH` columns) that no breath reaches
/// (`breath_on`), not before a grounded dragon's head nor on a way past it; `None` when none
/// is loaded.
pub fn out_of_breath(w: &World, feet: [f64; 3]) -> Option<[i32; 3]> {
    let clouds = breaths(w);
    let v = view(w).map(|(_, v)| v).filter(grounded);
    let fc = cell_of(feet);
    let mut cells = crate::path::Cells::new(w);
    let mut best: Option<(f64, [i32; 3])> = None;
    for x in (fc[0] - OUT_SEARCH)..=(fc[0] + OUT_SEARCH) {
        for z in (fc[2] - OUT_SEARCH)..=(fc[2] + OUT_SEARCH) {
            let d0 = flat(feet, mid([x, 0, z]));
            if d0 > f64::from(OUT_SEARCH) || best.is_some_and(|(bd, _)| d0 > bd) {
                continue;
            }
            let Some(y) = ((fc[1] - 3)..=(fc[1] + 2))
                .rev()
                .find(|y| cells.standable((x, *y, z)))
            else {
                continue;
            };
            let c = [x, y, z];
            let p = mid(c);
            if breath_on(&clouds, p).is_some()
                || v.is_some_and(|v| before_head(&v, p) || !way_clear(&v, feet, p))
            {
                continue;
            }
            if best.is_none_or(|(bd, bc)| d0 < bd || (d0 == bd && c < bc)) {
                best = Some((d0, c));
            }
        }
    }
    best.map(|(_, c)| c)
}

/// Does a breath reach `p` on the plane (its reach and `extra`)?
pub fn breathed(breaths: &[Breath], p: [f64; 3], extra: f64) -> bool {
    breaths.iter().any(|b| flat(b.pos, p) <= b.reach() + extra)
}

/// Is `p` within a breath's own radius, where it hurts?
pub fn in_breath(breaths: &[Breath], p: [f64; 3]) -> bool {
    breaths.iter().any(|b| flat(b.pos, p) <= b.radius)
}

/// Does a breath reach the straight way on the plane from `a` to `b`?
pub fn breath_on_way(breaths: &[Breath], a: [f64; 3], b: [f64; 3], extra: f64) -> bool {
    breaths
        .iter()
        .any(|br| segment_distance(br.pos, a, b) <= br.reach() + extra)
}

/// Blocks on the plane from `p` to the segment from `a` to `b`.
pub fn segment_distance(p: [f64; 3], a: [f64; 3], b: [f64; 3]) -> f64 {
    let (dx, dz) = (b[0] - a[0], b[2] - a[2]);
    let l2 = dx * dx + dz * dz;
    let t = if l2 < 1e-9 {
        0.0
    } else {
        (((p[0] - a[0]) * dx + (p[2] - a[2]) * dz) / l2).clamp(0.0, 1.0)
    };
    ((p[0] - (a[0] + t * dx)).powi(2) + (p[2] - (a[2] + t * dz)).powi(2)).sqrt()
}

/// The health over which the bot stands in the perched dragon's reach, and whether the head may
/// be struck there: two blows and the danger line when that is under full health (three blows
/// in 0.6 s killed one at 18.5), else one blow and the line, struck only from behind the body.
pub fn perch_line(danger: f32, blow: f32) -> f32 {
    let two = danger + 2.0 * blow;
    if two < FULL_HEALTH {
        two
    } else {
        danger + blow
    }
}

/// Blocks the head stand's feet are from the head part's box along the facing: E6's parity e05
/// (run 45866776019) struck from 2.0 off the box, full 6.0 strikes, never hurt (outside the box
/// grown by 1).
pub const HEAD_STAND_OFF: f64 = 2.0;
/// The head's blow box (its 1 by 1 grown by 1) and the neck's (3 by 3 grown by 1), half widths.
pub const HEAD_BLOW_HALF: f64 = 1.5;
pub const NECK_BLOW_HALF: f64 = 2.5;
/// Reach the head stand keeps short of `STRIKE_REACH`, so a dragon shifting a little leaves it
/// in reach.
pub const REACH_SLACK: f64 = 0.25;

/// Are feet at `p` (on the plane, with the player's half width) out of the head's and the neck's
/// blow boxes? Heights are not counted: under them in the basin is counted as in them.
pub fn clear_of_blow(v: &DragonView, p: [f64; 3]) -> bool {
    let (along, across) = ahead(v, p);
    let hw = physics::HALF_WIDTH;
    let out =
        |centre: f64, half: f64| (along - centre).abs() - hw >= half || across.abs() - hw >= half;
    out(HEAD_OUT, HEAD_BLOW_HALF) && out(NECK_OUT, NECK_BLOW_HALF)
}

/// May feet at cell `c` strike the head from there: out of the blow boxes, out of every breath's
/// reach, the eye within `reach` of the head's box.
fn head_stand_ok(v: &DragonView, clouds: &[Breath], c: [i32; 3], reach: f64) -> bool {
    let p = mid(c);
    let eye = [p[0], p[1] + physics::EYE_HEIGHT, p[2]];
    clear_of_blow(v, p) && !breathed(clouds, p, 0.0) && head_reach(v, eye) <= reach
}

/// The head stand: a ground cell `HEAD_STAND_OFF` beyond the head's box along the facing (or
/// about it), out of the blow boxes and every breath, the eye within reach of the head's box with
/// `REACH_SLACK` to spare; the nearest the ideal point.
pub fn head_stand(v: &DragonView, w: &World) -> Option<[i32; 3]> {
    let feet = w.body?.pos;
    let clouds = breaths(w);
    let [ix, iz] = beside(v, HEAD_OUT + HEAD_HALF + HEAD_STAND_OFF, 0.0);
    let ideal = [ix, 0.0, iz];
    let (cx, cz) = (ix.floor() as i32, iz.floor() as i32);
    let mut best: Option<(f64, [i32; 3])> = None;
    for x in (cx - 2)..=(cx + 2) {
        for z in (cz - 2)..=(cz + 2) {
            let Some(c) = ground(w, x, z, feet[1].floor() as i32) else {
                continue;
            };
            if !head_stand_ok(v, &clouds, c, STRIKE_REACH - REACH_SLACK) {
                continue;
            }
            let d = flat(ideal, mid(c));
            if best.is_none_or(|(bd, bc)| d < bd || (d == bd && c < bc)) {
                best = Some((d, c));
            }
        }
    }
    best.map(|(_, c)| c)
}

/// The head stand to hold: the one held while it still strikes (no slack), else `head_stand`.
pub fn keep_head_stand(v: &DragonView, w: &World, held: Option<[i32; 3]>) -> Option<[i32; 3]> {
    if let Some(h) = held
        && head_stand_ok(v, &breaths(w), h, STRIKE_REACH)
    {
        return Some(h);
    }
    head_stand(v, w)
}

/// Did the dragon itself hit the bot within `HIT_HOLD_TICKS` (its blow, not its breath)?
pub fn struck_by_dragon(w: &World) -> bool {
    let now = w.session.server_ticks;
    w.hits.iter().any(|h| {
        h.cause == Some(EntityKind::EnderDragon)
            && h.direct == Some(EntityKind::EnderDragon)
            && h.at_tick + HIT_HOLD_TICKS >= now
    })
}

/// The feet cell standing on the ground of column (x, z), looked for from `GROUND_UP` over the
/// feet down to `GROUND_DOWN` under them: the highest a body may stand in.
pub fn ground(w: &World, x: i32, z: i32, feet_y: i32) -> Option<[i32; 3]> {
    let mut cells = crate::path::Cells::new(w);
    ((feet_y - GROUND_DOWN)..=(feet_y + GROUND_UP))
        .rev()
        .find(|y| cells.standable((x, *y, z)))
        .map(|y| [x, y, z])
}

/// The ground cells of the basin's floor round the fountain (`BASIN_MIN` to `BASIN_MAX`).
pub fn basin_cells(w: &World, feet_y: i32) -> Vec<[i32; 3]> {
    let mut out = Vec::new();
    for x in -3..=3 {
        for z in -3..=3 {
            let d = flat(mid([x, 0, z]), [FOUNTAIN[0], 0.0, FOUNTAIN[1]]);
            if (BASIN_MIN..=BASIN_MAX).contains(&d)
                && let Some(c) = ground(w, x, z, feet_y)
            {
                out.push(c);
            }
        }
    }
    out
}

/// The stand behind the perched dragon's body: of the basin's floor cells, those at least
/// `BEHIND_BY` behind it and out of every breath's reach, the nearest the feet. `None` with no
/// body or no such cell loaded.
pub fn stand_behind(v: &DragonView, w: &World) -> Option<[i32; 3]> {
    let feet = w.body?.pos;
    let clouds = breaths(w);
    basin_cells(w, feet[1].floor() as i32)
        .into_iter()
        .filter(|c| !breathed(&clouds, mid(*c), 0.0) && behind(v, mid(*c), BEHIND_BY))
        .min_by(|a, b| {
            flat(feet, mid(*a))
                .total_cmp(&flat(feet, mid(*b)))
                .then(a.cmp(b))
        })
}

/// The stand to hold: the one held while it is out of every breath, the head not on it, and no
/// more than `STAND_SLACK` short of `BEHIND_BY` behind the body; else `stand_behind`.
pub fn keep_stand(v: &DragonView, w: &World, held: Option<[i32; 3]>) -> Option<[i32; 3]> {
    let clouds = breaths(w);
    if let Some(h) = held
        && !breathed(&clouds, mid(h), 0.0)
        && !in_bite(v, mid(h))
        && behind(v, mid(h), BEHIND_BY - STAND_SLACK)
    {
        return Some(h);
    }
    stand_behind(v, w)
}

/// The ground cell nearest the feet, of those `r` out from the fountain for each of `radii` in
/// `ESCAPE_WAYS` directions, that no breath reaches, that is not before the head nor on a way
/// past it; `None` when none is loaded (clojurecraft `ring-cell`).
pub fn ring_cell(w: &World, v: Option<&DragonView>, radii: &[f64]) -> Option<[i32; 3]> {
    let feet = w.body?.pos;
    let feet_cell = cell_of(feet);
    let clouds = breaths(w);
    let mut best: Option<(f64, [i32; 3])> = None;
    for r in radii {
        for k in 0..ESCAPE_WAYS {
            let a = f64::from(k) * std::f64::consts::TAU / f64::from(ESCAPE_WAYS);
            let x = (FOUNTAIN[0] + r * a.cos()).floor() as i32;
            let z = (FOUNTAIN[1] + r * a.sin()).floor() as i32;
            let Some(c) = ground(w, x, z, feet_cell[1]) else {
                continue;
            };
            if c == feet_cell
                || breathed(&clouds, mid(c), 0.0)
                || v.is_some_and(|v| before_head(v, mid(c)) || !way_clear(v, feet, mid(c)))
            {
                continue;
            }
            let d = flat(feet, mid(c));
            if best.is_none_or(|(bd, bc)| d < bd || (d == bd && c < bc)) {
                best = Some((d, c));
            }
        }
    }
    best.map(|(_, c)| c)
}

/// The nearest cell out of every breath's reach and the head's (`ESCAPE_FROM` to `ESCAPE_TO` out).
pub fn escape_cell(w: &World, v: Option<&DragonView>) -> Option<[i32; 3]> {
    let radii: Vec<f64> = (ESCAPE_FROM..=ESCAPE_TO).map(f64::from).collect();
    ring_cell(w, v, &radii)
}

/// The nearest cell `r` out from the fountain beyond the head's reach.
pub fn step_out_cell(w: &World, v: Option<&DragonView>, r: f64) -> Option<[i32; 3]> {
    ring_cell(w, v, &[r])
}

/// The exit portal's cells seen near the fountain (lit after the dragon's death), the nearest the
/// feet first. Its top face is open to the air, so a cell in view is a cell seen.
pub fn portal_cells(w: &World) -> Vec<[i32; 3]> {
    let Some(feet) = w.body.map(|b| b.pos) else {
        return Vec::new();
    };
    let fy = feet[1].floor() as i32;
    let mut out = Vec::new();
    for x in -3..=3 {
        for z in -3..=3 {
            for y in (fy - GROUND_DOWN)..=(fy + GROUND_UP) {
                if w.block_at(world::BlockPos::new(x, y, z)) == Some(BlockKind::EndPortal) {
                    out.push([x, y, z]);
                }
            }
        }
    }
    out.sort_by(|a, b| {
        flat(feet, mid(*a))
            .total_cmp(&flat(feet, mid(*b)))
            .then(a.cmp(b))
    });
    out
}

/// Blocks from the eye to an axis-aligned box `[x0, y0, z0, x1, y1, z1]`.
pub fn box_reach(eye: [f64; 3], b: [f64; 6]) -> f64 {
    let d = |v: f64, lo: f64, hi: f64| (lo - v).max(0.0).max(v - hi);
    let (dx, dy, dz) = (
        d(eye[0], b[0], b[3]),
        d(eye[1], b[1], b[4]),
        d(eye[2], b[2], b[5]),
    );
    (dx * dx + dy * dy + dz * dz).sqrt()
}

/// Eye to the head part's box.
pub fn head_reach(v: &DragonView, eye: [f64; 3]) -> f64 {
    let h = head_point(v);
    box_reach(
        eye,
        [
            h[0] - HEAD_HALF,
            h[1] - HEAD_HALF,
            h[2] - HEAD_HALF,
            h[0] + HEAD_HALF,
            h[1] + HEAD_HALF,
            h[2] + HEAD_HALF,
        ],
    )
}

/// Eye to the dragon's own box.
pub fn body_reach(v: &DragonView, eye: [f64; 3]) -> f64 {
    let p = v.pos;
    box_reach(
        eye,
        [
            p[0] - BODY_HALF,
            p[1],
            p[2] - BODY_HALF,
            p[0] + BODY_HALF,
            p[1] + BODY_HEIGHT,
            p[2] + BODY_HALF,
        ],
    )
}

/// Blocks on the plane between two points.
pub fn flat(a: [f64; 3], b: [f64; 3]) -> f64 {
    ((a[0] - b[0]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
}

fn lerp(a: [f64; 3], b: [f64; 3], t: f64) -> [f64; 3] {
    [
        a[0] + t * (b[0] - a[0]),
        a[1] + t * (b[1] - a[1]),
        a[2] + t * (b[2] - a[2]),
    ]
}

/// A cell's middle, at its floor.
pub fn mid(c: [i32; 3]) -> [f64; 3] {
    [
        f64::from(c[0]) + 0.5,
        f64::from(c[1]),
        f64::from(c[2]) + 0.5,
    ]
}

pub fn cell_of(p: [f64; 3]) -> [i32; 3] {
    [
        p[0].floor() as i32,
        p[1].floor() as i32,
        p[2].floor() as i32,
    ]
}

fn from_fountain(p: [f64; 3]) -> f64 {
    flat(p, [FOUNTAIN[0], 0.0, FOUNTAIN[1]])
}

/// What the fight wants this tick (`decide`).
#[derive(Clone, Debug, PartialEq)]
pub enum Want {
    /// The dragon is dead: into the exit portal's cell.
    Exit([i32; 3]),
    /// Walk to a cell, and why.
    Go([i32; 3], &'static str),
    /// Strike: the entity, and whether it is the head part.
    Strike { entity: i32, head: bool },
    /// Eat for health back in a lull.
    Eat,
    /// Stand where the feet are, looking down, and why.
    Hold(&'static str),
}

/// What the fight is told besides the world.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Ask {
    /// The danger line (`[survival] danger_health`).
    pub danger: f32,
    /// One head blow through the armour worn.
    pub blow: f32,
    /// Ticks the dragon has held over the fountain.
    pub perched_ticks: u32,
    /// The stand held.
    pub stand: Option<[i32; 3]>,
    /// The bunker (E3's `Plan.bunker`), the wait while the dragon flies.
    pub bunker: Option<[i32; 3]>,
    /// The dragon was seen dead (status 3, or its boss bar gone).
    pub dead: bool,
    /// Food is held that heals.
    pub food: bool,
    /// The way round the head held (`DragonState::way`).
    pub way: Option<[i32; 3]>,
}

/// The fight's choice for this tick, pure: the clojurecraft `toward-dragon` order, with the
/// bunker for the wait and the perch line from the armour.
pub fn decide(w: &World, ask: &Ask) -> Want {
    let Some(body) = w.body else {
        return Want::Hold("no body");
    };
    let feet = body.pos;
    let feet_cell = cell_of(feet);
    let eye = [feet[0], feet[1] + physics::EYE_HEIGHT, feet[2]];
    let health = w.player.health.unwrap_or(FULL_HEALTH);
    let clouds = breaths(w);
    if ask.dead {
        if let Some(p) = portal_cells(w).into_iter().next() {
            return Want::Exit(p);
        }
        // Not lit in view yet: wait on the basin's floor, the portal comes there.
        return match basin_cells(w, feet_cell[1]).into_iter().min_by(|a, b| {
            flat(feet, mid(*a))
                .total_cmp(&flat(feet, mid(*b)))
                .then(a.cmp(b))
        }) {
            Some(c) if flat(feet, mid(c)) > STAND_NEAR => Want::Go(c, "to the exit portal"),
            Some(_) | None => Want::Hold("waiting for the exit portal"),
        };
    }
    let v = view(w).map(|(_, v)| v);
    let id = view(w).map(|(id, _)| id);
    let line = perch_line(ask.danger, ask.blow);
    let ground_dragon = v.filter(grounded);
    // A breath near: hold on the stand in the basin out of the cloud itself while the dragon
    // sits (a walk up the rim goes into the neck), else out by the nearest cell.
    if breathed(&clouds, feet, 0.0) {
        let at_stand = ask.stand.is_some_and(|s| flat(feet, mid(s)) <= STAND_NEAR);
        let holds = ground_dragon.is_some()
            && from_fountain(feet) <= IN_BASIN
            && !in_breath(&clouds, feet)
            && at_stand;
        if !holds {
            return match escape_cell(w, ground_dragon.as_ref()) {
                Some(c) => Want::Go(c, "out of the breath"),
                None => Want::Hold("breath near, no cell out of it loaded"),
            };
        }
    }
    // The dragon's own blow: out of its reach for the hold.
    if struck_by_dragon(w) {
        let r = STEP_OUT_RADIUS.max(from_fountain(feet) + 4.0);
        return match step_out_cell(w, ground_dragon.as_ref(), r) {
            Some(c) if flat(feet, mid(c)) > STAND_NEAR => Want::Go(c, "out of reach after a blow"),
            Some(_) | None => Want::Hold("after a blow"),
        };
    }
    let lull = ground_dragon.is_none()
        && clouds.is_empty()
        && crate::defend::nearest_foe(w, CALM_RADIUS).is_none();
    let hurt = health < FULL_HEALTH && w.player.food.is_some_and(|f| f < 20);
    let (Some(v), Some(id)) = (v, id) else {
        return if lull && hurt && ask.food {
            Want::Eat
        } else {
            wait(w, ask, feet, "no dragon shown")
        };
    };
    if !grounded(&v) {
        if lull && hurt && ask.food {
            return Want::Eat;
        }
        return wait(w, ask, feet, "the dragon flies");
    }
    // Grounded from here: on the perch or landing on it.
    // Under the line: out past the head stand's reach and the wings', to the wait's ring.
    if health <= line {
        if from_fountain(feet) < WAIT_FROM || in_bite(&v, feet) {
            return match step_out_cell(w, Some(&v), WAIT_RING) {
                Some(c) => Want::Go(c, "under the perch line"),
                None => Want::Hold("under the perch line, no cell out loaded"),
            };
        }
        return Want::Hold("under the perch line");
    }
    if !yaw_seen(w) {
        return Want::Hold("the dragon's yaw not yet told");
    }
    let sat = perched(&v, ask.perched_ticks);
    // Sat: the head part (the dragon's id + 1, a full blow; E6's e05) from out of its blow box.
    if sat && clear_of_blow(&v, feet) && head_reach(&v, eye) <= STRIKE_REACH {
        return Want::Strike {
            entity: id + 1,
            head: true,
        };
    }
    if sat && let Some(hs) = keep_head_stand(&v, w, ask.stand) {
        if breath_on_way(&clouds, feet, mid(hs), BREATH_HOLD) {
            return Want::Hold("a breath on the way to the head stand");
        }
        // The way round held is kept until it is reached or blocked (race 45873410249 lane 0:
        // the way round and the head stand swapped every tick 9 off the fountain).
        if let Some(wp) = ask.way
            && flat(feet, mid(wp)) > STAND_NEAR
            && !before_head(&v, mid(wp))
            && !breathed(&clouds, mid(wp), 0.0)
            && way_clear(&v, feet, mid(wp))
        {
            return Want::Go(wp, "round the head");
        }
        if let Some(p) = around(&v, feet, mid(hs))
            && let Some(c) = ground(w, p[0].floor() as i32, p[1].floor() as i32, feet_cell[1])
            && flat(feet, mid(c)) > STAND_NEAR
        {
            return Want::Go(c, "round the head");
        }
        if flat(feet, mid(hs)) > STAND_NEAR {
            return Want::Go(hs, "to the head stand");
        }
    }
    // Landing, not yet sat, or no head stand open: out of its reach to the wait's ring, never
    // into the basin (race 45873410249 lane 0: the landing dragon's head took 10 from a bot
    // walking to a stand behind it, 4.4 along and 2.9 across).
    if from_fountain(feet) < WAIT_FROM || in_bite(&v, feet) {
        return match step_out_cell(w, Some(&v), WAIT_RING) {
            Some(c) if flat(feet, mid(c)) > STAND_NEAR => Want::Go(c, "out of the landing's reach"),
            Some(_) | None => Want::Hold("in the landing's reach, no cell out loaded"),
        };
    }
    Want::Hold(if sat {
        "no head stand open"
    } else {
        "the dragon not yet sat"
    })
}

/// Blocks from the fountain the wait with no bunker goes out to, and holds at from: past the
/// wings' reach of a dragon on the pillar (4.5 across plus 2 plus 4 inflated, 10.5).
pub const WAIT_RING: f64 = 12.0;
pub const WAIT_FROM: f64 = 11.0;

/// The wait while the dragon flies: the bunker when there is one, else a cell on the ring
/// `WAIT_RING` out, out of the breath.
fn wait(w: &World, ask: &Ask, feet: [f64; 3], why: &'static str) -> Want {
    if let Some(b) = ask.bunker {
        return if flat(feet, mid(b)) <= STAND_NEAR && (feet[1] - f64::from(b[1])).abs() < 1.5 {
            Want::Hold(why)
        } else {
            Want::Go(b, "to the bunker")
        };
    }
    // No bunker: out past the wings' reach at takeoff (4.5 across, the box inflated 4, a push
    // of 4 and 5 damage: E1's jar reading), on the ring, until it perches again.
    if from_fountain(feet) >= WAIT_FROM {
        return Want::Hold(why);
    }
    match ring_cell(w, None, &[WAIT_RING]) {
        Some(c) if flat(feet, mid(c)) > STAND_NEAR => Want::Go(c, "out to wait on the ring"),
        Some(_) | None => Want::Hold(why),
    }
}

/// The dragon's record across the run, kept by `Plan::dragon_watch` every tick.
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct DragonState {
    /// Ticks the dragon has held over the fountain.
    pub perched_ticks: u32,
    /// The phase last noted.
    pub phase: Option<i32>,
    /// A boss bar was shown in the End, and the dragon was shown.
    pub boss_seen: bool,
    pub seen: bool,
    /// The dragon's death noted, and how.
    pub dead: bool,
    /// The credits noted and the respawns sent for them.
    pub credits: bool,
    pub respawn_at: Option<u64>,
    pub respawns: u32,
    /// Back in the overworld after the credits.
    pub home: bool,
    /// The stand held.
    pub stand: Option<[i32; 3]>,
    /// Strikes sent: on the body and on the head.
    pub strikes: u32,
    pub head_strikes: u32,
    /// The dragon's health at the last strike, for the blow's note.
    pub last_health: Option<f32>,
    /// The walk out of a breath in hand, the walks begun, and the walks held at a breath's edge
    /// with the last note.
    #[serde(default)]
    pub breath_walk: Option<Walk>,
    #[serde(default)]
    pub breath_outs: u32,
    #[serde(default)]
    pub breath_waits: u64,
    #[serde(default)]
    pub breath_waited_at: u64,
    /// The way round the head held, and how many were chosen.
    #[serde(default)]
    pub way: Option<[i32; 3]>,
    #[serde(default)]
    pub ways: u32,
    /// Ticks a provoked enderman's pocket was passed over for the dragon, and the last note.
    #[serde(default)]
    pub first_skips: u64,
    #[serde(default)]
    pub first_noted_at: u64,
}

/// The fight in hand: a task (`Task::Dragon`).
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Fight {
    pub since: u64,
    /// The walk in hand and the cell it goes to.
    pub walk: Option<Walk>,
    /// The meal in hand.
    pub eat: Option<eat::Eat>,
    /// The last strike, in loop ticks.
    pub last_swing: Option<u64>,
    /// The last want noted, so a note is made on a change only.
    pub said: String,
}

impl Fight {
    pub fn new(tick: u64) -> Self {
        Self {
            since: tick,
            walk: None,
            eat: None,
            last_swing: None,
            said: String::new(),
        }
    }
}

fn in_end(w: &World) -> bool {
    w.dimension.as_ref().is_some_and(|d| d.name == THE_END)
}

fn looking_down(w: &World) -> Controls {
    Controls {
        yaw: f64::from(w.player.yaw),
        pitch: Some(90.0),
        ..Controls::default()
    }
}

impl Plan {
    /// The dragon's record, every tick (the one call site in `step_inner`): its phases and perch,
    /// its death, and the credits answered with a respawn. `Some` holds every other row while the
    /// credits wait for the overworld.
    pub(crate) fn dragon_watch(&mut self, w: &World, tick: u64) -> Option<Controls> {
        let now = w.session.server_ticks;
        if in_end(w) {
            if !w.bosses.is_empty() {
                self.dragon.boss_seen = true;
            }
            if let Some((_, v)) = view(w) {
                self.dragon.seen = true;
                self.dragon.perched_ticks = if over_fountain(&v) {
                    self.dragon.perched_ticks.saturating_add(1)
                } else {
                    0
                };
                if v.phase >= 0 && self.dragon.phase != Some(v.phase) {
                    let from = self
                        .dragon
                        .phase
                        .map_or_else(|| "none".to_owned(), |p| Phase::of(p).name());
                    self.dragon.phase = Some(v.phase);
                    self.note(
                        tick,
                        format!("dragon phase {from} -> {}", Phase::of(v.phase).name()),
                        format!(
                            "at ({:.0}, {:.0}, {:.0}), {:.0} from the fountain, health {:.0}",
                            v.pos[0],
                            v.pos[1],
                            v.pos[2],
                            from_fountain(v.pos),
                            v.health
                        ),
                    );
                }
            }
            if !self.dragon.dead {
                let status = died_in_view(w);
                let bar = self.dragon.boss_seen && w.bosses.is_empty() && !w.won;
                if status || bar || w.won {
                    self.dragon.dead = true;
                    self.note(
                        tick,
                        "dragon dead".to_owned(),
                        if status {
                            "entity event 3".to_owned()
                        } else if bar {
                            "its boss bar removed".to_owned()
                        } else {
                            "the credits".to_owned()
                        },
                    );
                }
            }
        }
        if !w.won || self.dragon.home {
            return None;
        }
        if !self.dragon.credits {
            self.dragon.credits = true;
            self.note(
                tick,
                "credits".to_owned(),
                "win_game: the exit portal entered with the dragon dead; the respawn the client sends after the credits".to_owned(),
            );
        }
        let home = !in_end(w) && w.body.is_some();
        if home && self.dragon.respawn_at.is_some() {
            self.dragon.home = true;
            let d = w.dimension.as_ref().map_or("none", |d| d.name.as_str());
            self.note(
                tick,
                format!("back from the End in {d}"),
                format!("{} respawns sent", self.dragon.respawns),
            );
            return None;
        }
        let due = self
            .dragon
            .respawn_at
            .is_none_or(|at| now.saturating_sub(at) >= RESPAWN_WAIT);
        if due && self.dragon.respawns < RESPAWN_TRIES {
            self.dragon.respawn_at = Some(now);
            self.dragon.respawns += 1;
            self.acts.push(Act::Respawn);
            self.note(
                tick,
                format!("respawn after the credits ({})", self.dragon.respawns),
                "client_command 0, as the client after the credits".to_owned(),
            );
        }
        self.task = Task::Idle;
        Some(Controls::default())
    }

    /// The fight with the dragon, in the End with no crystal shown and the credits not yet come,
    /// or `None` (E3's `end_task` calls it once the crystals are gone, `end::end_dragon`).
    pub fn dragon_task(&mut self, w: &World, tick: u64) -> Option<Task> {
        if !in_end(w) || w.won || w.body.is_none() {
            return None;
        }
        if w.entities
            .values()
            .any(|e| e.kind == EntityKind::EndCrystal && !e.dead)
        {
            return None;
        }
        let why = match view(w) {
            Some((_, v)) => format!(
                "no crystal shown; the dragon {} at {:.0} from the fountain, health {:.0}",
                v.phase().map_or_else(|| "unsaid".to_owned(), Phase::name),
                from_fountain(v.pos),
                v.health
            ),
            None if self.dragon.dead => "no crystal shown; the dragon dead".to_owned(),
            None => "no crystal shown; no dragon shown".to_owned(),
        };
        self.note(tick, "dragon fight".to_owned(), why);
        Some(Task::Dragon(Box::new(Fight::new(tick))))
    }

    /// One tick of the fight (`Task::Dragon`'s step).
    pub(crate) fn dragon_step(&mut self, w: &World, tick: u64) -> Controls {
        let rules = self.table.as_ref().map(|t| t.survival.clone());
        let danger = rules.as_ref().map_or(8.0, |r| r.danger_health);
        let food = rules
            .as_ref()
            .and_then(|r| eat::best_food(w, r))
            .or_else(|| {
                crate::held(w)
                    .into_iter()
                    .filter(|(_, n)| *n > 0)
                    .find(|(k, _)| data::generated::item_props::food_of(*k).is_some())
                    .map(|(k, _)| k)
            });
        // E3's bunker (`Plan.bunker`), the wait while the dragon flies.
        let bunker = self.bunker;
        let ask = Ask {
            danger,
            blow: HEAD_BLOW,
            perched_ticks: self.dragon.perched_ticks,
            stand: self.dragon.stand,
            bunker,
            dead: self.dragon.dead,
            food: food.is_some(),
            way: self.dragon.way,
        };
        let want = decide(w, &ask);
        let Task::Dragon(f) = &mut self.task else {
            return Controls::default();
        };
        if w.won || !in_end(w) || tick.saturating_sub(f.since) > FIGHT_TICKS {
            self.task = Task::Idle;
            self.retry_at = tick;
            self.note(
                tick,
                "dragon fight hands back".to_owned(),
                format!(
                    "{} strikes so far ({} on the head); plan again",
                    self.dragon.strikes, self.dragon.head_strikes
                ),
            );
            return Controls::default();
        }
        let key = match &want {
            Want::Exit(c) => format!("exit {c:?}"),
            Want::Go(c, why) => format!("go {c:?} {why}"),
            Want::Strike { head, .. } => format!("strike {}", if *head { "head" } else { "body" }),
            Want::Eat => "eat".to_owned(),
            Want::Hold(why) => format!("hold {why}"),
        };
        let changed = f.said != key;
        if changed {
            f.said.clone_from(&key);
        }
        // A meal goes on to its end unless a breath or the dragon's blow cuts it.
        if f.eat.is_some() && !matches!(want, Want::Eat | Want::Hold(_)) {
            f.eat = None;
            self.acts.push(Act::ReleaseUse);
        }
        if !matches!(want, Want::Go(..) | Want::Exit(_)) {
            f.walk = None;
        }
        let v = view(w).map(|(_, v)| v);
        // The stand held is the stand noted.
        if let Want::Go(c, "to the stand behind the dragon" | "to the head stand") = &want {
            self.dragon.stand = Some(*c);
        }
        // The way round is held while it is chosen, and with it the head stand it goes to.
        match &want {
            Want::Go(c, "round the head") => {
                if self.dragon.way != Some(*c) {
                    self.dragon.way = Some(*c);
                    self.dragon.ways += 1;
                }
                if let Some((_, v)) = view(w) {
                    self.dragon.stand = keep_head_stand(&v, w, self.dragon.stand);
                }
            }
            Want::Exit(_) | Want::Go(..) | Want::Strike { .. } | Want::Eat | Want::Hold(_) => {
                self.dragon.way = None;
            }
        }
        if changed {
            let at = w.body.map_or([0.0; 3], |b| b.pos);
            let why = match v {
                Some(v) => {
                    let (along, across) = ahead(&v, at);
                    format!(
                        "dragon {} health {:.0}, {:.1} along {:.1} across; bot health {:.1}, {:.1} from the fountain",
                        v.phase().map_or_else(|| "unsaid".to_owned(), Phase::name),
                        v.health,
                        along,
                        across,
                        w.player.health.unwrap_or(0.0),
                        from_fountain(at)
                    )
                }
                None => format!(
                    "no dragon shown; bot health {:.1}, {:.1} from the fountain",
                    w.player.health.unwrap_or(0.0),
                    from_fountain(at)
                ),
            };
            if !matches!(want, Want::Strike { .. }) {
                self.note(tick, format!("dragon: {key}"), why);
            }
        }
        let Task::Dragon(f) = &mut self.task else {
            return Controls::default();
        };
        match want {
            Want::Hold(_) => {
                if let Some(e) = &mut f.eat {
                    let (act, status) = e.step(w, tick);
                    self.acts.extend(act);
                    if !matches!(status, eat::EatStatus::Running) {
                        f.eat = None;
                    }
                }
                looking_down(w)
            }
            Want::Eat => {
                let Some(item) = food else {
                    return looking_down(w);
                };
                let e = f.eat.get_or_insert_with(|| eat::Eat::new(w, item, tick));
                let (act, status) = e.step(w, tick);
                self.acts.extend(act);
                if !matches!(status, eat::EatStatus::Running) {
                    f.eat = None;
                    let s = format!("{status:?}");
                    self.note(
                        tick,
                        format!("dragon lull meal {s}"),
                        format!(
                            "health {:.1}, food {}",
                            w.player.health.unwrap_or(0.0),
                            w.player.food.unwrap_or(0)
                        ),
                    );
                }
                looking_down(w)
            }
            Want::Strike { entity, head } => {
                let weapon = crate::hunt::weapon(w);
                if let Some(wp) = weapon {
                    match hold::toward(w, wp) {
                        Hold::Held | Hold::Missing => {}
                        Hold::Act(a) => {
                            self.acts.push(a);
                            return looking_down(w);
                        }
                        Hold::Wait => return looking_down(w),
                    }
                }
                let ready =
                    (crate::hunt::swing_ticks(w) * crate::dig::slowdown(w)).ceil() as u64 + 1;
                if f.last_swing.is_some_and(|t| tick.saturating_sub(t) < ready) {
                    return looking_down(w);
                }
                f.last_swing = Some(tick);
                self.acts.push(Act::Attack { entity });
                self.acts.push(Act::Swing);
                self.dragon.strikes += 1;
                if head {
                    self.dragon.head_strikes += 1;
                }
                let health = v.map_or(0.0, |v| v.health);
                let lost = self.dragon.last_health.map_or(0.0, |h| h - health);
                self.dragon.last_health = Some(health);
                let eye_at = w.body.map_or([0.0; 3], |b| {
                    [b.pos[0], b.pos[1] + physics::EYE_HEIGHT, b.pos[2]]
                });
                let reach = v.map_or(0.0, |v| {
                    if head {
                        head_reach(&v, eye_at)
                    } else {
                        body_reach(&v, eye_at)
                    }
                });
                self.note(
                    tick,
                    format!(
                        "dragon strike {} ({})",
                        if head { "head" } else { "body" },
                        weapon.map_or_else(|| "hand".to_owned(), |k: ItemKind| format!("{k:?}"))
                    ),
                    format!(
                        "reach {reach:.2}; dragon health {health:.1} ({lost:.1} since the last strike); bot health {:.1}; {} strikes",
                        w.player.health.unwrap_or(0.0),
                        self.dragon.strikes
                    ),
                );
                looking_down(w)
            }
            Want::Exit(c) => {
                let feet = w.body.map_or([0.0; 3], |b| b.pos);
                if from_fountain(feet) <= RIM {
                    // On the rim or the floor: straight in, the step down into the portal.
                    let to = mid(c);
                    return Controls {
                        forward: flat(feet, to) > 0.2,
                        yaw: crate::walk::yaw_to(feet, to),
                        pitch: Some(90.0),
                        ..Controls::default()
                    };
                }
                self.go(w, c, tick)
            }
            Want::Go(c, _) => self.go(w, c, tick),
        }
    }

    /// A walk to `c` inside the fight, kept while its target holds; looking down.
    fn go(&mut self, w: &World, c: [i32; 3], tick: u64) -> Controls {
        let Task::Dragon(f) = &mut self.task else {
            return Controls::default();
        };
        let cell: Cell = (c[0], c[1], c[2]);
        if f.walk.as_ref().is_none_or(|wk| wk.target != cell) {
            f.walk = Some(Walk::new(
                cell,
                Goal::Block { pos: cell },
                Arrive::Goal,
                tick,
            ));
        }
        let Some(wk) = f.walk.as_mut() else {
            return Controls::default();
        };
        let (mut c, status) = wk.step(w, tick);
        let (acts, said) = wk.take_out();
        let failed = match status {
            TaskStatus::Running => None,
            TaskStatus::Done => {
                f.walk = None;
                None
            }
            TaskStatus::Failed(e) => {
                f.walk = None;
                Some(e)
            }
        };
        self.acts.extend(acts);
        for (what, why) in said {
            self.note(tick, what, why);
        }
        if let Some(e) = failed {
            self.note(
                tick,
                format!("dragon walk to {cell:?} failed: {e:?}"),
                "the fight plans again".to_owned(),
            );
            self.task = Task::Idle;
            self.retry_at = tick + crate::RETRY_TICKS;
            return Controls::default();
        }
        if c.pitch.is_none() {
            c.pitch = Some(90.0);
        }
        c
    }

    /// Does the dragon come before a provoked enderman's pocket (`ender_threat`): in the End with a
    /// breath's reach on the feet, or the dragon on or over its perch with the feet within its
    /// reach of the fountain? Race 45873410249: both bots were held sealing a pocket, one in a
    /// fireball's cloud (dead in 80 ticks), one beside the sitting dragon (its head, 10, then a
    /// fall).
    pub(crate) fn dragon_first(&self, w: &World) -> bool {
        if !in_end(w) {
            return false;
        }
        let Some(feet) = w.body.map(|b| b.pos) else {
            return false;
        };
        breathed(&breaths(w), feet, 0.0)
            || view(w).is_some_and(|(_, v)| grounded(&v) && from_fountain(feet) < WAIT_RING + 4.0)
    }

    /// In the End, a breath's reach on the body (`breath_on`): the walk to the nearest cell out of
    /// every breath (`out_of_breath`), ahead of every row and task, the one rule for all the End's
    /// work (race 45873410249 lane 0 and E4's race 45875049490 lane 0 each stood in a fireball's
    /// cloud and died; E3's walk back to the bunker went through one). `None` with none on the
    /// body: what was running runs on.
    pub fn breath_step(&mut self, w: &World, tick: u64) -> Option<Controls> {
        if !in_end(w) {
            return None;
        }
        let feet = w.body?.pos;
        let clouds = breaths(w);
        let Some(on) = breath_on(&clouds, feet) else {
            if let Some(wk) = self.dragon.breath_walk.take() {
                self.note(
                    tick,
                    "out of the breath".to_owned(),
                    format!(
                        "at {:?}, {} ticks after the walk to {:?} began",
                        cell_of(feet),
                        tick.saturating_sub(wk.started),
                        wk.target
                    ),
                );
            }
            return None;
        };
        // The cell held while no breath reaches it; else the nearest one anew.
        let held = self
            .dragon
            .breath_walk
            .as_ref()
            .map(|wk| [wk.target.0, wk.target.1, wk.target.2])
            .filter(|c| breath_on(&clouds, mid(*c)).is_none());
        let target = match held {
            Some(c) => c,
            None => {
                let Some(c) = out_of_breath(w, feet) else {
                    if self.dragon.breath_walk.take().is_some() || self.dragon.breath_outs == 0 {
                        self.note(
                            tick,
                            "in the breath, no way out".to_owned(),
                            format!(
                                "a breath {:.1} off, radius {:.1}; no cell out of it loaded within {OUT_SEARCH}",
                                flat(on.pos, feet),
                                on.radius
                            ),
                        );
                    }
                    return None;
                };
                self.dragon.breath_outs += 1;
                self.dragon.breath_walk = Some(Walk::new(
                    (c[0], c[1], c[2]),
                    Goal::Block {
                        pos: (c[0], c[1], c[2]),
                    },
                    Arrive::Goal,
                    tick,
                ));
                self.note(
                    tick,
                    format!("out of the breath to {c:?}"),
                    format!(
                        "a breath {:.1} off (radius {:.1}) at {:?}; task {}; health {:.1}; {} walks out in the run",
                        flat(on.pos, feet),
                        on.radius,
                        cell_of(feet),
                        self.task.name(),
                        w.player.health.unwrap_or(0.0),
                        self.dragon.breath_outs
                    ),
                );
                c
            }
        };
        let wk = self.dragon.breath_walk.as_mut()?;
        let (mut c, status) = wk.step(w, tick);
        let (acts, said) = wk.take_out();
        self.acts.extend(acts);
        for (what, why) in said {
            self.note(tick, what, why);
        }
        if !matches!(status, TaskStatus::Running) {
            // Arrived (the breath grew onto it) or failed: the next tick looks again.
            self.dragon.breath_walk = None;
            if let TaskStatus::Failed(e) = status {
                self.note(
                    tick,
                    format!("walk out of the breath to {target:?} failed: {e:?}"),
                    "the next tick looks again".to_owned(),
                );
            }
        }
        if c.pitch.is_none() {
            c.pitch = Some(90.0);
        }
        Some(c)
    }

    /// In the End, a walk pressing on toward a breath's reach (the point `AHEAD` along its
    /// heading inside one, the feet not) waits at the edge instead, noted every
    /// `AHEAD_NOTE_TICKS`: a cloud lasts a while and the walk goes on once it is gone.
    pub fn breath_ahead(&mut self, w: &World, c: Controls) -> Controls {
        if !c.forward || !in_end(w) {
            return c;
        }
        let Some(feet) = w.body.map(|b| b.pos) else {
            return c;
        };
        let clouds = breaths(w);
        if clouds.is_empty() || breath_on(&clouds, feet).is_some() {
            return c;
        }
        let r = c.yaw.to_radians();
        let ahead = [
            feet[0] - r.sin() * AHEAD,
            feet[1],
            feet[2] + r.cos() * AHEAD,
        ];
        let Some(b) = breath_on(&clouds, ahead) else {
            return c;
        };
        let tick = w.ticks;
        self.dragon.breath_waits += 1;
        if tick >= self.dragon.breath_waited_at + AHEAD_NOTE_TICKS || self.dragon.breath_waits == 1
        {
            self.dragon.breath_waited_at = tick;
            self.note(
                tick,
                "walk waits at a breath's edge".to_owned(),
                format!(
                    "a breath {:.1} ahead (radius {:.1}) at {:?}; task {}; {} ticks waited in the run",
                    flat(b.pos, ahead),
                    b.radius,
                    cell_of(feet),
                    self.task.name(),
                    self.dragon.breath_waits
                ),
            );
        }
        Controls {
            forward: false,
            jump: false,
            ..c
        }
    }

    /// `dragon_first`, noted (at most every `FIRST_NOTE_TICKS`) when a provoked enderman's pocket
    /// is passed over for it.
    pub fn dragon_first_step(&mut self, w: &World, tick: u64) -> bool {
        let first = self.dragon_first(w);
        if first && let Some(id) = crate::endermen::angry(w) {
            self.dragon.first_skips += 1;
            if tick >= self.dragon.first_noted_at + FIRST_NOTE_TICKS || self.dragon.first_skips == 1
            {
                self.dragon.first_noted_at = tick;
                self.note(
                    tick,
                    format!("dragon first: no pocket for enderman {id}"),
                    format!(
                        "a breath's reach on the feet or the dragon on its perch within {:.0}; {} ticks passed over",
                        WAIT_RING + 4.0,
                        self.dragon.first_skips
                    ),
                );
            }
        }
        first
    }

    /// Is a meal by the always-row to wait: the dragon on or over its perch with the feet within
    /// its reach of the fountain (clojurecraft: a bot ate at 14.2 in the head's reach and died in
    /// the next second). The fight eats in its lulls.
    pub(crate) fn dragon_holds_meal(&self, w: &World) -> bool {
        in_end(w)
            && view(w).is_some_and(|(_, v)| grounded(&v))
            && w.body
                .is_some_and(|b| from_fountain(b.pos) < STEP_OUT_RADIUS + BITE_OUT)
    }
}
