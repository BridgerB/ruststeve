//! Overland to the fix (issue 19e, docs/problems/eyes-stronghold.md): the far walk on the
//! overworld's surface from where the bot came up to the eyes' fix, 1,280 to 2,816 blocks from
//! the origin for the first ring, by day, with a hut at night.
//!
//! - **Legs** of `leg_min` to `leg_max` blocks on the bearing to the fix, each to a cell on seen
//!   ground (the top block of a loaded column, a full block with two open cells over it: issue
//!   17's lesson, a leg aimed at rock is no leg), searched as a far walk that may bridge over
//!   water with held blocks (a river; `path::Cells::far_over_water`), `bridge_max` at most.
//! - **Water**: a run of water `ocean_wide` or wider on the bearing is an ocean: gone round, the
//!   bearing turned toward the side with more land seen (the fix's side on a tie). Known geography
//!   may add travel points round one (`around`, Bridger's ruling; empty until the map shows one).
//! - **Progress invariants**: a leg that comes no nearer its target in `leg_stall_ticks` turns the
//!   bearing `turn_deg` and notes it; the distance to the fix falls `progress_blocks` per
//!   `progress_ticks` of walking, else a note and a turn. Health under the danger line, with a
//!   way to heal, means a hut and a meal, never onward.
//! - **Night**: from `DUSK_MARGIN` before dusk a hut (`cover::hut_block`) round the body and a wait
//!   for the day; raw meat held is cooked in it when a furnace stands or is held; then the hut is
//!   opened on the bearing's side.
//! - **Done** once a frame is seen (the stronghold task's), or within `arrive` of the fix (the
//!   stronghold's search starts there).
//!
//! A ledger note per leg: "overland leg N: X blocks to the fix (err E), health H, food F, eyes K".

use data::{BlockKind, ItemKind};
use world::{BlockPos, World};

use crate::path::{self, Cell, Class, Goal};
use crate::walk::Walk;
use crate::{Plan, Task};

/// `[overland]` in `config/goals.toml`: every number has its source there.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct OverlandRules {
    pub leg_min: i32,
    pub leg_max: i32,
    pub leg_reach: f64,
    pub leg_stall_ticks: u64,
    pub leg_tries: u32,
    pub turn_deg: f64,
    pub progress_blocks: f64,
    pub progress_ticks: u64,
    pub ocean_wide: i32,
    pub bridge_keep: i64,
    pub bridge_max: usize,
    pub arrive: f64,
    pub hut_max: u32,
    pub heal_to: f32,
    pub heal_ticks: u64,
    pub scan_step: i32,
    /// Travel points (x, z) round a known ocean, in order (Bridger's geography ruling; each
    /// with its source in the config). Empty: none.
    pub around: Vec<[i32; 2]>,
    /// Development only (a dev start's race before the eyes' fix exists): a fix (x, z) used
    /// while none is held, noted as such. Never set on a merged branch.
    pub dev_fix: Option<[f64; 2]>,
}

impl Default for OverlandRules {
    fn default() -> Self {
        Self {
            leg_min: 64,
            leg_max: 128,
            leg_reach: 6.0,
            leg_stall_ticks: 600,
            leg_tries: 3,
            turn_deg: 30.0,
            progress_blocks: 32.0,
            progress_ticks: 1200,
            ocean_wide: 64,
            bridge_keep: 32,
            bridge_max: 40,
            arrive: 24.0,
            hut_max: 24,
            heal_to: 16.0,
            heal_ticks: 2400,
            scan_step: 8,
            around: Vec::new(),
            dev_fix: None,
        }
    }
}

/// One leg in hand.
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct OverLeg {
    pub n: u32,
    pub from: Cell,
    pub target: Cell,
    pub began: u64,
    /// The nearest the feet came to the target on the plane, and when.
    pub best: f64,
    pub best_at: u64,
    pub tries: u32,
}

/// Why a hut is up.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum HutWhy {
    #[default]
    Night,
    Hurt,
}

/// A hut in hand: why, when begun, blocks placed, closed, and the cook's and the exit's state.
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct Hut {
    pub why: HutWhy,
    pub began: u64,
    pub blocks: u32,
    pub at: u64,
    pub closed: bool,
    pub cooked: bool,
    pub furnace_placed: bool,
    pub health0: f32,
    /// The wall cells dug to leave, once the wait is over.
    pub leaving: bool,
    pub out_digs: u32,
}

