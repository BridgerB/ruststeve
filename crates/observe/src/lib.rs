//! What the bot senses, as data (issue 16e, phase C). Pure: the bot keeps a [`Snapshot`] of the
//! world each tick, [`diff`] turns two of them into the [`Fact`]s of the ledger (health, food, a
//! hit and what dealt it, items and tools worn or broken, sounds, mobs appearing, turning angry,
//! fusing, charging or facing the bot), and [`senses`] gives one [`Senses`] row a second (with the
//! feet's cell, what it stands on and any lava or water against the body). Nothing
//! here reads anything the bot was not sent: no X-ray, and no server truth.

#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::BTreeMap;

use data::generated::mobs::{MobCategory, mob};
use data::{BlockKind, EntityKind};
use protocol::metadata::MetaValue;
use serde::{Deserialize, Serialize};
use world::{BlockPos, World};

/// How far a mob is watched for the ledger (blocks): clojurecraft's ledger notes each within 32.
pub const WATCH: f64 = 32.0;
/// A watched mob is gone only past `WATCH` and this margin, so one at the edge does not flicker
/// in and out (a bat at 32 blocks filled the first re-read ledger, issue 16e).
pub const WATCH_MARGIN: f64 = 4.0;
/// Hostiles in a senses row, nearest first (clojurecraft's six).
pub const HOSTILES: usize = 6;
/// A sound or level event within this is in the ledger; farther, only the loud kinds.
pub const HEAR: f64 = 16.0;
/// A tool's wear is noted as it falls under each share of its uses (half, a quarter, a tenth).
pub const WEAR_MARKS: [f64; 3] = [0.5, 0.25, 0.1];
/// Degrees off the line to the bot that count as facing it (clojurecraft's stare, 12).
pub const FACING: f64 = 12.0;
/// Client ticks a click is waited on before the holdings are read again: until the server
/// answers, the stack the click moved is in no slot and not on the cursor (the world forgets the
/// cursor as it clicks), so every count would dip and come back. A third of the planner's
/// patience with a click (`plan::craft::CLICK_PATIENCE_TICKS`, 60): an answer takes two or three.
pub const CLICK_SETTLE: u64 = 20;
/// Client ticks a damage event stands for a fall in health that follows it: the server sends the
/// event as the blow lands and the health at the end of that tick or the next, so the two can
/// come a tick or two apart. Half a second; past it a fall in health has no event of its own.
pub const HIT_WINDOW: u64 = 10;
/// Uses left at which a tool gone from its slot is said to have broken: its last use (issue 16,
/// observability gaps wall 2: under 4 was called a break, and a moved or dropped tool read so).
pub const BREAK_AT: i32 = 1;

// Entity-data indexes read here, from the game's class layout on 26.1.2 (Entity 0 to 7,
// LivingEntity 8 to 14, Mob 15, then each class's own): the creeper's swell direction and
// ignited flag, the blaze's charge, the enderman's creepy flag, the ghast's charging. Issue 16e
// plans to generate these from the jar; until then each is checked by a test on recorded bytes.
const CREEPER_SWELL: u8 = 16;
const CREEPER_IGNITED: u8 = 18;
const ENDERMAN_CREEPY: u8 = 17;
const GHAST_CHARGING: u8 = 16;

/// What a mob shows, as the ledger tracks it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Signs {
    /// A creeper swelling or ignited.
    pub fuse: bool,
    /// A blaze charging its volley, a ghast charging its shot.
    pub charge: bool,
    /// An enderman with a target (creepy).
    pub angry: bool,
    /// Its head points at the bot.
    pub facing: bool,
}

/// One mob in a snapshot.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Seen {
    pub kind: EntityKind,
    pub d: f64,
    pub signs: Signs,
    /// It has come within `WATCH` since it was first seen (its appearance is in the ledger); it
    /// stays so until it leaves `WATCH` and the margin.
    #[serde(default)]
    pub announced: bool,
}

/// A projectile near the bot: its kind and the closest it has come.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Flying {
    pub kind: EntityKind,
    pub closest: f64,
}

/// Projectiles watched (blocks): clojurecraft's senses list those within 16.
pub const PROJECTILE_WATCH: f64 = 16.0;

fn projectile(kind: EntityKind) -> bool {
    matches!(
        kind,
        EntityKind::SmallFireball
            | EntityKind::Fireball
            | EntityKind::Arrow
            | EntityKind::WitherSkull
            | EntityKind::DragonFireball
    )
}

