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
/// Eye to a fought mob's box within which the shield yields to the fight's strike: the strike's
/// own reach (`defend::STRIKE_REACH`) and a block for the step that closes it. A shield raised
/// toward a charged blaze turned the bot from a wither skeleton in reach and dropped its strikes
/// (review of bba8161); a raised shield dropped 27 of 30 swings with blazes in touch (race
/// 45359254088).
pub const YIELD_REACH: f64 = crate::defend::STRIKE_REACH + 1.0;
/// A shield already up stays up through a fight in reach for a fireball in front that lands
/// within this many ticks: a raise made now would not block it (5 ticks), one made already does.
pub const LAND_TICKS: f64 = 6.0;
/// Blocks a tick a fireball whose velocity the server has not sent is taken to close at (a small
/// fireball's power is 0.1 a tick, drag 0.95: about 0.5 after its first second).
pub const ASSUMED_SPEED: f64 = 0.5;
/// Ticks the server needs a shield up before it blocks (`block_delay_seconds` 0.25).
pub const BLOCK_DELAY: u64 = 5;

/// What the tick asks of the hands, for the shield to weigh (`Shield::step`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Duty {
    /// No escape, no meal, no window or other work of the hands; a strike, a held-slot change or
    /// a click to place a block may still be in the tick.
    pub free: bool,
    /// The tick clicks to place a block (a shelter block, a bridge block).
    pub placing: bool,
    /// The fight has a mob in reach and strikes it (what, for the ledger).
    pub striking: Option<String>,
}

/// Ticks until a fireball at `pos` moving at `velocity` reaches `mid`, `None` when it is not
/// closing. One with no velocity yet closes at `ASSUMED_SPEED`.
pub fn landing_in(pos: [f64; 3], velocity: Option<[f64; 3]>, mid: [f64; 3]) -> Option<f64> {
    let d = distance(pos, mid);
    let closing = match velocity {
        Some(v) if d > 1e-6 => (0..3).map(|i| v[i] * (mid[i] - pos[i])).sum::<f64>() / d,
        Some(_) => 1.0,
        None => ASSUMED_SPEED,
    };
    (closing > 1e-6).then(|| d / closing)
}

/// What a defend has in the reach of its strike, for the shield to yield to: a blaze in touch
/// (`defend::blaze_in_touch`: the keep-off step and the strike go first, blaze-fight wall 19), or
/// in a fight, its foe or a mob of its kind (the ones it strikes) when that kind is a hostile
/// melee one (not of `ranged`), within `YIELD_REACH` of the eye by the box: a mob the fight does
/// not strike is no reason to lower the shield.
/// A blaze in reach but not in touch is not yielded to: yielding to it cost the model's eight
/// shield seeds 0.30 to 0.66 fireballs landed a thousand ticks and 522 to 658 health lost
/// (probes 45361576927 and 45362273722). `None` otherwise.
pub fn striking(w: &World, d: &crate::defend::Defend, ranged: &[EntityKind]) -> Option<String> {
    if crate::defend::blaze_in_touch(w) {
        return Some("a blaze in touch".to_owned());
    }
    if d.stance != crate::defend::Stance::Fight {
        return None;
    }
    let body = w.body?;
    let eye = [body.pos[0], body.pos[1] + physics::EYE_HEIGHT, body.pos[2]];
    w.entities
        .iter()
        .filter(|(id, e)| {
            !e.dead
                && (**id == d.foe || e.kind == d.kind)
                && crate::defend::hostile(e.kind)
                && !ranged.contains(&e.kind)
        })
        .map(|(id, e)| (crate::defend::box_distance(eye, e.pos), *id, e.kind))
        .filter(|(r, _, _)| *r <= YIELD_REACH)
        .min_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)))
        .map(|(r, id, kind)| format!("{kind:?} {id} in reach at {r:.1}"))
}