/// What the overland holds between looks. Every field defaults.
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct OverlandState {
    /// The fix walked to: its point on the plane and its error (the larger of along and across);
    /// set from `Plan.fix` (the eyes', Q3) by the caller.
    pub fix: Option<([f64; 2], f64)>,
    /// `fix` is a walk point for the next throw (`Plan::eyes_way`), not the eyes' fix: its
    /// arrival starts no search.
    pub way: bool,
    /// A throw's spot moved off its point (`nudges` times) after the throw waited there from
    /// `wait_from`: the point it came from, for the next `eyes_way` to be measured against.
    pub way_base: Option<[f64; 2]>,
    pub nudges: u32,
    pub wait_from: Option<u64>,
    pub legs: u32,
    pub leg: Option<OverLeg>,
    /// The bearing's turn off the line to the fix, degrees (left positive), from stalls and
    /// oceans; eased back by `turn_deg` after each leg done.
    pub turn: f64,
    /// The side the last turn took: 1 left, -1 right.
    pub side: i8,
    /// The progress mark: the distance to the fix, and the walking tick it was set.
    pub mark: Option<(f64, u64)>,
    pub hut: Option<Hut>,
    pub huts: u32,
    /// Travel points (`around`) reached.
    pub around_done: usize,
    pub arrived: bool,
    pub over_noted: bool,
    pub turns: u32,
    pub stalls: u32,
    pub oceans: u32,
}

impl OverlandState {
    /// A new target: the eyes' fix (`way` false) or a throw's walk point (`way` true). The leg,
    /// the progress mark and the arrival go with the old one; the turn and the counts stay.
    pub fn retarget(&mut self, fix: ([f64; 2], f64), way: bool) {
        self.fix = Some(fix);
        self.way = way;
        self.leg = None;
        self.mark = None;
        self.arrived = false;
        self.wait_from = None;
    }
}

/// What a column's top shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Top {
    /// A standing cell on its top block.
    Land(Cell),
    Water,
    Lava,
}

/// The top of a loaded column within 48 of the feet's height: the first block from the top that
/// is not open air, a plant or a tree's (leaves and logs are looked through). A face to the sky is
/// an exposed face: no X-ray. `None` when the column is not loaded or nothing is found.
pub fn top(w: &World, x: i32, z: i32, near_y: i32) -> Option<Top> {
    if !w.columns.contains_key(&BlockPos::new(x, near_y, z).chunk()) {
        return None;
    }
    let tree = |k: BlockKind| {
        ["leaves", "logs"]
            .iter()
            .filter_map(|t| data::generated::tags::block_tag(t))
            .any(|ks| ks.contains(&k))
    };
    let mut cells = path::Cells::new(w);
    for y in (near_y - 48..=near_y + 48).rev() {
        let Some(k) = w.block_at(BlockPos::new(x, y, z)) else {
            continue;
        };
        if matches!(k, BlockKind::Air | BlockKind::CaveAir | BlockKind::VoidAir) || tree(k) {
            continue;
        }
        match cells.at((x, y, z)) {
            Class::Water => return Some(Top::Water),
            Class::Lava => return Some(Top::Lava),
            Class::Clear => {}
            Class::Solid => {
                let c = (x, y + 1, z);
                return cells.standable(c).then_some(Top::Land(c));
            }
            Class::Awkward | Class::Unknown => return None,
        }
    }
    None
}

/// The unit vector on the plane at `deg` degrees from +x toward +z.
fn unit(deg: f64) -> (f64, f64) {
    let r = deg.to_radians();
    (r.cos(), r.sin())
}

/// The bearing in degrees from `from` to `to` on the plane (+x 0, +z 90).
pub fn bearing(from: [f64; 2], to: [f64; 2]) -> f64 {
    (to[1] - from[1]).atan2(to[0] - from[0]).to_degrees()
}

