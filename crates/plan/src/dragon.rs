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
/// Heights over and under the feet a stand's ground is looked for in: from the bunker three down
/// the surface is four up, and the stairs' top a block more (race 45882663266 lane 0: in the
/// bunker no head stand was found for a whole perch, 1,060 ticks with no strike).
pub const GROUND_UP: i32 = 6;
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
    out_of_breath_except(w, feet, &[])
}

/// Blocks more than a breath's reach, and the body's height a block more, a cell out of it keeps:
/// a cell only just out is reached with the feet still in (race 45882663266 lane 0 held 370 ticks
/// at 6.4 off a reach of 6.5, the walk to its own cell done each tick).
pub const OUT_SLACK: f64 = 1.0;

/// Is a body with its feet at `p` well clear of every breath (`breath_on` with `OUT_SLACK` more)?
pub fn clear_of_breath(breaths: &[Breath], p: [f64; 3]) -> bool {
    !breaths.iter().any(|b| {
        flat(b.pos, p) <= b.reach() + OUT_SLACK
            && p[1] <= b.pos[1] + BREATH_OVER + OUT_SLACK
            && p[1] + physics::HEIGHT + BREATH_UNDER + OUT_SLACK >= b.pos[1]
    })
}

/// `out_of_breath`, never the feet's own column (race 45882663266 lane 2: on the bunker's stair at
/// y 60.25, the cell under it read clear and was walked to 211 times, 20 health to dead), nor a
/// cell in `refused` (reached with the feet still in a breath).
pub fn out_of_breath_except(w: &World, feet: [f64; 3], refused: &[[i32; 3]]) -> Option<[i32; 3]> {
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
            if (x == fc[0] && z == fc[2])
                || refused.contains(&c)
                || !clear_of_breath(&clouds, p)
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

/// Blocks from the bot a charging dragon counts within, and the cosine its facing must be within
/// of the line to the bot (the charge flies at the player: `DragonChargePlayerPhase`).
pub const CHARGE_RANGE: f64 = 40.0;
pub const CHARGE_CONE: f64 = 0.8;
/// Blocks on the plane a bunker is gone into from at a charge.
pub const CHARGE_BUNKER_REACH: f64 = 12.0;

/// Blocks beyond a breath's reach on the plane that the way into the bunker is kept, and the bot
/// held in it.
pub const BUNKER_HOLD: f64 = 2.0;

/// Blocks on the plane from the bunker a breath on the body sends the bot into it.
pub const BUNKER_NEAR: f64 = 6.0;

/// Blocks on the plane a bunker is gone into from at an angry enderman.
pub const ENDER_BUNKER_REACH: f64 = 24.0;

/// Ticks a fight whose walk failed in cover waits before it plans again.
pub const COVERED_RETRY_TICKS: u64 = 200;

/// Blocks the feet rise over the cell a jump left before its block goes in (clojurecraft
/// `dragon/clear-rise`).
pub const RISE_CLEAR: f64 = 1.05;

/// Ticks between the notch roof's clicks, and the clicks it may take in all.
pub const ROOF_CLICK_TICKS: u64 = 6;
pub const ROOF_TRIES: u32 = 8;

/// The block of the wall at the feet's or the head's height in the cell beside the feet toward
/// `target` (along the plane's longer way to it), or `None` when both are open: what a walled-in
/// bot digs to get out. Never bedrock or obsidian.
pub fn wall_toward(w: &World, feet: [f64; 3], target: [i32; 3]) -> Option<[i32; 3]> {
    let f = cell_of(feet);
    let (dx, dz) = (target[0] - f[0], target[2] - f[2]);
    let step = if dx.abs() >= dz.abs() {
        (dx.signum(), 0)
    } else {
        (0, dz.signum())
    };
    if step == (0, 0) {
        return None;
    }
    [0, 1].iter().find_map(|dy| {
        let c = [f[0] + step.0, f[1] + dy, f[2] + step.1];
        match w.block_at(world::BlockPos::new(c[0], c[1], c[2])) {
            Some(BlockKind::Air | BlockKind::Bedrock | BlockKind::Obsidian) | None => None,
            Some(_) => Some(c),
        }
    })
}

/// May the block under feet cell `c` be dug for a notch: end stone, with a solid block under it to
/// land on (never a hole to the void at the island's edge)?
pub fn notch_diggable(w: &World, c: [i32; 3]) -> bool {
    let under = w.block_at(world::BlockPos::new(c[0], c[1] - 1, c[2]));
    let floor = w.block_at(world::BlockPos::new(c[0], c[1] - 2, c[2]));
    under == Some(BlockKind::EndStone) && floor.is_some_and(|k| k != BlockKind::Air)
}

/// The step out of a notch two deep dug from surface feet cell `from`: a cell beside it one under
/// the surface, end stone with air over it, whose digging leaves a stair (the notch's floor, a
/// step a block up, the surface). `None` when no side has one.
pub fn notch_step(w: &World, from: [i32; 3]) -> Option<[i32; 3]> {
    notch_step_except(w, from, None)
}

/// `notch_step`, never under the roof's side block `side` (a block over the step's head).
pub fn notch_step_except(w: &World, from: [i32; 3], side: Option<[i32; 3]>) -> Option<[i32; 3]> {
    [(1, 0), (-1, 0), (0, 1), (0, -1)]
        .iter()
        .find_map(|(dx, dz)| {
            let s = [from[0] + dx, from[1] - 1, from[2] + dz];
            if side.is_some_and(|b| b[0] == s[0] && b[2] == s[2]) {
                return None;
            }
            let k = |y: i32| w.block_at(world::BlockPos::new(s[0], y, s[2]));
            (k(s[1]) == Some(BlockKind::EndStone)
                && k(s[1] + 1) == Some(BlockKind::Air)
                && k(s[1] - 1).is_some_and(|b| b != BlockKind::Air))
            .then_some(s)
        })
}

/// The distance of the dragon when it is strafing (phase 1) within `STRAFE_RANGE` of `feet`:
/// `DragonStrafePlayerPhase` fires its fireball at a player in sight within 64.
pub fn strafing_at(w: &World, feet: [f64; 3]) -> Option<f64> {
    let (_, v) = view(w)?;
    if v.phase() != Some(Phase::Strafe) {
        return None;
    }
    let d = ((v.pos[0] - feet[0]).powi(2)
        + (v.pos[1] - feet[1]).powi(2)
        + (v.pos[2] - feet[2]).powi(2))
    .sqrt();
    (d <= STRAFE_RANGE).then_some(d)
}

/// The distance of the nearest dragon fireball in flight within `FIREBALL_RANGE` of `feet`.
pub fn fireball_at(w: &World, feet: [f64; 3]) -> Option<f64> {
    w.entities
        .values()
        .filter(|e| e.kind == EntityKind::DragonFireball)
        .map(|e| {
            ((e.pos[0] - feet[0]).powi(2)
                + (e.pos[1] - feet[1]).powi(2)
                + (e.pos[2] - feet[2]).powi(2))
            .sqrt()
        })
        .filter(|d| *d <= FIREBALL_RANGE)
        .min_by(f64::total_cmp)
}

/// Blocks from the feet a fireball in flight is sheltered from.
pub const FIREBALL_RANGE: f64 = 64.0;

/// Ticks of a strafe the bot stays hidden before it comes out to be seen. A strafe ends only by
/// its fireball, which needs the bot in sight (`DragonStrafePlayerPhase`; dev race 45937087645:
/// hidden in a notch, the strafe held 11,000 ticks and the dragon never perched). 200 is the
/// coordinator's cap (10 s), well over the 5 ticks in sight the fireball's charge needs.
pub const STRAFE_HIDE_TICKS: u64 = 200;

/// Ticks idle in the End with no note before the planner is handed back (the coordinator's
/// backstop; dev race 45940964839 lane 1 sat 13,000 ticks idle in the bunker with no note).
pub const IDLE_NOTE_TICKS: u64 = 600;
/// The dodge from a fireball in flight (wall 28): blocks from the cell the bot stood in, at
/// least and at most (over 4 from its hit, where `DragonFireball.onHit` puts its cloud on a body,
/// and the cloud's radius 3 at first, with a block of margin), and blocks off the fireball's line
/// it counts as aimed near (the strafe aims at the bot's middle; a miss of 6 lands the cloud more
/// than 4 off).
pub const DODGE_MIN: i32 = 6;
pub const DODGE_MAX: i32 = 10;
pub const DODGE_AIM: f64 = 6.0;
/// The route to a dodge cell: nodes searched, and waypoints at most (a detour longer than about
/// twice the farthest cell is no dodge: the fireball flies about 20 to 40 ticks).
pub const DODGE_NODES: u32 = 800;
pub const DODGE_STEPS: usize = 16;
/// Blocks a tick a fireball flies, for the note's ticks to its hit (dev race 45940964839: about
/// 40 ticks over 30 to 40 blocks).
pub const FIREBALL_SPEED: f64 = 1.0;
/// Columns each way round a dodge cell that must have ground under them (no void edge), and the
/// blocks under the feet the ground is looked for.
pub const EDGE_CLEAR: i32 = 3;
pub const EDGE_DEPTH: i32 = 6;
/// Blocks over a dodge cell's feet that must be air: the open sky, never under a roof.
pub const SKY_CLEAR: i32 = 6;
/// Blocks on the plane from the fountain's axis a dodge or pace cell keeps (its bedrock is 3 or
/// so across, the portal round it).
pub const FOUNTAIN_CLEAR: f64 = 5.0;

/// Is the feet cell `c` water (the planner has no start there)?
pub fn in_water(w: &World, c: [i32; 3]) -> bool {
    w.block_at(world::BlockPos::new(c[0], c[1], c[2])) == Some(BlockKind::Water)
}

/// Controls that walk straight from `feet` toward `to` on the plane.
pub fn toward(feet: [f64; 3], to: [f64; 3]) -> Controls {
    Controls {
        forward: true,
        yaw: (-(to[0] - feet[0])).atan2(to[2] - feet[2]).to_degrees(),
        ..Controls::default()
    }
}

/// The dragon's clouds alone (`breaths` less the fireballs in flight).
pub fn clouds_only(w: &World) -> Vec<Breath> {
    breaths(w)
        .into_iter()
        .filter(|b| {
            w.entities
                .get(&b.id)
                .is_some_and(|e| e.kind == EntityKind::AreaEffectCloud)
        })
        .collect()
}

/// Blocks between two points.
pub fn dist3(a: [f64; 3], b: [f64; 3]) -> f64 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
}