/// A defend's swing count before a tick: a strike the raised shield drops is given back to it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SwingCount {
    pub started: u64,
    pub swings: u32,
    pub passing: u32,
    pub falling: u32,
    pub last_swing: Option<u64>,
}

impl SwingCount {
    pub fn of(d: &crate::defend::Defend) -> Self {
        Self {
            started: d.started,
            swings: d.swings,
            passing: d.passing_swings,
            falling: d.falling_swings,
            last_swing: d.last_swing,
        }
    }

    /// Put the count back on `d` (the same defend, or a new one: from nothing); the swings
    /// given back.
    pub fn restore(self, d: &mut crate::defend::Defend) -> u32 {
        let was = if self.started == d.started {
            self
        } else {
            Self::default()
        };
        let back = d.swings.saturating_sub(was.swings);
        d.swings = was.swings.min(d.swings);
        d.passing_swings = was.passing.min(d.passing_swings);
        d.falling_swings = was.falling.min(d.falling_swings);
        d.last_swing = was.last_swing;
        back
    }
}

/// Whether `at` lies within the shield's 90 degrees either side of `yaw`, seen from `from`.
pub fn in_front(from: [f64; 3], at: [f64; 3], yaw: f64) -> bool {
    let d = (crate::walk::yaw_to(from, at) - yaw).rem_euclid(360.0);
    d.min(360.0 - d) < 90.0
}

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

/// Shields the kit holds before the portal (`config/goals.toml`, the `shields` row).
pub const KIT_SHIELDS: i32 = 2;

/// The goal row that makes the shields the held iron and wood allow, after the Nether kit's
/// tools: `shields` for two, `shield` for one, `None` when they are held already or nothing
/// held makes one. An ingot (or a raw iron to smelt) and six planks (a log is four) each.
pub fn kit_row(counts: &crate::make::Counts) -> Option<&'static str> {
    let have = |key: &str| crate::make::Need::parse(key).map_or(0, |n| n.have(counts));
    let held = have("item:shield");
    let iron = have("item:iron_ingot") + have("item:raw_iron");
    let planks = have("tag:planks") + 4 * have("tag:logs");
    let target = KIT_SHIELDS.min(held + iron.min(planks / 6));
    match target {
        t if t <= held => None,
        1 => Some("shield"),
        _ => Some("shields"),
    }
}

/// The row made in the Nether, from what is held: `kit_row`, but none once any shield is held.
/// A bot that made its shields at the surface (the overworld flow's cook site) arrives with
/// them and never stops for another; only a bot that came through with none makes one here.
pub fn nether_row(counts: &crate::make::Counts) -> Option<&'static str> {
    let held = counts.get(&ItemKind::Shield).copied().unwrap_or(0);
    if held > 0 {
        return None;
    }
    kit_row(counts)
}

/// For a shield row (`shield`, `shields`), how many ingots its smelt should cook: what the
/// shields still to make are short of. `None` for any other row.
pub fn smelt_count(row: &str, counts: &crate::make::Counts) -> Option<i32> {
    let target = match row {
        "shield" => 1,
        "shields" => KIT_SHIELDS,
        _ => return None,
    };
    let have = |key: &str| crate::make::Need::parse(key).map_or(0, |n| n.have(counts));
    Some((target - have("item:shield") - have("item:iron_ingot")).max(1))
}

/// What a furnace cooks into food: each a smelting result whose raw input the bot may hold.
pub const COOKED: [ItemKind; 7] = [
    ItemKind::CookedBeef,
    ItemKind::CookedPorkchop,
    ItemKind::CookedMutton,
    ItemKind::CookedChicken,
    ItemKind::CookedRabbit,
    ItemKind::CookedCod,
    ItemKind::CookedSalmon,
];

