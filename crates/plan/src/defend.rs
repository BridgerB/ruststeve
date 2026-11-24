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
    /// Strikes that took health off the foe (issue 16: the case file's swings landed).
    #[serde(default)]
    pub landed: u32,
    /// What the fight saw, for its note (the porting guide's CombatFrame, summed over the fight):
    /// the bot's health when it began, the foe's highest point over the bot's eye, and the ticks
    /// a blaze foe was charging a volley.
    #[serde(default)]
    pub health_start: Option<f32>,
    #[serde(default)]
    pub max_dy: f64,
    #[serde(default)]
    pub charged_ticks: u32,
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
    /// A retreat from a shooter: the walk to a cell none of them sees, and the tick cover was last
    /// looked for (blaze-fight wall 3).
    #[serde(default)]
    pub cover: Option<crate::walk::Walk>,
    #[serde(default)]
    pub covered_at: Option<u64>,
    /// Cover cells walked to this retreat.
    #[serde(default)]
    pub cover_tried: Vec<crate::path::Cell>,
    /// Ticks stepped away from a shooter in touch, and ticks a swing was held for a neutral mob
    /// in the sweep (blaze-fight walls 4 and 5): in the fight's note.
    #[serde(default)]
    pub kept_off: u32,
    /// A dodge under way (until this tick, along this yaw), the fireballs already answered, and
    /// how many dodges the fight made (blaze-fight wall 7).
    #[serde(default)]
    pub dodging: Option<(u64, f64)>,
    #[serde(default)]
    pub dodged: Vec<i32>,
    #[serde(default)]
    pub dodges: u32,
    /// Of those, the fireballs whose velocity the server had sent.
    #[serde(default)]
    pub dodges_known: u32,
    /// Ticks a ready swing has waited for the fall, and the swings sent falling (wall 8).
    #[serde(default)]
    pub crit_wait: u32,
    #[serde(default)]
    pub falling_swings: u32,
    /// Ticks in a row a shooter's fight has had no way to its foe.
    #[serde(default)]
    pub no_way: u32,
    #[serde(default)]
    pub sweeps_held: u32,
    /// What the fight did that the ledger should hold, taken by the planner each tick.
    #[serde(default)]
    pub notes: Vec<String>,
}

/// A blaze's box (vanilla `EntityType.BLAZE`: 0.6 wide, 1.8 high).
const SHOOTER_HALF_WIDTH: f64 = 0.3;
const SHOOTER_HEIGHT: f64 = 1.8;
/// How many cover cells are tried for a route a look.
const COVER_TRIES: usize = 4;

/// From `eye` to the nearest point of the box of a mob standing at `pos`: what the client's
/// reach is measured to (its crosshair ray meets the box), where `middle` is 0.9 up the box.
pub fn box_distance(eye: [f64; 3], pos: [f64; 3]) -> f64 {
    let near = [
        eye[0].clamp(pos[0] - SHOOTER_HALF_WIDTH, pos[0] + SHOOTER_HALF_WIDTH),
        eye[1].clamp(pos[1], pos[1] + SHOOTER_HEIGHT),
        eye[2].clamp(pos[2] - SHOOTER_HALF_WIDTH, pos[2] + SHOOTER_HALF_WIDTH),
    ];
    distance(eye, near)
}

/// Whether a cell stops a line of sight: a full block seen, or a cell never seen exposed (strict
/// no X-ray: what was not seen counts as rock).
fn blocks_sight(w: &World, c: crate::path::Cell) -> bool {
    crate::cast::seen(w, c)
        .is_none_or(|k| data::generated::shapes::collision(k) == Some(data::types::Collision::Full))
}

/// Whether the line from `from` to `to` crosses no sight-stopping cell.
pub fn sight(w: &World, from: [f64; 3], to: [f64; 3]) -> bool {
    !crate::scoop::ray_cells(from, to)
        .into_iter()
        .any(|c| blocks_sight(w, c))
}

/// A shooter of this kind strikes by touch too, within this of its target, feet to feet: a blaze
/// hits once a second inside 2 blocks (vanilla `BlazeAttackGoal`: `distanceToSqr < 4`), for 3.6 in
/// iron armour. The fight keeps this far off and strikes from its own reach of 3.
pub const KEEP_OFF: f64 = 2.4;
/// A sweep's reach round the mob struck (the game's sweep takes every mob within a block of the
/// target's box and 3 of the player), with a margin.
pub const SWEEP_NEAR: f64 = 2.0;

