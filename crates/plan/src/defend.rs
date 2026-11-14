//! Defence (issue 12d, brain/ruststeve/design/combat-and-pacing.md): the `defend` always-row, a
//! port of the core of clojurecraft's combat.clj. A hostile mob within `defend_radius`: face it,
//! hold the hardest-hitting weapon, strike when a full charge is ready, the look sent a tick before
//! the swing. Never fight under the danger line (the next two hits and a margin) and never fight a
//! creeper: back away from it instead, straight away, jumping what is in the way.

use data::EntityKind;
use physics::Controls;
use world::{Act, World};

use crate::hold::{self, Hold};
use crate::make::SurvivalRules;
use crate::walk::yaw_to;

/// Eye to the mob's middle within which it is struck (the client's 3-block entity reach).
pub const STRIKE_REACH: f64 = 3.0;
/// Blocks past the defence radius at which a fight or a retreat is over.
pub const CLEAR_MARGIN: f64 = 4.0;
/// The most a fight takes before it gives up and backs away (thirty seconds).
pub const FIGHT_TICKS: u64 = 600;
/// A fight that lands no hit for this long backs away instead (the progress invariant).
pub const NO_HIT_TICKS: u64 = 200;

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Stance {
    Fight,
    /// Backing away: under the danger line, from a creeper, or a fight that went nowhere.
    Retreat,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DefendStatus {
    Running,
    /// The mob is dead or far enough.
    Done,
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Defend {
    pub foe: i32,
    pub kind: EntityKind,
    pub stance: Stance,
    pub last_seen: [f64; 3],
    /// The tick the bot first faced it this approach (the look goes a tick before the swing).
    pub faced_at: Option<u64>,
    pub last_swing: Option<u64>,
    pub swings: u32,
    /// The mob's health when last struck, and the tick a strike last took some off.
    pub last_health: Option<f32>,
    pub last_hit_at: u64,
    pub started: u64,
}

/// Is this kind of mob hostile (the game's `monster` category)?
pub fn hostile(kind: EntityKind) -> bool {
    data::generated::mobs::MOBS
        .iter()
        .any(|m| m.entity == kind && m.category == data::generated::mobs::MobCategory::Monster)
}

fn middle(p: [f64; 3]) -> [f64; 3] {
    [p[0], p[1] + 0.9, p[2]]
}

fn distance(a: [f64; 3], b: [f64; 3]) -> f64 {
    (0..3).map(|i| (a[i] - b[i]).powi(2)).sum::<f64>().sqrt()
}

/// The nearest live hostile within `radius` of the feet.
pub fn nearest_foe(w: &World, radius: f64) -> Option<(i32, EntityKind)> {
    let body = w.body?;
    w.entities
        .iter()
        .filter(|(_, e)| !e.dead && hostile(e.kind))
        .map(|(id, e)| (distance(body.pos, e.pos), *id, e.kind))
        .filter(|(d, _, _)| *d <= radius)
        .min_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)))
        .map(|(_, id, k)| (id, k))
}

/// The foe to answer: the nearest hostile within `defend_radius`, else the nearest mob that
/// shoots within `ranged_radius`.
pub fn foe(w: &World, rules: &SurvivalRules) -> Option<(i32, EntityKind)> {
    if let Some(f) = nearest_foe(w, rules.defend_radius) {
        return Some(f);
    }
    let body = w.body?;
    w.entities
        .iter()
        .filter(|(_, e)| !e.dead && rules.ranged.contains(&e.kind))
        .map(|(id, e)| (distance(body.pos, e.pos), *id, e.kind))
        .filter(|(d, _, _)| *d <= rules.ranged_radius)
        .min_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)))
        .map(|(_, id, k)| (id, k))
}

/// Under the danger line: health the next two hits and a margin would take (data).
pub fn in_danger(w: &World, rules: &SurvivalRules) -> bool {
    w.player.health.is_some_and(|h| h <= rules.danger_health)
}