/// Game ticks of burning the held fuel gives, as the smelt burns it (`smelt::fuels_held`): coal
/// and planks whole, sticks over `make::STICK_RESERVE`.
pub fn fuel_ticks_held(counts: &crate::make::Counts) -> i64 {
    let ticks = |k: ItemKind| i64::from(data::generated::fuel::fuel_ticks(k).unwrap_or(0));
    let n = |k: ItemKind| i64::from(counts.get(&k).copied().unwrap_or(0).max(0));
    let wood: i64 = crate::make::burn_order()
        .into_iter()
        .map(|k| n(k) * ticks(k))
        .sum();
    let sticks = (n(ItemKind::Stick) - i64::from(crate::make::STICK_RESERVE)).max(0);
    wood + sticks * ticks(ItemKind::Stick)
}

/// After the shield, at its furnace: the raw meat held most of, cooked as far as the fuel left
/// burns (a raw beef is 3 hunger, a cooked one 8: the fight is won fed, docs/problems/shield.md).
/// `None` with no raw meat or no fuel for one.
pub fn cook_choice(counts: &crate::make::Counts) -> Option<(&'static data::types::Smelt, i32)> {
    let (r, n) = data::generated::recipes::SMELTING
        .iter()
        .filter(|r| COOKED.contains(&r.result))
        .map(|r| {
            let n: i32 = r
                .input
                .iter()
                .map(|i| counts.get(i).copied().unwrap_or(0))
                .sum();
            (r, n)
        })
        .filter(|(_, n)| *n > 0)
        .max_by_key(|(r, n)| (*n, std::cmp::Reverse(r.id)))?;
    let fits = fuel_ticks_held(counts) / i64::from(r.ticks.max(1));
    let count = i64::from(n).min(fits).min(64);
    (count >= 1).then(|| (r, i32::try_from(count).unwrap_or(1)))
}