/// The parts of the world the ledger compares from tick to tick: cheap to take every tick.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Snapshot {
    pub at: u64,
    pub health: Option<f32>,
    pub food: Option<i32>,
    pub on_fire: bool,
    pub dimension: Option<String>,
    pub stage: Option<String>,
    pub died: bool,
    pub won: bool,
    /// Items held, by name, summed over slots and the cursor's stack.
    pub items: BTreeMap<String, i32>,
    /// The stack on the cursor (counted in `items`): a stack left there is in no slot the bot
    /// can use.
    #[serde(default)]
    pub cursor: Option<(String, i32)>,
    /// Client ticks since a click the server has not answered, when one waits.
    #[serde(default)]
    pub click_age: Option<u64>,
    /// The world's client tick.
    #[serde(default)]
    pub ticks: u64,
    /// The newest blow's server tick and how many blows carry it: what is new is told from this,
    /// not from the list's length (the list is pruned as it grows, so in a long fight its length
    /// stands still and every blow would go unseen).
    #[serde(default)]
    pub hit_mark: Option<(u64, usize)>,
    /// Damage events no fall in health has answered yet, and the client tick the newest came
    /// (carried from tick to tick by `diff`; they lapse after `HIT_WINDOW`).
    #[serde(default)]
    pub hits_unanswered: u32,
    #[serde(default)]
    pub hit_tick: u64,
    /// Each slot's wearing item: its name and uses left.
    pub tools: BTreeMap<i16, (String, i32)>,
    /// Monsters within `WATCH` and its margin, by entity id.
    pub mobs: BTreeMap<i32, Seen>,
    pub effects: Vec<String>,
    /// How many hits, sounds, level events and chat lines the world had taken in (their lists
    /// are capped, so the newest past these are new).
    /// Projectiles within `PROJECTILE_WATCH`, by id, with the closest each came (carried from
    /// tick to tick by `diff`).
    #[serde(default)]
    pub projectiles: BTreeMap<i32, Flying>,
    /// Blows the bot landed on each watched mob (damage events it dealt).
    #[serde(default)]
    pub landed: BTreeMap<i32, usize>,
    pub hits: usize,
    pub sounds: usize,
    pub level_events: usize,
    pub chat: usize,
    /// The time of day (`World::day_time`), when the server has told it.
    #[serde(default)]
    pub day_time: Option<i64>,
}

/// A fact for the ledger.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Fact {
    Health {
        from: Option<f32>,
        to: f32,
    },
    Food {
        from: Option<i32>,
        to: i32,
    },
    /// A blow on the bot: its damage type, what dealt it, the health after.
    Hit {
        damage_type: String,
        by: Option<String>,
        health: Option<f32>,
    },
    /// The bot caught fire, and the likeliest cause (the newest blow, else unknown).
    Alight {
        cause: String,
    },
    Item {
        item: String,
        from: i32,
        to: i32,
    },
    /// A tool fell under a share of its uses.
    ToolWorn {
        item: String,
        uses_left: i32,
    },
    /// A tool is gone from its slot with its uses run out (it broke).
    ToolBroke {
        item: String,
    },
    /// A tool is gone from the holdings with uses left: dropped, thrown or lost, not worn out.
    ToolGone {
        item: String,
        uses_left: i32,
    },
    /// Health fell with no damage event of its own (none in the `HIT_WINDOW` ticks before it that
    /// another fall had not answered): what was nearest and whether the bot burned, so no loss
    /// of health is silent.
    HealthDrop {
        by: f32,
        to: f32,
        hostile: Option<(String, f64)>,
        on_fire: bool,
    },
    /// The cursor's stack changed: what it holds now (`None` is an empty cursor).
    Cursor {
        item: Option<String>,
        count: i32,
    },
    Dimension {
        to: String,
    },
    Stage {
        reached: String,
    },
    Death,
    Won,
    Effect {
        name: String,
        on: bool,
    },
    Sound {
        name: String,
        d: Option<f64>,
    },
    LevelEvent {
        name: String,
        d: f64,
    },
    Chat {
        via: String,
        text: String,
    },
    Appear {
        id: i32,
        kind: EntityKind,
        d: f64,
    },
    Gone {
        id: i32,
        kind: EntityKind,
    },
    /// A projectile near the bot is gone: its kind and the closest it came (the 24 of 212
    /// blaze fireballs that hit clojurecraft's bots all passed within 1.4 blocks).
    Shot {
        kind: EntityKind,
        closest: f64,
    },
    /// The bot's blow landed on a mob (a damage event it dealt).
    Landed {
        id: i32,
        kind: EntityKind,
    },
    /// A mob began to show a sign (fuse, charge, anger, facing the bot).
    Sign {
        id: i32,
        kind: EntityKind,
        sign: String,
        d: f64,
    },
    /// Night fell (`world::time::DUSK`), or the bot joined at night: the time of day then.
    Night {
        day_time: i64,
    },
    /// Day broke (`world::time::DAWN`): the time of day then.
    Day {
        day_time: i64,
    },
}

