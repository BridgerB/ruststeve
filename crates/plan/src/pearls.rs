//! The hunt (issue 18f, `docs/problems/pearls-hunt.md`): provoke, fight and collect from the
//! burrow (`burrow`), as a small rule table read whole. The rows are tried in order each tick;
//! the first that holds acts, and its id is the reason written to the ledger.
//!
//! | # | Row id | When | Act |
//! |---|---|---|---|
//! | 1 | `pearls-done` | `pearls_wanted` pearls held | the mouth sealed, eyes down; met |
//! | 2 | `breach` | hurt by an enderman, or the bot not in B with an angry enderman within 4, or the mouth sealed | seal M from B, eyes down, until none angry ticks within 16 |
//! | 3 | `strike` | the target angry, its box in reach, a full charge, the bot in B | the torso (not a stare), attack then swing |
//! | 4 | `hold` | an angry enderman present, ticking or frozen | stay in B, look level at the mouth at torso height |
//! | 5 | `collect` | a pearl within `collect_range` of O, none angry, a free slot | step to M, then on to it within `collect_reach` of B; back |
//! | 6 | `base` | the forest base not done (`base_task`, agent P5) | its steps |
//! | 7 | `build` | no burrow, the bot out of it, a block missing or the mouth closed | find, walk to, build, mend, reopen |
//! | 8 | `provoke` | in M, a calm ticking enderman 6 to 36 off in sight, none angry | its eye until the creepy flag, at most `provoke_ticks` server ticks; then B |
//! | 9 | `wait` | none of the above | in M, eyes level; the burrow moved after `burrow_idle_ticks` |
//!
//! Progress invariants (each writes a note when it fires): provoke tries per enderman; a fight's
//! health-fall window, its parked target and its cap with the mouth sealed; a kill per
//! `kill_window` or the burrow moves, two moves without a kill end the hunt; resources per pearl
//! in every kill's note; the weapon switched under `weapon_min_uses`; the bounce counter (one step
//! out of B a `step_out_every` without a kill or a pickup between).
//!
//! Never a stare but the provoke row's, at a calm enderman, until its creepy flag: an angry one
//! stared at within 16 freezes (clojurecraft's real bots parked endermen for minutes so, 5d894db).

use std::collections::BTreeMap;

use data::{EntityKind, ItemKind};
use physics::Controls;
use protocol::metadata::MetaValue;
use world::World;

use crate::burrow::{self, Burrow};
use crate::endermen::{self, CREEPY_INDEX, EYES_DOWN, Sighting};
use crate::path::{self, Goal};
use crate::walk::{Arrive, Walk, look_at, yaw_to};
use crate::{Plan, Task, cover, craft, defend, hold, look};

/// `[pearls]` in `config/goals.toml`: the hunt's numbers, each with its source there.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct PearlRules {
    pub pearls_wanted: i32,
    pub provoke_ticks: u64,
    pub provoke_min: f64,
    pub provoke_max: f64,
    pub provoke_out_min: f64,
    pub provoke_tries: u32,
    pub provoke_skip_ticks: u64,
    pub fall_window: u64,
    pub parked_ticks: u64,
    pub parked_look_ticks: u64,
    pub fight_cap: u64,
    pub kill_window: u64,
    pub burrow_idle_ticks: u64,
    pub moves_without_kill: u32,
    pub burrow_move: i32,
    pub site_every: u64,
    pub weapon_min_uses: i32,
    pub step_out_every: u64,
    pub build_max: u32,
    pub walk_tries: u32,
    pub collect_range: f64,
    pub collect_reach: f64,
    pub collect_ticks: u64,
    pub breach_near: f64,
    pub near_o: f64,
    pub calm_range: f64,
}

impl Default for PearlRules {
    fn default() -> Self {
        Self {
            pearls_wanted: 12,
            provoke_ticks: 60,
            provoke_min: 6.0,
            provoke_max: 36.0,
            provoke_out_min: 12.0,
            provoke_tries: 3,
            provoke_skip_ticks: 2400,
            fall_window: 200,
            parked_ticks: 600,
            parked_look_ticks: 40,
            fight_cap: 2400,
            kill_window: 12000,
            burrow_idle_ticks: 2400,
            moves_without_kill: 2,
            burrow_move: 32,
            site_every: 100,
            weapon_min_uses: 10,
            step_out_every: 1200,
            build_max: 40,
            walk_tries: 3,
            collect_range: 6.0,
            collect_reach: 4.0,
            collect_ticks: 200,
            breach_near: 4.0,
            near_o: 8.0,
            calm_range: 16.0,
        }
    }
}

/// Blocks the burrow is built of: full, and none `#enderman_holdable` (the 26.1.2 tag: dirt,
/// gravel and nylium are; these are not). The stone nothing else wants first (granite, tuff:
/// the frontier bot holds 64 and 23), cobblestone last: the base makes stone swords of it.
pub const BURROW_BLOCKS: &[ItemKind] = &[
    ItemKind::Granite,
    ItemKind::Tuff,
    ItemKind::Andesite,
    ItemKind::Diorite,
    ItemKind::Blackstone,
    ItemKind::Netherrack,
    ItemKind::CobbledDeepslate,
    ItemKind::Cobblestone,
];
/// Ticks between two burrow blocks (the shelter's `SHELTER_EVERY_TICKS`: the update arrives in
/// a tick or two).
const BUILD_EVERY: u64 = 10;
/// A step to a cell's middle is done this near it, on the plane.
const STEP_DONE: f64 = 0.2;
/// Blocks from the feet a forest floor cell walked to lies at least (a new place to search).
const FLOOR_MIN: i32 = 8;
/// Ticks a sighting is kept for the mouth's side.
const SIGHTINGS_KEPT: u64 = 600;
/// Ticks one step out of B takes at most, counted as one step.
const STEP_OUT_TICKS: u64 = 40;
/// Ticks between two notes of one standing row (a hold, a wait).
const NOTE_EVERY: u64 = 600;
/// An enderman's torso over its feet: the strike's aim (the plan's section 3: about 30 degrees off
/// its eye from B, far outside the stare's cone).
pub const TORSO: f64 = 1.2;

/// The fight in hand.
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Fight {
    pub id: i32,
    pub since: u64,
    pub last_fall: u64,
    pub last_health: Option<f32>,
    pub out_since: Option<u64>,
    pub falls: u32,
    pub swings: u32,
    pub weapon_before: i32,
    pub fall_noted: bool,
}

