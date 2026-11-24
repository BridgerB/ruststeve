//! A shield against fireballs (issue 16, `docs/problems/shield.md`): what a player does that
//! the bot could not. A shield held in the off hand and raised (`use_item` with hand 1, held
//! until `release_use`) stops a blaze's small fireball whole: no damage and no fire, when the
//! fireball comes from the front half of the look (the shield's `blocks_attacks` component:
//! `horizontal_blocking_angle` 90 either side, `block_delay_seconds` 0.25, bypassed only by the
//! `bypasses_shield` damage types, which `fireball` is not in).
//!
//! Pure reads of the world (`threats`, `should_raise`, `facing`) and a small holder (`Shield`)
//! that times the raise, keeps it up while a threat lasts, lowers it, and says what became of
//! every fireball that ended beside the bot. While the shield is up the bot strikes nothing and
//! walks at a fifth of its speed, and only toward the threat (`controls`): a client using an
//! item is slowed so and cannot attack, and the bot is held to what a client can do.

use std::collections::BTreeMap;

use data::{EntityKind, ItemKind};
use world::{Act, World};

/// Window 0's off-hand slot.
pub const OFF_HAND: i16 = 45;
/// Blocks within which a charged blaze counts: it shoots from its follow range of 48
/// (`cover::COVER_RANGE`).
pub const BLAZE_RANGE: f64 = 48.0;
/// Blocks within which a fireball in flight counts. A small fireball starts slow (0.1 a tick
/// more each tick, drag 0.95): 24 blocks is about a second and a half of flight.
pub const FIREBALL_RANGE: f64 = 24.0;
/// Ticks after a blaze's charge flag is first seen before the shield goes up for it. Vanilla
/// sets the flag, waits 60 ticks, shoots at 60, 66 and 72 and clears it at 78
/// (`Blaze.BlazeAttackGoal`); the raise takes 5 ticks on the server and a tick or two on the
/// wire, so 45 leaves 8 spare. It was 15: with three blazes one was always "due", and the shield
/// stayed up 866 ticks on end with nothing struck (race 45343415088, lane 0). A flag seen late
/// (28 ticks before the fireballs at the shortest, `world::Entity::blaze_charged`) is met by the
/// fireball itself, in flight.
pub const RAISE_AFTER: u64 = 45;
/// With the shield up the bot may still walk the way its tick chose when that way is within
/// this of the threat (the shield stops 90 either side of the look), one tick in `SLOW_EVERY`.
pub const ADVANCE_ARC: f64 = 60.0;
/// A client using an item walks at a fifth of its speed (the client's `LocalPlayer.aiStep`
/// scales both impulses by 0.2 and stops a sprint; the server does not check it). The body's
/// ground speed is linear in its input, so forward one tick in five averages the same.
pub const SLOW_EVERY: u64 = 5;
/// Uses left under this share of a new shield's (336) are said in the ledger: time for the spare.
pub const WORN_SHARE: f64 = 0.2;
/// Ticks the shield stays up after the last threat was seen: a fireball between two packets.
pub const HOLD_TICKS: u64 = 6;
/// While it is up, `use_item` goes again this often: the server ignores it when the use still
/// runs (`startUsingItem` does nothing while using) and takes it when the use was ended for the
/// bot (a hit that staggered it, a new dimension).
pub const RESEND_TICKS: u64 = 20;
/// A fireball that ended within this of the bot's middle is judged: blocked, or a hit.
pub const NEAR_END: f64 = 3.5;
/// Server ticks after a fireball ended before it is judged (the damage event and the block
/// sound come in the same tick or the next).
pub const JUDGE_TICKS: u64 = 4;

/// What the shield goes up against.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ThreatKind {
    /// A blaze whose charge flag is set, in range, with a line of sight to the bot.
    Blaze,
    /// A small fireball in range that is not flying away.
    Fireball,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Threat {
    pub id: i32,
    pub kind: ThreatKind,
    /// Where it is (a blaze's middle, the fireball itself).
    pub pos: [f64; 3],
    pub distance: f64,
}

fn distance(a: [f64; 3], b: [f64; 3]) -> f64 {
    (0..3).map(|i| (a[i] - b[i]).powi(2)).sum::<f64>().sqrt()
}

/// The bot's middle: what distances are measured to.
fn middle(w: &World) -> Option<[f64; 3]> {
    w.body.map(|b| [b.pos[0], b.pos[1] + 0.9, b.pos[2]])
}