/// A fact and the server tick it was seen.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Stamped {
    pub at: u64,
    pub fact: Fact,
}

fn feet(w: &World) -> Option<[f64; 3]> {
    w.body.map(|b| b.pos).or(w.player.pos)
}

fn dist(a: [f64; 3], b: [f64; 3]) -> f64 {
    (0..3).map(|i| (a[i] - b[i]).powi(2)).sum::<f64>().sqrt()
}

fn meta_bool(e: &world::Entity, i: u8) -> bool {
    match e.metadata.get(&i) {
        Some(MetaValue::Bool(b)) => *b,
        Some(MetaValue::Byte(b)) => *b != 0,
        Some(_) | None => false,
    }
}

/// The signs a mob shows, from its data and its head.
pub fn signs(e: &world::Entity, bot: [f64; 3]) -> Signs {
    let fuse = e.kind == EntityKind::Creeper
        && (matches!(e.metadata.get(&CREEPER_SWELL), Some(MetaValue::Int(v)) if *v > 0)
            || meta_bool(e, CREEPER_IGNITED));
    let charge = e.blaze_charged() || (e.kind == EntityKind::Ghast && meta_bool(e, GHAST_CHARGING));
    let angry = e.kind == EntityKind::Enderman && meta_bool(e, ENDERMAN_CREEPY);
    let facing = hostile(e.kind) && e.faces(bot, FACING).unwrap_or(false);
    Signs {
        fuse,
        charge,
        angry,
        facing,
    }
}

/// A monster by the game's own category.
pub fn hostile(kind: EntityKind) -> bool {
    mob(kind).is_some_and(|m| m.category == MobCategory::Monster)
}

fn item_name(i: data::ItemKind) -> String {
    i.name().unwrap_or("unknown").to_owned()
}

/// The parts of `w` the ledger compares.
pub fn snapshot(w: &World) -> Snapshot {
    let at = w.session.server_ticks;
    let bot = feet(w).unwrap_or([0.0; 3]);
    let mut items: BTreeMap<String, i32> = BTreeMap::new();
    for (item, n) in w.inventory.slots.values() {
        *items.entry(item_name(*item)).or_default() += n;
    }
    // The cursor's stack is held too: a pickaxe on the cursor is not a pickaxe lost.
    let cursor = w.inventory.carried.map(|(item, n)| (item_name(item), n));
    if let Some((item, n)) = &cursor {
        *items.entry(item.clone()).or_default() += n;
    }
    let tools = w
        .inventory
        .slots
        .iter()
        .filter_map(|(slot, (item, _))| {
            w.inventory
                .uses_left(*slot)
                .map(|u| (*slot, (item_name(*item), u)))
        })
        .collect();
    let mobs = w
        .entities
        .iter()
        .filter(|(_, e)| !e.dead && hostile(e.kind))
        .filter_map(|(id, e)| {
            let d = dist(e.pos, bot);
            (d <= WATCH + WATCH_MARGIN).then(|| {
                (
                    *id,
                    Seen {
                        kind: e.kind,
                        d,
                        signs: signs(e, bot),
                        announced: d <= WATCH,
                    },
                )
            })
        })
        .collect();
    let projectiles = w
        .entities
        .iter()
        .filter(|(_, e)| projectile(e.kind))
        .filter_map(|(id, e)| {
            let d = dist(e.pos, bot);
            (d <= PROJECTILE_WATCH).then_some((
                *id,
                Flying {
                    kind: e.kind,
                    closest: d,
                },
            ))
        })
        .collect();
    let landed = w
        .entities
        .iter()
        .filter(|(_, e)| hostile(e.kind))
        .map(|(id, e)| (*id, e.hurts.iter().filter(|h| h.by_bot).count()))
        .filter(|(_, n)| *n > 0)
        .collect();
    Snapshot {
        at,
        health: w.player.health,
        food: w.player.food,
        on_fire: w.player.on_fire,
        dimension: w.dimension.as_ref().map(|d| d.name.clone()),
        stage: w.stage_reached.map(|s| s.name().to_owned()),
        died: w.death.is_some(),
        won: w.won,
        items,
        cursor,
        click_age: w.click.map(|c| w.ticks.saturating_sub(c.at_tick)),
        ticks: w.ticks,
        hit_mark: w.hits.last().map(|h| {
            (
                h.at_tick,
                w.hits.iter().filter(|x| x.at_tick == h.at_tick).count(),
            )
        }),
        hits_unanswered: 0,
        hit_tick: 0,
        tools,
        mobs,
        effects: w.player.effects.keys().cloned().collect(),
        projectiles,
        landed,
        hits: w.hits.len(),
        sounds: w.sounds.len(),
        level_events: w.level_events.len(),
        chat: w.chat.len(),
        day_time: w.day_time(),
    }
}