/// The cell a fireball flying along `dir` (on the plane, unit) is dodged to from feet cell
/// `from`: sideways to its flight, `DODGE_MIN` to `DODGE_MAX` off, the side away from the dragon
/// at `dragon` first; a cell stood on (within 2 up or down) in the open (never a notch's floor:
/// two sides open at the feet, the sky over it), never in a `banned` column (the bunker and its
/// stair), ground under every column within `EDGE_CLEAR` (no void edge), clear of every cloud,
/// and walked to: a route found from the feet in `DODGE_NODES`, at most `DODGE_STEPS` long,
/// none of its cells within a cloud's reach (frontier race 45954274931: lanes 2 and 3's dodge
/// walks failed NoPath at once, lane 5's crossed a cloud). Sideways first, then 30 degrees
/// either way off it; never along the fireball's line. `None` when no such cell is known.
pub fn dodge_cell(
    w: &World,
    from: [i32; 3],
    dir: [f64; 2],
    dragon: Option<[f64; 3]>,
    banned: &[(i32, i32)],
    prefer: Option<f64>,
) -> Option<[i32; 3]> {
    let perp = [-dir[1], dir[0]];
    let (cx, cz) = (f64::from(from[0]) + 0.5, f64::from(from[2]) + 0.5);
    let away = dragon.is_some_and(|d| (d[0] - cx) * perp[0] + (d[2] - cz) * perp[1] > 0.0);
    // The side the bot already walks first (never a turn back), else away from the dragon.
    let sides = match prefer {
        Some(p) if p < 0.0 => [-1.0, 1.0],
        Some(_) => [1.0, -1.0],
        None if away => [-1.0, 1.0],
        None => [1.0, -1.0],
    };
    let clouds = clouds_only(w);
    let mut cells = crate::path::Cells::new(w);
    let ground = |x: i32, y: i32, z: i32| {
        (1..=EDGE_DEPTH).any(|d| {
            w.block_at(world::BlockPos::new(x, y - d, z))
                .is_some_and(|k| k != BlockKind::Air)
        })
    };
    let start: Cell = (from[0], from[1], from[2]);
    // In water the planner has no start (frontier race 45954274931 lanes 2 and 3: a layer of
    // water over the ground, every walk NoPath): the straight way is kept off the clouds instead,
    // and pressed straight (`in_water`).
    let wet = in_water(w, from);
    let walked = |c: [i32; 3]| {
        if wet {
            let off_start: Vec<Breath> = clouds
                .iter()
                .filter(|b| flat(b.pos, mid(from)) > b.reach())
                .copied()
                .collect();
            return !breath_on_way(&off_start, mid(from), mid(c), 0.0);
        }
        let r = crate::path::plan(
            w,
            start,
            &Goal::Block {
                pos: (c[0], c[1], c[2]),
            },
            DODGE_NODES,
        );
        matches!(r.status, crate::path::Status::Found)
            && r.waypoints.len() <= DODGE_STEPS
            && r.waypoints.iter().all(|p| {
                // A cloud already on the start is walked out of; any other is kept off the way.
                !clouds.iter().any(|b| {
                    flat(b.pos, mid(from)) > b.reach()
                        && flat(b.pos, mid([p.0, p.1, p.2])) <= b.reach()
                })
            })
    };
    // Each side sideways, then turned 30 degrees toward and away from the fireball's flight.
    let turns = [0.0_f64, 30.0, -30.0];
    let dirs: Vec<[f64; 2]> = sides
        .iter()
        .flat_map(|side| {
            turns.iter().map(move |t| {
                let (sn, cs) = t.to_radians().sin_cos();
                let p = [perp[0] * side, perp[1] * side];
                [p[0] * cs - p[1] * sn, p[0] * sn + p[1] * cs]
            })
        })
        .collect();
    dirs.iter().find_map(|d| {
        (DODGE_MIN..=DODGE_MAX).find_map(|k| {
            let k = f64::from(k);
            let (x, z) = (
                (cx + d[0] * k).floor() as i32,
                (cz + d[1] * k).floor() as i32,
            );
            [0, 1, -1, 2, -2].iter().find_map(|dy| {
                let c = [x, from[1] + dy, z];
                let air = |p: [i32; 3]| {
                    w.block_at(world::BlockPos::new(p[0], p[1], p[2])) == Some(BlockKind::Air)
                };
                let open_sides = [(1, 0), (-1, 0), (0, 1), (0, -1)]
                    .iter()
                    .filter(|(sx, sz)| air([x + sx, c[1], z + sz]))
                    .count();
                let sky = (2..=SKY_CLEAR).all(|up| air([x, c[1] + up, z]));
                // Never by the fountain (its bedrock and the portal; frontier race 45947478869
                // lane 0's dodge cell was on its rim).
                let fountain = (f64::from(x) + 0.5).hypot(f64::from(z) + 0.5) < FOUNTAIN_CLEAR;
                (!banned.contains(&(x, z))
                    && !fountain
                    && open_sides >= 2
                    && sky
                    && cells.standable((c[0], c[1], c[2]))
                    && (-EDGE_CLEAR..=EDGE_CLEAR).all(|ex| {
                        (-EDGE_CLEAR..=EDGE_CLEAR).all(|ez| ground(x + ex, c[1], z + ez))
                    })
                    && clear_of_breath(&clouds, mid(c))
                    && walked(c))
                .then_some(c)
            })
        })
    })
}

/// Blocks within which an angry enderman is answered (beyond, the task goes on: it teleports to
/// the bot only from over 16, `EndermanLookForPlayerGoal`, and cannot come into a 2-high cell),
/// and the ticks any hold from one is kept (the coordinator's cap: its anger never ends).
pub const ENDER_NEAR: f64 = 8.0;
/// Ticks of a strafe out of the dragon's sight before the bot rises into it, and the blocks it
/// rises at most (the strafe fires after 5 ticks in sight; `DragonStrafePlayerPhase`).
pub const STRAFE_UNSEEN_TICKS: u64 = 100;
pub const STRAFE_RISES: i32 = 10;
pub const ENDER_HOLD_CAP: u64 = 600;

/// Rows named in the backstop's note at most.
pub const ROWS_KEPT: usize = 8;

/// Blocks the strafing dragon fires within (`DragonStrafePlayerPhase`; E4's `THREAT_NEAR`).
pub const STRAFE_RANGE: f64 = 64.0;