fn mob_health(w: &World, id: i32) -> Option<f32> {
    w.entities.get(&id).and_then(world::Entity::health)
}

impl Defend {
    pub fn new(w: &World, foe: i32, kind: EntityKind, rules: &SurvivalRules, tick: u64) -> Self {
        let stance = if rules.never_fight.contains(&kind) || in_danger(w, rules) {
            Stance::Retreat
        } else {
            Stance::Fight
        };
        Self {
            foe,
            kind,
            stance,
            last_seen: w.entities.get(&foe).map_or([0.0; 3], |e| e.pos),
            faced_at: None,
            last_swing: None,
            swings: 0,
            last_health: None,
            last_hit_at: tick,
            started: tick,
        }
    }

    pub fn step(
        &mut self,
        w: &World,
        rules: &SurvivalRules,
        tick: u64,
    ) -> (Controls, Option<Act>, DefendStatus) {
        let still = Controls::default();
        let Some(body) = w.body else {
            return (still, None, DefendStatus::Done);
        };
        let Some(pos) = w.entities.get(&self.foe).filter(|e| !e.dead).map(|e| e.pos) else {
            return (still, None, DefendStatus::Done);
        };
        self.last_seen = pos;
        let d = distance(body.pos, pos);
        let reach = if rules.ranged.contains(&self.kind) {
            rules.ranged_radius
        } else {
            rules.defend_radius
        };
        if d > reach + CLEAR_MARGIN {
            return (still, None, DefendStatus::Done);
        }
        // The progress invariants: the danger line, a fight that lands nothing, a long fight.
        if self.stance == Stance::Fight {
            let health = mob_health(w, self.foe);
            if health
                .zip(self.last_health)
                .is_some_and(|(now, was)| now < was)
            {
                self.last_hit_at = tick;
            }
            self.last_health = health.or(self.last_health);
            if in_danger(w, rules)
                || tick.saturating_sub(self.last_hit_at) > NO_HIT_TICKS
                || tick.saturating_sub(self.started) > FIGHT_TICKS
            {
                self.stance = Stance::Retreat;
            }
        }
        let target = middle(pos);
        let toward = yaw_to(body.pos, target);
        match self.stance {
            Stance::Retreat => {
                // Straight away from it, jumping what is in the way.
                let away = (toward + 180.0) % 360.0;
                (
                    Controls {
                        forward: true,
                        jump: body.horizontal_collision || body.in_water,
                        sneak: false,
                        yaw: away,
                    },
                    None,
                    DefendStatus::Running,
                )
            }
            Stance::Fight => {
                let eye = [body.pos[0], body.pos[1] + physics::EYE_HEIGHT, body.pos[2]];
                let face = Controls {
                    yaw: toward,
                    ..still
                };
                if distance(eye, target) > STRIKE_REACH {
                    // Close in, facing it.
                    self.faced_at = None;
                    return (
                        Controls {
                            forward: true,
                            jump: body.horizontal_collision,
                            sneak: false,
                            yaw: toward,
                        },
                        None,
                        DefendStatus::Running,
                    );
                }
                if let Some(wp) = crate::hunt::weapon(w) {
                    match hold::toward(w, wp) {
                        Hold::Held | Hold::Missing => {}
                        Hold::Act(a) => return (face, Some(a), DefendStatus::Running),
                        Hold::Wait => return (face, None, DefendStatus::Running),
                    }
                }
                let faced = *self.faced_at.get_or_insert(tick);
                let ready =
                    (crate::hunt::swing_ticks(w) * crate::dig::slowdown(w)).ceil() as u64 + 1;
                if tick > faced
                    && self
                        .last_swing
                        .is_none_or(|t| tick.saturating_sub(t) >= ready)
                {
                    self.last_swing = Some(tick);
                    self.swings += 1;
                    return (
                        face,
                        Some(Act::Attack { entity: self.foe }),
                        DefendStatus::Running,
                    );
                }
                (face, None, DefendStatus::Running)
            }
        }
    }
}
