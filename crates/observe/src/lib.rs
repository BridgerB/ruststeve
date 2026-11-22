//! What the bot senses, as data (issue 16e, phase C). Pure: the bot keeps a [`Snapshot`] of the
//! world each tick, [`diff`] turns two of them into the [`Fact`]s of the ledger (health, food, a
//! hit and what dealt it, items and tools worn or broken, sounds, mobs appearing, turning angry,
//! fusing, charging or facing the bot), and [`senses`] gives one [`Senses`] row a second. Nothing
//! here reads anything the bot was not sent: no X-ray, and no server truth.

#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::BTreeMap;

use data::EntityKind;
use data::generated::mobs::{MobCategory, mob};
use protocol::metadata::MetaValue;
use serde::{Deserialize, Serialize};
use world::World;

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
    /// Items held, by name, summed over slots.
    pub items: BTreeMap<String, i32>,
    /// Each slot's wearing item: its name and uses left.
    pub tools: BTreeMap<i16, (String, i32)>,
    /// Monsters within `WATCH` and its margin, by entity id.
    pub mobs: BTreeMap<i32, Seen>,
    pub effects: Vec<String>,
    /// How many hits, sounds, level events and chat lines the world had taken in (their lists
    /// are capped, so the newest past these are new).
    pub hits: usize,
    pub sounds: usize,
    pub level_events: usize,
    pub chat: usize,
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
    /// A mob began to show a sign (fuse, charge, anger, facing the bot).
    Sign {
        id: i32,
        kind: EntityKind,
        sign: String,
        d: f64,
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
        tools,
        mobs,
        effects: w.player.effects.keys().cloned().collect(),
        hits: w.hits.len(),
        sounds: w.sounds.len(),
        level_events: w.level_events.len(),
        chat: w.chat.len(),
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
    // New blows: the world keeps a capped list, so take as many from its end as it grew by (a
    // list pruned in the same tick shows none new, and the next blow is seen).
    let new_hits = next.hits.saturating_sub(prev.hits);
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
        let gone = next.tools.get(slot).is_none_or(|(i, _)| i != item);
        // A tool with a few uses left gone from its slot, and one fewer held: it broke (a tool
        // moved or dropped keeps its count).
        let fewer =
            next.items.get(item).copied().unwrap_or(0) < prev.items.get(item).copied().unwrap_or(0);
        if gone && fewer && *left <= 3 {
            out.push(Fact::ToolBroke { item: item.clone() });
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
}

/// The senses row for `w`, with the task and its reason as the planner holds them.
pub fn senses(w: &World, task: &str, why: &str, tick_us: Option<u64>) -> Senses {
    let bot = feet(w);
    let here = bot.unwrap_or([0.0; 3]);
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
        }
    }
}