/// The run of water on a bearing from `feet`: blocks from the first water-topped column to the
/// next land (or the last loaded column), in `step`s, up to `reach`; 0 with no water. Pure over
/// the world.
pub fn water_run(w: &World, feet: Cell, deg: f64, reach: i32, step: i32) -> i32 {
    let (ux, uz) = unit(deg);
    let mut start: Option<i32> = None;
    let mut last = 0;
    let mut d = step;
    while d <= reach {
        let x = feet.0 + (ux * f64::from(d)).round() as i32;
        let z = feet.2 + (uz * f64::from(d)).round() as i32;
        match top(w, x, z, feet.1) {
            Some(Top::Water) => {
                start.get_or_insert(d);
                last = d;
            }
            Some(Top::Land(_)) if start.is_some() => break,
            None if start.is_some() => break,
            Some(Top::Land(_) | Top::Lava) | None => {}
        }
        d += step;
    }
    start.map_or(0, |s| last - s + step)
}

/// Land columns seen on a bearing from `feet` out to `reach`.
fn land_on(w: &World, feet: Cell, deg: f64, reach: i32, step: i32) -> usize {
    let (ux, uz) = unit(deg);
    (1..=reach / step.max(1))
        .filter(|k| {
            let d = f64::from(k * step);
            let x = feet.0 + (ux * d).round() as i32;
            let z = feet.2 + (uz * d).round() as i32;
            matches!(top(w, x, z, feet.1), Some(Top::Land(_)))
        })
        .count()
}

/// A leg's target on a bearing: a standing cell on seen ground, the farthest of `len` down to
/// half `leg_min` in `step`s, straight on first, then `step` and twice it aside. Pure over the
/// world.
pub fn leg_target(w: &World, feet: Cell, deg: f64, len: i32, min: i32, step: i32) -> Option<Cell> {
    let (ux, uz) = unit(deg);
    let (px, pz) = (-uz, ux);
    let mut d = len;
    while d >= (min / 2).max(step) {
        for aside in [0, step, -step, 2 * step, -2 * step] {
            let x = feet.0 + (ux * f64::from(d) + px * f64::from(aside)).round() as i32;
            let z = feet.2 + (uz * f64::from(d) + pz * f64::from(aside)).round() as i32;
            if let Some(Top::Land(c)) = top(w, x, z, feet.1) {
                return Some(c);
            }
        }
        d -= step;
    }
    None
}

fn flat_d(a: Cell, b: Cell) -> f64 {
    f64::from(a.0 - b.0).hypot(f64::from(a.2 - b.2))
}

fn centre_of(c: Cell) -> [f64; 2] {
    [f64::from(c.0) + 0.5, f64::from(c.2) + 0.5]
}

/// Raw meat a furnace cooks into safe food (the cook's kinds).
const RAW: [ItemKind; 4] = [
    ItemKind::Beef,
    ItemKind::Porkchop,
    ItemKind::Mutton,
    ItemKind::Chicken,
];

impl Plan {
    pub(crate) fn overland_rules(&self) -> OverlandRules {
        self.table
            .as_ref()
            .map(|t| t.overland.clone())
            .unwrap_or_default()
    }

    fn overland_state(&self, w: &World, d: f64, err: f64) -> String {
        format!(
            "{d:.0} blocks to the fix (err {err:.0}), health {:?}, food {:?}, eyes {}",
            w.player.health,
            w.player.food,
            crate::craft::count(w, ItemKind::EnderEye)
        )
    }