/// The hunt's state, one field of `Plan` (`Plan::pearls`).
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct PearlState {
    /// The hunt has begun (`Plan::hunting` held once).
    #[serde(default)]
    pub started: bool,
    #[serde(default)]
    pub burrow: Option<Burrow>,
    #[serde(default)]
    pub built: bool,
    /// Sites given up (a walk or a build that failed) and the burrows moved from.
    #[serde(default)]
    pub refused: Vec<[i32; 3]>,
    #[serde(default)]
    pub moved_from: Vec<[i32; 3]>,
    #[serde(default)]
    pub site_at: Option<u64>,
    #[serde(default)]
    pub site_none_noted: Option<u64>,
    #[serde(default)]
    pub blocks: u32,
    /// Burrow blocks placed at the site in hand (built and mended; seals not counted).
    #[serde(default)]
    pub site_blocks: u32,
    /// Built burrow cells found open (the `BlockTaken` fact's cells), each noted once until
    /// mended.
    #[serde(default)]
    pub taken: Vec<[i32; 3]>,
    #[serde(default)]
    pub place_at: u64,
    #[serde(default)]
    pub walks: u32,
    /// When the burrow was built (the idle clock starts here or at the last kill).
    #[serde(default)]
    pub since: Option<u64>,
    #[serde(default)]
    pub hunt_start: Option<u64>,
    #[serde(default)]
    pub last_kill: Option<u64>,
    #[serde(default)]
    pub moves: u32,
    #[serde(default)]
    pub moves_no_kill: u32,
    #[serde(default)]
    pub over: bool,
    /// The provoke in hand: the target and the server tick it began.
    #[serde(default)]
    pub provoke: Option<(i32, u64)>,
    /// Where the provoke in hand is made from: M, or O when none is in sight from M.
    #[serde(default)]
    pub provoke_from: Option<[i32; 3]>,
    /// Provoke tries per enderman, and until when it is skipped.
    #[serde(default)]
    pub tries: Vec<(i32, u32, u64)>,
    #[serde(default)]
    pub provokes: u32,
    /// Endermen stared at on purpose (the provoke row's targets): their stared-at flag is no
    /// near miss.
    #[serde(default)]
    pub provoked: Vec<i32>,
    /// Ticking endermen's feet seen lately, with the tick: the burrow's mouth faces the most.
    #[serde(default)]
    pub sightings: Vec<(u64, [f64; 3])>,
    /// The tick a provoke's creepy flag came, until the bot is back in B (noted).
    #[serde(default)]
    pub back_since: Option<u64>,
    /// Forest floor cells walked to (or tried) for a new site search.
    #[serde(default)]
    pub floor_tried: Vec<[i32; 3]>,
    /// Weapon uses spent in fights, summed at each kill.
    #[serde(default)]
    pub weapon_spent: u64,
    #[serde(default)]
    pub angers: u32,
    #[serde(default)]
    pub fight: Option<Fight>,
    #[serde(default)]
    pub kills: u32,
    #[serde(default)]
    pub falls: u64,
    #[serde(default)]
    pub swings: u64,
    #[serde(default)]
    pub last_swing: Option<u64>,
    /// The mouth is sealed (a breach, a fight given up, the hunt over) and why.
    #[serde(default)]
    pub sealed: Option<String>,
    #[serde(default)]
    pub breaches: u32,
    /// The last enderman hit on the bot taken into account, server tick.
    #[serde(default)]
    pub hit_seen: u64,
    #[serde(default)]
    pub step_outs: u32,
    #[serde(default)]
    pub last_step_out: Option<u64>,
    #[serde(default)]
    pub step_outs_held: u32,
    #[serde(default)]
    pub pearls_seen: i32,
    #[serde(default)]
    pub picked: u32,
    #[serde(default)]
    pub last_pickup: Option<u64>,
    /// The collect in hand: the item and when it began; items given up.
    #[serde(default)]
    pub collect: Option<(i32, u64)>,
    #[serde(default)]
    pub collect_refused: Vec<i32>,
    /// A parked target's look from M runs until this tick.
    #[serde(default)]
    pub parked_until: Option<u64>,
    #[serde(default)]
    pub start_filler: i32,
    #[serde(default)]
    pub start_picks: i32,
    #[serde(default)]
    pub start_weapons: i32,
    /// The last row that acted, and when a standing row was last noted.
    #[serde(default)]
    pub row: String,
    #[serde(default)]
    pub noted: BTreeMap<String, u64>,
    #[serde(default)]
    pub weapon_short_noted: bool,
    /// The provoke row's target this tick: the one look `keep_look` lets stare.
    #[serde(skip)]
    pub provoking: Option<u32>,
}

/// An enderman whose creepy flag (index 17) is set: it has the bot as its target.
pub fn creepy(w: &World, id: i32) -> bool {
    w.entities
        .get(&id)
        .is_some_and(|e| matches!(e.metadata.get(&CREEPY_INDEX), Some(MetaValue::Bool(true))))
}

/// The angry endermen within 64, ticking or frozen (an angry frozen enderman is present: the
/// issue's rule), the nearest first; one that hit the bot within `defend::PROVOKED_TICKS` counts
/// as angry too (the nearest).
pub fn angry_endermen(w: &World) -> Vec<Sighting> {
    let all = endermen::endermen(w);
    let mut v: Vec<Sighting> = all
        .iter()
        .filter(|s| s.dist <= endermen::APPROACH_FROM && creepy(w, s.id))
        .copied()
        .collect();
    if v.is_empty()
        && defend::provoked(w, EntityKind::Enderman)
        && let Some(s) = all.iter().find(|s| s.dist <= endermen::APPROACH_FROM)
    {
        v.push(*s);
    }
    v
}

/// The eye of a body standing in a cell's middle.
pub fn eye_in(c: [i32; 3]) -> [f64; 3] {
    let p = Burrow::centre(c);
    [p[0], p[1] + physics::EYE_HEIGHT, p[2]]
}

fn eye_of(e: [f64; 3]) -> [f64; 3] {
    [e[0], e[1] + look::ENDERMAN_EYE, e[2]]
}

fn torso_of(e: [f64; 3]) -> [f64; 3] {
    [e[0], e[1] + TORSO, e[2]]
}

/// The calm ticking endermen the provoke row may take from an eye at `eye`: within
/// `provoke_min` to `provoke_max`, whose eye the eye sees past every block seen; the nearest
/// first.
pub fn provoke_candidates(w: &World, eye: [f64; 3], r: &PearlRules) -> Vec<Sighting> {
    provoke_candidates_from(w, eye, r.provoke_min, r)
}

/// `provoke_candidates` no nearer than `min`, eye to eye.
pub fn provoke_candidates_from(
    w: &World,
    eye: [f64; 3],
    min: f64,
    r: &PearlRules,
) -> Vec<Sighting> {
    endermen::endermen(w)
        .into_iter()
        .filter(|s| s.ticking && !creepy(w, s.id))
        .filter(|s| {
            let d = dist(eye, eye_of(s.pos));
            d >= min && d <= r.provoke_max
        })
        .filter(|s| look::clear(w, eye, eye_of(s.pos)))
        .collect()
}

fn dist(a: [f64; 3], b: [f64; 3]) -> f64 {
    (0..3).map(|i| (a[i] - b[i]).powi(2)).sum::<f64>().sqrt()
}