/// Whether a step along `yaw` is one a shooter's fight may take: the cells 0.9 and 1.7 ahead hold
/// no lava and no fire at the feet or head, and have a seen floor right under them or one block
/// down, with no lava there. Stricter than `safe_ahead` (which allows a three-block drop): two
/// bots walked off fortress bridges into lava closing in on a blaze and a skeleton, and one
/// walked back into the fire it had just left 13 times (race 45336326999). An unseen cell is
/// rock, as everywhere.
pub fn safe_step(w: &World, pos: [f64; 3], yaw: f64) -> bool {
    let rad = yaw.to_radians();
    let seen = |c: crate::path::Cell| crate::cast::seen(w, c);
    let bad = |c: crate::path::Cell| {
        matches!(
            seen(c),
            Some(data::BlockKind::Lava | data::BlockKind::Fire | data::BlockKind::SoulFire)
        )
    };
    let floor = |c: crate::path::Cell| {
        seen(c).is_none_or(|k| {
            data::generated::shapes::collision(k) != Some(data::types::Collision::Empty)
        })
    };
    [0.9, 1.7].into_iter().all(|far| {
        let c =
            crate::path::feet_cell([pos[0] - rad.sin() * far, pos[1], pos[2] + rad.cos() * far]);
        let under = (c.0, c.1 - 1, c.2);
        let under2 = (c.0, c.1 - 2, c.2);
        !bad(c)
            && !bad((c.0, c.1 + 1, c.2))
            && !bad(under)
            && (floor(under) || (!bad(under2) && floor(under2)))
    })
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

/// Whether a cell has a fall beside it: a neighbour column with no floor (seen, or never seen)
/// within three blocks under the cell's own. The top of a bridge's rail is such a cell: a bot
/// took cover on one, was struck there and fell to its death (race 45343424571, at
/// (127, 62, 165)).
pub fn edge(w: &World, c: crate::path::Cell) -> bool {
    let floor = |n: crate::path::Cell| {
        crate::cast::seen(w, n).is_none_or(|k| {
            data::generated::shapes::collision(k) != Some(data::types::Collision::Empty)
        })
    };
    [(1, 0), (-1, 0), (0, 1), (0, -1)]
        .into_iter()
        .any(|(dx, dz)| !(1..=4).any(|down| floor((c.0 + dx, c.1 - down, c.2 + dz))))
}

/// Fire in the cell 0.9 ahead along `yaw`, at the feet or the head.
fn fire_ahead(w: &World, pos: [f64; 3], yaw: f64) -> bool {
    let rad = yaw.to_radians();
    let c = crate::path::feet_cell([pos[0] - rad.sin() * 0.9, pos[1], pos[2] + rad.cos() * 0.9]);
    [c, (c.0, c.1 + 1, c.2)].into_iter().any(|c| {
        matches!(
            crate::cast::seen(w, c),
            Some(data::BlockKind::Fire | data::BlockKind::SoulFire)
        )
    })
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
        .filter(|(id, e)| {
            !e.dead
                && hostile(e.kind)
                && (!rules.neutral.contains(&e.kind) || provoked(w, e.kind))
                // A foe a fight never struck nor was hurt by (a spider in a pit 4.5 under the eye,
                // fought 96 times: blaze wall 15) waits, unless its kind hits the bot.
                && (!ignored.contains(id)
                    || (provoked(w, e.kind) && !rules.ranged.contains(&e.kind)))
        })
        .map(|(id, e)| (distance(body.pos, e.pos), *id, e.kind))
        .filter(|(d, _, _)| *d <= rules.defend_radius)
        .min_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)))
        .map(|(_, id, k)| (id, k));
    if near.is_some() {
        return near;
    }
    // A shooter farther off only with the health for it (issue 16: blazes).
    if w.player.health.is_some_and(|h| h < rules.engage_health) {
        return None;
    }
    w.entities
        .iter()
        .filter(|(id, e)| !e.dead && rules.ranged.contains(&e.kind) && !ignored.contains(id))
        // One whose feet are more than a strike's reach over the eye is no fight from the floor
        // (a blaze 18.9 over the eye and a skeleton 16.2 over it were each held as the foe while
        // a wither skeleton or a fall killed the bot: race 45337951964).
        .filter(|(_, e)| e.pos[1] - (body.pos[1] + physics::EYE_HEIGHT) <= STRIKE_REACH)
        .map(|(id, e)| (distance(body.pos, e.pos), *id, e.kind))
        .filter(|(d, _, _)| *d <= rules.ranged_radius)
        .min_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)))
        .map(|(_, id, k)| (id, k))
}