/// Loud sounds heard from farther than `HEAR`: shots, warnings, stares, screams, teleports,
/// deaths (clojurecraft's far-sound list).
fn loud(name: &str) -> bool {
    [
        "shoot", "warn", "stare", "scream", "teleport", "death", "primed", "fireball",
    ]
    .iter()
    .any(|k| name.contains(k))
}

/// The facts between two snapshots of the same run, `w` being the world at `next`.
pub fn diff(prev: &Snapshot, next: &mut Snapshot, w: &World) -> Vec<Stamped> {
    let at = next.at;
    let mut out: Vec<Fact> = Vec::new();
    let bot = feet(w).unwrap_or([0.0; 3]);
    if let Some(h) = next.health
        && next.health != prev.health
    {
        out.push(Fact::Health {
            from: prev.health,
            to: h,
        });
    }
    if let Some(f) = next.food
        && next.food != prev.food
    {
        out.push(Fact::Food {
            from: prev.food,
            to: f,
        });
    }
    // New blows: those stamped after the newest seen before, and any more at that same stamp.
    // The world prunes its list as it grows, so the list's length says nothing in a long fight.
    let new_hits = match prev.hit_mark {
        Some((t, n)) => {
            let after = w.hits.iter().filter(|h| h.at_tick > t).count();
            let same = w.hits.iter().filter(|h| h.at_tick == t).count();
            after + same.saturating_sub(n)
        }
        None => w.hits.len(),
    };
    // A fall in health is answered by a damage event that came with it or just before it.
    let lapsed = next.ticks.saturating_sub(prev.hit_tick) > HIT_WINDOW;
    next.hits_unanswered = if lapsed { 0 } else { prev.hits_unanswered };
    next.hit_tick = prev.hit_tick;
    if new_hits > 0 {
        next.hits_unanswered += u32::try_from(new_hits).unwrap_or(u32::MAX);
        next.hit_tick = next.ticks;
    }
    if let (Some(was), Some(now)) = (prev.health, next.health)
        && now < was
    {
        if next.hits_unanswered > 0 {
            next.hits_unanswered -= 1;
        } else {
            let hostile = next
                .mobs
                .values()
                .min_by(|a, b| a.d.total_cmp(&b.d))
                .map(|m| (m.kind.name().unwrap_or("unknown").to_owned(), m.d));
            out.push(Fact::HealthDrop {
                by: was - now,
                to: now,
                hostile,
                on_fire: next.on_fire,
            });
        }
    }
    for h in w.hits.iter().rev().take(new_hits).rev() {
        out.push(Fact::Hit {
            damage_type: h.damage_type.clone(),
            by: h
                .cause
                .or(h.direct)
                .map(|k| k.name().unwrap_or("unknown").to_owned()),
            health: next.health,
        });
    }
    if next.on_fire && !prev.on_fire {
        let cause = w
            .hits
            .last()
            .filter(|h| h.at_tick + 40 >= at)
            .map_or_else(|| "unknown".to_owned(), |h| h.damage_type.clone());
        out.push(Fact::Alight { cause });
    }
    // A click not yet answered: the holdings are as they last settled (see `CLICK_SETTLE`).
    if next.click_age.is_some_and(|a| a <= CLICK_SETTLE) {
        next.items.clone_from(&prev.items);
        next.tools.clone_from(&prev.tools);
        next.cursor.clone_from(&prev.cursor);
    }
    if next.cursor != prev.cursor {
        out.push(Fact::Cursor {
            item: next.cursor.as_ref().map(|(i, _)| i.clone()),
            count: next.cursor.as_ref().map_or(0, |(_, n)| *n),
        });
    }
    for (item, n) in &next.items {
        let was = prev.items.get(item).copied().unwrap_or(0);
        if *n != was {
            out.push(Fact::Item {
                item: item.clone(),
                from: was,
                to: *n,
            });
        }
    }
    for (item, was) in &prev.items {
        if !next.items.contains_key(item) {
            out.push(Fact::Item {
                item: item.clone(),
                from: *was,
                to: 0,
            });
        }
    }
    for (slot, (item, left)) in &next.tools {
        if let Some((pitem, pleft)) = prev.tools.get(slot)
            && pitem == item
        {
            let max = data::generated::item_props::max_damage(
                data::ItemKind::from_name(item).unwrap_or(data::ItemKind::Air),
            )
            .max(1);
            for m in WEAR_MARKS {
                let line = (f64::from(max) * m).floor() as i32;
                if *pleft > line && *left <= line {
                    out.push(Fact::ToolWorn {
                        item: item.clone(),
                        uses_left: *left,
                    });
                }
            }
        }
    }
    for (slot, (item, left)) in &prev.tools {
        match next.tools.get(slot).filter(|(i, _)| i == item) {
            // Still in its slot: broken only if its uses ran out there.
            Some((_, now)) => {
                if *now <= 0 && *left > 0 {
                    out.push(Fact::ToolBroke { item: item.clone() });
                }
            }
            // Gone from its slot, and one fewer held in every slot and on the cursor (a tool
            // moved to another slot or to the cursor keeps the count): broken if it was on its
            // last use, else gone with uses left, which is said as such and never as a break.
            None => {
                let held = |s: &Snapshot| s.items.get(item).copied().unwrap_or(0);
                if held(next) < held(prev) {
                    out.push(if *left <= BREAK_AT {
                        Fact::ToolBroke { item: item.clone() }
                    } else {
                        Fact::ToolGone {
                            item: item.clone(),
                            uses_left: *left,
                        }
                    });
                }
            }
        }
    }
    if next.dimension != prev.dimension
        && let Some(d) = &next.dimension
    {
        out.push(Fact::Dimension { to: d.clone() });
    }
    if next.stage != prev.stage
        && let Some(s) = &next.stage
    {
        out.push(Fact::Stage { reached: s.clone() });
    }
    if next.died && !prev.died {
        out.push(Fact::Death);
    }
    if next.won && !prev.won {
        out.push(Fact::Won);
    }
    let night = |s: &Snapshot| s.day_time.map(world::time::is_night);
    match (night(prev), night(next), next.day_time) {
        (Some(true), Some(false), Some(day_time)) => out.push(Fact::Day { day_time }),
        (Some(false) | None, Some(true), Some(day_time)) => out.push(Fact::Night { day_time }),
        _ => {}
    }
    for e in &next.effects {
        if !prev.effects.contains(e) {
            out.push(Fact::Effect {
                name: e.clone(),
                on: true,
            });
        }
    }
    for e in &prev.effects {
        if !next.effects.contains(e) {
            out.push(Fact::Effect {
                name: e.clone(),
                on: false,
            });
        }
    }
    let new_sounds = next.sounds.saturating_sub(prev.sounds);
    for s in w.sounds.iter().rev().take(new_sounds).rev() {
        let d = s.pos.map(|p| dist(p, bot));
        if d.is_none_or(|d| d <= HEAR) || loud(&s.name) {
            out.push(Fact::Sound {
                name: s.name.clone(),
                d,
            });
        }
    }
    let new_events = next.level_events.saturating_sub(prev.level_events);
    for e in w.level_events.iter().rev().take(new_events).rev() {
        let p = [f64::from(e.pos.0), f64::from(e.pos.1), f64::from(e.pos.2)];
        let d = dist(p, bot);
        let name = e
            .name
            .clone()
            .unwrap_or_else(|| format!("level_event:{}", e.id));
        if d <= HEAR || loud(&name) {
            out.push(Fact::LevelEvent { name, d });
        }
    }
    let new_chat = next.chat.saturating_sub(prev.chat);
    for c in w.chat.iter().rev().take(new_chat).rev() {
        out.push(Fact::Chat {
            via: c.via.clone(),
            text: c.text.clone(),
        });
    }
    for (id, m) in &mut next.mobs {
        let before = prev.mobs.get(id).filter(|p| p.announced);
        m.announced = before.is_some() || m.d <= WATCH;
        match before {
            None if m.announced => out.push(Fact::Appear {
                id: *id,
                kind: m.kind,
                d: m.d,
            }),
            None => {}
            Some(p) => {
                for (sign, now, was) in [
                    ("fuse", m.signs.fuse, p.signs.fuse),
                    ("charge", m.signs.charge, p.signs.charge),
                    ("angry", m.signs.angry, p.signs.angry),
                    ("facing", m.signs.facing, p.signs.facing),
                ] {
                    if now && !was {
                        out.push(Fact::Sign {
                            id: *id,
                            kind: m.kind,
                            sign: sign.to_owned(),
                            d: m.d,
                        });
                    }
                }
            }
        }
    }
    for (id, m) in &prev.mobs {
        if m.announced && !next.mobs.contains_key(id) {
            out.push(Fact::Gone {
                id: *id,
                kind: m.kind,
            });
        }
    }
    for (id, f) in &mut next.projectiles {
        if let Some(p) = prev.projectiles.get(id) {
            f.closest = f.closest.min(p.closest);
        }
    }
    for (id, p) in &prev.projectiles {
        if !next.projectiles.contains_key(id) {
            out.push(Fact::Shot {
                kind: p.kind,
                closest: p.closest,
            });
        }
    }
    for (id, n) in &next.landed {
        let was = prev.landed.get(id).copied().unwrap_or(0);
        if *n > was
            && let Some(m) = w.entities.get(id)
        {
            out.push(Fact::Landed {
                id: *id,
                kind: m.kind,
            });
        }
    }
    out.into_iter().map(|fact| Stamped { at, fact }).collect()
}