    // >>> overland (Q4, issue 19e): the planner's one entry to the overland.
    /// The next step toward the fix (`OverlandState::fix`, set from the eyes' `Plan.fix`): a
    /// leg's far walk, a hut block, a wait, a meal's cook, a dig out of the hut, or `Task::Idle`
    /// to look again. `None` once a frame is seen (the stronghold task takes over), on arrival
    /// within `arrive` of the fix (the stronghold's search starts; `StrongholdState::at_fix` is
    /// set), with no fix, or outside the overworld.
    pub fn overland_task(&mut self, w: &World, tick: u64) -> Option<Task> {
        if w.dimension
            .as_ref()
            .is_none_or(|d| d.name != crate::stronghold::OVERWORLD)
        {
            return None;
        }
        let rules = self.overland_rules();
        if self.overland.fix.is_none()
            && let Some(p) = rules.dev_fix
        {
            self.overland.fix = Some((p, 0.0));
            self.note(
                tick,
                format!("overland dev fix {p:?}"),
                "development only: [overland] dev_fix, never a real race's".to_owned(),
            );
        }
        let (fix, err) = self.overland.fix?;
        let body = w.body?;
        let feet = path::stand_cell(w, body.pos);
        let here = centre_of(feet);
        let d = (fix[0] - here[0]).hypot(fix[1] - here[1]);
        // Frames seen: the stronghold task's.
        let frames = crate::stronghold::frames_near(w, self.stronghold_frame_chunks());
        if !frames.is_empty() && self.overland.hut.is_none() {
            if !self.overland.over_noted {
                self.overland.over_noted = true;
                self.note(
                    tick,
                    "overland over: frames seen".to_owned(),
                    format!(
                        "{} frames on record, the nearest {:?}; {}",
                        frames.len(),
                        frames.first(),
                        self.overland_state(w, d, err)
                    ),
                );
            }
            return None;
        }
        // The hut: night, or hurt with a way to heal.
        if let Some(t) = self.overland_hut(w, &rules, feet, fix, tick) {
            return Some(t);
        }
        // A throw's walk point is reached at a leg's own reach, not the fix's `arrive` (24): a
        // spot moved 20 off counted as reached on the spot, and the throw waited on the same
        // canopy through four moves (race 45729721263 lane 0).
        if self.overland.way && d <= rules.leg_reach {
            if !self.overland.arrived {
                self.overland.arrived = true;
                self.note(
                    tick,
                    "overland at the throw's spot".to_owned(),
                    format!(
                        "the eyes' next reading here; {}",
                        self.overland_state(w, d, err)
                    ),
                );
            }
            return None;
        }
        if !self.overland.way && d <= rules.arrive {
            if !self.overland.arrived {
                self.overland.arrived = true;
                self.stronghold.at_fix = Some([fix[0].floor() as i32, fix[1].floor() as i32]);
                self.note(
                    tick,
                    "overland arrived at the fix".to_owned(),
                    format!(
                        "no frame seen; the search starts here; {}",
                        self.overland_state(w, d, err)
                    ),
                );
            }
            return None;
        }
        // The progress invariant over the whole way: the fix nearer by `progress_blocks` every
        // `progress_ticks` of walking (a hut's time is not walking: its end resets the mark).
        match self.overland.mark {
            None => self.overland.mark = Some((d, tick)),
            Some((best, _)) if d <= best - rules.progress_blocks => {
                self.overland.mark = Some((d, tick));
            }
            Some((best, at)) if tick.saturating_sub(at) > rules.progress_ticks => {
                self.overland.mark = Some((d, tick));
                self.overland_turn(
                    &rules,
                    feet,
                    tick,
                    &format!(
                        "the fix came {:.0} nearer in {} walking ticks (under {:.0})",
                        best - d,
                        rules.progress_ticks,
                        rules.progress_blocks
                    ),
                );
            }
            Some(_) => {}
        }
        // The leg in hand.
        if let Some(mut leg) = self.overland.leg.take() {
            let to = flat_d(feet, leg.target);
            if to + 1.0 < leg.best {
                leg.best = to;
                leg.best_at = tick;
            }
            if to <= rules.leg_reach {
                self.overland.turn = ease(self.overland.turn, rules.turn_deg);
                self.note(
                    tick,
                    format!("overland leg {} done", leg.n),
                    format!(
                        "in {} ticks at {feet:?}; {}",
                        tick.saturating_sub(leg.began),
                        self.overland_state(w, d, err)
                    ),
                );
            } else if tick.saturating_sub(leg.best_at) > rules.leg_stall_ticks
                || leg.tries >= rules.leg_tries
            {
                self.overland.stalls += 1;
                let why = format!(
                    "leg {} came no nearer than {:.0} to {:?} in {} ticks ({} walks)",
                    leg.n,
                    leg.best,
                    leg.target,
                    tick.saturating_sub(leg.best_at),
                    leg.tries
                );
                self.overland_turn(&rules, feet, tick, &why);
            } else if let Some(t) = self.overland_walk(w, &rules, &mut leg, tick) {
                self.overland.leg = Some(leg);
                return Some(t);
            } else {
                self.overland.stalls += 1;
                self.overland_turn(
                    &rules,
                    feet,
                    tick,
                    &format!("leg {} has no walk to {:?}", leg.n, leg.target),
                );
            }
        }
        // A new leg: a target on seen ground on the bearing (or the next travel point round an
        // ocean), the bearing turned for an ocean ahead.
        let (aim, aim_why) = match rules.around.get(self.overland.around_done) {
            Some(p) => {
                let p = [f64::from(p[0]), f64::from(p[1])];
                if (p[0] - here[0]).hypot(p[1] - here[1]) <= rules.arrive {
                    self.overland.around_done += 1;
                    self.note(
                        tick,
                        format!("overland travel point reached: {p:?}"),
                        "known geography round an ocean (Bridger's ruling)".to_owned(),
                    );
                    (fix, "the fix")
                } else {
                    (p, "a travel point round an ocean")
                }
            }
            None => (fix, "the fix"),
        };
        let aim_d = (aim[0] - here[0]).hypot(aim[1] - here[1]);
        let mut deg = bearing(here, aim) + self.overland.turn;
        let len = rules
            .leg_max
            .min(aim_d.round() as i32)
            .max(rules.leg_min.min(aim_d as i32));
        let wet = water_run(w, feet, deg, len, rules.scan_step);
        if wet >= rules.ocean_wide {
            self.overland.oceans += 1;
            let left = land_on(w, feet, deg + 45.0, len, rules.scan_step);
            let right = land_on(w, feet, deg - 45.0, len, rules.scan_step);
            let side: f64 = if left > right {
                1.0
            } else if right > left {
                -1.0
            } else if self.overland.turn < 0.0 {
                // The fix's side: back toward the line to it.
                1.0
            } else {
                -1.0
            };
            self.overland.turn += side * 2.0 * rules.turn_deg;
            deg = bearing(here, aim) + self.overland.turn;
            self.note(
                tick,
                format!("overland ocean ahead: {wet} blocks of water"),
                format!(
                    "the shore followed: the bearing turned {} to {:.0} off the fix (land seen \
                     left {left}, right {right})",
                    if side > 0.0 { "left" } else { "right" },
                    self.overland.turn
                ),
            );
        }
        let target = leg_target(w, feet, deg, len, rules.leg_min, rules.scan_step).or_else(|| {
            // Nothing seen that far: the nearest seen ground on the bearing, never past the aim (a
            // throw's spot moved 20 off is not overshot by a 32-block leg).
            leg_target(
                w,
                feet,
                deg,
                (rules.leg_min / 2).min(len.max(rules.scan_step)),
                rules.scan_step,
                rules.scan_step / 2,
            )
        });
        let Some(target) = target else {
            self.overland_turn(
                &rules,
                feet,
                tick,
                &format!("no seen ground on the bearing {deg:.0} within {len}"),
            );
            self.retry_at = tick + crate::RETRY_TICKS;
            return Some(Task::Idle);
        };
        self.overland.legs += 1;
        let mut leg = OverLeg {
            n: self.overland.legs,
            from: feet,
            target,
            began: tick,
            best: flat_d(feet, target),
            best_at: tick,
            tries: 0,
        };
        self.note(
            tick,
            format!(
                "overland leg {}: {d:.0} blocks to the fix (err {err:.0}), health {:.1}, food {}, \
                 eyes {}",
                leg.n,
                w.player.health.unwrap_or(0.0),
                w.player.food.unwrap_or(0),
                crate::craft::count(w, ItemKind::EnderEye)
            ),
            format!(
                "toward {target:?} ({:.0} off) on the bearing {deg:.0} to {aim_why} (turned \
                 {:.0}); from {feet:?}, filler {}",
                flat_d(feet, target),
                self.overland.turn,
                path::bridge_blocks(w)
            ),
        );
        let t = self.overland_walk(w, &rules, &mut leg, tick);
        self.overland.leg = Some(leg);
        Some(t.unwrap_or(Task::Idle))
    }
    // <<< overland