/// Is a shield in the off hand?
pub fn held(w: &World) -> bool {
    w.inventory
        .slots
        .get(&OFF_HAND)
        .is_some_and(|(k, _)| *k == ItemKind::Shield)
}

/// The click that puts a held shield into the off hand: a swap of its slot with the off hand
/// (click mode 2 with button 40, the game's `F` over a slot). `None` with one there already, no
/// shield held, a window open, a click in flight or something on the cursor.
pub fn to_off_hand(w: &World) -> Option<(i16, Act)> {
    if held(w) || w.window.is_some() || w.click.is_some() || w.inventory.carried.is_some() {
        return None;
    }
    let slot = w
        .inventory
        .slots
        .iter()
        .find(|(s, (k, _))| (9..=44).contains(*s) && *k == ItemKind::Shield)
        .map(|(s, _)| *s)?;
    Some((
        slot,
        Act::Click {
            slot,
            button: 40,
            mode: 2,
        },
    ))
}

/// Every threat now, nearest first, fireballs ahead of blazes: charged blazes within
/// `BLAZE_RANGE` that see the bot, and small fireballs within `FIREBALL_RANGE` not known to be
/// flying away (by the velocity the server last set; one with none yet counts).
pub fn threats(w: &World) -> Vec<Threat> {
    let (Some(body), Some(mid)) = (w.body, middle(w)) else {
        return Vec::new();
    };
    let eye = [body.pos[0], body.pos[1] + physics::EYE_HEIGHT, body.pos[2]];
    let mut out: Vec<Threat> = w
        .entities
        .iter()
        .filter(|(_, e)| !e.dead)
        .filter_map(|(id, e)| match e.kind {
            EntityKind::SmallFireball => {
                let d = distance(e.pos, mid);
                let away = e
                    .velocity
                    .is_some_and(|v| (0..3).map(|i| v[i] * (mid[i] - e.pos[i])).sum::<f64>() < 0.0);
                (d <= FIREBALL_RANGE && !away).then_some(Threat {
                    id: *id,
                    kind: ThreatKind::Fireball,
                    pos: e.pos,
                    distance: d,
                })
            }
            EntityKind::Blaze if e.blaze_charged() => {
                let at = [e.pos[0], e.pos[1] + 0.9, e.pos[2]];
                let d = distance(at, mid);
                (d <= BLAZE_RANGE
                    && (crate::defend::sight(w, at, eye) || crate::defend::sight(w, at, mid)))
                .then_some(Threat {
                    id: *id,
                    kind: ThreatKind::Blaze,
                    pos: at,
                    distance: d,
                })
            }
            _ => None,
        })
        .collect();
    out.sort_by(|a, b| {
        (a.kind == ThreatKind::Blaze, a.distance, a.id)
            .partial_cmp(&(b.kind == ThreatKind::Blaze, b.distance, b.id))
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    out
}

/// Should a shield be up now: a charged blaze in range that sees the bot, or a fireball coming.
pub fn should_raise(w: &World) -> bool {
    !threats(w).is_empty()
}

/// The yaw to hold the shield toward: the nearest fireball coming, else the nearest charged
/// blaze. The shield stops what comes from within 90 degrees either side of it.
pub fn facing(w: &World) -> Option<f64> {
    let body = w.body?;
    threats(w)
        .first()
        .map(|t| crate::walk::yaw_to(body.pos, t.pos))
}

/// The tick's controls under a raised shield held toward `yaw`: walking on, slowed, when the
/// tick's own way lies within `ADVANCE_ARC` of the threat (closing on a blaze behind the
/// shield); else standing, turned to the threat.
pub fn controls(c: physics::Controls, yaw: f64, tick: u64) -> physics::Controls {
    let d = (c.yaw - yaw).rem_euclid(360.0);
    if c.forward && d.min(360.0 - d) <= ADVANCE_ARC {
        let go = tick.is_multiple_of(SLOW_EVERY);
        return physics::Controls {
            forward: go,
            jump: c.jump && go,
            ..c
        };
    }
    physics::Controls {
        forward: false,
        jump: false,
        yaw,
        ..c
    }
}

/// What the holder asks of the tick.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Step {
    /// The shield is up (or going up) toward this yaw: `controls`, and strike nothing.
    pub yaw: Option<f64>,
    /// `UseOffHand` to raise, `ReleaseUse` to lower.
    pub act: Option<Act>,
    /// For the ledger.
    pub notes: Vec<String>,
}

/// A fireball that ended beside the bot, waiting to be judged.
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Ended {
    pub id: i32,
    /// Server tick it was last seen.
    pub at: u64,
    pub distance: f64,
    /// Was the shield up (for at least the 5 ticks the server asks) when it ended?
    pub up: bool,
    pub health: Option<f32>,
}

/// The raise and the lower, and the count of what the shield met.
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct Shield {
    /// When the raise was sent; `None` while it is down.
    pub raised_at: Option<u64>,
    /// When `use_item` last went.
    pub sent_at: u64,
    /// When a due threat was last seen.
    pub threat_at: u64,
    /// When each blaze's charge was first seen.
    pub charged_since: BTreeMap<i32, u64>,
    /// Each fireball in flight: its last distance from the bot's middle.
    pub balls: BTreeMap<i32, f64>,
    pub ended: Vec<Ended>,
    pub raises: u32,
    /// Fireballs that ended beside the bot with no fireball hit, the shield up; the same with
    /// it down (a miss); and fireball hits with it up and down.
    pub blocked: u32,
    pub missed: u32,
    pub hits_up: u32,
    pub hits_down: u32,
    /// `item.shield.block` sounds heard, and the server tick they are counted up to.
    pub blocks_heard: u32,
    pub heard_until: u64,
    /// How many of them were counted at `heard_until` itself.
    pub heard_there: u32,
    /// The worn note was written for the shield in hand.
    pub worn_noted: bool,
}

impl Shield {
    /// The threats the shield is due up for: every fireball, and each blaze `RAISE_AFTER` ticks
    /// after its charge was first seen.
    fn due(&mut self, w: &World, tick: u64) -> Vec<Threat> {
        let all = threats(w);
        let charged: Vec<i32> = w
            .entities
            .iter()
            .filter(|(_, e)| !e.dead && e.blaze_charged())
            .map(|(id, _)| *id)
            .collect();
        self.charged_since.retain(|id, _| charged.contains(id));
        for id in charged {
            self.charged_since.entry(id).or_insert(tick);
        }
        let mut due: Vec<Threat> = all
            .into_iter()
            .filter(|t| match t.kind {
                ThreatKind::Fireball => true,
                ThreatKind::Blaze => self
                    .charged_since
                    .get(&t.id)
                    .is_some_and(|since| tick.saturating_sub(*since) >= RAISE_AFTER),
            })
            .collect();
        // Fireballs first, the nearest first (as `threats` has them); then the blaze charged
        // longest: its volley comes first.
        due.sort_by_key(|t| match t.kind {
            ThreatKind::Fireball => 0,
            ThreatKind::Blaze => 1 + self.charged_since.get(&t.id).copied().unwrap_or(tick),
        });
        due
    }

    /// The block sounds since the last tick, counted; and once a shield, its wear under
    /// `WORN_SHARE`.
    fn wear(&mut self, w: &World, notes: &mut Vec<String>) {
        let now = w.session.server_ticks;
        // Server ticks come twenty at a time (`set_time`), so many sounds share one: those at
        // the newest tick are counted again each step, less what was counted there already.
        let blocks = |at: u64| {
            let n = w
                .sounds
                .iter()
                .filter(|s| s.at_tick == at && s.name.ends_with("shield.block"))
                .count();
            u32::try_from(n).unwrap_or(u32::MAX)
        };
        if now > self.heard_until {
            self.blocks_heard += blocks(self.heard_until).saturating_sub(self.heard_there);
            self.heard_there = 0;
            self.heard_until = now;
        }
        let there = blocks(now);
        self.blocks_heard += there.saturating_sub(self.heard_there);
        self.heard_there = there;
        let Some(left) = w.inventory.uses_left(OFF_HAND).filter(|_| held(w)) else {
            self.worn_noted = false;
            return;
        };
        let new = data::generated::item_props::max_damage(ItemKind::Shield);
        let worn = f64::from(left) < f64::from(new) * WORN_SHARE;
        if worn && !self.worn_noted {
            let spares = w
                .inventory
                .slots
                .iter()
                .filter(|(s, (k, _))| **s != OFF_HAND && *k == ItemKind::Shield)
                .count();
            notes.push(format!(
                "shield worn: {left} of {new} uses left, {spares} spare shields held, {} blocks heard",
                self.blocks_heard
            ));
        }
        self.worn_noted = worn;
    }

    /// Fireballs gone since the last tick are kept to be judged; those kept long enough are.
    fn judge(&mut self, w: &World, tick: u64, notes: &mut Vec<String>) {
        let Some(mid) = middle(w) else {
            return;
        };
        let now = w.session.server_ticks;
        let up = self
            .raised_at
            .is_some_and(|at| tick.saturating_sub(at) >= 5);
        let mut flying = BTreeMap::new();
        for (id, e) in &w.entities {
            if e.kind == EntityKind::SmallFireball {
                flying.insert(*id, distance(e.pos, mid));
            }
        }
        for (id, d) in &self.balls {
            if !flying.contains_key(id) && *d <= NEAR_END {
                self.ended.push(Ended {
                    id: *id,
                    at: now,
                    distance: *d,
                    up,
                    health: w.player.health,
                });
            }
        }
        self.balls = flying;
        let (ripe, wait): (Vec<Ended>, Vec<Ended>) = self
            .ended
            .iter()
            .partition(|e| now.saturating_sub(e.at) >= JUDGE_TICKS);
        self.ended = wait;
        for e in ripe {
            let since = e.at.saturating_sub(2);
            let hit = w
                .hits
                .iter()
                .any(|h| h.at_tick >= since && h.damage_type.contains("fireball"));
            let heard = w
                .sounds
                .iter()
                .any(|s| s.at_tick >= since && s.name.ends_with("shield.block"));
            let what = match (hit, e.up) {
                (true, true) => {
                    self.hits_up += 1;
                    "hit the bot with the shield up"
                }
                (true, false) => {
                    self.hits_down += 1;
                    "hit the bot with the shield down"
                }
                (false, true) => {
                    self.blocked += 1;
                    "vanished with no damage, the shield up"
                }
                (false, false) => {
                    self.missed += 1;
                    "vanished with no damage, the shield down"
                }
            };
            notes.push(format!(
                "fireball {} {what}: last seen {:.1} off, health {:?} to {:?}, block sound {}; {} blocked, {} hit up, {} hit down, {} missed",
                e.id,
                e.distance,
                e.health,
                w.player.health,
                if heard { "heard" } else { "not heard" },
                self.blocked,
                self.hits_up,
                self.hits_down,
                self.missed
            ));
        }
    }

    /// One tick. `free`: the hands and the feet can be spared (no escape, no meal, no window
    /// work); when not, a raised shield comes down.
    pub fn step(&mut self, w: &World, tick: u64, free: bool) -> Step {
        let mut step = Step::default();
        self.judge(w, tick, &mut step.notes);
        self.wear(w, &mut step.notes);
        let due = self.due(w, tick);
        let Some(body) = w.body else {
            return step;
        };
        let usable = free && held(w) && w.window.is_none();
        if let Some(first) = due.first().filter(|_| usable) {
            self.threat_at = tick;
            step.yaw = Some(crate::walk::yaw_to(body.pos, first.pos));
            match self.raised_at {
                None => {
                    self.raised_at = Some(tick);
                    self.sent_at = tick;
                    self.raises += 1;
                    step.act = Some(Act::UseOffHand);
                    step.notes.push(format!(
                        "shield raised against {} {} at {:.1} ({} threats, health {:?})",
                        match first.kind {
                            ThreatKind::Blaze => "blaze",
                            ThreatKind::Fireball => "fireball",
                        },
                        first.id,
                        first.distance,
                        due.len(),
                        w.player.health
                    ));
                }
                Some(_) if tick.saturating_sub(self.sent_at) >= RESEND_TICKS => {
                    self.sent_at = tick;
                    step.act = Some(Act::UseOffHand);
                }
                Some(_) => {}
            }
            return step;
        }
        let Some(at) = self.raised_at else {
            return step;
        };
        // No threat due (or no hands for it): held a moment for a fireball between two
        // packets, then lowered.
        if usable && tick.saturating_sub(self.threat_at) < HOLD_TICKS {
            step.yaw = Some(w.controls.yaw);
            return step;
        }
        self.raised_at = None;
        step.act = Some(Act::ReleaseUse);
        step.notes.push(format!(
            "shield lowered after {} ticks ({}; {} blocks heard in all; health {:?})",
            tick.saturating_sub(at),
            if usable {
                "no threat left"
            } else {
                "the hands are needed"
            },
            self.blocks_heard,
            w.player.health
        ));
        step
    }
}