/// A mob in a senses row.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Near {
    pub kind: EntityKind,
    pub d: f64,
    pub health: Option<f32>,
    pub signs: Signs,
    /// What it holds in its main hand.
    pub held: Option<String>,
}

/// One row a second: where the bot is, how it is, what it holds, what it is doing and why, and
/// what is near (issue 16e; clojurecraft's senses line).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Senses {
    pub at: u64,
    pub pos: Option<[f64; 3]>,
    pub dimension: Option<String>,
    pub health: Option<f32>,
    pub food: Option<i32>,
    pub saturation: Option<f32>,
    pub air: Option<i32>,
    pub on_fire: bool,
    /// The held item and its uses left.
    pub held: Option<String>,
    pub held_uses: Option<i32>,
    /// Every wearing item held, with its uses left (pickaxes, swords, armour).
    pub tools: Vec<(String, i32)>,
    /// Empty slots in the main inventory and hotbar.
    pub free_slots: u32,
    pub task: String,
    pub why: String,
    /// The nearest hostiles within `WATCH`, nearest first (`HOSTILES`).
    pub hostiles: Vec<Near>,
    /// Endermen within 128.
    pub endermen: u32,
    /// The newest sound heard.
    pub sound: Option<String>,
    /// The planner's time for its last tick, microseconds.
    pub tick_us: Option<u64>,
    /// The cell the feet are in: the position floored, as the planner's `plan::path::feet_cell`
    /// has it (a rounded position reads one cell off: -18.3 is cell -19, not -18).
    #[serde(default)]
    pub cell: Option<[i32; 3]>,
    /// The block in the feet's cell (air on a full block; soul sand, a slab or a fluid when the
    /// feet are inside its cell) and the block under that cell. `None` where no column is loaded.
    #[serde(default)]
    pub feet_block: Option<String>,
    #[serde(default)]
    pub under: Option<String>,
    /// Lava and water in the body's two cells or against them.
    #[serde(default)]
    pub lava: Touch,
    #[serde(default)]
    pub water: Touch,
    /// The stack on the cursor, when there is one.
    #[serde(default)]
    pub cursor: Option<(String, i32)>,
    /// The time of day, 0 to 23,999 (`World::day_time`), when the server has told it.
    #[serde(default)]
    pub day_time: Option<i64>,
}