/// Whether a kit act is work where the bot stands (a craft, a smelt, a station put down), not a
/// trip for wood or ore.
pub fn kit_act(row: &crate::make::Row) -> bool {
    match row.act {
        Some(
            crate::make::Act::Craft(_) | crate::make::Act::Smelt(_) | crate::make::Act::Place(_),
        ) => true,
        Some(
            crate::make::Act::Gather
            | crate::make::Act::Mine { .. }
            | crate::make::Act::Redig(_)
            | crate::make::Act::Scoop,
        )
        | None => false,
    }
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
    /// A click to place a block waits: a threat is due and the shield goes up instead.
    pub hold_place: bool,
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
    /// Ticks a due threat was left to the fight's strike (a mob in reach), the episodes of it,
    /// and whether one runs now.
    pub yield_ticks: u32,
    pub yields: u32,
    pub yielding: bool,
    /// Ticks the shield stayed up through a fight in reach for a fireball in front about to land.
    pub kept_for_ball: u32,
    /// Clicks to place a block held for the shield, in all and since the raise.
    pub places_held: u32,
    pub places_held_up: u32,
    /// Strikes the defend counted that the raised shield dropped (given back to the defend).
    pub strikes_held: u32,
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

    /// Whether the shield is up against a threat due now: a click to place a block waits for it
    /// (`Step::hold_place`). For a task that would rather not click at all.
    pub fn guarding(&self, tick: u64) -> bool {
        self.raised_at.is_some() && tick.saturating_sub(self.threat_at) <= 1
    }

    /// The nearest fireball due that a shield raised at `raised` blocks before it lands within
    /// `LAND_TICKS`, coming from within 90 degrees of the look: its id and ticks to land.
    fn ball_in_front(
        &self,
        w: &World,
        due: &[Threat],
        tick: u64,
        raised: u64,
    ) -> Option<(i32, f64)> {
        let body = w.body?;
        let mid = middle(w)?;
        let ready = raised + BLOCK_DELAY;
        due.iter()
            .filter(|t| t.kind == ThreatKind::Fireball)
            .filter_map(|t| {
                let e = w.entities.get(&t.id)?;
                let eta = landing_in(e.pos, e.velocity, mid)?;
                (eta <= LAND_TICKS
                    && (tick as f64 + eta) >= ready as f64
                    && in_front(body.pos, e.pos, w.controls.yaw))
                .then_some((t.id, eta))
            })
            .min_by(|a, b| a.1.total_cmp(&b.1).then(a.0.cmp(&b.0)))
    }

    /// One tick. `duty.free`: the hands and the feet can be spared (no escape, no meal, no
    /// window work); when not, a raised shield comes down. A click to place a block waits while a
    /// threat is due (`Step::hold_place`), else it is made with the shield down. A mob in the
    /// fight's reach is struck, the shield down, unless a fireball the shield already blocks
    /// lands in front within `LAND_TICKS`.
    pub fn step(&mut self, w: &World, tick: u64, duty: &Duty) -> Step {
        let mut step = Step::default();
        self.judge(w, tick, &mut step.notes);
        self.wear(w, &mut step.notes);
        let due = self.due(w, tick);
        let Some(body) = w.body else {
            return step;
        };
        let mut usable = duty.free && held(w) && w.window.is_none();
        // A placing click: held while a threat is due (the shield is what the tick needs, and a
        // shield lowered for each click every 6 or 10 ticks is never up the 5 the server asks),
        // else placed with the shield down.
        if duty.placing && usable {
            if due.is_empty() {
                usable = false;
            } else {
                step.hold_place = true;
                self.places_held += 1;
                self.places_held_up += 1;
                if self.places_held_up == 1 {
                    step.notes.push(format!(
                        "placing held for the shield: {} threats due ({} held in all)",
                        due.len(),
                        self.places_held
                    ));
                }
            }
        }
        // A mob in the fight's reach: the strike, not the shield (a raise turns the bot from it
        // and drops the strike), unless a fireball the raised shield already blocks lands in
        // front within a few ticks.
        if let Some(why) = duty.striking.as_ref().filter(|_| usable && !due.is_empty()) {
            if let Some(at) = self.raised_at
                && let Some((id, eta)) = self.ball_in_front(w, &due, tick, at)
            {
                self.threat_at = tick;
                self.kept_for_ball += 1;
                step.yaw = Some(w.controls.yaw);
                if self.kept_for_ball == 1 || self.yielding {
                    step.notes.push(format!(
                        "shield kept up for fireball {id} landing in {eta:.0} ticks in front ({why}); {} such ticks",
                        self.kept_for_ball
                    ));
                }
                self.yielding = false;
                return step;
            }
            self.yield_ticks += 1;
            if !self.yielding {
                self.yielding = true;
                self.yields += 1;
                step.notes.push(format!(
                    "shield yields to the strike: {why}, {} threats due ({} yields)",
                    due.len(),
                    self.yields
                ));
            }
            if let Some(at) = self.raised_at.take() {
                self.places_held_up = 0;
                step.act = Some(Act::ReleaseUse);
                step.notes.push(format!(
                    "shield lowered after {} ticks (the fight strikes; {} blocks heard in all; health {:?})",
                    tick.saturating_sub(at),
                    self.blocks_heard,
                    w.player.health
                ));
            }
            return step;
        }
        self.yielding = false;
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
        // Not with a mob in the fight's reach: it is struck at once.
        if usable && duty.striking.is_none() && tick.saturating_sub(self.threat_at) < HOLD_TICKS {
            step.yaw = Some(w.controls.yaw);
            return step;
        }
        self.raised_at = None;
        step.act = Some(Act::ReleaseUse);
        step.notes.push(format!(
            "shield lowered after {} ticks ({}; {} placing clicks held; {} blocks heard in all; health {:?})",
            tick.saturating_sub(at),
            match (usable, duty.placing && duty.free) {
                (true, _) => "no threat left",
                (false, true) => "a block to place",
                (false, false) => "the hands are needed",
            },
            self.places_held_up,
            self.blocks_heard,
            w.player.health
        ));
        self.places_held_up = 0;
        step
    }
}