    fn stronghold_frame_chunks(&self) -> i32 {
        self.table.as_ref().map_or(8, |t| t.stronghold.frame_chunks)
    }

    /// The leg's walk: a far walk to within `leg_reach` of its target that may bridge over water
    /// with `bridge_keep` held back and `bridge_max` at most. `None` with no route.
    fn overland_walk(
        &mut self,
        w: &World,
        rules: &OverlandRules,
        leg: &mut OverLeg,
        tick: u64,
    ) -> Option<Task> {
        leg.tries += 1;
        let goal = Goal::Plane {
            pos: leg.target,
            range: rules.leg_reach - 1.0,
        };
        let mut walk = Walk::far_to(leg.target, goal, tick);
        walk.more.keep = Some(rules.bridge_keep);
        walk.more.water = true;
        let ok = walk.route_now(w, tick);
        self.expanded += walk.expanded;
        let bridges = walk.bridges_ahead();
        // Boxed in where it stands (a hut built wider than its exit's one cell: race 45725479883
        // lane 1, 22 blocks, and every far walk died in 4 expansions from dawn to the cap): a
        // wall dug toward the leg's target, else on any side, at the head and the feet.
        if !ok
            && walk.expanded <= ENCLOSED_EXPANSIONS
            && let Some(t) = self.overland_dig_out(w, leg.target, tick)
        {
            return Some(t);
        }
        if !ok || bridges > rules.bridge_max {
            self.note(
                tick,
                format!("overland leg {} walk refused", leg.n),
                format!(
                    "{:?}, {bridges} to bridge (at most {}), {} expansions",
                    walk.route, rules.bridge_max, walk.expanded
                ),
            );
            return None;
        }
        self.walks += 1;
        self.note(
            tick,
            format!("overland leg {} by far walk", leg.n),
            format!(
                "{:?}, {} cells, {bridges} to bridge over water, {} expansions; try {}",
                walk.route,
                walk.waypoints.as_ref().map_or(0, Vec::len),
                walk.expanded,
                leg.tries
            ),
        );
        Some(Task::Explore(walk))
    }