/// How a fluid touches the body (the feet's cell and the one over it).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Touch {
    /// In neither cell and in no cell against them.
    #[default]
    No,
    /// In a cell against the body: one of the four beside either cell, the one under the feet or
    /// the one over the head.
    Beside,
    /// In the feet's cell or the head's.
    In,
}

/// The cell a position's feet are in: each coordinate floored. The same as the planner's
/// `plan::path::feet_cell` (a test in `results` holds the two together).
pub fn feet_cell(pos: [f64; 3]) -> [i32; 3] {
    [
        pos[0].floor() as i32,
        pos[1].floor() as i32,
        pos[2].floor() as i32,
    ]
}

/// How `fluid` touches a body whose feet are in `cell`.
fn touch(w: &World, cell: [i32; 3], fluid: BlockKind) -> Touch {
    let is = |dx: i32, dy: i32, dz: i32| {
        w.block_at(BlockPos::new(cell[0] + dx, cell[1] + dy, cell[2] + dz)) == Some(fluid)
    };
    if is(0, 0, 0) || is(0, 1, 0) {
        return Touch::In;
    }
    let beside = [(1, 0), (-1, 0), (0, 1), (0, -1)]
        .iter()
        .any(|(dx, dz)| is(*dx, 0, *dz) || is(*dx, 1, *dz));
    if beside || is(0, -1, 0) || is(0, 2, 0) {
        Touch::Beside
    } else {
        Touch::No
    }
}

