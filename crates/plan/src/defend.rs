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
/// A flee route is searched again when the foe has moved this far from where it was planned, or
/// this long after a search found none.
pub const FLEE_REPLAN_BLOCKS: i32 = 3;
pub const FLEE_RETRY_TICKS: u64 = 20;

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
    /// A retreat's route away, and the foe's cell it was planned from (wall 28).
    #[serde(default)]
    pub flee: Option<(crate::walk::Walk, crate::path::Cell)>,
    /// The tick the last flee route was searched (no route: straight away, a while).
    #[serde(default)]
    pub fled_at: Option<u64>,
    /// Steps not taken: a fight's close-in or a retreat's straight-away step held because lava
    /// was seen ahead or under the cell ahead, or the drop there was too deep (wall 29).
    #[serde(default)]
    pub held_back: u32,
}

/// A drop this deep or deeper ahead holds a step: three blocks of fall cost no health.
const DROP_HELD: i32 = 4;

/// Whether a step forward along `yaw` from `body` is safe: no lava seen in the cell ahead at the
/// feet or the head, or in the cells under it down to `DROP_HELD`, and a seen floor within
/// `DROP_HELD`. Strict no X-ray: an unseen cell counts as rock (a floor, no lava). The raw
/// forward steps of a fight's close-in and a retreat with no route were the only moves that did
/// not ask: 15 of 19 lava and fire deaths in races 44052153072 and 44052157794 came within 300
/// ticks of a defend.
fn safe_ahead(w: &World, body: &physics::Body, yaw: f64) -> bool {
    let rad = yaw.to_radians();
    let ahead = [
        body.pos[0] - rad.sin() * 0.9,
        body.pos[1],
        body.pos[2] + rad.cos() * 0.9,
    ];
    let cell = crate::path::feet_cell(ahead);
    let lava = |c: crate::path::Cell| crate::cast::seen(w, c) == Some(data::BlockKind::Lava);
    if lava(cell) || lava((cell.0, cell.1 + 1, cell.2)) {
        return false;
    }
    for down in 1..=DROP_HELD {
        let c = (cell.0, cell.1 - down, cell.2);
        if lava(c) {
            return false;
        }
        let floor = crate::cast::seen(w, c).is_none_or(|k| {
            data::generated::shapes::collision(k) != Some(data::types::Collision::Empty)
        });
        if floor {
            return true;
        }
    }
    false
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
/// `ignored`: ranged foes a fight landed nothing on lately (out of reach behind rock), left alone
/// unless they come within `defend_radius` (wall 30).
pub fn foe(w: &World, rules: &SurvivalRules, ignored: &[i32]) -> Option<(i32, EntityKind)> {
    let body = w.body?;
    // A kind never fought (a creeper) first, from farther out: backed away from before its fuse.
    let flee = w
        .entities
        .iter()
        .filter(|(_, e)| !e.dead && rules.never_fight.contains(&e.kind))
        .map(|(id, e)| (distance(body.pos, e.pos), *id, e.kind))
        .filter(|(d, _, _)| *d <= rules.flee_radius)
        .min_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)))
        .map(|(_, id, k)| (id, k));
    if flee.is_some() {
        return flee;
    }
    // A neutral kind only once one of its kind has hit the bot: striking a zombified piglin
    // angers its group, and 11 of 112 bots of the first nether race died to piglins after
    // "defend ZombifiedPiglin" (docs/problems/fortress.md wall 2).
    let near = w
        .entities
        .iter()
        .filter(|(_, e)| {
            !e.dead && hostile(e.kind) && (!rules.neutral.contains(&e.kind) || provoked(w, e.kind))
        })
        .map(|(id, e)| (distance(body.pos, e.pos), *id, e.kind))
        .filter(|(d, _, _)| *d <= rules.defend_radius)
        .min_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)))
        .map(|(_, id, k)| (id, k));
    if near.is_some() {
        return near;
    }
    w.entities
        .iter()
        .filter(|(id, e)| !e.dead && rules.ranged.contains(&e.kind) && !ignored.contains(id))
        .map(|(id, e)| (distance(body.pos, e.pos), *id, e.kind))
        .filter(|(d, _, _)| *d <= rules.ranged_radius)
        .min_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)))
        .map(|(_, id, k)| (id, k))
}

/// Server ticks a hit from a neutral kind keeps it a foe: vanilla's anger lasts 20 to 39 seconds
/// (`ZombifiedPiglin`'s persistent anger); the shortest, 400 ticks.
pub const PROVOKED_TICKS: u64 = 400;

/// Whether one of `kind` has hit the bot within `PROVOKED_TICKS`.
pub fn provoked(w: &World, kind: EntityKind) -> bool {
    let now = w.session.server_ticks;
    w.hits
        .iter()
        .any(|h| h.cause == Some(kind) && h.at_tick + PROVOKED_TICKS >= now)
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
            flee: None,
            fled_at: None,
            held_back: 0,
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
        let never = rules.never_fight.contains(&self.kind);
        let reach = if never {
            rules.flee_clear - CLEAR_MARGIN
        } else if rules.ranged.contains(&self.kind) {
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
                // Along a route to beyond the reach, the way the tunnels go: straight away ran
                // bots into a tunnel's end with the creeper behind (wall 28).
                let foe_cell = crate::path::feet_cell(pos);
                let moved = self.flee.as_ref().is_some_and(|(_, c)| {
                    (c.0 - foe_cell.0)
                        .abs()
                        .max((c.1 - foe_cell.1).abs())
                        .max((c.2 - foe_cell.2).abs())
                        >= FLEE_REPLAN_BLOCKS
                });
                if moved {
                    self.flee = None;
                }
                if self.flee.is_none()
                    && self
                        .fled_at
                        .is_none_or(|t| tick.saturating_sub(t) >= FLEE_RETRY_TICKS)
                {
                    self.fled_at = Some(tick);
                    let mut walk = crate::walk::Walk::new(
                        foe_cell,
                        crate::path::Goal::Away {
                            pos: foe_cell,
                            range: reach + CLEAR_MARGIN + 1.0,
                        },
                        crate::walk::Arrive::Goal,
                        tick,
                    );
                    if walk.route_now(w, tick) {
                        self.flee = Some((walk, foe_cell));
                    }
                }
                if let Some((walk, _)) = &mut self.flee {
                    let (c, status) = walk.step(w, tick);
                    if status == crate::walk::TaskStatus::Running {
                        return (c, None, DefendStatus::Running);
                    }
                    self.flee = None;
                }
                // No route: straight away from it, jumping what is in the way, unless lava or a
                // deep drop lies that way: then stand (wall 29).
                let away = (toward + 180.0) % 360.0;
                if !safe_ahead(w, &body, away) {
                    self.held_back += 1;
                    return (Controls { yaw: away, ..still }, None, DefendStatus::Running);
                }
                (
                    Controls {
                        forward: true,
                        jump: body.horizontal_collision || body.in_water,
                        sneak: false,
                        yaw: away,
                        pitch: None,
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
                    // Close in, facing it, unless lava or a deep drop lies between: then wait for
                    // it facing it (wall 29).
                    self.faced_at = None;
                    if !safe_ahead(w, &body, toward) {
                        self.held_back += 1;
                        return (face, None, DefendStatus::Running);
                    }
                    return (
                        Controls {
                            forward: true,
                            jump: body.horizontal_collision,
                            sneak: false,
                            yaw: toward,
                            pitch: None,
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