    /// The bearing turned `turn_deg` (alternating sides, toward the fix's side first), the leg
    /// ended, noted.
    fn overland_turn(&mut self, rules: &OverlandRules, feet: Cell, tick: u64, why: &str) {
        self.overland.leg = None;
        self.overland.turns += 1;
        let side: i8 = if self.overland.side == 0 {
            if self.overland.turn > 0.0 { -1 } else { 1 }
        } else {
            -self.overland.side
        };
        self.overland.side = side;
        // Alternate widening: 30 one way, then 30 the other way past the line, and so on.
        let k = f64::from(self.overland.turns.div_ceil(2));
        self.overland.turn = (f64::from(side) * k * rules.turn_deg).clamp(-150.0, 150.0);
        self.note(
            tick,
            format!("overland turned {:.0} off the fix", self.overland.turn),
            format!(
                "{why}; at {feet:?}, turn {} of the way",
                self.overland.turns
            ),
        );
    }

    /// The hut, while due: night near or fallen, or health under the danger line with a way to
    /// heal. Built, waited in (eating is the always-row's; raw meat cooked when a furnace stands
    /// or is held), then opened on the bearing's side. `None` with no hut due or in hand.
    fn overland_hut(
        &mut self,
        w: &World,
        rules: &OverlandRules,
        feet: Cell,
        fix: [f64; 2],
        tick: u64,
    ) -> Option<Task> {
        let survival = self.table.as_ref().map(|t| t.survival.clone())?;
        let health = w.player.health.unwrap_or(20.0);
        let night = w
            .day_time()
            .is_some_and(|t| world::time::ticks_to_dusk(t) < crate::DUSK_MARGIN);
        let food_held = crate::eat::food_points(w, &survival) > 0;
        let heals = crate::can_heal(w.player.food, food_held);
        let hurt = health < survival.danger_health && heals;
        if self.overland.hut.is_none() {
            if !night && !hurt {
                return None;
            }
            let why = if night { HutWhy::Night } else { HutWhy::Hurt };
            self.overland.huts += 1;
            self.overland.leg = None;
            self.overland.hut = Some(Hut {
                why,
                began: tick,
                health0: health,
                ..Hut::default()
            });
            self.note(
                tick,
                format!(
                    "overland hut {} ({})",
                    self.overland.huts,
                    match why {
                        HutWhy::Night => "night",
                        HutWhy::Hurt => "hurt: hut and eat",
                    }
                ),
                format!(
                    "at {feet:?}; day time {:?}, health {health:.1}, food {:?}, food held {}",
                    w.day_time(),
                    w.player.food,
                    food_held
                ),
            );
        }
        let mut hut = self.overland.hut.take()?;
        let task = self.overland_hut_step(w, rules, &mut hut, feet, fix, night, tick);
        if task.is_some() {
            self.overland.hut = Some(hut);
        } else {
            // The hut is over: the progress mark starts again (a hut's time is not walking).
            self.overland.mark = None;
        }
        task
    }