fn block_name(w: &World, x: i32, y: i32, z: i32) -> Option<String> {
    w.block_at(BlockPos::new(x, y, z))
        .map(|b| b.name().unwrap_or("unknown").to_owned())
}

/// Blocks a body's cell holds when nothing is in it.
const AIRS: [&str; 3] = ["air", "cave_air", "void_air"];

impl Senses {
    /// The feet's cell: as recorded, else the position floored (rows from before the cell was
    /// kept).
    pub fn feet(&self) -> Option<[i32; 3]> {
        self.cell.or_else(|| self.pos.map(feet_cell))
    }

    /// Where the bot stands, in plain terms: `cell 30,31,-19 on netherrack, lava beside`,
    /// `cell 56,59,-115 over air (in the air or on an edge)`, or
    /// `cell 30,31,-19 in soul_sand over netherrack` when the feet's own cell is not empty.
    /// Rows from before the blocks were kept give the cell alone.
    pub fn standing(&self) -> String {
        let Some([x, y, z]) = self.feet() else {
            return "cell -".to_owned();
        };
        let mut s = format!("cell {x},{y},{z}");
        let under = self.under.as_deref().unwrap_or("?");
        match self.feet_block.as_deref() {
            // Nothing under the cell: in the air, or the body's box rests on a block beside.
            Some(b) if AIRS.contains(&b) && AIRS.contains(&under) => {
                s.push_str(" over air (in the air or on an edge)");
            }
            Some(b) if AIRS.contains(&b) => s.push_str(&format!(" on {under}")),
            Some(b) => s.push_str(&format!(" in {b} over {under}")),
            None if self.under.is_some() => s.push_str(&format!(" in ? over {under}")),
            None => {}
        }
        for (name, t) in [("lava", self.lava), ("water", self.water)] {
            match t {
                // Said already when the feet's own cell is the fluid.
                Touch::In if self.feet_block.as_deref() == Some(name) => {}
                Touch::In => s.push_str(&format!(", in {name}")),
                Touch::Beside => s.push_str(&format!(", {name} beside")),
                Touch::No => {}
            }
        }
        s
    }
}

/// The senses row for `w`, with the task and its reason as the planner holds them.
pub fn senses(w: &World, task: &str, why: &str, tick_us: Option<u64>) -> Senses {
    let bot = feet(w);
    let here = bot.unwrap_or([0.0; 3]);
    let cell = bot.map(feet_cell);
    let held_slot = 36 + i16::from(w.held);
    let held = w
        .inventory
        .slots
        .get(&held_slot)
        .map(|(i, _)| item_name(*i));
    let mut tools: Vec<(String, i32)> = w
        .inventory
        .slots
        .iter()
        .filter_map(|(slot, (item, _))| w.inventory.uses_left(*slot).map(|u| (item_name(*item), u)))
        .collect();
    tools.sort();
    let free_slots = (9..=44)
        .filter(|s| !w.inventory.slots.contains_key(s))
        .count() as u32;
    let mut hostiles: Vec<Near> = w
        .entities
        .values()
        .filter(|e| !e.dead && hostile(e.kind))
        .filter_map(|e| {
            let d = dist(e.pos, here);
            (d <= WATCH).then(|| Near {
                kind: e.kind,
                d,
                health: e.health(),
                signs: signs(e, here),
                held: e.equipment.get(&0).map(|i| item_name(*i)),
            })
        })
        .collect();
    hostiles.sort_by(|a, b| a.d.total_cmp(&b.d));
    hostiles.truncate(HOSTILES);
    let endermen = w
        .entities
        .values()
        .filter(|e| !e.dead && e.kind == EntityKind::Enderman && dist(e.pos, here) <= 128.0)
        .count() as u32;
    Senses {
        at: w.session.server_ticks,
        pos: bot,
        dimension: w.dimension.as_ref().map(|d| d.name.clone()),
        health: w.player.health,
        food: w.player.food,
        saturation: w.player.saturation,
        air: w.player.air,
        on_fire: w.player.on_fire,
        held_uses: w.inventory.uses_left(held_slot),
        held,
        tools,
        free_slots,
        task: task.to_owned(),
        why: why.to_owned(),
        hostiles,
        endermen,
        sound: w.sounds.last().map(|s| s.name.clone()),
        tick_us,
        cell,
        feet_block: cell.and_then(|[x, y, z]| block_name(w, x, y, z)),
        under: cell.and_then(|[x, y, z]| block_name(w, x, y - 1, z)),
        lava: cell.map_or(Touch::No, |c| touch(w, c, BlockKind::Lava)),
        water: cell.map_or(Touch::No, |c| touch(w, c, BlockKind::Water)),
        cursor: w.inventory.carried.map(|(item, n)| (item_name(item), n)),
        day_time: w.day_time(),
    }
}