/// Server ticks a hit keeps its dealer the pressing foe: two swings of a wither skeleton.
pub const PRESSING_TICKS: u64 = 40;

/// The foe a running defend should turn to while its own is farther than `defend_radius`: the
/// nearest live hostile within `defend_radius` (a neutral kind only once provoked), else the
/// nearest mob within `ranged_radius` of a kind that hit the bot in the last `PRESSING_TICKS`.
/// Three bots died inside a defend held on a blaze or a skeleton out of reach while a wither
/// skeleton struck them (race 45337951964); with no such defend in hand one was killed in four
/// swings, unhurt.
pub fn pressing(w: &World, rules: &SurvivalRules, current: i32) -> Option<(i32, EntityKind)> {
    let body = w.body?;
    let now = w.session.server_ticks;
    // Never a shooter: two blazes, each "the kind that just hit", were turned to in turn six
    // times a tick and the fight never swung (race 45343420833 lane 0: 0 swings, dead at 600).
    let near = |radius: f64, want: &dyn Fn(&world::Entity) -> bool| {
        w.entities
            .iter()
            .filter(|(id, e)| {
                **id != current
                    && !e.dead
                    && hostile(e.kind)
                    && !rules.ranged.contains(&e.kind)
                    && !rules.never_fight.contains(&e.kind)
                    && want(e)
            })
            .map(|(id, e)| (distance(body.pos, e.pos), *id, e.kind))
            .filter(|(d, _, _)| *d <= radius)
            .min_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)))
            .map(|(_, id, k)| (id, k))
    };
    near(rules.defend_radius, &|e| {
        !rules.neutral.contains(&e.kind) || provoked(w, e.kind)
    })
    .or_else(|| {
        near(rules.ranged_radius, &|e| {
            w.hits
                .iter()
                .any(|h| h.cause == Some(e.kind) && h.at_tick + PRESSING_TICKS >= now)
        })
    })
}

/// A fireball whose line passes this near the body's middle is stepped out of: clojurecraft
/// measured 212 small fireballs, and every one of the 24 that hit passed within 1.4 blocks; none
/// of 76 passing 1.5 or more away did (brain/oracle/architecture/cc-blaze-hunt.md).
pub const DODGE_CLEAR: f64 = 1.5;
/// Ticks a dodge walks: 8 at a walk are 1.7 blocks. A fireball arrives 6 to 15 ticks after it
/// appears.
pub const DODGE_TICKS: u64 = 8;
/// Fireballs farther than this are not looked at.
pub const DODGE_RANGE: f64 = 30.0;

/// The way to step out of a fireball's line, on the plane (a unit vector), or `None` when it
/// will pass `DODGE_CLEAR` or more from `mid` or is flying away. `velocity` unknown (the server
/// has sent none yet): it is taken to fly straight at `mid`. A line through the body is left to
/// the left of its heading.
pub fn dodge_side(mid: [f64; 3], ball: [f64; 3], velocity: Option<[f64; 3]>) -> Option<[f64; 2]> {
    let rel = [mid[0] - ball[0], mid[1] - ball[1], mid[2] - ball[2]];
    let v = velocity
        .filter(|v| (v[0] * v[0] + v[2] * v[2]).sqrt() > 0.05)
        .unwrap_or(rel);
    let vv = v[0] * v[0] + v[1] * v[1] + v[2] * v[2];
    if vv < 1e-9 {
        return None;
    }
    let t = (rel[0] * v[0] + rel[1] * v[1] + rel[2] * v[2]) / vv;
    if t < 0.0 {
        return None;
    }
    let off = [rel[0] - v[0] * t, rel[1] - v[1] * t, rel[2] - v[2] * t];
    if (off[0] * off[0] + off[1] * off[1] + off[2] * off[2]).sqrt() >= DODGE_CLEAR {
        return None;
    }
    let flat = (off[0] * off[0] + off[2] * off[2]).sqrt();
    if flat > 0.2 {
        return Some([off[0] / flat, off[2] / flat]);
    }
    let h = (v[0] * v[0] + v[2] * v[2]).sqrt();
    (h > 1e-9).then(|| [v[2] / h, -v[0] / h])
}