    #[allow(clippy::too_many_arguments)]
    fn overland_hut_step(
        &mut self,
        w: &World,
        rules: &OverlandRules,
        hut: &mut Hut,
        feet: Cell,
        fix: [f64; 2],
        night: bool,
        tick: u64,
    ) -> Option<Task> {
        let body = w.body?;
        // A furnace held and raw meat held: put down beside the body before the walls go up.
        let raw: i32 = RAW.iter().map(|k| crate::craft::count(w, *k)).sum();
        if !hut.closed
            && !hut.furnace_placed
            && raw > 0
            && crate::craft::count(w, ItemKind::Furnace) > 0
            && self.near_block(w, BlockKind::Furnace, tick).is_none()
        {
            hut.furnace_placed = true;
            if let Some(p) = crate::place::Place::new(BlockKind::Furnace, tick) {
                self.note(
                    tick,
                    "overland hut: furnace put down".to_owned(),
                    format!("{raw} raw meat held to cook"),
                );
                return Some(Task::Place(p));
            }
        }
        if !hut.closed {
            let next = crate::cover::hut_block(w, body.pos);
            // Junk blocks first (andesite, dirt, the Nether's soul soil: what would be dropped),
            // then the shelter's own; the hut is where junk goes, never a drop that comes back.
            let item = HUT_JUNK
                .iter()
                .chain(crate::SHELTER_BLOCKS.iter())
                .copied()
                .find(|k| crate::craft::count(w, *k) > 0);
            let end = match (item, next) {
                (_, None) => Some("hut closed"),
                (None, Some(_)) => Some("no block held for a hut"),
                _ if hut.blocks >= rules.hut_max => Some("hut given up"),
                _ => None,
            };
            if let Some(why) = end {
                hut.closed = true;
                self.note(
                    tick,
                    format!("overland {why} ({} blocks placed)", hut.blocks),
                    format!("at {feet:?}"),
                );
            } else if let Some((item, (cell, click, face))) = item.zip(next) {
                if w.window.is_some() && w.click.is_none() && self.acts.is_empty() {
                    self.acts.push(world::Act::CloseWindow);
                    return Some(Task::Idle);
                }
                if w.window.is_some() || w.click.is_some() || !self.acts.is_empty() || tick < hut.at
                {
                    return Some(Task::Idle);
                }
                match crate::hold::toward(w, item) {
                    crate::hold::Hold::Held => {
                        hut.at = tick + crate::SHELTER_EVERY_TICKS;
                        hut.blocks += 1;
                        self.acts.push(world::Act::UseOn {
                            pos: BlockPos::new(click.0, click.1, click.2),
                            face,
                        });
                        self.note(
                            tick,
                            format!("overland hut block {} at {cell:?}", hut.blocks),
                            format!("on {click:?} face {face}"),
                        );
                    }
                    crate::hold::Hold::Act(a) => self.acts.push(a),
                    crate::hold::Hold::Wait => {}
                    crate::hold::Hold::Missing => hut.closed = true,
                }
                return Some(Task::Idle);
            }
        }
        // Inside: raw meat cooked at a furnace standing near, once a hut.
        if raw > 0 && !hut.cooked {
            hut.cooked = true;
            if let Some(t) = self.cook_held(w, tick) {
                return Some(t);
            }
        }
        // The wait: the night out, or the health back (`heal_to`, or `heal_ticks` at most).
        let health = w.player.health.unwrap_or(20.0);
        let waiting = match hut.why {
            HutWhy::Night => night,
            HutWhy::Hurt => {
                health < rules.heal_to && tick.saturating_sub(hut.began) < rules.heal_ticks
            }
        };
        if waiting && !hut.leaving {
            return Some(Task::Idle);
        }
        // Out: the wall on the bearing's side dug at the feet and the head.
        if !hut.leaving {
            hut.leaving = true;
            self.note(
                tick,
                "overland hut over".to_owned(),
                format!(
                    "after {} ticks; health {:.1} (from {:.1}), food {:?}, day time {:?}",
                    tick.saturating_sub(hut.began),
                    health,
                    hut.health0,
                    w.player.food,
                    w.day_time()
                ),
            );
        }
        let dir = crate::stronghold::axis_toward(
            feet,
            (fix[0].floor() as i32, feet.1, fix[1].floor() as i32),
        );
        for dy in [1, 0] {
            let c = (feet.0 + dir.0, feet.1 + dy, feet.2 + dir.1);
            let k = w.block_at(BlockPos::new(c.0, c.1, c.2));
            let solid = path::classify(w.state_at(BlockPos::new(c.0, c.1, c.2))) == Class::Solid;
            if solid && !crate::stairs::infested(k) && hut.out_digs < 4 {
                hut.out_digs += 1;
                return Some(Task::Dig(crate::dig::Dig::new(w, c, tick)));
            }
        }
        None
    }
}