impl Fact {
    /// The fact as one line of a case file or a timeline.
    pub fn line(&self) -> String {
        let kind = |k: &EntityKind| k.name().unwrap_or("unknown").to_owned();
        match self {
            Fact::Health { from, to } => match from {
                Some(f) => format!("health {f:.1} -> {to:.1}"),
                None => format!("health {to:.1}"),
            },
            Fact::Food { from, to } => match from {
                Some(f) => format!("food {f} -> {to}"),
                None => format!("food {to}"),
            },
            Fact::Hit {
                damage_type,
                by,
                health,
            } => format!(
                "hit {damage_type}{}{}",
                by.as_ref().map_or_else(String::new, |b| format!(" by {b}")),
                health.map_or_else(String::new, |h| format!(", health {h:.1}"))
            ),
            Fact::Alight { cause } => format!("alight ({cause})"),
            Fact::Item { item, from, to } => format!("item {item} {from} -> {to}"),
            Fact::ToolWorn { item, uses_left } => {
                format!("tool worn: {item}, {uses_left} uses left")
            }
            Fact::ToolBroke { item } => format!("tool broke: {item}"),
            Fact::ToolGone { item, uses_left } => format!(
                "tool gone: {item} with {uses_left} uses left (dropped or lost, not broken)"
            ),
            Fact::HealthDrop {
                by,
                to,
                hostile,
                on_fire,
            } => format!(
                "health dropped {by:.1} to {to:.1} with no damage event; nearest hostile {}; on fire {}",
                hostile
                    .as_ref()
                    .map_or_else(|| "none".to_owned(), |(k, d)| format!("{k} at {d:.1}")),
                if *on_fire { "yes" } else { "no" }
            ),
            Fact::Cursor { item, count } => match item {
                Some(i) => format!("on cursor: {i} {count}"),
                None => "cursor empty".to_owned(),
            },
            Fact::Dimension { to } => format!("dimension {to}"),
            Fact::Stage { reached } => format!("stage {reached}"),
            Fact::Death => "died".to_owned(),
            Fact::Won => "won".to_owned(),
            Fact::Effect { name, on } => {
                format!("effect {name} {}", if *on { "on" } else { "off" })
            }
            Fact::Sound { name, d } => match d {
                Some(d) => format!("heard {name} at {d:.0}"),
                None => format!("heard {name}"),
            },
            Fact::LevelEvent { name, d } => format!("level event {name} at {d:.0}"),
            Fact::Chat { via, text } => format!("{via}: {text}"),
            Fact::Appear { id, kind: k, d } => format!("{}#{id} appeared at {d:.0}", kind(k)),
            Fact::Gone { id, kind: k } => format!("{}#{id} gone", kind(k)),
            Fact::Sign {
                id,
                kind: k,
                sign,
                d,
            } => format!("{}#{id} {sign} at {d:.0}", kind(k)),
            Fact::Shot { kind: k, closest } => format!("{} passed within {closest:.1}", kind(k)),
            Fact::Landed { id, kind: k } => format!("blow landed on {}#{id}", kind(k)),
            Fact::Night { day_time } => format!("night fell (day time {day_time})"),
            Fact::Day { day_time } => format!("day broke (day time {day_time})"),
        }
    }
}