/// A jump leaves the ground for about 12 ticks and falls from the seventh: the jump goes this
/// many ticks before the swing is ready, and a ready swing waits at most `CRIT_WAIT` for the fall.
pub const CRIT_LEAD: u64 = 7;
pub const CRIT_WAIT: u32 = 8;

/// Ticks a shooter's fight stands with its foe out of reach and no step to take toward it (a
/// floor missing, fire or lava ahead, or the foe over the reach) before it is answered from
/// cover: a second.
pub const NO_WAY_TICKS: u32 = 20;

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

/// The stance a foe of `kind` is met with: a retreat from a `never_fight` kind, under the danger
/// line, and from a shooter at or under its own line (a fight begun at 9.9 with two blazes in
/// touch ended in 20 ticks: blaze-fight wall 4).
fn stance_for(w: &World, kind: EntityKind, rules: &SurvivalRules) -> Stance {
    let hurt = rules.ranged.contains(&kind)
        && w.player
            .health
            .is_some_and(|h| h <= rules.shooter_retreat_health);
    if rules.never_fight.contains(&kind) || in_danger(w, rules) || hurt {
        Stance::Retreat
    } else {
        Stance::Fight
    }
}

impl Defend {
    pub fn new(w: &World, foe: i32, kind: EntityKind, rules: &SurvivalRules, tick: u64) -> Self {
        let stance = stance_for(w, kind, rules);
        Self {
            foe,
            kind,
            stance,
            last_seen: w.entities.get(&foe).map_or([0.0; 3], |e| e.pos),
            faced_at: None,
            last_swing: None,
            swings: 0,
            landed: 0,
            health_start: w.player.health,
            max_dy: f64::NEG_INFINITY,
            charged_ticks: 0,
            last_health: None,
            last_hit_at: tick,
            started: tick,
            flee: None,
            fled_at: None,
            held_back: 0,
            cover: None,
            covered_at: None,
            cover_tried: Vec::new(),
            kept_off: 0,
            dodging: None,
            dodged: Vec::new(),
            crit_wait: 0,
            falling_swings: 0,
            dodges: 0,
            dodges_known: 0,
            no_way: 0,
            sweeps_held: 0,
            notes: Vec::new(),
        }
    }

    /// A retreat from a shooter (blaze-fight wall 3): done where none of them sees the bot, else
    /// along a route to the nearest cell none sees. `None` when there is no such cell or no way
    /// to one (the retreat then backs away as from any foe), and while the foe is within
    /// `defend_radius` with the bot unseen.
    fn cover_step(
        &mut self,
        w: &World,
        body: &physics::Body,
        near: bool,
        tick: u64,
    ) -> Option<(Controls, Option<Act>, DefendStatus)> {
        let still = Controls::default();
        let eyes = crate::cover::shooters(w);
        let feet = crate::path::feet_cell(body.pos);
        if !crate::cover::seen_body(w, &eyes, body.pos) {
            if near {
                return None;
            }
            self.notes.push(format!(
                "in cover at {feet:?}: none of {} shooters sees the bot",
                eyes.len()
            ));
            return Some((still, None, DefendStatus::Done));
        }
        if self.cover.is_none()
            && self
                .covered_at
                .is_none_or(|t| tick.saturating_sub(t) >= FLEE_RETRY_TICKS)
        {
            self.covered_at = Some(tick);
            // A cell walked to and still seen from (a wall's top the body slides off, a blaze
            // that moved) is not walked to again: one bot walked to (127, 62, 165) eleven times
            // in 220 ticks and burned there (race 45338756013 lane 2).
            for c in crate::cover::cover_cells(w, &eyes)
                .into_iter()
                .filter(|c| !self.cover_tried.contains(c) && !edge(w, *c))
                .take(COVER_TRIES)
            {
                let mut walk = crate::walk::Walk::new(
                    c,
                    crate::path::Goal::Block { pos: c },
                    crate::walk::Arrive::Goal,
                    tick,
                );
                if walk.route_now(w, tick) && walk.route == Some(crate::path::Status::Found) {
                    self.notes.push(format!(
                        "retreat to cover at {c:?} from {feet:?}: {} shooters see the bot",
                        eyes.len()
                    ));
                    self.cover = Some(walk);
                    break;
                }
            }
        }
        let walk = self.cover.as_mut()?;
        let (c, status) = walk.step(w, tick);
        if status != crate::walk::TaskStatus::Running {
            // There, or the walk failed: the next tick looks again, for another cell.
            self.cover_tried.push(walk.target);
            self.cover = None;
        }
        Some((c, None, DefendStatus::Running))
    }