/// Junk a hut is built of before the shelter's own blocks: full blocks that do not fall (never
/// gravel or sand) and are not a plant's (never leaf litter), all on `[survival] junk` or
/// `[stronghold] junk`.
const HUT_JUNK: &[ItemKind] = &[
    ItemKind::Dirt,
    ItemKind::Andesite,
    ItemKind::Diorite,
    ItemKind::Granite,
    ItemKind::Tuff,
    ItemKind::Calcite,
    ItemKind::SmoothBasalt,
    ItemKind::SoulSoil,
    ItemKind::Basalt,
    ItemKind::WarpedWartBlock,
    ItemKind::NetherWartBlock,
];

/// A far walk that dies within this many expansions is boxed in where it stands.
pub const ENCLOSED_EXPANSIONS: u64 = 8;

impl Plan {
    /// One wall cell dug out of an enclosure: on the axis toward `to` first, then the other three,
    /// the head's cell before the feet's; never a block a silverfish hides in, a liquid's
    /// neighbour or the furnace. `None` with none to dig (noted).
    fn overland_dig_out(&mut self, w: &World, to: Cell, tick: u64) -> Option<Task> {
        let body = w.body?;
        let feet = path::stand_cell(w, body.pos);
        let first = crate::stronghold::axis_toward(feet, to);
        let dirs = [
            first,
            (-first.1, first.0),
            (first.1, -first.0),
            (-first.0, -first.1),
        ];
        for (dx, dz) in dirs {
            for dy in [1, 0] {
                let c = (feet.0 + dx, feet.1 + dy, feet.2 + dz);
                let p = BlockPos::new(c.0, c.1, c.2);
                let k = w.block_at(p);
                let solid = path::classify(w.state_at(p)) == Class::Solid;
                if solid
                    && !crate::stairs::infested(k)
                    && k != Some(BlockKind::Furnace)
                    && !self.refused_now(c, tick)
                {
                    self.note(
                        tick,
                        format!("overland dig out at {c:?}"),
                        format!("boxed in at {feet:?}: the far walk died within {ENCLOSED_EXPANSIONS} expansions"),
                    );
                    return Some(Task::Dig(crate::dig::Dig::new(w, c, tick)));
                }
            }
        }
        None
    }
}

/// The turn eased back toward the line to the fix by `step` degrees.
fn ease(turn: f64, step: f64) -> f64 {
    if turn.abs() <= step {
        0.0
    } else {
        turn - step * turn.signum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bearings_and_ease() {
        assert!((bearing([0.0, 0.0], [10.0, 0.0])).abs() < 1e-9);
        assert!((bearing([0.0, 0.0], [0.0, 10.0]) - 90.0).abs() < 1e-9);
        assert!((ease(60.0, 30.0) - 30.0).abs() < 1e-9);
        assert!((ease(-20.0, 30.0)).abs() < 1e-9);
    }
}
