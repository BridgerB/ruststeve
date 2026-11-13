//! Hunting (issue 12c, brain/ruststeve/design/food-and-hunting.md): the `hunt` always-row, a port
//! of the core of clojurecraft's forage.clj. Hungry with nothing edible held: the nearest animal
//! in view within the hunt radius whose loot is safe food and that a route reaches; walk beside
//! it, hold the best weapon, strike paced by the weapon's cooldown (a swing before it is charged
//! does a fifth of the damage), then pick up what it dropped. The meat is eaten raw by `eat`.

use data::{EntityKind, ItemKind};
use physics::Controls;
use world::{Act, World};

use crate::hold::{self, Hold};
use crate::make::SurvivalRules;
use crate::path::{Cell, Goal, feet_cell};
use crate::walk::{Arrive, Failure, TaskStatus, Walk, yaw_to};

/// Eye to the animal's middle within which it is struck (the client's 3-block entity reach).
pub const STRIKE_REACH: f64 = 3.0;
/// The animal moved this far from where the walk aims: aim again.
pub const RE_AIM: f64 = 2.0;
/// A walk ends this near the animal (octile on the plane plus height).
pub const APPROACH_RANGE: f64 = 2.0;
/// The most a hunt takes, from its start (a minute).
pub const HUNT_TICKS: u64 = 1200;
/// After the kill, how long its drops are waited for and picked up.
pub const COLLECT_TICKS: u64 = 300;
/// Drops are looked for this near where the animal died.
pub const DROP_RANGE: f64 = 4.0;
/// Animals asked for a route each choice (clojurecraft's `candidates`).
pub const CANDIDATES: usize = 3;

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum HuntFailure {
    /// No route to the animal.
    NoPath,
    /// The walk stopped getting closer.
    NoProgress,
    /// The animal is gone, not by the bot's hand.
    Lost,
    /// Over `HUNT_TICKS`.
    Slow,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HuntStatus {
    Running,
    Done,
    Failed(HuntFailure),
}

#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum HuntStage {
    Approach,
    Strike,
    /// The kill made: pick up its drops near `at`.
    Collect {
        at: [f64; 3],
        since: u64,
    },
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Hunt {
    pub prey: i32,
    pub kind: EntityKind,
    pub stage: HuntStage,
    pub walk: Option<Walk>,
    /// The cell the walk aims at.
    pub aimed: Option<Cell>,
    pub last_seen: [f64; 3],
    pub last_swing: Option<u64>,
    pub swings: u32,
    pub kills_before: u64,
    pub started: u64,
}

/// The safe foods a kind of animal drops.
fn meat(kind: EntityKind, rules: &SurvivalRules) -> Vec<ItemKind> {
    data::generated::loot::mob_drops_of(kind)
        .iter()
        .map(|d| d.item)
        .filter(|i| {
            data::generated::item_props::food_of(*i).is_some() && !rules.unsafe_foods.contains(i)
        })
        .collect()
}

/// Is this kind of mob game: a creature (passive) whose loot is safe food?
pub fn game(kind: EntityKind, rules: &SurvivalRules) -> bool {
    data::generated::mobs::MOBS
        .iter()
        .any(|m| m.entity == kind && m.category == data::generated::mobs::MobCategory::Creature)
        && !meat(kind, rules).is_empty()
}

fn middle(p: [f64; 3]) -> [f64; 3] {
    [p[0], p[1] + 0.5, p[2]]
}

fn distance(a: [f64; 3], b: [f64; 3]) -> f64 {
    (0..3).map(|i| (a[i] - b[i]).powi(2)).sum::<f64>().sqrt()
}

/// The animals in view, nearest first: game, alive, not refused, within `radius` of the feet.
pub fn prey(
    w: &World,
    rules: &SurvivalRules,
    radius: f64,
    refused: &[i32],
) -> Vec<(i32, EntityKind)> {
    let Some(body) = w.body else {
        return Vec::new();
    };
    let mut v: Vec<(f64, i32, EntityKind)> = w
        .entities
        .iter()
        .filter(|(id, e)| !e.dead && !refused.contains(id) && game(e.kind, rules))
        .map(|(id, e)| (distance(body.pos, e.pos), *id, e.kind))
        .filter(|(d, _, _)| *d <= radius)
        .collect();
    v.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
    v.into_iter().map(|(_, id, k)| (id, k)).collect()
}

/// The held item that hits hardest, if any hits harder than the hand.
pub fn weapon(w: &World) -> Option<ItemKind> {
    crate::held(w)
        .into_iter()
        .filter(|(_, n)| *n > 0)
        .filter_map(|(k, _)| data::generated::item_props::weapon_of(k).map(|wp| (k, wp.damage)))
        .filter(|(_, d)| *d > 0.0)
        .max_by(|a, b| a.1.total_cmp(&b.1).then(b.0.id().cmp(&a.0.id())))
        .map(|(k, _)| k)
}

/// Ticks between full swings with what is held: 20 / (4 + the attack speed modifier).
pub fn swing_ticks(w: &World) -> f64 {
    let held = crate::dig::held_item(w);
    let speed = 4.0
        + held
            .and_then(data::generated::item_props::weapon_of)
            .map_or(0.0, |wp| wp.speed);
    20.0 / speed.max(0.1)
}

fn approach(target: Cell, tick: u64) -> Walk {
    Walk::new(
        target,
        Goal::Near {
            pos: target,
            range: APPROACH_RANGE,
        },
        Arrive::Goal,
        tick,
    )
}

impl Hunt {
    pub fn new(w: &World, prey: i32, kind: EntityKind, tick: u64) -> Self {
        let last_seen = w.entities.get(&prey).map_or([0.0; 3], |e| e.pos);
        Self {
            prey,
            kind,
            stage: HuntStage::Approach,
            walk: None,
            aimed: None,
            last_seen,
            last_swing: None,
            swings: 0,
            kills_before: w.kills,
            started: tick,
        }
    }

    /// Can a walk reach beside it now? (the churn invariant: no hunt starts without a way).
    pub fn reachable(&mut self, w: &World, tick: u64) -> bool {
        let cell = feet_cell(self.last_seen);
        let mut walk = approach(cell, tick);
        let ok = walk.route_now(w, tick);
        if ok {
            self.walk = Some(walk);
            self.aimed = Some(cell);
        }
        ok
    }

    pub fn step(
        &mut self,
        w: &World,
        rules: &SurvivalRules,
        tick: u64,
    ) -> (Controls, Option<Act>, HuntStatus) {
        let still = Controls::default();
        let Some(body) = w.body else {
            return (still, None, HuntStatus::Failed(HuntFailure::Lost));
        };
        if tick.saturating_sub(self.started) > HUNT_TICKS {
            return (still, None, HuntStatus::Failed(HuntFailure::Slow));
        }
        let alive = w
            .entities
            .get(&self.prey)
            .filter(|e| !e.dead)
            .map(|e| e.pos);
        if let Some(p) = alive {
            self.last_seen = p;
        }
        if !matches!(self.stage, HuntStage::Collect { .. }) && alive.is_none() {
            if w.kills > self.kills_before {
                self.stage = HuntStage::Collect {
                    at: self.last_seen,
                    since: tick,
                };
                self.walk = None;
            } else {
                return (still, None, HuntStatus::Failed(HuntFailure::Lost));
            }
        }
        let eye = [body.pos[0], body.pos[1] + physics::EYE_HEIGHT, body.pos[2]];
        match self.stage {
            HuntStage::Collect { at, since } => {
                let foods = meat(self.kind, rules);
                let drop = w
                    .entities
                    .values()
                    .filter(|e| e.item().is_some_and(|(k, _)| foods.contains(&k)))
                    .map(|e| e.pos)
                    .filter(|p| distance(*p, at) <= DROP_RANGE)
                    .min_by(|a, b| distance(*a, body.pos).total_cmp(&distance(*b, body.pos)));
                let waited = tick.saturating_sub(since);
                let Some(p) = drop else {
                    // Nothing (left) to pick up once the drops have had time to land.
                    return if waited > 20 {
                        (still, None, HuntStatus::Done)
                    } else {
                        (still, None, HuntStatus::Running)
                    };
                };
                if waited > COLLECT_TICKS {
                    return (still, None, HuntStatus::Done);
                }
                let cell = feet_cell(p);
                if self.aimed != Some(cell) || self.walk.is_none() {
                    let mut walk = Walk::new(
                        cell,
                        Goal::Near {
                            pos: cell,
                            range: 1.0,
                        },
                        Arrive::Goal,
                        tick,
                    );
                    if !walk.route_now(w, tick) {
                        return (still, None, HuntStatus::Done);
                    }
                    self.walk = Some(walk);
                    self.aimed = Some(cell);
                }
                let Some(walk) = self.walk.as_mut() else {
                    return (still, None, HuntStatus::Running);
                };
                let (c, status) = walk.step(w, tick);
                if !matches!(status, TaskStatus::Running) {
                    self.walk = None;
                }
                (c, None, HuntStatus::Running)
            }
            HuntStage::Approach | HuntStage::Strike => {
                let target = middle(self.last_seen);
                if distance(eye, target) <= STRIKE_REACH {
                    self.stage = HuntStage::Strike;
                    self.walk = None;
                    let face = Controls {
                        yaw: yaw_to(body.pos, target),
                        ..still
                    };
                    if let Some(wp) = weapon(w) {
                        match hold::toward(w, wp) {
                            Hold::Held | Hold::Missing => {}
                            Hold::Act(a) => return (face, Some(a), HuntStatus::Running),
                            Hold::Wait => return (face, None, HuntStatus::Running),
                        }
                    }
                    let ready = (swing_ticks(w) * crate::dig::slowdown(w)).ceil() as u64 + 1;
                    if self
                        .last_swing
                        .is_none_or(|t| tick.saturating_sub(t) >= ready)
                    {
                        self.last_swing = Some(tick);
                        self.swings += 1;
                        return (
                            face,
                            Some(Act::Attack { entity: self.prey }),
                            HuntStatus::Running,
                        );
                    }
                    return (face, None, HuntStatus::Running);
                }
                self.stage = HuntStage::Approach;
                let cell = feet_cell(self.last_seen);
                let moved = self.aimed.is_none_or(|a| {
                    let d = |x: i32, y: i32| f64::from(x - y);
                    (d(a.0, cell.0).powi(2) + d(a.2, cell.2).powi(2)).sqrt() > RE_AIM
                });
                if moved || self.walk.is_none() {
                    let mut walk = approach(cell, tick);
                    if !walk.route_now(w, tick) {
                        return (still, None, HuntStatus::Failed(HuntFailure::NoPath));
                    }
                    self.walk = Some(walk);
                    self.aimed = Some(cell);
                }
                let Some(walk) = self.walk.as_mut() else {
                    return (still, None, HuntStatus::Running);
                };
                let (c, status) = walk.step(w, tick);
                match status {
                    TaskStatus::Running => (c, None, HuntStatus::Running),
                    // Arrived beside it and still not in reach: aim again next tick.
                    TaskStatus::Done => {
                        self.walk = None;
                        (c, None, HuntStatus::Running)
                    }
                    TaskStatus::Failed(Failure::NoProgress) => {
                        (still, None, HuntStatus::Failed(HuntFailure::NoProgress))
                    }
                    TaskStatus::Failed(
                        Failure::NoPath | Failure::Deadline | Failure::TargetLost,
                    ) => (still, None, HuntStatus::Failed(HuntFailure::NoPath)),
                }
            }
        }
    }
}