    /// The nearest live mob of the foe's kind within `KEEP_OFF` of the feet: one in touch.
    fn in_touch(&self, w: &World, feet: [f64; 3]) -> Option<[f64; 3]> {
        w.entities
            .values()
            .filter(|e| !e.dead && e.kind == self.kind)
            .map(|e| (distance(feet, e.pos), e.pos))
            .filter(|(d, _)| *d < KEEP_OFF)
            .min_by(|a, b| a.0.total_cmp(&b.0))
            .map(|(_, p)| p)
    }

    /// The live mob of the foe's kind nearest the eye by its box, within `STRIKE_REACH` and in
    /// plain sight: the one a shooter's fight strikes, whichever it set out for (blaze-fight
    /// wall 2).
    fn in_reach(&self, w: &World, eye: [f64; 3]) -> Option<i32> {
        w.entities
            .iter()
            .filter(|(_, e)| !e.dead && e.kind == self.kind)
            .map(|(id, e)| (box_distance(eye, e.pos), *id, e.pos))
            .filter(|(d, _, pos)| *d <= STRIKE_REACH && sight(w, eye, middle(*pos)))
            .min_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)))
            .map(|(_, id, _)| id)
    }

    /// The yaw to walk this tick to step out of a fireball's line (blaze-fight wall 7): a dodge
    /// begun holds `DODGE_TICKS`; each fireball starts one dodge. The side is the one its line
    /// leaves the body on, or the other when no safe step lies that way.
    fn dodge(&mut self, w: &World, body: &physics::Body, tick: u64) -> Option<f64> {
        if let Some((until, yaw)) = self.dodging {
            if tick < until {
                return safe_step(w, body.pos, yaw).then_some(yaw);
            }
            self.dodging = None;
        }
        let mid = middle(body.pos);
        let seen = self.dodged.clone();
        let (id, side) = w
            .entities
            .iter()
            .filter(|(id, e)| !e.dead && e.kind == EntityKind::SmallFireball && !seen.contains(id))
            .filter(|(_, e)| distance(e.pos, mid) <= DODGE_RANGE)
            .filter_map(|(id, e)| {
                dodge_side(mid, e.pos, e.velocity)
                    .map(|s| (distance(e.pos, mid), *id, s, e.velocity.is_some()))
            })
            .min_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)))
            .map(|(_, id, s, known)| {
                self.dodges_known += u32::from(known);
                (id, s)
            })?;
        self.dodged.push(id);
        if self.dodged.len() > 64 {
            self.dodged.remove(0);
        }
        let yaw_of = |s: [f64; 2]| (-s[0]).atan2(s[1]).to_degrees().rem_euclid(360.0);
        let yaw = [side, [-side[0], -side[1]]]
            .into_iter()
            .map(yaw_of)
            .find(|y| safe_step(w, body.pos, *y))?;
        self.dodging = Some((tick + DODGE_TICKS, yaw));
        self.dodges += 1;
        Some(yaw)
    }

    /// One tick of the defence. A shooter's fight steps out of a fireball's line first, its
    /// strikes going on (the server judges a strike by where the bot stands, not where it looks).
    pub fn step(
        &mut self,
        w: &World,
        rules: &SurvivalRules,
        tick: u64,
    ) -> (Controls, Option<Act>, DefendStatus) {
        let (c, act, status) = self.step_inner(w, rules, tick);
        if status == DefendStatus::Running
            && self.stance == Stance::Fight
            && rules.ranged.contains(&self.kind)
            && let Some(body) = w.body
            && let Some(yaw) = self.dodge(w, &body, tick)
        {
            let c = Controls {
                forward: true,
                jump: body.horizontal_collision,
                sneak: false,
                yaw,
                pitch: c.pitch,
            };
            return (c, act, status);
        }
        (c, act, status)
    }

    fn step_inner(
        &mut self,
        w: &World,
        rules: &SurvivalRules,
        tick: u64,
    ) -> (Controls, Option<Act>, DefendStatus) {
        let still = Controls::default();
        let Some(body) = w.body else {
            return (still, None, DefendStatus::Done);
        };
        // A foe pressing the bot comes first: the defend turns to it while its own is farther
        // than `defend_radius` or gone (blaze-fight wall 6).
        let own = w
            .entities
            .get(&self.foe)
            .filter(|e| !e.dead)
            .map(|e| distance(body.pos, e.pos));
        if own.is_none_or(|d| d > rules.defend_radius)
            && let Some((id, kind)) = pressing(w, rules, self.foe)
        {
            self.notes.push(format!(
                "defend switched to {kind:?} {id} from {:?} {} at {:.1}: it presses the bot",
                self.kind,
                self.foe,
                own.unwrap_or(f64::NAN)
            ));
            self.foe = id;
            self.kind = kind;
            self.stance = stance_for(w, kind, rules);
            self.last_health = mob_health(w, id);
            self.last_hit_at = tick;
            self.faced_at = None;
            self.flee = None;
            self.cover = None;
            self.no_way = 0;
        }
        let Some(pos) = w.entities.get(&self.foe).filter(|e| !e.dead).map(|e| e.pos) else {
            return (still, None, DefendStatus::Done);
        };
        self.last_seen = pos;
        let eye_y = body.pos[1] + physics::EYE_HEIGHT;
        self.max_dy = self.max_dy.max(pos[1] - eye_y);
        if w.entities
            .get(&self.foe)
            .is_some_and(world::Entity::blaze_charged)
        {
            self.charged_ticks += 1;
        }
        let d = distance(body.pos, pos);
        let never = rules.never_fight.contains(&self.kind);
        let shooter = rules.ranged.contains(&self.kind);
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
                self.landed += 1;
            }
            self.last_health = health.or(self.last_health);
            // A shooter costs health while nothing lands, and its fire burns on after the hit:
            // less patience and an earlier line (blaze-fight walls 2 and 3).
            let patience = if shooter {
                rules.shooter_patience.min(NO_HIT_TICKS)
            } else {
                NO_HIT_TICKS
            };
            let hurt = shooter
                && w.player
                    .health
                    .is_some_and(|h| h <= rules.shooter_retreat_health);
            let stale = tick.saturating_sub(self.last_hit_at) > patience;
            if in_danger(w, rules)
                || hurt
                || stale
                || tick.saturating_sub(self.started) > FIGHT_TICKS
            {
                self.stance = Stance::Retreat;
                if shooter {
                    self.notes.push(format!(
                        "shooter fight given up: health {:?}, {} ticks since a blow landed (patience {patience}), foe {:.1} over the eye at {d:.1}",
                        w.player.health,
                        tick.saturating_sub(self.last_hit_at),
                        pos[1] - eye_y
                    ));
                }
            }
        }
        let target = middle(pos);
        let toward = yaw_to(body.pos, target);
        match self.stance {
            Stance::Retreat => {
                // From a shooter: out of every shooter's sight, not out of this one's reach.
                if shooter
                    && let Some(r) = self.cover_step(w, &body, d <= rules.defend_radius, tick)
                {
                    return r;
                }
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
                let out_of_reach = if shooter {
                    box_distance(eye, pos) > STRIKE_REACH || !sight(w, eye, target)
                } else {
                    distance(eye, target) > STRIKE_REACH
                };
                // A shooter's fight with its foe out of reach strikes whichever of the kind is in
                // reach, measured to the box: the one it set out for may sit on a roof while
                // another hovers beside it. A foe in reach is kept (no flipping between two).
                if shooter
                    && out_of_reach
                    && let Some(id) = self.in_reach(w, eye)
                    && id != self.foe
                {
                    self.notes
                        .push(format!("struck {id} in reach, not {}", self.foe));
                    self.foe = id;
                    self.last_health = mob_health(w, id);
                    self.faced_at = None;
                    let at = w.entities.get(&id).map_or(pos, |e| e.pos);
                    return (
                        Controls {
                            yaw: yaw_to(body.pos, middle(at)),
                            ..still
                        },
                        None,
                        DefendStatus::Running,
                    );
                }
                let face = Controls {
                    yaw: toward,
                    ..still
                };
                // A shooter in touch is stepped away from, the strikes going on: its touch costs
                // more than its fireballs (blaze-fight wall 4).
                let face = match self.in_touch(w, body.pos).filter(|_| shooter) {
                    Some(at) => {
                        let away = (yaw_to(body.pos, middle(at)) + 180.0) % 360.0;
                        self.kept_off += 1;
                        Controls {
                            forward: safe_step(w, body.pos, away),
                            jump: body.horizontal_collision,
                            yaw: away,
                            ..still
                        }
                    }
                    None => face,
                };
                if out_of_reach {
                    // Close in, facing it, unless lava or a deep drop lies between: then wait for
                    // it facing it (wall 29). A shooter's fight steps only onto a floor, never
                    // into fire (blaze-fight wall 4).
                    self.faced_at = None;
                    let safe = if shooter {
                        safe_step(w, body.pos, toward)
                    } else {
                        // Any fight's close-in stays out of fire: the fire escape and a defend
                        // on a wither skeleton cut each other every tick across a fire cell,
                        // 20 to 2.3 health with no swing (the fire arena, probe 45343436986).
                        safe_ahead(w, &body, toward) && !fire_ahead(w, body.pos, toward)
                    };
                    // A shooter with no step toward it, or over the reach, is answered from
                    // cover after a second, not stood under (blaze-fight wall 6).
                    if shooter && (!safe || pos[1] - eye_y > STRIKE_REACH) {
                        self.no_way += 1;
                        if self.no_way >= NO_WAY_TICKS {
                            self.stance = Stance::Retreat;
                            self.notes.push(format!(
                                "shooter fight given up: no way to {:?} {} for {} ticks, {:.1} over the eye at {d:.1}",
                                self.kind,
                                self.foe,
                                self.no_way,
                                pos[1] - eye_y
                            ));
                        }
                    }
                    if !safe {
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
                self.no_way = 0;
                if let Some(wp) = crate::hunt::weapon(w) {
                    match hold::toward(w, wp) {
                        Hold::Held | Hold::Missing => {}
                        Hold::Act(a) => return (face, Some(a), DefendStatus::Running),
                        Hold::Wait => return (face, None, DefendStatus::Running),
                    }
                }
                // No swing while a neutral mob stands in the sweep: a sweep that caught a
                // zombified piglin brought its blow of 5 and the bot's death (blaze-fight wall 5).
                let swept = shooter
                    && w.entities.values().any(|e| {
                        !e.dead
                            && rules.neutral.contains(&e.kind)
                            && distance(e.pos, pos) <= SWEEP_NEAR
                    });
                if swept {
                    self.sweeps_held += 1;
                    return (face, None, DefendStatus::Running);
                }
                let faced = *self.faced_at.get_or_insert(tick);
                let ready =
                    (crate::hunt::swing_ticks(w) * crate::dig::slowdown(w)).ceil() as u64 + 1;
                // A shooter is struck on the way down from a jump: a falling blow is a critical
                // one, half as much again (9 for 6 with an iron sword: a blaze's 20 in three
                // blows, not four; blaze-fight wall 8). The jump goes `CRIT_LEAD` ticks before
                // the swing is ready; a swing ready on the ground or on the way up waits for the
                // fall at most `CRIT_WAIT` ticks.
                let due = self
                    .last_swing
                    .map_or(0, |t| (t + ready).saturating_sub(tick));
                let falling = !body.on_ground && body.vel[1] < 0.0;
                let face = if shooter
                    && body.on_ground
                    && due <= CRIT_LEAD
                    && !edge(w, crate::path::feet_cell(body.pos))
                {
                    Controls { jump: true, ..face }
                } else {
                    face
                };
                if shooter
                    && tick > faced
                    && due == 0
                    && !body.on_ground
                    && !falling
                    && self.crit_wait < CRIT_WAIT
                {
                    self.crit_wait += 1;
                    return (face, None, DefendStatus::Running);
                }
                if tick > faced
                    && self
                        .last_swing
                        .is_none_or(|t| tick.saturating_sub(t) >= ready)
                {
                    self.crit_wait = 0;
                    self.falling_swings += u32::from(falling);
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