fn flat(a: [f64; 3], b: [f64; 3]) -> f64 {
    ((a[0] - b[0]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
}

/// Main inventory slots (9 to 44) with nothing in them.
pub fn free_slots(w: &World) -> usize {
    (9i16..=44)
        .filter(|s| !w.inventory.slots.contains_key(s))
        .count()
}

/// The weapon slots held (window 0's 9 to 44) with their damage and uses left.
fn weapons(w: &World) -> Vec<(i16, ItemKind, f64, i32)> {
    w.inventory
        .slots
        .iter()
        .filter(|(s, _)| (9..=44).contains(*s))
        .filter_map(|(s, (k, _))| {
            let wp = data::generated::item_props::weapon_of(*k)?;
            (wp.damage > 0.0).then(|| (*s, *k, wp.damage, w.inventory.uses_left(*s).unwrap_or(0)))
        })
        .collect()
}

/// The weapon to strike with: the hardest-hitting with more than `min` uses left, the most
/// worn of those first (a sword under `min` is switched at once). `None` with none.
pub fn weapon_slot(w: &World, min: i32) -> Option<i16> {
    weapons(w)
        .into_iter()
        .filter(|(_, _, _, u)| *u > min)
        .max_by(|a, b| a.2.total_cmp(&b.2).then(b.3.cmp(&a.3)).then(b.0.cmp(&a.0)))
        .map(|(s, _, _, _)| s)
}

/// Uses left on every weapon held, summed.
pub fn weapon_uses(w: &World) -> i32 {
    weapons(w).iter().map(|x| x.3).sum()
}

/// The next step to hold the weapon in `slot`: `None` once the selected hotbar slot is it.
fn hold_slot(w: &World, slot: i16) -> Option<world::Act> {
    if w.click.is_some() || w.window.is_some() {
        return None;
    }
    if (36..=44).contains(&slot) {
        let i = u8::try_from(slot - 36).ok()?;
        return (w.held != i).then_some(world::Act::Hold { slot: i });
    }
    // In the main inventory: a number-key swap into the selected hotbar slot, one click.
    Some(world::Act::Click {
        slot,
        button: i8::try_from(w.held).unwrap_or(0),
        mode: 2,
    })
}

/// The controls of a step to the middle of cell `c` from feet at `pos`, and whether it is there.
/// Sneaking for the last part: no overshoot, no step off an edge.
pub fn step_to(pos: [f64; 3], c: [i32; 3]) -> (Controls, bool) {
    let t = Burrow::centre(c);
    let d = flat(pos, t);
    if d < STEP_DONE {
        return (Controls::default(), true);
    }
    (
        Controls {
            forward: true,
            sneak: d < 0.7,
            yaw: yaw_to(pos, t),
            pitch: Some(0.0),
            jump: false,
        },
        false,
    )
}

/// Blocks behind B's middle (toward the back wall) the fight holds the body at: pressed against
/// the wall it rests 0.2 back. On a real server the bot drifted 0.14 toward M once its forward
/// key was let go, and the enderman's reach came within 0.23 of it, not 0.37 (P4's parity row
/// e04, parity 45531046147).
pub const HOLD_BACK: f64 = 0.1;

/// Between strikes, the body pressed back against B's back wall (facing it, sneaking: no stare
/// at a wall), unless it is already `HOLD_BACK` behind B's middle or the wall is missing; else
/// `c`.
pub fn press_back(w: &World, pos: [f64; 3], b: Burrow, c: Controls) -> Controls {
    let bc = Burrow::centre(b.b);
    let off = (pos[0] - bc[0]) * f64::from(b.dir[0]) + (pos[2] - bc[2]) * f64::from(b.dir[1]);
    let back = [b.b[0] - b.dir[0], b.b[1], b.b[2] - b.dir[1]];
    let wall = w
        .block_at(world::BlockPos::new(back[0], back[1], back[2]))
        .and_then(data::generated::shapes::collision)
        == Some(data::types::Collision::Full);
    if off <= -HOLD_BACK || !wall {
        return c;
    }
    Controls {
        forward: true,
        sneak: true,
        yaw: b.out_yaw() + 180.0,
        pitch: Some(0.0),
        jump: false,
    }
}

/// The look straight along `yaw` at `pitch`.
fn look(yaw: f64, pitch: f32) -> Controls {
    Controls {
        yaw,
        pitch: Some(pitch),
        ..Controls::default()
    }
}

fn arr(c: path::Cell) -> [i32; 3] {
    [c.0, c.1, c.2]
}

fn cells(v: &[[i32; 3]]) -> Vec<path::Cell> {
    v.iter().map(|c| (c[0], c[1], c[2])).collect()
}

/// What a burrow block's placing did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Placed {
    Building,
    Done,
    Cannot(&'static str),
}

impl Plan {
    /// The hunt's numbers: the goal table's `[pearls]`, else the defaults.
    fn pearl_rules(&self) -> PearlRules {
        self.table
            .as_ref()
            .map_or_else(PearlRules::default, |t| t.pearls.clone())
    }

    /// Whether the hunt runs: agent P2's `EnderPhase::Hunt` (its hand-over, "warped forest reached:
    /// on to the hunt"), and from then on.
    pub(crate) fn hunting(&mut self, _w: &World) -> bool {
        if !self.pearls.started && self.ender.phase == endermen::EnderPhase::Hunt {
            self.pearls.started = true;
        }
        self.pearls.started
    }

    /// The bot stands in its built burrow (B or M): the hunt's rows answer an angry enderman
    /// there, not `ender_threat`'s pocket.
    pub(crate) fn in_burrow(&self, w: &World) -> bool {
        let (Some(b), Some(body)) = (self.pearls.burrow, w.body) else {
            return false;
        };
        self.pearls.built && b.inside(arr(path::stand_cell(w, body.pos)))
    }

    /// A note once a `NOTE_EVERY` for a standing condition `key`.
    fn pearl_note(&mut self, tick: u64, key: &str, what: String, why: String) {
        if self
            .pearls
            .noted
            .get(key)
            .is_some_and(|t| tick < t + NOTE_EVERY)
        {
            return;
        }
        self.pearls.noted.insert(key.to_owned(), tick);
        self.note(tick, what, why);
    }

    /// A note once for `key` in the run.
    fn pearl_note_once(&mut self, tick: u64, key: &str, what: String, why: String) {
        if self.pearls.noted.contains_key(key) {
            return;
        }
        self.pearls.noted.insert(key.to_owned(), tick);
        self.note(tick, what, why);
    }

    /// The row that acts this tick, noted when it changes (strike and hold are one fight).
    fn row(&mut self, tick: u64, id: &str, why: impl FnOnce() -> String) {
        let fight = |r: &str| r == "strike" || r == "hold";
        if self.pearls.row == id || (fight(&self.pearls.row) && fight(id)) {
            self.pearls.row = id.to_owned();
            return;
        }
        self.pearls.row = id.to_owned();
        let why = why();
        self.note(tick, format!("pearls row {id}"), why);
    }

    /// Drops a task the hunt's rows cut in on (a walk, a base step), saying so.
    fn cut_task(&mut self, tick: u64, row: &str) {
        if !matches!(self.task, Task::Idle) {
            let name = self.task.name();
            self.task = Task::Idle;
            self.note(
                tick,
                format!("{name} cut by {row}"),
                "an enderman's row comes first".to_owned(),
            );
        }
    }

    /// The hunt's call site (one fenced call in `step_inner`, after the always-rows): rows 1 to 5
    /// and 7 to 9 as controls; rows 6 and 7's tasks begun through `pearls_task`. `None` lets the
    /// task in hand run, or (out of the hunt) everything else.
    pub(crate) fn pearls_step(&mut self, w: &World, tick: u64) -> Option<Controls> {
        if !self.hunting(w) {
            return None;
        }
        self.pearls.provoking = None;
        let body = w.body?;
        let r = self.pearl_rules();
        self.pearls_account(w, tick);
        let feet = arr(path::stand_cell(w, body.pos));
        // 1. pearls-done.
        if craft::count(w, ItemKind::EnderPearl) >= r.pearls_wanted {
            return Some(self.row_done(w, tick, &r, feet));
        }
        let angry = angry_endermen(w);
        let inside = self.in_burrow(w);
        if let Some(b) = self.pearls.burrow.filter(|_| inside) {
            // 2. breach.
            let hurt = w
                .hits
                .iter()
                .filter(|h| {
                    h.cause == Some(EntityKind::Enderman) && h.at_tick > self.pearls.hit_seen
                })
                .map(|h| h.at_tick)
                .max();
            if let Some(at) = hurt {
                self.pearls.hit_seen = at;
                self.pearls.breaches += 1;
                self.pearls.sealed = Some("hurt by an enderman".to_owned());
                self.note(
                    tick,
                    "breach: hurt by an enderman".to_owned(),
                    format!(
                        "at {feet:?} (B {:?}), health {:?}; {} breaches in the run; seal M",
                        b.b, w.player.health, self.pearls.breaches
                    ),
                );
            }
            let near = angry
                .iter()
                .find(|s| s.dist <= r.breach_near)
                .filter(|_| feet != b.b);
            if let Some(s) = near
                && self.pearls.sealed.is_none()
            {
                self.pearls.breaches += 1;
                self.pearls.sealed =
                    Some(format!("enderman {} within {:.1} out of B", s.id, s.dist));
                self.note(
                    tick,
                    format!(
                        "breach: enderman {} {:.1} off with the bot out of B",
                        s.id, s.dist
                    ),
                    format!(
                        "at {feet:?}; seal M from B; {} breaches",
                        self.pearls.breaches
                    ),
                );
            }
            if self.pearls.sealed.is_some() || self.pearls.over {
                self.cut_task(tick, "breach");
                return Some(self.row_breach(w, tick, &r, b, feet, &angry));
            }
            // 3 and 4. strike and hold.
            if !angry.is_empty() {
                self.cut_task(tick, "hold");
                return Some(self.row_fight(w, tick, &r, b, feet, &angry));
            }
            self.pearls.fight = None;
        }
        // 5. collect (it may take the bot out of the burrow onto O and beside it).
        if let Some(b) = self.pearls.burrow.filter(|_| self.pearls.built)
            && angry.is_empty()
            && !endermen::endermen(w)
                .iter()
                .any(|s| dist(s.pos, Burrow::centre(b.o)) <= r.near_o)
            && let Some(c) = self.row_collect(w, tick, &r, b, feet)
        {
            self.cut_task(tick, "collect");
            return Some(c);
        }
        // Out of the burrow but by its line (back from a collect): home by steps.
        if let Some(b) = self.pearls.burrow.filter(|_| self.pearls.built)
            && !b.inside(feet)
            && flat(body.pos, Burrow::centre(b.b)) <= r.collect_reach + 1.0
            && matches!(self.task, Task::Idle)
            && (self.pearls.provoke_from.is_none() || !angry.is_empty())
        {
            // An angry one with the bot out (a provoke from O): straight back to B.
            if !angry.is_empty() {
                self.pearls.provoke_from = None;
                return Some(step_to(body.pos, b.b).0);
            }
            let next = if feet == b.o { b.m } else { b.o };
            let next = if flat(body.pos, Burrow::centre(b.m)) < 0.8 {
                b.b
            } else {
                next
            };
            return Some(step_to(body.pos, next).0);
        }
        // Out of a built burrow with an angry enderman: issue 17's sealed pocket where the bot
        // stands (`ender_threat`), not a walk or a base step in the open.
        if !angry.is_empty() && !inside {
            return None;
        }
        // Rows 6 and 7 that are tasks (the base, a walk to B, a dig to reopen the mouth).
        if !matches!(self.task, Task::Idle) {
            return None;
        }
        let provoking_out = self.pearls.provoke_from.is_some();
        if self
            .pearls
            .burrow
            .is_some_and(|b| b.inside(feet) || (provoking_out && feet == b.o))
            && let Some(c) = self.row_standing(w, tick, &r, feet)
        {
            return Some(c);
        }
        if tick < self.retry_at {
            return Some(Controls::default());
        }
        match self.pearls_task(w, tick) {
            // The base waits (an act in flight, nothing seen): stand, and ask again next tick.
            Some(Task::Idle) => Some(Controls::default()),
            Some(t) => {
                self.begin(t);
                None
            }
            None => {
                self.retry_at = tick + crate::RETRY_TICKS;
                Some(Controls::default())
            }
        }
    }

    /// Kills, pearls picked up, a fight's health falls: counted every tick of the hunt.
    fn pearls_account(&mut self, w: &World, tick: u64) {
        if self.pearls.hunt_start.is_none() {
            self.pearls.hunt_start = Some(tick);
            self.pearls.start_filler = endermen::filler_held(w);
            self.pearls.start_picks = endermen::pickaxe_uses_left(w);
            self.pearls.start_weapons = weapon_uses(w);
            self.pearls.pearls_seen = craft::count(w, ItemKind::EnderPearl);
            self.pearls.hit_seen = w
                .hits
                .iter()
                .filter(|h| h.cause == Some(EntityKind::Enderman))
                .map(|h| h.at_tick)
                .max()
                .unwrap_or(0);
            self.note(
                tick,
                "pearls hunt begins".to_owned(),
                format!(
                    "health {:?}, food {:?}; weapon uses {}, pickaxe uses {}, filler {}, pearls {}, free slots {}",
                    w.player.health,
                    w.player.food,
                    self.pearls.start_weapons,
                    self.pearls.start_picks,
                    self.pearls.start_filler,
                    self.pearls.pearls_seen,
                    free_slots(w)
                ),
            );
        }
        let pearls = craft::count(w, ItemKind::EnderPearl);
        if pearls > self.pearls.pearls_seen {
            let n = pearls - self.pearls.pearls_seen;
            self.pearls.picked += u32::try_from(n).unwrap_or(0);
            self.pearls.last_pickup = Some(tick);
            self.pearls.collect = None;
            self.note(
                tick,
                format!("pearl picked ({pearls} held)"),
                format!(
                    "{} picked in the run, {} kills",
                    self.pearls.picked, self.pearls.kills
                ),
            );
        }
        self.pearls.pearls_seen = pearls;
        // Ticking endermen seen, every 20 ticks, kept 600 (the mouth's side at site choice).
        if tick.is_multiple_of(20) {
            self.pearls
                .sightings
                .retain(|(t, _)| tick < t + SIGHTINGS_KEPT);
            for s in endermen::endermen(w)
                .iter()
                .filter(|s| s.ticking && s.dist <= 64.0)
            {
                self.pearls.sightings.push((tick, s.pos));
            }
        }
        // A provoke that angered its target (the fight's rows take it from B on).
        if let Some((id, at)) = self.pearls.provoke
            && creepy(w, id)
        {
            self.pearls.back_since = Some(tick);
            self.pearls.provoke = None;
            self.pearls.provoke_from = None;
            self.pearls.angers += 1;
            self.note(
                tick,
                format!("provoked enderman {id}"),
                format!(
                    "its creepy flag after {} server ticks; {} angers of {} provokes; back to B",
                    w.session.server_ticks.saturating_sub(at),
                    self.pearls.angers,
                    self.pearls.provokes
                ),
            );
        }
        let Some(mut f) = self.pearls.fight.clone() else {
            return;
        };
        let e = w.entities.get(&f.id);
        let health = e.and_then(world::Entity::health);
        if let (Some(now), Some(was)) = (health, f.last_health)
            && now < was
        {
            f.falls += 1;
            f.last_fall = tick;
            f.fall_noted = false;
            self.pearls.falls += 1;
        }
        f.last_health = health.or(f.last_health);
        let dead = e.is_some_and(|e| e.dead) || health.is_some_and(|h| h <= 0.0);
        if dead {
            self.pearls.kills += 1;
            self.pearls.last_kill = Some(tick);
            self.pearls.moves_no_kill = 0;
            self.pearls.fight = None;
            let held = craft::count(w, ItemKind::EnderPearl).max(1);
            // Weapon uses spent fight by fight (the base makes swords after the hunt begins: race
            // 45537373222 wrote -245 a pearl from the run's start).
            self.pearls.weapon_spent +=
                u64::try_from((f.weapon_before - weapon_uses(w)).max(0)).unwrap_or(0);
            let weapon = i32::try_from(self.pearls.weapon_spent).unwrap_or(i32::MAX);
            let filler = (self.pearls.start_filler - endermen::filler_held(w)).max(0);
            let picks = (self.pearls.start_picks - endermen::pickaxe_uses_left(w)).max(0);
            self.note(
                tick,
                format!("enderman {} killed", f.id),
                format!(
                    "{} hits ({} swings) in {} ticks; kill {} of the run; health {:?}; per pearl ({} held): hits {:.1}, weapon uses {:.1}, filler {:.1}, pickaxe uses {:.1}",
                    f.falls,
                    f.swings,
                    tick.saturating_sub(f.since),
                    self.pearls.kills,
                    w.player.health,
                    craft::count(w, ItemKind::EnderPearl),
                    self.pearls.falls as f64 / f64::from(held),
                    f64::from(weapon) / f64::from(held),
                    f64::from(filler) / f64::from(held),
                    f64::from(picks) / f64::from(held),
                ),
            );
            return;
        }
        if e.is_none() || !creepy(w, f.id) && !defend::provoked(w, EntityKind::Enderman) {
            self.pearls.fight = None;
            self.note(
                tick,
                format!(
                    "fight with enderman {} over: {}",
                    f.id,
                    if e.is_none() { "gone" } else { "calm" }
                ),
                format!(
                    "{} hits in {} ticks, no kill",
                    f.falls,
                    tick.saturating_sub(f.since)
                ),
            );
            return;
        }
        self.pearls.fight = Some(f);
    }

    /// Row 1: the wanted pearls held. The mouth sealed from B, eyes down, the aim met.
    fn row_done(&mut self, w: &World, tick: u64, r: &PearlRules, feet: [i32; 3]) -> Controls {
        self.row(tick, "pearls-done", || {
            format!("{} pearls held", r.pearls_wanted)
        });
        self.pearls.fight = None;
        if let Some(b) = self.pearls.burrow.filter(|_| self.pearls.built)
            && b.inside(feet)
        {
            if feet == b.m {
                let body_pos = w.body.map_or([0.0; 3], |x| x.pos);
                return step_to(body_pos, b.b).0;
            }
            let mouth: Vec<[i32; 3]> = b.mouth().into_iter().collect();
            if self.place_burrow(w, tick, &mouth, "seal") == Placed::Building {
                return look(b.out_yaw(), EYES_DOWN);
            }
        }
        if !self.met {
            self.met = true;
            self.note(
                tick,
                "pearls met".to_owned(),
                format!(
                    "{} pearls held after {} kills; health {:?}",
                    craft::count(w, ItemKind::EnderPearl),
                    self.pearls.kills,
                    w.player.health
                ),
            );
        }
        look(w.player.yaw.into(), EYES_DOWN)
    }

    /// Row 2: the mouth sealed from B, eyes down, until no angry enderman ticks within
    /// `calm_range` (then the build row reopens it); for good once the hunt is over.
    fn row_breach(
        &mut self,
        w: &World,
        tick: u64,
        r: &PearlRules,
        b: Burrow,
        feet: [i32; 3],
        angry: &[Sighting],
    ) -> Controls {
        let why = self
            .pearls
            .sealed
            .clone()
            .unwrap_or_else(|| "the hunt is over".to_owned());
        self.row(tick, "breach", || {
            format!("{why}; seal M from B, eyes down")
        });
        let pos = w.body.map_or([0.0; 3], |x| x.pos);
        if feet != b.b {
            return step_to(pos, b.b).0;
        }
        let mouth: Vec<[i32; 3]> = b.mouth().into_iter().collect();
        let placed = self.place_burrow(w, tick, &mouth, "seal");
        let near = angry
            .iter()
            .filter(|s| s.ticking && s.dist <= r.calm_range)
            .count();
        if placed == Placed::Done && near == 0 && !self.pearls.over {
            self.pearls.sealed = None;
            self.pearls.fight = None;
            self.note(
                tick,
                "breach: sealed and calm".to_owned(),
                format!(
                    "no angry enderman ticking within {}; the mouth reopens",
                    r.calm_range
                ),
            );
        } else {
            self.pearl_note(
                tick,
                "breach",
                "breach: sealed".to_owned(),
                format!(
                    "{why}; {near} angry ticking within {}; {placed:?}",
                    r.calm_range
                ),
            );
        }
        look(b.out_yaw(), EYES_DOWN)
    }

    /// Rows 3 and 4: the fight from B. A full charge on a target in reach: the torso, attack then
    /// swing; else hold in B looking level at the mouth at torso height. The fight's invariants:
    /// a health fall every `fall_window` in reach, a parked target looked at from M after
    /// `parked_ticks`, the fight given up with the mouth sealed after `fight_cap`.
    fn row_fight(
        &mut self,
        w: &World,
        tick: u64,
        r: &PearlRules,
        b: Burrow,
        feet: [i32; 3],
        angry: &[Sighting],
    ) -> Controls {
        let pos = w.body.map_or([0.0; 3], |x| x.pos);
        let target = self
            .pearls
            .fight
            .as_ref()
            .map(|f| f.id)
            .filter(|id| angry.iter().any(|s| s.id == *id))
            .or_else(|| angry.first().map(|s| s.id));
        let Some(id) = target else {
            return look(b.out_yaw(), 0.0);
        };
        let Some(s) = angry.iter().find(|s| s.id == id).copied() else {
            return look(b.out_yaw(), 0.0);
        };
        if self.pearls.fight.as_ref().is_none_or(|f| f.id != id) {
            self.pearls.fight = Some(Fight {
                id,
                since: tick,
                last_fall: tick,
                last_health: w.entities.get(&id).and_then(world::Entity::health),
                weapon_before: weapon_uses(w),
                ..Fight::default()
            });
            self.note(
                tick,
                format!("fight enderman {id} from B"),
                format!(
                    "{:.1} off, ticking {}; {} angry; health {:?}; weapon uses {}",
                    s.dist,
                    s.ticking,
                    angry.len(),
                    w.player.health,
                    weapon_uses(w)
                ),
            );
        }
        let since = self.pearls.fight.as_ref().map_or(tick, |f| f.since);
        if tick.saturating_sub(since) > r.fight_cap {
            self.pearls.sealed = Some(format!(
                "fight with enderman {id} over {} ticks",
                r.fight_cap
            ));
            self.note(
                tick,
                format!("fight given up: enderman {id}"),
                format!(
                    "{} ticks; the mouth sealed until it calms or wanders",
                    tick.saturating_sub(since)
                ),
            );
            return look(b.out_yaw(), EYES_DOWN);
        }
        self.pearls.parked_until = None;
        // On the burrow's top: 2 or so from B over the roof, out of reach (P4's note).
        if b.on_top(arr(path::feet_cell(s.pos))) {
            self.pearl_note_once(
                tick,
                &format!("roof {id}"),
                format!("enderman {id} on the roof"),
                format!(
                    "at ({:.1}, {:.1}, {:.1}) over the burrow at {:?}; held in B",
                    s.pos[0], s.pos[1], s.pos[2], b.b
                ),
            );
        }
        if feet != b.b {
            return step_to(pos, b.b).0;
        }
        if let Some(at) = self.pearls.back_since.take() {
            self.note(
                tick,
                format!(
                    "back in B {} ticks after the creepy flag",
                    tick.saturating_sub(at)
                ),
                format!("enderman {id} {:.1} off", s.dist),
            );
        }
        self.row(tick, "hold", || {
            format!("enderman {id} angry, {:.1} off", s.dist)
        });
        let eye = [pos[0], pos[1] + physics::EYE_HEIGHT, pos[2]];
        let reach = defend::box_distance_of(eye, s.pos, EntityKind::Enderman);
        let torso = torso_of(s.pos);
        let clear = look::clear(w, eye, torso);
        let in_reach = reach <= defend::STRIKE_REACH && clear;
        // The weapon in the hand from the fight's start: a change of the held item resets the
        // attack strength (race 45534591345 lane 1: the first swing went out the tick after the
        // switch, `attack.weak`, 1.2 damage).
        if let Some(slot) = weapon_slot(w, r.weapon_min_uses)
            && let Some(a) = hold_slot(w, slot)
        {
            if self.acts.is_empty() {
                self.acts.push(a);
                self.pearls.last_swing = Some(tick);
            }
            return press_back(w, pos, b, look(b.out_yaw(), 0.0));
        }
        // Close by the mouth and out of reach: where, once a fight (the closed loop's question).
        if !in_reach && s.dist <= 3.5 {
            self.pearl_note_once(
                tick,
                &format!("close {id}"),
                format!("enderman {id} close but out of reach"),
                format!(
                    "it at ({:.2}, {:.2}, {:.2}), the bot at ({:.2}, {:.2}, {:.2}), O {:?}; eye to box {reach:.2}, line to the torso clear {clear}",
                    s.pos[0], s.pos[1], s.pos[2], pos[0], pos[1], pos[2], b.o
                ),
            );
        }
        if !in_reach {
            let f = self.pearls.fight.get_or_insert_with(Fight::default);
            let out = *f.out_since.get_or_insert(tick);
            if tick.saturating_sub(out) >= r.parked_ticks {
                f.out_since = Some(tick);
                // Never a look from M with an angry one present: the bot in M was hit once by one
                // that came back at an angle (P4's parity run 45535279839); B never was.
                {
                    self.note(
                        tick,
                        format!("enderman {id} parked: wait in B"),
                        format!(
                            "out of reach {} ticks, {:.1} off; {} angry",
                            r.parked_ticks,
                            s.dist,
                            angry.len()
                        ),
                    );
                }
            }
            // Level at the mouth, at torso height over O; pressed against the back wall.
            let o = Burrow::centre(b.o);
            let (yaw, pitch) = look_at(eye, [o[0], o[1] + TORSO, o[2]]);
            return press_back(w, pos, b, look(yaw, pitch));
        }
        if let Some(f) = self.pearls.fight.as_mut() {
            // Into reach: the health-fall window starts here, not at the provoke.
            if f.out_since.take().is_some() {
                f.last_fall = tick;
            }
            if tick.saturating_sub(f.last_fall) >= r.fall_window && !f.fall_noted {
                f.fall_noted = true;
                let (falls, swings) = (f.falls, f.swings);
                self.note(
                    tick,
                    format!("fight: no health fall on enderman {id}"),
                    format!(
                        "{} ticks in reach with no fall; {falls} hits, {swings} swings",
                        r.fall_window
                    ),
                );
            }
        }
        let (yaw, pitch) = look_at(eye, torso);
        #[allow(clippy::cast_possible_truncation)]
        let staring = look::stares(w, eye, yaw as f32, pitch).is_some();
        let Some(slot) = weapon_slot(w, r.weapon_min_uses) else {
            if !self.pearls.weapon_short_noted {
                self.pearls.weapon_short_noted = true;
                self.note(
                    tick,
                    "no weapon over its minimum".to_owned(),
                    format!(
                        "no weapon with more than {} uses: hold in B, the base step makes more",
                        r.weapon_min_uses
                    ),
                );
            }
            return look(yaw, pitch);
        };
        if let Some(a) = hold_slot(w, slot) {
            if self.acts.is_empty() {
                self.acts.push(a);
            }
            return look(yaw, pitch);
        }
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let ready = (crate::hunt::swing_ticks(w) * crate::dig::slowdown(w)).ceil() as u64 + 1;
        let charged = self
            .pearls
            .last_swing
            .is_none_or(|t| tick.saturating_sub(t) >= ready);
        if charged && !staring && self.acts.is_empty() && w.click.is_none() {
            self.row(tick, "strike", String::new);
            self.pearls.last_swing = Some(tick);
            self.pearls.swings += 1;
            self.swings_sent += 1;
            if let Some(f) = self.pearls.fight.as_mut() {
                f.swings += 1;
            }
            // The attack first, then the swing: swing-first made every hit about 1 damage
            // (clojurecraft).
            self.acts.push(world::Act::Attack { entity: id });
            self.acts.push(world::Act::Swing);
            return look(yaw, pitch);
        }
        press_back(w, pos, b, look(yaw, pitch))
    }

    /// Whether a step out of B may be taken now (the bounce counter): one a `step_out_every`,
    /// or any after a kill or a pickup since the last. Counted, and noted when held back.
    fn step_out_ok(&mut self, tick: u64, r: &PearlRules) -> bool {
        let last = self.pearls.last_step_out;
        // A step out begun is walked to its end (a step is some 15 ticks sneaking): the closed
        // loop held the bot in B for good when every tick of one step counted as another.
        if last.is_some_and(|l| tick < l + STEP_OUT_TICKS) {
            return true;
        }
        let event = self
            .pearls
            .last_kill
            .max(self.pearls.last_pickup)
            .zip(last)
            .is_some_and(|(e, l)| e >= l);
        let ok = last.is_none_or(|l| tick >= l + r.step_out_every) || event;
        if ok {
            self.pearls.last_step_out = Some(tick);
            self.pearls.step_outs += 1;
        } else {
            self.pearls.step_outs_held += 1;
            self.pearl_note(
                tick,
                "bounce",
                "step out held: the bounce counter".to_owned(),
                format!(
                    "{} steps out, {} held back; one a {} ticks without a kill or pickup",
                    self.pearls.step_outs, self.pearls.step_outs_held, r.step_out_every
                ),
            );
        }
        ok
    }

    /// Row 5: a pearl on the ground within `collect_range` of O, a free slot: step to M, then
    /// to it within `collect_reach` of B; given up after `collect_ticks`.
    fn row_collect(
        &mut self,
        w: &World,
        tick: u64,
        r: &PearlRules,
        b: Burrow,
        feet: [i32; 3],
    ) -> Option<Controls> {
        if free_slots(w) == 0 {
            self.pearl_note(
                tick,
                "full",
                "no slot for a pearl".to_owned(),
                "every main slot full: the junk drop first".to_owned(),
            );
            return None;
        }
        let o = Burrow::centre(b.o);
        let bc = Burrow::centre(b.b);
        let items: Vec<(i32, [f64; 3])> = w
            .entities
            .iter()
            .filter(|(id, e)| {
                e.item().is_some_and(|(k, _)| k == ItemKind::EnderPearl)
                    && !self.pearls.collect_refused.contains(id)
            })
            .map(|(id, e)| (*id, e.pos))
            .filter(|(_, p)| dist(*p, o) <= r.collect_range)
            .collect();
        let near = items
            .iter()
            .filter(|(_, p)| flat(*p, bc) <= r.collect_reach && (p[1] - bc[1]).abs() <= 1.5)
            .min_by(|a, b2| {
                flat(a.1, bc)
                    .total_cmp(&flat(b2.1, bc))
                    .then(a.0.cmp(&b2.0))
            })
            .copied();
        let Some((id, p)) = near else {
            if let Some((id, _)) = self.pearls.collect.take() {
                self.note(
                    tick,
                    format!("pearl {id} gone or out of reach"),
                    "back to B".to_owned(),
                );
            }
            return None;
        };
        let since = match self.pearls.collect {
            Some((c, at)) if c == id => at,
            _ => {
                self.pearls.collect = Some((id, tick));
                self.note(
                    tick,
                    format!("collect pearl {id}"),
                    format!(
                        "{:.1} from B; none angry; {} free slots",
                        flat(p, bc),
                        free_slots(w)
                    ),
                );
                tick
            }
        };
        if tick.saturating_sub(since) > r.collect_ticks {
            self.pearls.collect = None;
            self.pearls.collect_refused.push(id);
            self.note(
                tick,
                format!("collect pearl {id} given up"),
                format!("{} ticks; it stays", r.collect_ticks),
            );
            return None;
        }
        self.row(tick, "collect", || format!("pearl {id} on the ground"));
        let pos = w.body.map_or([0.0; 3], |x| x.pos);
        let m = Burrow::centre(b.m);
        // From B, M first (the line out of the mouth); from M, straight at it.
        if feet == b.b && flat(p, m) > 0.5 {
            return Some(step_to(pos, b.m).0);
        }
        Some(step_to(pos, arr(path::feet_cell(p))).0)
    }

    /// Rows 7 (the immediate parts: build and mend from B), 8 and 9 for a bot in its burrow.
    fn row_standing(
        &mut self,
        w: &World,
        tick: u64,
        r: &PearlRules,
        feet: [i32; 3],
    ) -> Option<Controls> {
        let b = self.pearls.burrow?;
        let pos = w.body.map_or([0.0; 3], |x| x.pos);
        // 7. a block missing: built or mended from B.
        let missing = burrow::missing(w, &b);
        if self.pearls.built {
            for c in &missing {
                if !self.pearls.taken.contains(c) {
                    self.pearls.taken.push(*c);
                    self.note(
                        tick,
                        format!("burrow block taken at {c:?}"),
                        "a built burrow cell is open with no dig of ours: mended from B".to_owned(),
                    );
                }
            }
        }
        if missing.is_empty() {
            self.pearls.taken.clear();
        }
        if !missing.is_empty() {
            let (c, there) = step_to(pos, b.b);
            if feet != b.b || !there {
                return Some(c);
            }
            self.row(tick, "build", || {
                format!("{} burrow blocks missing at {:?}", missing.len(), b.b)
            });
            return match self.place_burrow(w, tick, &missing, "burrow") {
                Placed::Building | Placed::Done => Some(look(b.out_yaw(), 0.0)),
                Placed::Cannot(why) => {
                    self.refuse_site(tick, b, why);
                    Some(Controls::default())
                }
            };
        }
        if !self.pearls.built {
            self.pearls.built = true;
            self.pearls.since = Some(tick);
            self.note(
                tick,
                format!("burrow built at {:?}", b.b),
                format!(
                    "mouth toward ({}, {}), {} blocks placed",
                    b.dir[0], b.dir[1], self.pearls.site_blocks
                ),
            );
        }
        // The mouth closed (a seal of a breach that is over): reopened by `pearls_task`'s dig.
        if b.mouth().iter().any(|c| {
            w.block_at(world::BlockPos::new(c[0], c[1], c[2]))
                .and_then(data::generated::shapes::collision)
                .is_some_and(|k| k != data::types::Collision::Empty)
        }) {
            return None;
        }
        // The hunt's clocks: a kill a `kill_window`, else the burrow moves.
        let last = self.pearls.last_kill.or(self.pearls.since).unwrap_or(tick);
        let idle = tick.saturating_sub(last);
        let calm_near = !provoke_candidates(w, eye_in(b.m), r).is_empty();
        if idle >= r.kill_window || (idle >= r.burrow_idle_ticks && !calm_near) {
            self.move_burrow(tick, r, b, idle, calm_near);
            return Some(Controls::default());
        }
        // Any enderman within `near_o` of O, calm or not: the bot is in B, never out (P4's real
        // runs on the solid shell: B never hit in 9 rows over 4 runs; M hit in 2 of 4 runs, once
        // each, by one that came round angry).
        let o = Burrow::centre(b.o);
        if let Some(s) = endermen::endermen(w)
            .into_iter()
            .find(|s| dist(s.pos, o) <= r.near_o)
        {
            self.pearls.provoke = None;
            self.pearls.provoke_from = None;
            self.row(tick, "hold", || {
                format!("enderman {} {:.1} from O: in B", s.id, dist(s.pos, o))
            });
            if feet != b.b {
                return Some(step_to(pos, b.b).0);
            }
            return Some(press_back(w, pos, b, look(b.out_yaw(), 0.0)));
        }
        // 8. provoke.
        if let Some(c) = self.row_provoke(w, tick, r, b, feet) {
            return Some(c);
        }
        // 9. wait: in M, eyes level.
        self.row(tick, "wait", || {
            format!("burrow at {:?}; no calm ticking enderman in sight", b.b)
        });
        self.pearl_note(
            tick,
            "wait",
            "waiting at the burrow".to_owned(),
            format!(
                "{idle} ticks since the last kill or the build; nearest enderman {}; {} kills, {} pearls",
                endermen::endermen(w)
                    .first()
                    .map_or("none".to_owned(), |s| format!("{} at {:.1}, ticking {}", s.id, s.dist, s.ticking)),
                self.pearls.kills,
                craft::count(w, ItemKind::EnderPearl)
            ),
        );
        if feet == b.o {
            self.pearls.provoke_from = None;
            return Some(step_to(pos, b.m).0);
        }
        if feet == b.b && self.step_out_ok(tick, r) {
            return Some(step_to(pos, b.m).0);
        }
        if feet == b.m {
            let (c, _) = step_to(pos, b.m);
            if c.forward {
                return Some(c);
            }
        }
        Some(look(b.out_yaw(), 0.0))
    }

    /// Row 8: a calm ticking enderman in sight from M: its eye until the creepy flag, at most
    /// `provoke_ticks` server ticks; `provoke_tries` tries an enderman, then skipped a while.
    fn row_provoke(
        &mut self,
        w: &World,
        tick: u64,
        r: &PearlRules,
        b: Burrow,
        feet: [i32; 3],
    ) -> Option<Controls> {
        let server = w.session.server_ticks;
        let pos = w.body.map_or([0.0; 3], |x| x.pos);
        if let Some((id, at)) = self.pearls.provoke {
            let from = self.pearls.provoke_from.unwrap_or(b.m);
            let seen = w
                .entities
                .get(&id)
                .is_some_and(|e| !e.dead && look::clear(w, eye_in(from), eye_of(e.pos)));
            if server.saturating_sub(at) > r.provoke_ticks || !seen || feet != from {
                self.pearls.provoke = None;
                self.pearls.provoke_from = None;
                let t = self.provoke_try(id, tick, r);
                self.note(
                    tick,
                    format!("provoke enderman {id}: no anger"),
                    format!(
                        "{} server ticks, in sight {seen}; try {t} of {}",
                        server.saturating_sub(at),
                        r.provoke_tries
                    ),
                );
            } else if let Some(e) = w.entities.get(&id) {
                let eye = [pos[0], pos[1] + physics::EYE_HEIGHT, pos[2]];
                let (yaw, pitch) = look_at(eye, eye_of(e.pos));
                self.pearls.provoking = u32::try_from(id).ok();
                return Some(look(yaw, pitch));
            }
        }
        // From M first; with none in sight there, from O at one far enough that the step back
        // to B (two cells, some 10 ticks) is made before it can come (the 26.1.2 jar: within 16 it
        // walks; past 16 it teleports after 15 goal ticks). M's mouth sees one way only: three
        // dev-forest bots waited 3,300 to 4,000 ticks with endermen 15 to 30 off out of its sight
        // (race 45534591345).
        let skipped = |s: &Sighting| {
            self.pearls
                .tries
                .iter()
                .any(|(i, _, until)| *i == s.id && tick < *until)
        };
        let from_m = provoke_candidates(w, eye_in(b.m), r)
            .into_iter()
            .find(|s| !skipped(s))
            .map(|s| (s, b.m));
        let from_o = || {
            provoke_candidates_from(w, eye_in(b.o), r.provoke_out_min, r)
                .into_iter()
                .find(|s| !skipped(s))
                .map(|s| (s, b.o))
        };
        let (cand, from) = from_m.or_else(from_o)?;
        self.row(tick, "provoke", || {
            format!("enderman {} calm and ticking {:.1} off", cand.id, cand.dist)
        });
        if feet == b.b {
            if self.step_out_ok(tick, r) {
                self.pearls.provoke_from = Some(from);
                return Some(step_to(pos, from).0);
            }
            return Some(look(b.out_yaw(), 0.0));
        }
        self.pearls.provoke_from = Some(from);
        let (c, there) = step_to(pos, from);
        if !there {
            return Some(c);
        }
        if !burrow::missing(w, &b).is_empty() {
            self.pearls.provoke_from = None;
            return None;
        }
        self.pearls.provoke = Some((cand.id, server));
        self.pearls.provokes += 1;
        if !self.pearls.provoked.contains(&cand.id) {
            self.pearls.provoked.push(cand.id);
        }
        self.note(
            tick,
            format!("provoke enderman {}", cand.id),
            format!(
                "from {} at {from:?}, {:.1} off, ticking; up to {} server ticks; burrow checked whole",
                if from == b.m { "M" } else { "O" },
                cand.dist,
                r.provoke_ticks
            ),
        );
        let eye = [pos[0], pos[1] + physics::EYE_HEIGHT, pos[2]];
        let (yaw, pitch) = look_at(eye, eye_of(cand.pos));
        self.pearls.provoking = u32::try_from(cand.id).ok();
        Some(look(yaw, pitch))
    }

    /// One more provoke try on `id`; past `provoke_tries` it is skipped `provoke_skip_ticks`.
    fn provoke_try(&mut self, id: i32, tick: u64, r: &PearlRules) -> u32 {
        let i = match self.pearls.tries.iter().position(|t| t.0 == id) {
            Some(i) => i,
            None => {
                self.pearls.tries.push((id, 0, 0));
                self.pearls.tries.len() - 1
            }
        };
        self.pearls.tries[i].1 += 1;
        let n = self.pearls.tries[i].1;
        if n >= r.provoke_tries {
            self.pearls.tries[i] = (id, 0, tick + r.provoke_skip_ticks);
            self.note(
                tick,
                format!("enderman {id} skipped"),
                format!(
                    "{n} provokes with no anger; skipped {} ticks",
                    r.provoke_skip_ticks
                ),
            );
        }
        n
    }

    /// The burrow moved: no kill in `kill_window`, or no calm enderman in `burrow_idle_ticks`.
    /// Two moves without a kill end the hunt (the mouth sealed, held in B).
    fn move_burrow(&mut self, tick: u64, r: &PearlRules, b: Burrow, idle: u64, calm: bool) {
        if self.pearls.moves_no_kill >= r.moves_without_kill {
            self.pearls.over = true;
            self.pearls.sealed = Some("the hunt is over".to_owned());
            self.note(
                tick,
                "pearls hunt over".to_owned(),
                format!(
                    "{} moves without a kill; {} kills, {} pearls; sealed in",
                    self.pearls.moves_no_kill, self.pearls.kills, self.pearls.pearls_seen
                ),
            );
            return;
        }
        self.pearls.moves += 1;
        self.pearls.moves_no_kill += 1;
        self.pearls.moved_from.push(b.b);
        self.pearls.burrow = None;
        self.pearls.built = false;
        self.pearls.since = None;
        self.pearls.last_kill = None;
        self.pearls.site_at = None;
        self.note(
            tick,
            "burrow moved: none came".to_owned(),
            format!(
                "{idle} ticks since the last kill or the build (calm in sight {calm}); move {} ({} without a kill); a new site {}+ off",
                self.pearls.moves, self.pearls.moves_no_kill, r.burrow_move
            ),
        );
    }

    /// A site given up: its B refused, the burrow forgotten.
    fn refuse_site(&mut self, tick: u64, b: Burrow, why: &str) {
        self.pearls.refused.push(b.b);
        self.pearls.burrow = None;
        self.pearls.built = false;
        self.pearls.site_at = None;
        self.note(
            tick,
            format!("burrow site at {:?} given up", b.b),
            format!("{why}; {} sites refused", self.pearls.refused.len()),
        );
    }

    /// The next block of `cells` (a burrow's missing shell, the mouth's seal) from where the
    /// body stands, one a `BUILD_EVERY`, with a block no enderman can take.
    fn place_burrow(&mut self, w: &World, tick: u64, want: &[[i32; 3]], what: &str) -> Placed {
        // The cells no support block may go into: B, M and O two high (O three), less the cells
        // wanted (a seal fills M).
        let keep: Vec<[i32; 3]> = self
            .pearls
            .burrow
            .map(|b| {
                let mut v = Vec::new();
                for c in [b.b, b.m, b.o] {
                    for y in 0..3 {
                        v.push([c[0], c[1] + y, c[2]]);
                    }
                }
                v.retain(|c| !want.contains(c) && !b.shell().contains(c));
                v
            })
            .unwrap_or_default();
        let keep_open = &keep[..];
        let Some(body) = w.body else {
            return Placed::Building;
        };
        let missing: Vec<[i32; 3]> = want
            .iter()
            .filter(|c| {
                w.block_at(world::BlockPos::new(c[0], c[1], c[2]))
                    .and_then(data::generated::shapes::collision)
                    != Some(data::types::Collision::Full)
            })
            .copied()
            .collect();
        if missing.is_empty() {
            return Placed::Done;
        }
        let Some((cell, click, face)) =
            cover::fill_block(w, &cells(&missing), body.pos, &cells(keep_open))
        else {
            return Placed::Cannot("nothing to click for a burrow block");
        };
        if !body.on_ground {
            return Placed::Building;
        }
        if what == "burrow" && self.pearls.site_blocks >= self.pearl_rules().build_max {
            return Placed::Cannot("burrow blocks over the cap");
        }
        let Some(item) = BURROW_BLOCKS
            .iter()
            .copied()
            .find(|k| craft::count(w, *k) > 0)
        else {
            return Placed::Cannot("no burrow block held");
        };
        if w.window.is_some() && w.click.is_none() && self.acts.is_empty() {
            self.acts.push(world::Act::CloseWindow);
            return Placed::Building;
        }
        if w.window.is_some()
            || w.click.is_some()
            || !self.acts.is_empty()
            || tick < self.pearls.place_at
        {
            return Placed::Building;
        }
        match hold::toward(w, item) {
            hold::Hold::Held => {
                self.pearls.place_at = tick + BUILD_EVERY;
                self.pearls.blocks += 1;
                if what == "burrow" {
                    self.pearls.site_blocks += 1;
                }
                self.acts.push(world::Act::UseOn {
                    pos: world::BlockPos::new(click.0, click.1, click.2),
                    face,
                });
                let mended = if self.pearls.built && what == "burrow" {
                    "burrow mended at"
                } else {
                    "block at"
                };
                self.note(
                    tick,
                    format!("{what} {mended} {cell:?}"),
                    format!(
                        "{item:?} on {click:?} face {face}; {} left to place; {} in the run",
                        missing.len(),
                        self.pearls.blocks
                    ),
                );
                Placed::Building
            }
            hold::Hold::Act(a) => {
                self.acts.push(a);
                Placed::Building
            }
            hold::Hold::Wait => Placed::Building,
            hold::Hold::Missing => Placed::Cannot("no burrow block held"),
        }
    }

    /// A walk to a seen warped nylium cell with room over it, `FLOOR_MIN` to `SITE_RADIUS` off and
    /// at least `burrow_move` from every burrow moved from, not tried before: the forest's own
    /// floor, where the next site search starts. The nearest first, two tried a call.
    fn walk_to_forest_floor(
        &mut self,
        w: &World,
        tick: u64,
        feet: [i32; 3],
        r: &PearlRules,
    ) -> Option<Task> {
        let dim = w.dimension.as_ref()?.name.clone();
        let (cx, cz) = (feet[0] >> 4, feet[2] >> 4);
        let mut cells: Vec<(i32, [i32; 3])> = Vec::new();
        for dx in -2..=2 {
            for dz in -2..=2 {
                for c in w
                    .memory
                    .indexed_in_column(&dim, cx + dx, cz + dz, data::BlockKind::WarpedNylium)
                    .unwrap_or_default()
                {
                    let at = [c.0, c.1 + 1, c.2];
                    let d2 = (at[0] - feet[0]).pow(2) + (at[2] - feet[2]).pow(2);
                    let far = self.pearls.moved_from.iter().all(|m| {
                        (m[0] - at[0]).pow(2) + (m[2] - at[2]).pow(2) >= r.burrow_move.pow(2)
                    });
                    if (FLOOR_MIN * FLOOR_MIN..=burrow::SITE_RADIUS * burrow::SITE_RADIUS)
                        .contains(&d2)
                        && far
                        && !self.pearls.floor_tried.contains(&at)
                        && w.block_at(world::BlockPos::new(at[0], at[1], at[2]))
                            .and_then(data::generated::shapes::collision)
                            == Some(data::types::Collision::Empty)
                    {
                        cells.push((d2 + (at[1] - feet[1]).pow(2), at));
                    }
                }
            }
        }
        cells.sort();
        for (_, at) in cells.into_iter().take(2) {
            self.pearls.floor_tried.push(at);
            let c = (at[0], at[1], at[2]);
            let mut walk = Walk::new(c, Goal::Near { pos: c, range: 2.0 }, Arrive::Goal, tick);
            if walk.route_now(w, tick) {
                self.walks += 1;
                self.note(
                    tick,
                    format!("walk to the forest floor at {at:?}"),
                    "no burrow site here; a site is looked for from there".to_owned(),
                );
                return Some(Task::Explore(walk));
            }
        }
        None
    }

    /// The hunt's tasks, for a planner with no task in hand (agent P2's `EnderPhase::Hunt` calls
    /// this; so does `pearls_step`): the base (row 6), then the build row's walks and digs: a site
    /// found and walked to, the mouth reopened, a walk away after a move. `None` with nothing to
    /// begin (the standing rows act).
    pub fn pearls_task(&mut self, w: &World, tick: u64) -> Option<Task> {
        let body = w.body?;
        // 6. base.
        if let Some(t) = self.base_task(w, tick) {
            return Some(t);
        }
        let r = self.pearl_rules();
        let feet = arr(path::stand_cell(w, body.pos));
        if self.pearls.burrow.is_none() {
            if self.pearls.site_at.is_some_and(|t| tick < t + r.site_every) {
                return None;
            }
            self.pearls.site_at = Some(tick);
            let mut avoid = self.pearls.refused.clone();
            avoid.extend(self.pearls.moved_from.iter().copied());
            // A refused site is never tried again (within 1); a moved-from one is left by
            // `burrow_move` (a new place for spawns).
            let seen: Vec<[f64; 3]> = self.pearls.sightings.iter().map(|(_, p)| *p).collect();
            let (found, why) = if self.pearls.moved_from.is_empty() {
                burrow::site_toward(w, &w.memory, feet, &avoid, 1, &seen)
            } else {
                let (f, mut y) = burrow::site_toward(
                    w,
                    &w.memory,
                    feet,
                    &self.pearls.moved_from,
                    r.burrow_move,
                    &seen,
                );
                match f {
                    Some(b) if !self.pearls.refused.contains(&b.b) => (Some(b), y),
                    _ => {
                        y.push(("refused before", 1));
                        (None, y)
                    }
                }
            };
            let reasons = why
                .iter()
                .map(|(k, n)| format!("{n} {k}"))
                .collect::<Vec<_>>()
                .join(", ");
            let Some(b) = found else {
                if self
                    .pearls
                    .site_none_noted
                    .is_none_or(|t| tick >= t + NOTE_EVERY)
                {
                    self.pearls.site_none_noted = Some(tick);
                    self.note(
                        tick,
                        "no burrow site".to_owned(),
                        format!(
                            "none within {} of {feet:?}; refused: {reasons}",
                            burrow::SITE_RADIUS
                        ),
                    );
                }
                // Walk on to warped nylium seen (the forest's floor), never off into another biome
                // (race 45534591345 lane 0 walked away from its feet on a cap into the basalt
                // deltas, 71 blocks bridged, and died to a magma cube).
                if let Some(t) = self.walk_to_forest_floor(w, tick, feet, &r) {
                    return Some(t);
                }
                let from = self.pearls.moved_from.last().copied().unwrap_or(feet);
                let goal = Goal::Away {
                    pos: (from[0], from[1], from[2]),
                    range: f64::from(r.burrow_move + 8),
                };
                let mut walk = Walk::far_to((from[0], from[1], from[2]), goal, tick);
                if walk.route_now(w, tick) {
                    self.walks += 1;
                    self.note(
                        tick,
                        "walk on for a burrow site".to_owned(),
                        format!("away from {from:?}"),
                    );
                    return Some(Task::Explore(walk));
                }
                return self.explore_walk(w, tick, "no burrow site and no walk away");
            };
            self.pearls.burrow = Some(b);
            self.pearls.built = false;
            self.pearls.walks = 0;
            self.pearls.site_blocks = 0;
            self.note(
                tick,
                format!(
                    "burrow site at {:?}, mouth toward ({}, {})",
                    b.b, b.dir[0], b.dir[1]
                ),
                format!(
                    "nearest the feet {feet:?} within {}; {} of {} endermen seen lately on the mouth's side; refused: {reasons}",
                    burrow::SITE_RADIUS,
                    burrow::side_score(b.b, b.dir, &seen),
                    seen.len()
                ),
            );
        }
        let b = self.pearls.burrow?;
        if !b.inside(feet) {
            if self.pearls.walks >= r.walk_tries {
                self.refuse_site(tick, b, "no walk to B");
                return None;
            }
            self.pearls.walks += 1;
            let c = (b.b[0], b.b[1], b.b[2]);
            let mut walk = Walk::new(c, Goal::Block { pos: c }, Arrive::Goal, tick);
            if walk.route_now(w, tick) {
                self.walks += 1;
                self.note(
                    tick,
                    format!("walk to the burrow at {:?}", b.b),
                    format!("walk {} of {}", self.pearls.walks, r.walk_tries),
                );
                return Some(Task::Explore(walk));
            }
            self.note(
                tick,
                format!("no way to the burrow at {:?}", b.b),
                format!("try {} of {}", self.pearls.walks, r.walk_tries),
            );
            return None;
        }
        // The mouth closed after a breach: reopened, a pickaxe use a block.
        if self.pearls.sealed.is_none() && burrow::missing(w, &b).is_empty() {
            for c in b.mouth() {
                let shut = w
                    .block_at(world::BlockPos::new(c[0], c[1], c[2]))
                    .and_then(data::generated::shapes::collision)
                    .is_some_and(|k| k != data::types::Collision::Empty);
                if shut {
                    self.note(
                        tick,
                        format!("burrow reopened: dig {c:?}"),
                        format!(
                            "the seal is over; {} pickaxe uses left",
                            endermen::pickaxe_uses_left(w)
                        ),
                    );
                    return Some(Task::Dig(crate::dig::Dig::new(w, (c[0], c[1], c[2]), tick)));
                }
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn burrow_blocks_are_none_an_enderman_can_take() {
        let holdable = data::generated::tags::block_tag("enderman_holdable").unwrap_or(&[]);
        assert!(!holdable.is_empty(), "the tag is generated");
        for item in BURROW_BLOCKS {
            let block = data::BlockKind::from_name(item.name().unwrap_or_default());
            assert!(block.is_some_and(|b| !holdable.contains(&b)), "{item:?}");
        }
        assert!(burrow::holdable(data::BlockKind::WarpedNylium));
        assert!(burrow::holdable(data::BlockKind::Dirt));
    }

    #[test]
    fn a_step_ends_at_the_cells_middle() {
        let (c, there) = step_to([0.5, 64.0, 0.5], [1, 64, 0]);
        assert!(!there && c.forward);
        assert!((c.yaw - -90.0).abs() < 1e-6, "east is yaw -90: {}", c.yaw);
        let (_, there) = step_to([1.45, 64.0, 0.55], [1, 64, 0]);
        assert!(there);
    }
}