/// The distance of the dragon when it is charging (phase 8) within `CHARGE_RANGE` of `feet` with
/// its facing on the line to them (its yaw unsaid: taken as on it), else `None`.
pub fn charging_at(w: &World, feet: [f64; 3]) -> Option<f64> {
    let (_, e) = dragon(w)?;
    let (_, v) = view(w)?;
    if v.phase() != Some(Phase::ChargingPlayer) {
        return None;
    }
    let d = ((v.pos[0] - feet[0]).powi(2)
        + (v.pos[1] - feet[1]).powi(2)
        + (v.pos[2] - feet[2]).powi(2))
    .sqrt();
    if d > CHARGE_RANGE {
        return None;
    }
    let flat_d = flat(v.pos, feet);
    if e.yaw.is_none() || flat_d < 1e-6 {
        return Some(d);
    }
    let [fx, fz] = facing(v.yaw);
    let cos = (fx * (feet[0] - v.pos[0]) + fz * (feet[2] - v.pos[2])) / flat_d;
    (cos >= CHARGE_CONE).then_some(d)
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

/// Are feet at `p` out of the head's and the neck's blow boxes: under them (`under_wings`), or on
/// the plane with the player's half width?
pub fn clear_of_blow(v: &DragonView, p: [f64; 3]) -> bool {
    // Under them: the head's and neck's boxes grown by 1 reach down to 2 under the dragon's feet
    // (E1), the wings' sweep's floor (race 45901376114 lane 0: feet at 62 under a dragon at 67,
    // the stand 2.0 off the box out of reach, "no head stand open" every turn).
    if under_wings(v, p[1]) {
        return true;
    }
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
    head_stand_for(v, w, false)
}

/// The damage a perched dragon takes before it takes off: `sittingDamageReceived` over a
/// quarter of 200 (E1, `EnderDragon.hurt`). The count is kept across perches and only reset when
/// it goes over (race 45876898172 lane 2: 18.4 in one perch and 32 in the next, then takeoff).
pub const PERCH_TAKEOFF: f32 = 50.0;
/// One head strike with the iron sword (E6's e05: 6.0), and a margin for a crit.
pub const STRIKE_DAMAGE: f32 = 6.0;
pub const STRIKE_MARGIN: f32 = 1.0;
/// The wings' box at takeoff reaches this far under the dragon's feet (E1: inflate(4, 2, 4) moved
/// −2 from the wing parts 2 over the feet), and the margin the head keeps under it.
pub const WING_DROP: f64 = 2.0;
pub const WING_CLEAR: f64 = 0.2;

/// Ticks of the attacking turn struck in before the bot leaves the flame's way: the turn is 40
/// (`DragonSittingAttackingPhase`); a walk of 4 out of the cloud's reach takes about 18, and race
/// 45886819862 lane 2 left at 15, before its first strike, every turn.
pub const FLAME_LEAVE_TICKS: u64 = 25;
/// The flame's cloud: radius 5 at the head (`DragonSittingFlamingPhase`); the bot keeps its
/// reach (radius and `BREATH_MARGIN`) and a block more.
pub const FLAME_RADIUS: f64 = 5.0;
pub const FLAME_CLEAR: f64 = FLAME_RADIUS + BREATH_MARGIN + 1.0;

/// The ground cell nearest the feet `FLAME_CLEAR` or more from the head's point on the plane,
/// out of every breath, not before the head nor on a way past it, `ESCAPE_WAYS` directions at
/// `FLAME_CLEAR` and a block and two more out.
pub fn flame_clear_cell(w: &World, v: &DragonView, feet: [f64; 3]) -> Option<[i32; 3]> {
    let h = head_point(v);
    let clouds = breaths(w);
    let fy = feet[1].floor() as i32;
    let mut best: Option<(f64, [i32; 3])> = None;
    for r in [FLAME_CLEAR, FLAME_CLEAR + 1.0, FLAME_CLEAR + 2.0] {
        for k in 0..ESCAPE_WAYS {
            let a = f64::from(k) * std::f64::consts::TAU / f64::from(ESCAPE_WAYS);
            let x = (h[0] + r * a.cos()).floor() as i32;
            let z = (h[2] + r * a.sin()).floor() as i32;
            let Some(c) = ground(w, x, z, fy) else {
                continue;
            };
            let p = mid(c);
            // Never within the turning head's sweep of the body either (race 45886819862 lane 0:
            // held out of the flame 3.6 from the fountain, the scanning turn's head took 10).
            if flat(p, h) < FLAME_CLEAR
                || flat(p, v.pos) < BITE_OUT + 1.0
                || breath_on(&clouds, p).is_some()
                || before_head(v, p)
                || !way_clear(v, feet, p)
            {
                continue;
            }
            let d = flat(feet, p);
            if best.is_none_or(|(bd, bc)| d < bd || (d == bd && c < bc)) {
                best = Some((d, c));
            }
        }
    }
    best.map(|(_, c)| c)
}

/// Server ticks after a blow a takeoff counts as the blow's: the server resets its count then.
pub const TAKEOFF_AFTER_BLOW: u64 = 20;

/// The cell a low stand is made in through the flame: a ground cell `HEAD_OUT + HEAD_HALF +
/// HEAD_STAND_OFF` from the body on a bearing at least a right angle off the head's, out of the
/// flame's reach and every breath's, under the wings' sweep or one dig of end stone from it; the
/// nearest the feet.
pub fn low_prep_cell(w: &World, v: &DragonView, feet: [f64; 3]) -> Option<[i32; 3]> {
    let clouds = breaths(w);
    let h = head_point(v);
    let out = HEAD_OUT + HEAD_HALF + HEAD_STAND_OFF;
    let fy = feet[1].floor() as i32;
    let mut best: Option<(f64, [i32; 3])> = None;
    for k in 0..ESCAPE_WAYS {
        let a = f64::from(k) * std::f64::consts::TAU / f64::from(ESCAPE_WAYS);
        let x = (v.pos[0] + out * a.cos()).floor() as i32;
        let z = (v.pos[2] + out * a.sin()).floor() as i32;
        let Some(c) = ground(w, x, z, fy) else {
            continue;
        };
        let p = mid(c);
        if ahead(v, p).0 > 0.0
            || flat(p, h) < FLAME_CLEAR
            || breath_on(&clouds, p).is_some()
            || !(under_wings(v, f64::from(c[1]))
                || (under_wings(v, f64::from(c[1] - 1)) && notch_diggable(w, c)))
        {
            continue;
        }
        let d = flat(feet, p);
        if best.is_none_or(|(bd, bc)| d < bd || (d == bd && c < bc)) {
            best = Some((d, c));
        }
    }
    best.map(|(_, c)| c)
}

/// Would the next strike make the dragon take off (`PERCH_TAKEOFF`)?
pub fn crossing(sat_damage: f32) -> bool {
    sat_damage + STRIKE_DAMAGE + STRIKE_MARGIN > PERCH_TAKEOFF
}

/// Is a body with its feet at `feet_y` under the wings' sweep at takeoff, and the head's blow box
/// (race 45876898172 lane 2: the strike that made it take off was struck from the surface, and the
/// head and wings took 15 and threw the bot 120 blocks off the island)?
pub fn under_wings(v: &DragonView, feet_y: f64) -> bool {
    feet_y + physics::HEIGHT + WING_CLEAR <= v.pos[1] - WING_DROP
}

/// Can the floor under feet cell `c` be dug for a stand one lower that still strikes: end stone
/// (the island's), the cell under it solid to stand on?
fn lowerable(w: &World, v: &DragonView, clouds: &[Breath], c: [i32; 3]) -> bool {
    let under = world::BlockPos::new(c[0], c[1] - 1, c[2]);
    let floor = world::BlockPos::new(c[0], c[1] - 2, c[2]);
    w.block_at(under) == Some(BlockKind::EndStone)
        && w.block_at(floor).is_some_and(|k| k != BlockKind::Air)
        && head_stand_ok(v, clouds, [c[0], c[1] - 1, c[2]], STRIKE_REACH)
}

/// `head_stand`; with `low`, only a cell under the wings' sweep or one dig of end stone from it
/// (one block: a climb back out is a jump).
pub fn head_stand_for(v: &DragonView, w: &World, low: bool) -> Option<[i32; 3]> {
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
            if low
                && !under_wings(v, f64::from(c[1]))
                && !(under_wings(v, f64::from(c[1] - 1)) && lowerable(w, v, &clouds, c))
            {
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

/// Blocks from the fountain every way out keeps to while the dragon scans: inside the 20 it looks
/// for a player in before it charges one, past the head's sweep (8) and the turn's stand (9).
pub const SCAN_KEEP: f64 = 12.0;

/// Blocks on the plane from a stand's middle the feet step onto it from.
pub const CENTRE_NEAR: f64 = 0.25;

/// Ticks of the scanning phase the bot waits for the head to turn onto it at `scan_stand`
/// (the turn takes about 20, `DragonSittingScanningPhase`).
pub const SCAN_WAIT_TICKS: u64 = 40;

/// The scanning turn's stand: the scanning dragon turns its head onto the bot
/// (`DragonSittingScanningPhase`, about 20 ticks), so the stand is where the head will be struck
/// from once it has: `HEAD_OUT + HEAD_HALF + HEAD_STAND_OFF` out from the body on the line to the
/// feet, a ground cell out of the blow boxes as the head points now and of every breath, under the
/// wings' sweep with `low` as `head_stand_for`; the nearest that point (race 45886819862 lane 2:
/// chasing the turning head's stand, 1 strike in 4 perches).
pub fn scan_stand(v: &DragonView, w: &World, low: bool) -> Option<[i32; 3]> {
    let feet = w.body?.pos;
    let clouds = breaths(w);
    let (dx, dz) = (feet[0] - v.pos[0], feet[2] - v.pos[2]);
    let n = (dx * dx + dz * dz).sqrt();
    if n < 1e-6 {
        return None;
    }
    let out = HEAD_OUT + HEAD_HALF + HEAD_STAND_OFF;
    let ideal = [v.pos[0] + dx / n * out, 0.0, v.pos[2] + dz / n * out];
    let (cx, cz) = (ideal[0].floor() as i32, ideal[2].floor() as i32);
    let mut best: Option<(f64, [i32; 3])> = None;
    for x in (cx - 1)..=(cx + 1) {
        for z in (cz - 1)..=(cz + 1) {
            let Some(c) = ground(w, x, z, feet[1].floor() as i32) else {
                continue;
            };
            let p = mid(c);
            if !clear_of_blow(v, p) || breathed(&clouds, p, 0.0) {
                continue;
            }
            if low
                && !under_wings(v, f64::from(c[1]))
                && !(under_wings(v, f64::from(c[1] - 1)) && lowerable(w, v, &clouds, c))
            {
                continue;
            }
            let d = flat(ideal, p);
            if best.is_none_or(|(bd, bc)| d < bd || (d == bd && c < bc)) {
                best = Some((d, c));
            }
        }
    }
    best.map(|(_, c)| c)
}

/// The head stand to hold: the one held while it still strikes (no slack), else `head_stand`.
pub fn keep_head_stand(
    v: &DragonView,
    w: &World,
    held: Option<[i32; 3]>,
    low: bool,
) -> Option<[i32; 3]> {
    let clouds = breaths(w);
    if let Some(h) = held
        && head_stand_ok(v, &clouds, h, STRIKE_REACH)
        && (!low
            || under_wings(v, f64::from(h[1]))
            || (under_wings(v, f64::from(h[1] - 1)) && lowerable(w, v, &clouds, h)))
    {
        return Some(h);
    }
    head_stand_for(v, w, low)
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
    /// Dig the block under the feet (a stand one lower, under the wings' sweep).
    Dig([i32; 3]),
    /// Step onto a stand's middle.
    Centre([i32; 3]),
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
    /// The perched dragon's damage counted toward its takeoff (`DragonState::sat_damage`).
    pub sat_damage: f32,
    /// Server ticks the dragon has held its phase.
    pub phase_ticks: u64,
    /// The stand a way round was walked to the end of (`DragonState::way_done`).
    pub way_done: Option<[i32; 3]>,
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
    // Scanning, the dragon charges a bot it cannot reach (`DragonSittingScanningPhase`: none
    // within 20 for 100 ticks): every way out stays within `SCAN_KEEP` of the fountain then (race
    // 45895541443 lane 2: out of reach after a blow 19 off, a charge, 10 and two fly-bys, dead).
    let scanning_near = ground_dragon.is_some_and(|d| d.phase() == Some(Phase::SittingScanning));
    let keep = |r: f64| if scanning_near { r.min(SCAN_KEEP) } else { r };
    if struck_by_dragon(w) {
        let r = keep(STEP_OUT_RADIUS.max(from_fountain(feet) + 4.0));
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
        let ring = keep(WAIT_RING);
        let out_of_scan = scanning_near && from_fountain(feet) > SCAN_KEEP + 1.0;
        if from_fountain(feet) < ring - 1.0 || in_bite(&v, feet) || out_of_scan {
            return match step_out_cell(w, Some(&v), ring) {
                Some(c) if flat(feet, mid(c)) > STAND_NEAR => Want::Go(c, "under the perch line"),
                Some(_) => Want::Hold("under the perch line"),
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
    // The strike that makes it take off only from under the wings' sweep: one dig of end stone
    // down first where that makes it so.
    // The flame: the attacking turn (40 ticks, `DragonSittingAttackingPhase`) ends in a cloud of
    // radius 5 at the head (`DragonSittingFlamingPhase`), over the head stand. Strikes for its
    // first `FLAME_LEAVE_TICKS`, then out of its reach before it comes, and there while it burns
    // (races 45873410249 and 45876898172: 3.0 a flame, walking out of it).
    let flame = match v.phase() {
        Some(Phase::SittingAttacking) => ask.phase_ticks >= FLAME_LEAVE_TICKS,
        Some(Phase::SittingFlaming) => true,
        Some(_) | None => false,
    };
    if sat && flame {
        // The strike that makes it take off wants a stand under the wings, and one dig in the
        // turn's window is too slow (race 45931228063 lane 2: the count at 44, "no head stand
        // open" every turn, the dragon at 156 for 9,000 ticks). It is dug through the flame: a
        // notch 9 out from the body on the side away from the head, out of the flame's reach;
        // the scanning turn brings the head to it.
        if crossing(ask.sat_damage)
            && let Some(c) = low_prep_cell(w, &v, feet)
        {
            if flat(feet, mid(c)) > STAND_NEAR {
                return Want::Go(c, "to a low stand, through the flame");
            }
            if body.on_ground && !under_wings(&v, feet[1]) && notch_diggable(w, feet_cell) {
                return Want::Dig([feet_cell[0], feet_cell[1] - 1, feet_cell[2]]);
            }
            return Want::Hold("in the low stand, the flame burning");
        }
        return match flame_clear_cell(w, &v, feet) {
            Some(c) if flat(feet, mid(c)) > STAND_NEAR => Want::Go(c, "out of the flame's way"),
            Some(_) => Want::Hold("out of the flame's way"),
            None => Want::Hold("the flame coming, no cell out of its way loaded"),
        };
    }
    // Every strike from under the wings' and head's sweep where one dig makes it so (race
    // 45876898172 lane 1: a head stand on the surface took 10 in the attacking turn, and the
    // natural takeoff's wings 5); the one that makes it take off from nowhere else.
    let must_low = crossing(ask.sat_damage);
    if sat && clear_of_blow(&v, feet) && head_reach(&v, eye) <= STRIKE_REACH {
        let lowerable_here = body.on_ground
            && under_wings(&v, feet[1] - 1.0)
            && lowerable(w, &v, &clouds, feet_cell);
        if under_wings(&v, feet[1]) || (!must_low && !lowerable_here) {
            return Want::Strike {
                entity: id + 1,
                head: true,
            };
        }
        if lowerable_here {
            return Want::Dig([feet_cell[0], feet_cell[1] - 1, feet_cell[2]]);
        }
    }
    // Scanning, the head turns onto the bot: wait where it will come (`scan_stand`), the stand
    // held while it is still on the line within a block; else the head stand as it points.
    // A turn that has not come in `SCAN_WAIT_TICKS` is not waited for: the head stand as it points
    // (E6's end_loop dragon never turns, and held the bot 15,000 ticks behind it).
    let scanning = v.phase() == Some(Phase::SittingScanning) && ask.phase_ticks < SCAN_WAIT_TICKS;
    let scan = |low: bool| {
        let fresh = scan_stand(&v, w, low)?;
        Some(
            ask.stand
                .filter(|h| flat(mid(*h), mid(fresh)) <= 1.5 && clear_of_blow(&v, mid(*h)))
                .unwrap_or(fresh),
        )
    };
    let hs = if scanning {
        scan(true).or_else(|| (!must_low).then(|| scan(false)).flatten())
    } else {
        None
    }
    .or_else(|| keep_head_stand(&v, w, ask.stand, true))
    .or_else(|| {
        (!must_low)
            .then(|| keep_head_stand(&v, w, ask.stand, false))
            .flatten()
    });
    if sat && let Some(hs) = hs {
        if breath_on_way(&clouds, feet, mid(hs), BREATH_HOLD) {
            return Want::Hold("a breath on the way to the head stand");
        }
        // The way round held is kept until it is reached or blocked (race 45873410249 lane 0:
        // the way round and the head stand swapped every tick 9 off the fountain).
        // Not dropped for the way from the feet to it: that judgement flips as the feet near the
        // head's strip (E6's end_loop: the way round and the stand swapped every 8 ticks, 15,000
        // ticks).
        if let Some(wp) = ask.way
            && flat(feet, mid(wp)) > STAND_NEAR
            && !before_head(&v, mid(wp))
            && !breathed(&clouds, mid(wp), 0.0)
        {
            return Want::Go(wp, "round the head");
        }
        // A way round already walked to this stand is not asked again: the last leg grazes the
        // strip's end, and asking again sent the bot back round (E6's end_loop: every 8 ticks).
        if ask.way_done != Some(hs)
            && let Some(p) = around(&v, feet, mid(hs))
            && let Some(c) = ground(w, p[0].floor() as i32, p[1].floor() as i32, feet_cell[1])
            && flat(feet, mid(c)) > STAND_NEAR
        {
            return Want::Go(c, "round the head");
        }
        if flat(feet, mid(hs)) > STAND_NEAR {
            return Want::Go(hs, "to the head stand");
        }
        // Near the stand but not in reach from where the feet are (race 45924968115 lane 1: 0.7
        // off its middle, 3.2 from the head's box, "no head stand open" every turn): onto its
        // middle.
        if flat(feet, mid(hs)) > CENTRE_NEAR {
            return Want::Centre(hs);
        }
    }
    // Landing, not yet sat, or no head stand open: the bunker (under every sweep: race
    // 45876898172 lane 1 took the landing wings' 5 twice on the ring 11 out), else out of its
    // reach to the wait's ring, never into the basin (race 45873410249 lane 0: the landing
    // dragon's head took 10 from a bot walking to a stand behind it).
    // Sat with no stand open: held near, out of the bite (the bunker is a walk away, and the
    // next turn opens a stand: race 45886819862 lane 2 went for the bunker mid-perch).
    if sat {
        if in_bite(&v, feet) || before_head(&v, feet) {
            return match step_out_cell(w, Some(&v), STEP_OUT_RADIUS + 4.0) {
                Some(c) if flat(feet, mid(c)) > STAND_NEAR => {
                    Want::Go(c, "out of the bite, no stand open")
                }
                Some(_) | None => Want::Hold("in the bite, no cell out loaded"),
            };
        }
        if scanning_near && from_fountain(feet) > SCAN_KEEP + 1.0 {
            return match step_out_cell(w, Some(&v), SCAN_KEEP) {
                Some(c) if flat(feet, mid(c)) > STAND_NEAR => Want::Go(c, "into the scan's reach"),
                Some(_) | None => Want::Hold("no head stand open"),
            };
        }
        return Want::Hold("no head stand open");
    }
    if ask.bunker.is_some() {
        return wait(w, ask, feet, "the dragon not yet sat, in the bunker");
    }
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
/// wings' reach of a dragon on the pillar (4.5 across plus 2 plus 4 inflated, 10.5), and of the
/// landing one coming in over the ring (race 45876898172 lane 1: hit 5 at 11 out, twice).
pub const WAIT_RING: f64 = 16.0;
pub const WAIT_FROM: f64 = 15.0;

/// The wait while the dragon flies: the bunker when there is one, else a cell on the ring
/// `WAIT_RING` out, out of the breath.
fn wait(w: &World, ask: &Ask, feet: [f64; 3], why: &'static str) -> Want {
    // Already in a roofed 2-high cell (a pocket sealed for an enderman, a notch): the wait is
    // there (race 45901376114 lane 0: sealed in a pocket, the walk to the bunker failed NoPath
    // 656 times, 13,000 ticks).
    if crate::end::covered(w) {
        return Want::Hold("in cover");
    }
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
    /// A charge in force, the charges in the run, and the walk to the bunker at one.
    #[serde(default)]
    pub charging: bool,
    #[serde(default)]
    pub charges: u32,
    #[serde(default)]
    pub charge_walk: Option<Walk>,
    /// The notch at a charge: the surface cell it was begun from, the dig in hand, and the step
    /// out to dig once the charge is over.
    #[serde(default)]
    pub notch_from: Option<[i32; 3]>,
    #[serde(default)]
    pub charge_dig: Option<crate::dig::Dig>,
    #[serde(default)]
    pub notch_step: Option<[i32; 3]>,
    /// The notch's roof once laid, its side block, and the clicks for them.
    #[serde(default)]
    pub notch_roof: Option<[i32; 3]>,
    #[serde(default)]
    pub notch_side: Option<[i32; 3]>,
    #[serde(default)]
    pub roof_tries: u32,
    #[serde(default)]
    pub roof_click_at: u64,
    /// The walk to the bunker from an angry enderman, the last strike at one, and the last note.
    #[serde(default)]
    pub ender_walk: Option<Walk>,
    #[serde(default)]
    pub ender_swing_at: Option<u64>,
    #[serde(default)]
    pub ender_noted: Option<String>,
    #[serde(default)]
    pub ender_noted_at: u64,
    /// Down a notch from a breath, and from an enderman.
    #[serde(default)]
    pub breath_notch: bool,
    #[serde(default)]
    pub ender_notch: bool,
    /// The notch's dig failed: crouched where the feet are, no dig tried again.
    #[serde(default)]
    pub notch_failed: bool,
    /// The height a notch is risen out to, a block at a jump, and the cell the last jump left.
    #[serde(default)]
    pub notch_rise_to: Option<i32>,
    /// Out of cover by a shaft up, the dragon on its perch.
    #[serde(default)]
    pub climb_out: bool,
    #[serde(default)]
    pub rise_from: Option<[i32; 3]>,
    /// The wall cell dug to get out of a pocket in a breath.
    #[serde(default)]
    pub breath_wall: Option<[i32; 3]>,
    /// Cells reached with the feet still in a breath, refused until out; the last cell noted.
    #[serde(default)]
    pub breath_refused: Vec<[i32; 3]>,
    #[serde(default)]
    pub breath_noted: Option<[i32; 3]>,
    #[serde(default)]
    pub breath_waits: u64,
    #[serde(default)]
    pub breath_waited_at: u64,
    /// When the phase last changed, server ticks.
    #[serde(default)]
    pub phase_at: Option<u64>,
    /// The perched dragon's damage toward its takeoff, as the server counts it, the dragon's health
    /// last seen, and the digs down for a low stand.
    #[serde(default)]
    pub sat_damage: f32,
    #[serde(default)]
    pub seen_health: Option<f32>,
    /// The last drop in its health while sitting, server ticks.
    #[serde(default)]
    pub drop_at: Option<u64>,
    #[serde(default)]
    pub low_digs: u32,
    /// The way round the head held, and how many were chosen; the stand one was walked to.
    #[serde(default)]
    pub way: Option<[i32; 3]>,
    #[serde(default)]
    pub way_done: Option<[i32; 3]>,
    #[serde(default)]
    pub ways: u32,
    /// Ticks a provoked enderman's pocket was passed over for the dragon, and the last note.
    #[serde(default)]
    pub first_skips: u64,
    #[serde(default)]
    pub first_noted_at: u64,
    /// In a strafe: the surface cell of the notch dug beside the bot, which it stands by in the
    /// dragon's sight until a fireball comes; the tick it was first hidden in this strafe.
    #[serde(default)]
    pub strafe_notch: Option<[i32; 3]>,
    #[serde(default)]
    pub hidden_since: Option<u64>,
    #[serde(default)]
    pub strafe_seen_noted: bool,
    /// The rows of `step_inner` whose controls were sent since the idle wait began, and in the End
    /// the tick and the note count when the bot was last idle with no new note
    /// (`Plan::end_idle_watch`).
    #[serde(default)]
    pub rows: Vec<String>,
    #[serde(default)]
    pub idle_since: Option<(u64, u64)>,
    #[serde(default)]
    pub idle_backstops: u32,
    /// The fireball being dodged, the cell the bot stood in when it was seen, the cell dodged to
    /// and the walk there, and the dodges in the run.
    #[serde(default)]
    pub dodge_id: Option<i32>,
    #[serde(default)]
    pub dodge_from: Option<[i32; 3]>,
    #[serde(default)]
    pub dodge_to: Option<[i32; 3]>,
    #[serde(default)]
    pub dodge_walk: Option<Walk>,
    #[serde(default)]
    pub dodges: u32,
    /// The enderman whose hold is running and when it began, whether it was capped, and the
    /// enderman a walk to the bunker failed for.
    #[serde(default)]
    pub ender_hold: Option<(i32, u64)>,
    #[serde(default)]
    pub ender_capped: bool,
    #[serde(default)]
    pub ender_walk_failed: Option<i32>,
    /// In a strafe: since when the dragon has had no line to the bot, the noted case, and the
    /// height the rise into its sight began from.
    #[serde(default)]
    pub unseen_since: Option<u64>,
    #[serde(default)]
    pub unseen_noted: bool,
    #[serde(default)]
    pub unseen_rise_from: Option<i32>,
    /// In a strafe: the pace leg's end and its walk, the side it went, the legs in the run; and
    /// the dodge pressed straight on after its walk failed.
    #[serde(default)]
    pub pace_to: Option<[i32; 3]>,
    #[serde(default)]
    pub pace_walk: Option<Walk>,
    #[serde(default)]
    pub pace_side: f64,
    #[serde(default)]
    pub pace_legs: u32,
    #[serde(default)]
    pub dodge_straight: bool,
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
    /// The dig down for a low stand in hand.
    #[serde(default)]
    pub dig: Option<crate::dig::Dig>,
}

impl Fight {
    pub fn new(tick: u64) -> Self {
        Self {
            since: tick,
            walk: None,
            eat: None,
            last_swing: None,
            said: String::new(),
            dig: None,
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
                // The takeoff count: damage taken while sitting, reset as the server does when it
                // goes over.
                // The blow that sends it over is seen with the phase already takeoff (race
                // 45895541443 lane 0: the count never reset, and no strike after 100 dealt).
                let was_sitting = self.dragon.phase.is_some_and(|p| Phase::of(p).sitting());
                if let Some(was) = self.dragon.seen_health
                    && v.health < was
                    && (was_sitting || v.phase().is_some_and(Phase::sitting))
                {
                    self.dragon.sat_damage += was - v.health;
                    self.dragon.drop_at = Some(now);
                }
                let takeoff_now = v.phase() == Some(Phase::Takeoff)
                    && self.dragon.phase != Some(v.phase)
                    && self
                        .dragon
                        .drop_at
                        .is_some_and(|t| now.saturating_sub(t) <= TAKEOFF_AFTER_BLOW)
                    && self.dragon.sat_damage + STRIKE_DAMAGE + STRIKE_MARGIN > PERCH_TAKEOFF;
                if self.dragon.sat_damage > PERCH_TAKEOFF || takeoff_now {
                    self.note(
                        tick,
                        format!("dragon takeoff count over ({:.1})", self.dragon.sat_damage),
                        format!("health {:.1}; the count starts again", v.health),
                    );
                    self.dragon.sat_damage = 0.0;
                }
                self.dragon.seen_health = Some(v.health);
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
                    self.dragon.phase_at = Some(now);
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
        // A way round reached: its stand is gone to straight from here.
        if let (Some(wp), Some(b)) = (self.dragon.way, w.body)
            && flat(b.pos, mid(wp)) <= STAND_NEAR
        {
            self.dragon.way = None;
            self.dragon.way_done = self.dragon.stand;
        }
        let ask = Ask {
            danger,
            blow: HEAD_BLOW,
            perched_ticks: self.dragon.perched_ticks,
            stand: self.dragon.stand,
            bunker,
            dead: self.dragon.dead,
            food: food.is_some(),
            way: self.dragon.way,
            way_done: self.dragon.way_done,
            sat_damage: self.dragon.sat_damage,
            phase_ticks: self
                .dragon
                .phase_at
                .map_or(0, |at| w.session.server_ticks.saturating_sub(at)),
        };
        // Out of cover by a shaft up, the dragon on its perch: the block over the head dug, else
        // a block put under the feet at a jump, until the sky is over the head.
        if self.dragon.climb_out {
            if world::stage::open_sky(w) || !view(w).is_some_and(|(_, v)| grounded(&v)) {
                self.dragon.climb_out = false;
                self.dragon.rise_from = None;
                self.note(
                    tick,
                    "dragon: out of cover".to_owned(),
                    format!("at {:?}", w.body.map(|b| cell_of(b.pos))),
                );
            } else if let Some(body) = w.body
                && breathed(&breaths(w), body.pos, 0.0)
            {
                // A breath over the shaft's top: stay in cover until it clears.
                return Controls {
                    sneak: true,
                    ..looking_down(w)
                };
            } else if let Some(body) = w.body {
                let fc = cell_of(body.pos);
                let head = [fc[0], fc[1] + 2, fc[2]];
                let solid = w
                    .block_at(world::BlockPos::new(head[0], head[1], head[2]))
                    .is_some_and(|k| k != BlockKind::Air);
                if solid && body.on_ground {
                    return self
                        .charge_dig_step(w, tick, head)
                        .unwrap_or_else(|| looking_down(w));
                }
                return self.rise_step(w, tick, body);
            }
        }
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
            Want::Dig(c) => format!("dig down at {c:?} for a stand under the wings"),
            Want::Centre(c) => format!("onto the middle of {c:?}"),
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
                    self.dragon.stand =
                        keep_head_stand(&v, w, self.dragon.stand, crossing(self.dragon.sat_damage));
                }
            }
            Want::Go(..) => {
                self.dragon.way = None;
            }
            Want::Exit(_)
            | Want::Strike { .. }
            | Want::Eat
            | Want::Dig(_)
            | Want::Centre(_)
            | Want::Hold(_) => {
                self.dragon.way = None;
                self.dragon.way_done = None;
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
        if !matches!(want, Want::Dig(_)) {
            f.dig = None;
        }
        match want {
            Want::Dig(c) => {
                let cell: Cell = (c[0], c[1], c[2]);
                if f.dig.as_ref().is_none_or(|d| d.target != cell) {
                    f.dig = Some(crate::dig::Dig::new(w, cell, tick));
                    self.dragon.low_digs += 1;
                }
                let Some(d) = f.dig.as_mut() else {
                    return looking_down(w);
                };
                let (mut c, act, status) = d.step(w, tick);
                self.acts.extend(act);
                if !matches!(status, crate::dig::DigStatus::Running)
                    || w.block_at(world::BlockPos::new(cell.0, cell.1, cell.2))
                        == Some(BlockKind::Air)
                {
                    f.dig = None;
                }
                if c.pitch.is_none() {
                    c.pitch = Some(90.0);
                }
                c
            }
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
            Want::Centre(c) => {
                let feet = w.body.map_or([0.0; 3], |b| b.pos);
                let to = mid(c);
                Controls {
                    forward: true,
                    sneak: true,
                    yaw: crate::walk::yaw_to(feet, to),
                    pitch: Some(90.0),
                    ..Controls::default()
                }
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
            // Sealed in cover, a walk out has no way: the next choice waits a while, the End's
            // own rows (the pocket's, the bunker's) open it.
            let covered = crate::end::covered(w);
            // In cover with the dragon on its perch: out by a shaft straight up, the roof dug and a
            // block put under the feet at each jump (race 45918669138 lane 1: held covered from an
            // angry enderman the whole race, 0 strikes in 24,000 ticks).
            if covered && view(w).is_some_and(|(_, v)| grounded(&v)) {
                self.dragon.climb_out = true;
                self.note(
                    tick,
                    format!("dragon walk to {cell:?} failed: {e:?}"),
                    "in cover with the dragon on its perch: out by a shaft up".to_owned(),
                );
                return Controls::default();
            }
            self.note(
                tick,
                format!("dragon walk to {cell:?} failed: {e:?}"),
                if covered {
                    format!("in cover; the fight plans again in {COVERED_RETRY_TICKS} ticks")
                } else {
                    "the fight plans again".to_owned()
                },
            );
            self.task = Task::Idle;
            self.retry_at = tick
                + if covered {
                    COVERED_RETRY_TICKS
                } else {
                    crate::RETRY_TICKS
                };
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
        // >>> E4 (issue 20f): up a pillar or a spike there is no cell to walk out to; the
        // crystals' way down takes the body out of a breath (race 45899557907 lane 1: the walk
        // out of a breath on a pillar's top at y 83, "NoPath" every tick, then the fall).
        if self.spikes_up_high(w) {
            return None;
        }
        // <<< E4
        let feet = w.body?.pos;
        // Clouds only: a fireball in flight is the dodge's, never a notch or the bunker (wall 28).
        let clouds = clouds_only(w);
        let crouch = Controls {
            sneak: true,
            yaw: f64::from(w.player.yaw),
            pitch: Some(90.0),
            ..Controls::default()
        };
        let Some(on) = breath_on(&clouds, feet) else {
            // Down a notch from a breath: held there while a cloud lies over it on the plane,
            // then out by the roof and the step.
            if self.dragon.breath_notch {
                if breathed(&clouds, feet, 0.0) {
                    return Some(crouch);
                }
                self.dragon.breath_notch = false;
                self.note(
                    tick,
                    "breath: out of the notch".to_owned(),
                    format!("no breath over {:?}", cell_of(feet)),
                );
                self.notch_leave(w);
            }
            if let Some(c) = self.notch_exit_step(w, tick) {
                return Some(c);
            }
            // The way into the bunker kept to the end while a breath lies near on the plane, and
            // in it the bot stays while one lies over it: the body's height at the stair's top is
            // in and out of the cloud from tick to tick (race 45918669138 lane 2: "into the
            // bunker" and "out of the breath" 40 times each, the stair climbed back into the cloud
            // between, 20 to dead).
            // Not through the strike window: with the dragon scanning or attacking on its perch the
            // fight goes (race 45924968115 lane 2: held in the bunker 7,320 ticks by the clouds the
            // dragon kept sending, the dragon at 200).
            let window = view(w).is_some_and(|(_, v)| {
                matches!(
                    v.phase(),
                    Some(Phase::SittingScanning | Phase::SittingAttacking)
                )
            });
            if let Some(b) = self.bunker
                && !window
                && breathed(&clouds, feet, BUNKER_HOLD)
            {
                // With nothing in hand the End's own rows keep the bunker shut (E3's "stays
                // shut for a … near its stair"); a task in hand is held.
                if cell_of(feet) == b {
                    if matches!(self.task, Task::Idle) {
                        return None;
                    }
                    return Some(crouch);
                }
                if self
                    .dragon
                    .breath_walk
                    .as_ref()
                    .is_some_and(|wk| wk.target == (b[0], b[1], b[2]))
                {
                    return self.breath_walk_step(w, tick, b);
                }
            }
            self.dragon.breath_refused.clear();
            self.dragon.breath_noted = None;
            self.dragon.breath_wall = None;
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
        // In a notch already: down and roofed, never a walk out sideways.
        if self.dragon.breath_notch {
            return Some(self.notch_dig_step(w, tick, crouch, "breath"));
        }
        // Shelter near (the bunker within `BUNKER_NEAR`): into it, down, never out sideways
        // (E4's race 45912100302 lane 0: nine walks out of a fireball's cloud by the bunker failed
        // NoPath, then dead). In it with the breath still on: a notch from there.
        // Only a bunker clear of every breath (on the plane and in height); else a notch here.
        // And only a bunker shut, roof and door: an open one lets the fireball in (frontier race
        // 45931225605 lane 1: sheltered in it, the cloud on the bot inside it).
        if let Some(b) = self.bunker
            && flat(feet, mid(b)) <= BUNKER_NEAR
            && (cell_of(feet) == b || (clear_of_breath(&clouds, mid(b)) && self.bunker_shut(w)))
        {
            if cell_of(feet) == b {
                self.dragon.breath_notch = true;
                self.note(
                    tick,
                    format!("breath: in the bunker at {b:?}, a notch down"),
                    format!(
                        "a breath {:.1} off, radius {:.1}",
                        flat(on.pos, feet),
                        on.radius
                    ),
                );
                return Some(self.notch_dig_step(w, tick, crouch, "breath"));
            }
            let cell: Cell = (b[0], b[1], b[2]);
            if self
                .dragon
                .breath_walk
                .as_ref()
                .is_none_or(|wk| wk.target != cell)
            {
                self.dragon.breath_walk = Some(Walk::new(
                    cell,
                    Goal::Block { pos: cell },
                    Arrive::Goal,
                    tick,
                ));
                self.note(
                    tick,
                    format!("breath: into the bunker at {b:?}"),
                    format!(
                        "a breath {:.1} off, radius {:.1}",
                        flat(on.pos, feet),
                        on.radius
                    ),
                );
            }
            return self.breath_walk_step(w, tick, b);
        }
        // The cell held while no breath reaches it; else the nearest one anew.
        let held = self
            .dragon
            .breath_walk
            .as_ref()
            .map(|wk| [wk.target.0, wk.target.1, wk.target.2])
            .filter(|c| clear_of_breath(&clouds, mid(*c)) && *c != cell_of(feet));
        let target = match held {
            Some(c) => c,
            None => {
                let Some(c) = out_of_breath_except(w, feet, &self.dragon.breath_refused) else {
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
                if self.dragon.breath_noted == Some(c) {
                    return self.breath_walk_step(w, tick, c);
                }
                self.dragon.breath_noted = Some(c);
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
        self.breath_walk_step(w, tick, target)
    }

    /// One tick of the walk out of a breath to `target`.
    fn breath_walk_step(&mut self, w: &World, tick: u64, target: [i32; 3]) -> Option<Controls> {
        // A wall dig begun before the notch rule finishes (none is begun now).
        if let Some(cell) = self.dragon.breath_wall {
            let air = |c: [i32; 3]| {
                w.block_at(world::BlockPos::new(c[0], c[1], c[2])) == Some(BlockKind::Air)
            };
            if !air(cell) {
                return self.charge_dig_step(w, tick, cell);
            }
            self.dragon.breath_wall = None;
            let next = wall_toward(w, w.body.map_or([0.0; 3], |b| b.pos), target);
            if let Some(n) = next {
                self.dragon.breath_wall = Some(n);
                return self.charge_dig_step(w, tick, n);
            }
        }
        let wk = self.dragon.breath_walk.as_mut()?;
        let (mut c, status) = wk.step(w, tick);
        let (acts, said) = wk.take_out();
        self.acts.extend(acts);
        for (what, why) in said {
            self.note(tick, what, why);
        }
        if !matches!(status, TaskStatus::Running) {
            // Arrived with the feet still in a breath (it grew onto the cell) or failed: that cell
            // is refused until out, and the next tick looks again.
            self.dragon.breath_walk = None;
            if self.dragon.breath_refused.len() < 32 {
                self.dragon.breath_refused.push(target);
            }
            if let TaskStatus::Failed(e) = status {
                // No way out on foot: down a notch where the feet are, roofed, not a dig sideways
                // through walls (E4's race 45912100302 lane 0; frontier lane 1 on the stair).
                self.dragon.breath_notch = true;
                self.note(
                    tick,
                    format!("walk out of the breath to {target:?} failed: {e:?}"),
                    "a notch down where the feet are".to_owned(),
                );
            }
        }
        if c.pitch.is_none() {
            c.pitch = Some(90.0);
        }
        Some(c)
    }

    /// In the End, the dragon charging the bot (`charging_at`) with the feet under the open sky:
    /// into the bunker when it is within `CHARGE_BUNKER_REACH`, else crouched still where the feet
    /// are, no dig and no walk begun in the open (E4's race 4: a charge hit a bot digging down two
    /// over the ground for 10 and 5 and threw it 48 high). A breath on the feet is left to
    /// `breath_step`. Noted "charge: hold …" once a charge.
    pub fn charge_step(&mut self, w: &World, tick: u64) -> Option<Controls> {
        if !in_end(w) {
            self.dragon.charge_walk = None;
            return None;
        }
        let body = w.body?;
        // A charge, or a strafe: a fireball comes only in the strafe, at a player in sight, so
        // the shelter is taken before it (frontier race 45924970608: two bots died of breath on
        // open ground, the cloud already on them before the notch was dug).
        let charge = charging_at(w, body.pos);
        // A fireball in flight counts as the strafe that sent it: the strafe may end, or start
        // beyond 64, before it lands (frontier race 45931225605 lane 0: the strafe 56 off at its
        // end, the fireball 40 ticks in the air with the phase holding, the cloud on the bot).
        let fireball = fireball_at(w, body.pos);
        let strafe = charge
            .is_none()
            .then(|| strafing_at(w, body.pos).or(fireball))
            .flatten();
        let Some(d) = charge.or(strafe) else {
            self.dragon.dodge_id = None;
            self.dragon.dodge_walk = None;
            self.dragon.strafe_notch = None;
            self.dragon.hidden_since = None;
            self.dragon.strafe_seen_noted = false;
            self.dragon.unseen_since = None;
            self.dragon.unseen_noted = false;
            self.dragon.unseen_rise_from = None;
            self.dragon.pace_to = None;
            self.dragon.pace_walk = None;
            if self.dragon.charging {
                // Stayed in until no cloud lies within reach.
                if breathed(&breaths(w), body.pos, 0.0) {
                    return Some(Controls {
                        sneak: true,
                        yaw: f64::from(w.player.yaw),
                        pitch: Some(90.0),
                        ..Controls::default()
                    });
                }
                self.dragon.charging = false;
                self.dragon.charge_walk = None;
                self.dragon.charge_dig = None;
                self.note(
                    tick,
                    "charge: over".to_owned(),
                    format!("at {:?}", cell_of(body.pos)),
                );
                self.notch_leave(w);
            }
            return self.notch_exit_step(w, tick);
        };
        // Up a pillar or a spike the crystals' own holds answer (E4's guard ring, the way down).
        if self.spikes_up_high(w) {
            return None;
        }
        // A fireball in flight aimed near: dodged on open ground, at least `DODGE_MIN` from where
        // the bot stood (wall 28: its cloud is put on a body within 4 of its hit, walls or not).
        // Ahead of the walk out of a cloud on the feet: the dodge cell is clear of every cloud,
        // and the walk out went the other way (dev race 45947481217 lane 2, wall 32).
        if charge.is_none() && fireball.is_some() && world::stage::open_sky(w) {
            if let Some(c) = self.fireball_dodge_step(w, tick) {
                return Some(c);
            }
            // Not aimed near: the strafe met in sight, else on as it was.
            strafing_at(w, body.pos)?;
        }
        // Clouds only: a fireball in flight is answered by the dodge, never by cover (wall 28).
        let clouds = clouds_only(w);
        if breath_on(&clouds, body.pos).is_some() {
            return None;
        }
        // E3's bunker in the making answers a strafe itself: no bunker yet (its dig waits for
        // the dragon) or its shaft being dug, roofed as it goes (E3's test
        // a_flying_dragon_near_holds_the_bunker_shaft).
        if strafe.is_some()
            && (self.bunker.is_none()
                || self
                    .end
                    .work
                    .as_ref()
                    .is_some_and(|wk| wk.kind == crate::end::WorkKind::Build))
        {
            return None;
        }
        // A strafe is met in the dragon's sight: hidden, the strafe never ends (wall 25). Only a
        // cloud within reach (no fireball's) sends the bot to cover.
        if charge.is_none() {
            let cloud = breathed(&clouds, body.pos, OUT_SLACK);
            if let Some(c) = self.strafe_seen_step(w, tick, body, cloud) {
                return c;
            }
            if fireball.is_some() && !cloud {
                return None;
            }
        }
        if !world::stage::open_sky(w) {
            return None;
        }
        // For a strafe only a bunker shut (roof and door): an open one lets the fireball in.
        let shut = strafe.is_none() || self.bunker_shut(w);
        let bunker = self.bunker.filter(|b| {
            shut && flat(body.pos, mid(*b)) <= CHARGE_BUNKER_REACH
                && (body.pos[1] - f64::from(b[1])).abs() <= 5.0
        });
        if !self.dragon.charging {
            self.dragon.charging = true;
            self.dragon.charges += 1;
            self.dragon.roof_tries = 0;
            self.note(
                tick,
                match (bunker, strafe.is_some()) {
                    (Some(b), true) => {
                        format!("strafe: shelter from a cloud within reach (the bunker at {b:?})")
                    }
                    (None, true) => {
                        "strafe: shelter from a cloud within reach (a notch)".to_owned()
                    }
                    (Some(b), false) => format!("charge: hold in the bunker at {b:?}"),
                    (None, false) => "charge: hold crouched where the feet are".to_owned(),
                },
                format!(
                    "the dragon charging {d:.0} off; task {}; health {:.1}; {} charges in the run",
                    self.task.name(),
                    w.player.health.unwrap_or(0.0),
                    self.dragon.charges
                ),
            );
        }
        let crouch = Controls {
            sneak: true,
            yaw: f64::from(w.player.yaw),
            pitch: Some(90.0),
            ..Controls::default()
        };
        let Some(b) = bunker else {
            // No bunker near: a notch two deep where the feet are, dug at once and dropped into,
            // under the charge's head and wings (E4's race 4; race 45895541443 lane 2 crouched on
            // the surface and took 10, then 5 and 5).
            return Some(self.notch_dig_step(w, tick, crouch, "charge"));
        };
        let cell: Cell = (b[0], b[1], b[2]);
        if self
            .dragon
            .charge_walk
            .as_ref()
            .is_none_or(|wk| wk.target != cell)
        {
            self.dragon.charge_walk = Some(Walk::new(
                cell,
                Goal::Block { pos: cell },
                Arrive::Goal,
                tick,
            ));
        }
        let wk = self.dragon.charge_walk.as_mut()?;
        let (mut c, status) = wk.step(w, tick);
        let (acts, said) = wk.take_out();
        self.acts.extend(acts);
        for (what, why) in said {
            self.note(tick, what, why);
        }
        match status {
            TaskStatus::Running => {}
            TaskStatus::Done => {
                self.dragon.charge_walk = None;
                return Some(crouch);
            }
            TaskStatus::Failed(e) => {
                self.dragon.charge_walk = None;
                self.note(
                    tick,
                    format!("charge: walk to the bunker failed: {e:?}"),
                    "crouched where the feet are".to_owned(),
                );
                return Some(crouch);
            }
        }
        if c.pitch.is_none() {
            c.pitch = Some(90.0);
        }
        Some(c)
    }

    /// The row `row` of `step_inner` sent controls `c`: kept for the idle backstop's note.
    pub(crate) fn held(&mut self, row: &str, c: Controls) -> Controls {
        if self.dragon.rows.len() < ROWS_KEPT && !self.dragon.rows.iter().any(|r| r == row) {
            self.dragon.rows.push(row.to_owned());
        }
        c
    }

    /// The backstop: in the End, idle with no note for `IDLE_NOTE_TICKS`, noted with the rows that
    /// held it and handed back to the planner (E5's holds dropped, the choice made again now).
    pub fn end_idle_watch(&mut self, w: &World, tick: u64) {
        if !in_end(w) || !matches!(self.task, Task::Idle) {
            self.dragon.idle_since = None;
            self.dragon.rows.clear();
            return;
        }
        let count = self.event_count;
        let Some((since, _)) = self.dragon.idle_since.filter(|(_, n)| *n == count) else {
            self.dragon.idle_since = Some((tick, count));
            self.dragon.rows.clear();
            return;
        };
        if tick < since + IDLE_NOTE_TICKS {
            return;
        }
        self.dragon.idle_backstops += 1;
        let d = &mut self.dragon;
        d.charging = false;
        d.charge_walk = None;
        d.charge_dig = None;
        d.notch_from = None;
        d.notch_step = None;
        d.notch_roof = None;
        d.notch_side = None;
        d.notch_rise_to = None;
        d.notch_failed = false;
        d.strafe_notch = None;
        d.hidden_since = None;
        d.breath_walk = None;
        d.breath_wall = None;
        self.retry_at = tick;
        let rows = if self.dragon.rows.is_empty() {
            "none".to_owned()
        } else {
            self.dragon.rows.join(", ")
        };
        self.note(
            tick,
            format!(
                "end: idle {} ticks at {}",
                tick - since,
                w.body
                    .map_or_else(|| "no body".to_owned(), |b| format!("{:?}", cell_of(b.pos)))
            ),
            format!(
                "task {}, rows held {rows}; {} backstops; handed back to the planner",
                self.task.name(),
                self.dragon.idle_backstops
            ),
        );
        self.dragon.idle_since = Some((tick, self.event_count));
        self.dragon.rows.clear();
    }

    /// One tick of a strafe met in sight (no charge, no fireball in flight: `fireball_dodge_step`
    /// answers that). `cloud` is a cloud within reach. Hidden in its notch the bot stays
    /// `STRAFE_HIDE_TICKS`, then rises out; in the open it stands where its feet are, seen (the
    /// strafe ends only by its fireball, which needs the bot in sight). `None` leaves the tick to
    /// the shelter as it was: a cloud within reach in the open.
    fn strafe_seen_step(
        &mut self,
        w: &World,
        tick: u64,
        body: physics::Body,
        cloud: bool,
    ) -> Option<Option<Controls>> {
        let fc = cell_of(body.pos);
        let crouch = Controls {
            sneak: true,
            yaw: f64::from(w.player.yaw),
            pitch: Some(90.0),
            ..Controls::default()
        };
        let air = |c: [i32; 3]| {
            w.block_at(world::BlockPos::new(c[0], c[1], c[2])) == Some(BlockKind::Air)
        };
        // Hidden in its notch.
        if self.dragon.notch_from.is_some_and(|f| fc[1] <= f[1] - 2) {
            if cloud {
                return Some(Some(self.notch_dig_step(w, tick, crouch, "strafe")));
            }
            let since = *self.dragon.hidden_since.get_or_insert(tick);
            if tick < since + STRAFE_HIDE_TICKS {
                return Some(Some(crouch));
            }
            let from = self.dragon.notch_from.take();
            self.dragon.notch_side = None;
            self.dragon.notch_step = None;
            self.dragon.notch_rise_to = from.map(|f| f[1]);
            self.dragon.charging = false;
            self.dragon.hidden_since = None;
            self.note(
                tick,
                format!("strafe: out of the notch to be seen at {fc:?}"),
                format!(
                    "hidden {} ticks in the strafe; risen out a block at a jump",
                    tick - since
                ),
            );
            return Some(self.notch_exit_step(w, tick));
        }
        // On the way out of a notch (its roof, its step, the rise).
        if (self.dragon.notch_rise_to.is_some()
            || self.dragon.notch_step.is_some()
            || self.dragon.notch_roof.is_some_and(|r| !air(r)))
            && let Some(c) = self.notch_exit_step(w, tick)
        {
            return Some(Some(c));
        }
        // Out of the dragon's sight under a strafe: counted, whatever the sky (frontier race
        // 45943073264 lane 1: stood 10 under the surface, the strafe held 23,500 ticks).
        let seen = view(w).is_some_and(|(_, v)| {
            crate::cover::seen_body(w, &[[v.pos[0], v.pos[1] + 1.0, v.pos[2]]], body.pos)
        });
        let unseen = if seen {
            self.dragon.unseen_since = None;
            0
        } else {
            tick - *self.dragon.unseen_since.get_or_insert(tick)
        };
        if !world::stage::open_sky(w) || cloud {
            if unseen >= STRAFE_UNSEEN_TICKS && !self.dragon.unseen_noted {
                self.dragon.unseen_noted = true;
                self.note(
                    tick,
                    format!("strafe: unseen {unseen} ticks at {fc:?}, no sky"),
                    "the strafe holds while the bot is out of sight".to_owned(),
                );
            }
            return None;
        }
        // Unseen under the sky (a pit, a shaft, beside a wall): up a block at a jump until seen,
        // at most `STRAFE_RISES` over where the rise began.
        if unseen >= STRAFE_UNSEEN_TICKS
            && self
                .dragon
                .unseen_rise_from
                .is_none_or(|y| fc[1] < y + STRAFE_RISES)
        {
            let from = *self.dragon.unseen_rise_from.get_or_insert(fc[1]);
            if !self.dragon.unseen_noted {
                self.dragon.unseen_noted = true;
                self.note(
                    tick,
                    format!("strafe: unseen {unseen} ticks at {fc:?}, risen a block at a jump"),
                    format!(
                        "from y {from}, at most {STRAFE_RISES} up, until the dragon sees the bot"
                    ),
                );
            }
            return Some(Some(self.rise_step(w, tick, body)));
        }
        // In sight and on the move: a leg of `DODGE_MIN` to `DODGE_MAX` sideways to the dragon's
        // bearing, then back (a fireball aims where the bot is when it fires, with no lead; 20
        // ticks on, a walking bot is 4 off: frontier race 45947478869, three bots stood or turned
        // and the cloud was put on them). No notch, no cover (wall 28).
        let stand = Controls {
            sneak: true,
            yaw: f64::from(w.player.yaw),
            ..Controls::default()
        };
        Some(Some(self.pace_step(w, tick, body, stand)))
    }

    /// One tick of the answer to a dragon fireball in flight aimed near the bot: a walk on open
    /// ground, sideways to its flight, to a cell at least `DODGE_MIN` from the cell the bot stood
    /// in when it was first seen (wall 28: `DragonFireball.onHit` puts its cloud on a body within
    /// 4 of the hit, walls or not, so only distance shelters). Never into a notch or the bunker.
    /// `None` when no fireball is aimed near (the tick goes on as it would).
    pub fn fireball_dodge_step(&mut self, w: &World, tick: u64) -> Option<Controls> {
        let body = w.body?;
        let feet = body.pos;
        let Some((id, fb)) = w
            .entities
            .iter()
            .filter(|(_, e)| e.kind == EntityKind::DragonFireball)
            .map(|(id, e)| (*id, e))
            .filter(|(_, e)| dist3(e.pos, feet) <= FIREBALL_RANGE)
            .min_by(|a, b| dist3(a.1.pos, feet).total_cmp(&dist3(b.1.pos, feet)))
        else {
            self.dragon.dodge_id = None;
            self.dragon.dodge_walk = None;
            return None;
        };
        // Its flight: the server's velocity, else straight at the bot (the strafe aims there).
        let flight = fb
            .velocity
            .filter(|v| v[0].hypot(v[2]) > 1e-6)
            .map_or([feet[0] - fb.pos[0], feet[2] - fb.pos[2]], |v| [v[0], v[2]]);
        let len = flight[0].hypot(flight[1]);
        if len < 1e-6 {
            return None;
        }
        let dir = [flight[0] / len, flight[1] / len];
        let to_bot = [feet[0] - fb.pos[0], feet[2] - fb.pos[2]];
        let along = to_bot[0] * dir[0] + to_bot[1] * dir[1];
        let miss = (to_bot[0] * dir[1] - to_bot[1] * dir[0]).abs();
        if self.dragon.dodge_id != Some(id) {
            if along <= 0.0 || miss > DODGE_AIM {
                return None;
            }
            self.dragon.dodge_id = Some(id);
            self.dragon.dodge_walk = None;
            // From water no walk plans: pressed straight on from the first tick.
            self.dragon.dodge_straight = in_water(w, cell_of(feet));
            let from = cell_of(feet);
            self.dragon.dodge_from = Some(from);
            let banned = self.bunker_columns();
            // On the way the bot already walks (a pace leg): the side its leg goes.
            let perp = [-dir[1], dir[0]];
            let prefer = self.dragon.pace_to.map(|c| {
                (f64::from(c[0]) + 0.5 - feet[0]) * perp[0]
                    + (f64::from(c[2]) + 0.5 - feet[2]) * perp[1]
            });
            self.dragon.pace_to = None;
            self.dragon.pace_walk = None;
            let to = dodge_cell(w, from, dir, view(w).map(|(_, v)| v.pos), &banned, prefer);
            self.dragon.dodge_to = to;
            let eta = (along / FIREBALL_SPEED).ceil();
            match to {
                Some(c) => {
                    self.dragon.dodges += 1;
                    self.note(
                        tick,
                        format!("fireball: dodge to {c:?}"),
                        format!(
                            "flight {:.2},{:.2}; ticks {eta:.0}; from {from:?}, {:.1} off its line; {} dodges",
                            dir[0], dir[1], miss, self.dragon.dodges
                        ),
                    );
                }
                None => self.note(
                    tick,
                    format!("fireball: no dodge cell from {from:?}"),
                    format!(
                        "flight {:.2},{:.2}; ticks {eta:.0}; stood in the open",
                        dir[0], dir[1]
                    ),
                ),
            }
        }
        let stand = Controls {
            yaw: f64::from(w.player.yaw),
            ..Controls::default()
        };
        let Some(c) = self.dragon.dodge_to else {
            return Some(stand);
        };
        let cell: Cell = (c[0], c[1], c[2]);
        if cell_of(feet) == c {
            self.dragon.dodge_walk = None;
            return Some(stand);
        }
        if self.dragon.dodge_straight {
            return Some(toward(feet, mid(c)));
        }
        if self
            .dragon
            .dodge_walk
            .as_ref()
            .is_none_or(|wk| wk.target != cell)
        {
            self.dragon.dodge_walk = Some(Walk::new(
                cell,
                Goal::Block { pos: cell },
                Arrive::Goal,
                tick,
            ));
        }
        let wk = self.dragon.dodge_walk.as_mut()?;
        let (ctl, status) = wk.step(w, tick);
        let (acts, said) = wk.take_out();
        self.acts.extend(acts);
        for (what, why) in said {
            self.note(tick, what, why);
        }
        match status {
            TaskStatus::Running => Some(ctl),
            TaskStatus::Done => {
                self.dragon.dodge_walk = None;
                Some(stand)
            }
            TaskStatus::Failed(e) => {
                // Pressed straight on toward it, never stood (frontier race 45947478869 lane 1:
                // NoPath at once, stood, the cloud on the bot).
                self.dragon.dodge_walk = None;
                self.dragon.dodge_straight = true;
                self.note(
                    tick,
                    format!("fireball: the dodge to {c:?} failed: {e:?}"),
                    "pressed straight on toward it".to_owned(),
                );
                Some(toward(feet, mid(c)))
            }
        }
    }

    /// The columns on the plane of the bunker (its cell and roof) and its stair's cells; none with
    /// no bunker.
    pub fn bunker_columns(&self) -> Vec<(i32, i32)> {
        let Some(b) = self.bunker else {
            return Vec::new();
        };
        let mut cols = vec![(b[0], b[2])];
        if let Some(door) = self.end.door {
            let (_, _, cells) = crate::end::bunker_cells((b[0], b[1], b[2]), door);
            cols.extend(cells.iter().map(|c| (c.0, c.2)));
        }
        cols
    }

    /// One tick of the pace in a strafe: the leg in hand walked, a new one at its end (the other
    /// side), stood where the feet are when none is to be had.
    fn pace_step(
        &mut self,
        w: &World,
        tick: u64,
        body: physics::Body,
        stand: Controls,
    ) -> Controls {
        let feet = body.pos;
        let fc = cell_of(feet);
        let done = self
            .dragon
            .pace_to
            .is_none_or(|c| fc == c || flat(feet, mid(c)) < 1.0);
        if done {
            self.dragon.pace_walk = None;
            let Some((_, v)) = view(w) else {
                return self.strafe_stood(tick, fc, stand);
            };
            let (bx, bz) = (feet[0] - v.pos[0], feet[2] - v.pos[2]);
            let len = bx.hypot(bz);
            if len < 1e-6 || !body.on_ground {
                return self.strafe_stood(tick, fc, stand);
            }
            self.dragon.pace_side = if self.dragon.pace_side > 0.0 {
                -1.0
            } else {
                1.0
            };
            let banned = self.bunker_columns();
            let Some(to) = dodge_cell(
                w,
                fc,
                [bx / len, bz / len],
                Some(v.pos),
                &banned,
                Some(self.dragon.pace_side),
            ) else {
                self.dragon.pace_to = None;
                return self.strafe_stood(tick, fc, stand);
            };
            self.dragon.pace_to = Some(to);
            self.dragon.pace_legs += 1;
            if !self.dragon.strafe_seen_noted {
                self.dragon.strafe_seen_noted = true;
                self.note(
                    tick,
                    format!("strafe: paced in sight from {fc:?} to {to:?}"),
                    format!(
                        "sideways to the dragon's bearing; {} legs in the run",
                        self.dragon.pace_legs
                    ),
                );
            }
        }
        let Some(c) = self.dragon.pace_to else {
            return self.strafe_stood(tick, fc, stand);
        };
        // From water no walk plans: straight on toward the leg's end (wall 33).
        if in_water(w, fc) {
            self.dragon.pace_walk = None;
            return toward(feet, mid(c));
        }
        let cell: Cell = (c[0], c[1], c[2]);
        if self
            .dragon
            .pace_walk
            .as_ref()
            .is_none_or(|wk| wk.target != cell)
        {
            self.dragon.pace_walk = Some(Walk::new(
                cell,
                Goal::Block { pos: cell },
                Arrive::Goal,
                tick,
            ));
        }
        let Some(wk) = self.dragon.pace_walk.as_mut() else {
            return stand;
        };
        let (ctl, status) = wk.step(w, tick);
        let (acts, said) = wk.take_out();
        self.acts.extend(acts);
        for (what, why) in said {
            self.note(tick, what, why);
        }
        match status {
            TaskStatus::Running => ctl,
            TaskStatus::Done | TaskStatus::Failed(_) => {
                self.dragon.pace_walk = None;
                self.dragon.pace_to = None;
                stand
            }
        }
    }

    /// Stood where the feet are in a strafe, in sight, noted once a strafe.
    fn strafe_stood(&mut self, tick: u64, fc: [i32; 3], stand: Controls) -> Controls {
        if !self.dragon.strafe_seen_noted {
            self.dragon.strafe_seen_noted = true;
            self.note(
                tick,
                format!("strafe: stood in sight at {fc:?}"),
                "no cover; its fireball is dodged".to_owned(),
            );
        }
        stand
    }

    /// One tick of the notch two deep where the feet are: the end stone under them dug, twice,
    /// then roofed (`notch_roof_step`); crouched still when there is nothing more to dig. Begun
    /// where the feet are the first time; `why` names it in the notes.
    fn notch_dig_step(&mut self, w: &World, tick: u64, crouch: Controls, why: &str) -> Controls {
        let Some(body) = w.body else {
            return crouch;
        };
        let fc = cell_of(body.pos);
        if self.dragon.notch_from.is_none() {
            self.dragon.notch_from = Some(fc);
            self.dragon.roof_tries = 0;
            self.dragon.notch_failed = false;
        }
        if self.dragon.notch_failed {
            return crouch;
        }
        let from = self.dragon.notch_from.unwrap_or(fc);
        if fc[1] > from[1] - 2 && body.on_ground && notch_diggable(w, fc) {
            if self.dragon.charge_dig.is_none() {
                self.note(
                    tick,
                    format!("{why}: dig a notch at {:?}", [fc[0], fc[1] - 1, fc[2]]),
                    format!("{} of 2 down", from[1] - fc[1] + 1),
                );
            }
            return self
                .charge_dig_step(w, tick, [fc[0], fc[1] - 1, fc[2]])
                .unwrap_or(crouch);
        }
        // Down in it: roofed over the head, a block on a side first for the roof to go against
        // (a fireball's cloud on an open notch is on the body).
        if fc[1] <= from[1] - 2 {
            let roof = [fc[0], from[1], fc[2]];
            if let Some(c) = self.notch_roof_step(w, tick, roof, crouch) {
                return c;
            }
        }
        crouch
    }

    /// The notch done with: its step out found (never under the roof's side block), the roof
    /// left for `notch_exit_step` to dig back.
    fn notch_leave(&mut self, w: &World) {
        let Some(body) = w.body else {
            return;
        };
        let side = self.dragon.notch_side.take();
        let from = self.dragon.notch_from.take();
        self.dragon.notch_step = from
            .filter(|f| cell_of(body.pos)[1] <= f[1] - 2)
            .and_then(|f| notch_step_except(w, f, side));
        // With no step to dig (the bunker's floor, walls all round: race 45918669138 lane 1 stood
        // 2 under the bunker 3,000 ticks, "no walk to the bunker" every tick), it rises out by a
        // block put under the feet at each jump.
        self.dragon.notch_rise_to = from
            .filter(|f| cell_of(body.pos)[1] < f[1] && self.dragon.notch_step.is_none())
            .map(|f| f[1]);
    }

    /// The way out of a notch, a tick at a time: the roof dug back (the jump needs the head
    /// clear), then the step beside it; `None` once done.
    fn notch_exit_step(&mut self, w: &World, tick: u64) -> Option<Controls> {
        if let Some(r) = self.dragon.notch_roof
            && w.block_at(world::BlockPos::new(r[0], r[1], r[2]))
                .is_some_and(|k| k != BlockKind::Air)
        {
            return self.charge_dig_step(w, tick, r);
        }
        self.dragon.notch_roof = None;
        if let Some(top) = self.dragon.notch_rise_to {
            let body = w.body?;
            let fc = cell_of(body.pos);
            if fc[1] >= top {
                self.dragon.notch_rise_to = None;
                self.note(
                    tick,
                    format!("notch: risen out at {fc:?}"),
                    "on with the task".to_owned(),
                );
                return None;
            }
            return Some(self.rise_step(w, tick, body));
        }
        if let Some(s) = self.dragon.notch_step {
            if w.block_at(world::BlockPos::new(s[0], s[1], s[2])) == Some(BlockKind::Air) {
                self.dragon.notch_step = None;
                self.dragon.charge_dig = None;
                self.note(
                    tick,
                    format!("notch: the step out dug at {s:?}"),
                    "on with the task".to_owned(),
                );
                return None;
            }
            return self.charge_dig_step(w, tick, s);
        }
        None
    }

    /// One tick of a rise by a block under the feet: the block held, a jump from the ground, and
    /// once the feet are `RISE_CLEAR` over the cell they left, the block clicked onto the top of
    /// the one under it.
    fn rise_step(&mut self, w: &World, tick: u64, body: physics::Body) -> Controls {
        let down = Controls {
            yaw: f64::from(w.player.yaw),
            pitch: Some(90.0),
            ..Controls::default()
        };
        let Some(item) = crate::spikes::pillar_item(w) else {
            return down;
        };
        match hold::toward(w, item) {
            Hold::Held => {}
            Hold::Act(a) => {
                if self.acts.is_empty() {
                    self.acts.push(a);
                }
                return down;
            }
            Hold::Wait | Hold::Missing => return down,
        }
        let left = self.dragon.rise_from.unwrap_or_else(|| cell_of(body.pos));
        if body.on_ground {
            self.dragon.rise_from = Some(cell_of(body.pos));
            return Controls { jump: true, ..down };
        }
        if body.pos[1] >= f64::from(left[1]) + RISE_CLEAR
            && w.block_at(world::BlockPos::new(left[0], left[1], left[2])) == Some(BlockKind::Air)
            && self.acts.is_empty()
            && w.click.is_none()
            && tick >= self.dragon.roof_click_at + ROOF_CLICK_TICKS
        {
            self.dragon.roof_click_at = tick;
            self.acts.push(Act::UseOn {
                pos: world::BlockPos::new(left[0], left[1] - 1, left[2]),
                face: 1,
            });
        }
        down
    }

    /// The notch's roof over the head at `roof`: a block on a side of it first when nothing holds
    /// it (the side's own block on the wall's top), then the roof against it. `None` once roofed,
    /// or with no block held or nothing to click; a click every `ROOF_CLICK_TICKS`, at most
    /// `ROOF_TRIES` a cell.
    fn notch_roof_step(
        &mut self,
        w: &World,
        tick: u64,
        roof: [i32; 3],
        crouch: Controls,
    ) -> Option<Controls> {
        let air = |c: [i32; 3]| {
            w.block_at(world::BlockPos::new(c[0], c[1], c[2])) == Some(BlockKind::Air)
        };
        if !air(roof) {
            if self.dragon.notch_roof != Some(roof) {
                self.dragon.notch_roof = Some(roof);
                self.note(
                    tick,
                    format!("charge: the notch roofed at {roof:?}"),
                    format!("{} clicks", self.dragon.roof_tries),
                );
            }
            return None;
        }
        let item = crate::spikes::pillar_item(w)?;
        let (x, y, z) = (roof[0], roof[1], roof[2]);
        let target = if crate::spikes::support(w, (x, y, z)).is_some() {
            roof
        } else {
            let side = [(1, 0), (-1, 0), (0, 1), (0, -1)]
                .iter()
                .map(|(dx, dz)| [x + dx, y, z + dz])
                .find(|c| air(*c) && crate::spikes::support(w, (c[0], c[1], c[2])).is_some())?;
            self.dragon.notch_side = Some(side);
            side
        };
        if self.dragon.roof_tries >= ROOF_TRIES {
            return None;
        }
        let (n, face) = crate::spikes::support(w, (target[0], target[1], target[2]))?;
        match hold::toward(w, item) {
            Hold::Held => {
                if self.acts.is_empty()
                    && w.click.is_none()
                    && (self.dragon.roof_tries == 0
                        || tick >= self.dragon.roof_click_at + ROOF_CLICK_TICKS)
                {
                    self.dragon.roof_tries += 1;
                    self.dragon.roof_click_at = tick;
                    self.acts.push(Act::UseOn {
                        pos: world::BlockPos::new(n.0, n.1, n.2),
                        face,
                    });
                }
            }
            Hold::Act(a) => {
                if self.acts.is_empty() {
                    self.acts.push(a);
                }
            }
            Hold::Wait | Hold::Missing => {}
        }
        Some(crouch)
    }

    /// Is the bunker shut: its roof and the first cell of its door solid (E3's own test of it,
    /// `end::bunker_cells`)? False with no bunker or no door known.
    pub fn bunker_shut(&self, w: &World) -> bool {
        let (Some(b), Some(door)) = (self.bunker, self.end.door) else {
            return false;
        };
        let (_, roof, cells) = crate::end::bunker_cells((b[0], b[1], b[2]), door);
        let solid = |c: Cell| {
            w.block_at(world::BlockPos::new(c.0, c.1, c.2))
                .is_some_and(|k| !matches!(k, BlockKind::Air | BlockKind::Water))
        };
        solid(roof) && solid(cells[0])
    }

    /// One tick of a dig for the notch (or its step out), crouched, looking down.
    fn charge_dig_step(&mut self, w: &World, tick: u64, c: [i32; 3]) -> Option<Controls> {
        let cell: Cell = (c[0], c[1], c[2]);
        if self
            .dragon
            .charge_dig
            .as_ref()
            .is_none_or(|d| d.target != cell)
        {
            self.dragon.charge_dig = Some(crate::dig::Dig::new(w, cell, tick));
        }
        let d = self.dragon.charge_dig.as_mut()?;
        let (mut ctl, act, status) = d.step(w, tick);
        self.acts.extend(act);
        if !matches!(status, crate::dig::DigStatus::Running)
            || w.block_at(world::BlockPos::new(c[0], c[1], c[2])) == Some(BlockKind::Air)
        {
            if let crate::dig::DigStatus::Failed(e) = status {
                self.note(
                    tick,
                    format!("charge: dig at {c:?} failed: {e:?}"),
                    "crouched where the feet are".to_owned(),
                );
                self.dragon.notch_step = None;
                // Never the same failed dig again each tick (race 45913263506 lane 2: a wall cell
                // in water, "failed: Gone" 289 times while a fireball's cloud burned it).
                self.dragon.breath_wall = None;
                self.dragon.notch_failed = true;
            }
            self.dragon.charge_dig = None;
        }
        ctl.sneak = true;
        ctl.forward = false;
        if ctl.pitch.is_none() {
            ctl.pitch = Some(90.0);
        }
        Some(ctl)
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
        // Clouds only, and never the dodge's walk (wall 28).
        if self.dragon.dodge_walk.is_some() {
            return c;
        }
        let clouds = clouds_only(w);
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

    /// In the End, an angry enderman answered with no pocket ever sealed. An enderman with a
    /// target keeps its anger for good (its timer restarts every tick, brain/game/mc-mob-facts.md),
    /// so every hold is bounded (E4's race 45942992471 lane 0: 12,000 ticks covered from one):
    /// - beyond `ENDER_NEAR` (8) the task goes on: it teleports to the bot only from over 16 and
    ///   cannot come into a 2-high cell;
    /// - within 8, in a roofed 2-high cell (the bunker, a notch, a tunnel): struck from there when
    ///   in reach over the danger line, else held there;
    /// - within 8 on open ground: into the bunker within `ENDER_BUNKER_REACH` (a failed walk is not
    ///   tried again for it), else down a notch two deep, never a fight in the open;
    /// - every hold from one enderman is capped at `ENDER_HOLD_CAP` (600 ticks), then the choice is
    ///   made again: struck from cover in reach over the line, else the task goes on.
    ///
    /// Noted "end: no pocket for enderman N (…)" at most every `FIRST_NOTE_TICKS` for each case.
    pub fn end_enderman_step(&mut self, w: &World, tick: u64) -> Option<Controls> {
        let Some(id) = crate::endermen::angry(w) else {
            self.dragon.ender_walk = None;
            self.dragon.ender_hold = None;
            // Down a notch from an enderman: out once it is calm, by the roof and the step.
            if self.dragon.ender_notch {
                self.dragon.ender_notch = false;
                self.note(
                    tick,
                    "end: out of the enderman's notch".to_owned(),
                    "it is calm or gone".to_owned(),
                );
                self.notch_leave(w);
            }
            return self.notch_exit_step(w, tick);
        };
        let body = w.body?;
        // E3's and E4's own answers first, as inside `ender_threat`: covered (the bunker, a
        // tunnel), and up a pillar or a spike.
        let covered = self.end_covered(w, tick);
        if self.spikes_up_high(w) {
            self.dragon.ender_walk = None;
            self.spikes_note(
                tick,
                "spikes: up high, no enderman pocket".to_owned(),
                format!("enderman {id} provoked; the crystals' work goes on"),
            );
            return None;
        }
        let e = w.entities.get(&id)?;
        let off = dist3(e.pos, body.pos);
        let eye = [body.pos[0], body.pos[1] + physics::EYE_HEIGHT, body.pos[2]];
        let in_reach =
            crate::defend::box_distance_of(eye, e.pos, EntityKind::Enderman) <= STRIKE_REACH;
        let danger = self
            .table
            .as_ref()
            .map_or(8.0, |t| t.survival.danger_health);
        let health = w.player.health.unwrap_or(FULL_HEALTH);
        let above = health > danger;
        // The hold's clock, one for each enderman.
        let since = match self.dragon.ender_hold {
            Some((i, t)) if i == id => t,
            _ => {
                self.dragon.ender_hold = Some((id, tick));
                self.dragon.ender_capped = false;
                tick
            }
        };
        if !self.dragon.ender_capped && tick >= since + ENDER_HOLD_CAP {
            self.dragon.ender_capped = true;
            self.note(
                tick,
                format!("end: the hold from enderman {id} capped"),
                format!(
                    "{} ticks; {off:.1} off; choose again: struck from cover in reach over the danger line, else on with the task",
                    tick - since
                ),
            );
        }
        let capped = self.dragon.ender_capped;
        // Struck from the covered cell's edge in reach, over the line (40 health, the sword's 6).
        if covered && in_reach && above {
            self.dragon.ender_walk = None;
            return Some(self.ender_strike(w, tick, id));
        }
        if off > ENDER_NEAR || capped {
            self.dragon.ender_walk = None;
            if self.dragon.ender_notch {
                self.dragon.ender_notch = false;
                self.note(
                    tick,
                    "end: out of the enderman's notch".to_owned(),
                    format!(
                        "enderman {id} {off:.1} off{}",
                        if capped { ", the hold capped" } else { "" }
                    ),
                );
                self.notch_leave(w);
            }
            self.ender_note(
                tick,
                id,
                if capped {
                    "the hold capped: on with the task"
                } else {
                    "beyond 8: on with the task"
                },
            );
            return self.notch_exit_step(w, tick);
        }
        let crouch = Controls {
            sneak: true,
            yaw: f64::from(w.player.yaw),
            pitch: Some(90.0),
            ..Controls::default()
        };
        if covered {
            self.dragon.ender_walk = None;
            self.ender_note(tick, id, "covered within 8: held");
            return Some(crouch);
        }
        let bunker = self.bunker.filter(|b| {
            self.dragon.ender_walk_failed != Some(id)
                && (!above || flat(body.pos, mid(*b)) <= ENDER_BUNKER_REACH)
        });
        if let Some(b) = bunker {
            self.ender_note(tick, id, "to the bunker");
            let cell: Cell = (b[0], b[1], b[2]);
            if self
                .dragon
                .ender_walk
                .as_ref()
                .is_none_or(|wk| wk.target != cell)
            {
                self.dragon.ender_walk = Some(Walk::new(
                    cell,
                    Goal::Block { pos: cell },
                    Arrive::Goal,
                    tick,
                ));
            }
            let wk = self.dragon.ender_walk.as_mut()?;
            let (mut c, status) = wk.step(w, tick);
            let (acts, said) = wk.take_out();
            self.acts.extend(acts);
            for (what, why) in said {
                self.note(tick, what, why);
            }
            if !matches!(status, TaskStatus::Running) {
                self.dragon.ender_walk = None;
                if let TaskStatus::Failed(e) = status {
                    // Never the same failed walk again for it (E4's race 45942992471 lane 0: the
                    // walk refused 3,071 times).
                    self.dragon.ender_walk_failed = Some(id);
                    self.note(
                        tick,
                        format!("end: the walk from enderman {id} to the bunker failed: {e:?}"),
                        "a notch where the feet are instead".to_owned(),
                    );
                    return Some(crouch);
                }
            }
            if c.pitch.is_none() {
                c.pitch = Some(90.0);
            }
            return Some(c);
        }
        // No bunker near: down a notch two deep where the feet are, roofed, which it can neither
        // enter nor see into (race 45913296602 lane 1: fought in the open, 7 a blow, dead at tick
        // 260 with the enderman at 13.6). A fight in the open is lost: 40 health against 6 a
        // strike, 7 a blow every second.
        if body.on_ground || self.dragon.ender_notch {
            if !self.dragon.ender_notch {
                self.dragon.ender_notch = true;
                self.dragon.notch_from = None;
            }
            self.ender_note(tick, id, "a notch down, no bunker near");
            return Some(self.notch_dig_step(w, tick, crouch, "enderman"));
        }
        self.ender_note(tick, id, "in the air, no bunker near: on with the task");
        None
    }

    /// One tick of a strike at enderman `id`: the weapon in hand, a swing when it is ready.
    fn ender_strike(&mut self, w: &World, tick: u64, id: i32) -> Controls {
        if let Some(wp) = crate::hunt::weapon(w) {
            match hold::toward(w, wp) {
                Hold::Held | Hold::Missing => {}
                Hold::Act(a) => {
                    self.acts.push(a);
                    return looking_down(w);
                }
                Hold::Wait => return looking_down(w),
            }
        }
        let ready = (crate::hunt::swing_ticks(w) * crate::dig::slowdown(w)).ceil() as u64 + 1;
        if self
            .dragon
            .ender_swing_at
            .is_none_or(|t| tick.saturating_sub(t) >= ready)
        {
            self.dragon.ender_swing_at = Some(tick);
            self.acts.push(Act::Attack { entity: id });
            self.acts.push(Act::Swing);
            self.ender_note(tick, id, "struck from cover over the danger line");
        }
        looking_down(w)
    }

    fn ender_note(&mut self, tick: u64, id: i32, how: &str) {
        if self.dragon.ender_noted.as_deref() == Some(how)
            && tick < self.dragon.ender_noted_at + FIRST_NOTE_TICKS
        {
            return;
        }
        self.dragon.ender_noted = Some(how.to_owned());
        self.dragon.ender_noted_at = tick;
        self.note(
            tick,
            format!("end: no pocket for enderman {id} ({how})"),
            format!("the bunker {:?}", self.bunker),
        );
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
