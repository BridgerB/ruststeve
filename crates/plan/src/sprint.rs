//! The pace policy (issue 56, docs/problems/sprint.md): which moves sprint. Walks, explores,
//! retreats and escapes on land ask for it (the sprint key held in `Controls.sprint`); a retreat
//! from a `never_fight` kind always does; the gate here keeps the wish only where the policy
//! allows and writes why to the ledger.
//!
//! - **Food**: sprint costs a food point every 40 metres of flat ground (config/sprint.toml
//!   `[exhaustion]`), saturation first. A move sprints only while one point is left to spend above
//!   `hungry_below` (goals.toml `[survival]`), so a sprint alone never takes the food under that
//!   line. A retreat from a `never_fight` kind skips the budget: outrunning a creeper is worth the
//!   food (iron-kit walls 21, 28, 29); the body still refuses at food 6 or under
//!   (`physics::sprint_allowed`).
//! - **Ground**: never toward or along a lava edge, a drop, water or footing the physics cannot
//!   simulate, read along the facing for `LOOK_AHEAD` blocks. No sprint in water: there is no swim
//!   model.
//! - **Path**: a step-by-step move (a bridge laid, a sneak at an edge, stairs or a travel tunnel)
//!   keeps walking.
//!
//! A sprint hit (a knock-back blow) ends a sprint (`Player.causeExtraKnockback`): the gate asks
//! again every tick, so a retreat that swings sprints again the next tick.
//!
//! The policy is pure: the world, the task in hand and the numbers below. The numbers are
//! config/sprint.toml `[policy]`'s, mirrored as constants because the core reads no files
//! (`the_policy_is_config_sprint_toml` holds them equal).

use physics::Controls;
use world::World;

use crate::path::{self, Cell, Class};
use crate::{Plan, Task};

/// Metres of flat sprint one food point pays for (`[policy] metres_per_point`: `[exhaustion]`
/// per_point 4.0 over sprint_per_metre 0.1).
pub const METRES_PER_POINT: f64 = 40.0;
/// Blocks ahead along the facing read before a sprint (`[policy] look_ahead`: a tick's sprint
/// move, the coast after release and half the body's width make 0.92 blocks).
pub const LOOK_AHEAD: i32 = 2;
/// Ticks a refusal holds before the sprint is asked again (`[policy] hold_ticks`): a sprint
/// stopped and started each tick sends a `player_command` pair a tick (CI 46120536785), and the
/// sprint's speed takes about five ticks to build (0.546⁵ of the walk's left), so a second is
/// four builds.
pub const HOLD_TICKS: u64 = 20;
/// Blocks the body must go from where a sprint was last stopped or refused before it is asked
/// again in the same task (`[policy] clear_blocks`): past `LOOK_AHEAD`, so the cells that refused
/// it are behind (probe 46122310768: the cast's walk by the lava restarted its sprint 40 times in
/// one task, each hold running out beside the same cells).
pub const CLEAR_BLOCKS: f64 = 3.0;
/// Each stop in a task doubles the next hold, this many times at most (`[policy] max_doublings`:
/// 20, 40, 80, 160 ticks).
pub const MAX_DOUBLINGS: u32 = 3;

/// The kind of move this tick, as the task in hand makes it: a closed set.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pace {
    /// Not moving forward: nothing to judge.
    Still,
    /// A walk's plain move: to a log, a block, a furnace, any task's walk.
    Walk,
    /// A walk out to see more (an explore, a far leg).
    Explore,
    /// Backing away from a foe; from a `never_fight` kind it skips the food budget.
    Retreat { never_fight: bool },
    /// Out of water, lava or fire.
    Escape,
    /// A fight's close-in: a sprinting blow is never a critical (`Player.canCriticalAttack`).
    Fight,
    /// A step-by-step move, which keeps walking.
    Step(Step),
}

/// A step-by-step move.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Step {
    /// A bridge cell's floor laid before the step (`MoveKind::Bridge`).
    Bridge,
    /// Sneak held at a lava or void edge (`walk::lava_edge`).
    Sneak,
    /// Stairs cut down, or a travel tunnel (`Task::Stairs`).
    Stairs,
}

/// Why a move that asked to sprint walks.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Refusal {
    /// Food at or under 6, where the body cannot sprint (`FoodData.hasEnoughFood`).
    FoodFloor,
    /// No point left to spend above `hungry_below` (or the food is not known yet).
    Budget,
    /// Lava ahead, under or beside the way (a lava edge), or the body in it.
    Lava,
    /// A floor missing ahead, or a side dropping to the void in the End.
    Drop,
    /// Water ahead or under the way.
    Water,
    /// The body in water: no swim model.
    Swim,
    /// A floor or cell ahead the physics cannot simulate (`path::Class::Awkward`).
    Footing,
    /// A cell ahead not loaded.
    Unseen,
    /// Walking into a wall (a collision that is not minor), which stops a sprint
    /// (`LocalPlayer.shouldStopRunSprinting`): not asked again until the hold is out.
    Wall,
    /// A step-by-step move.
    Path(Step),
}

impl Refusal {
    /// The note's words: no digits or brackets, so the race summary's "notes by kind" counts
    /// each reason apart.
    pub fn words(self) -> &'static str {
        match self {
            Refusal::FoodFloor => "food floor",
            Refusal::Budget => "food budget",
            Refusal::Lava => "lava",
            Refusal::Drop => "drop",
            Refusal::Water => "water",
            Refusal::Swim => "swim",
            Refusal::Footing => "footing",
            Refusal::Unseen => "unseen",
            Refusal::Wall => "wall",
            Refusal::Path(Step::Bridge) => "bridge",
            Refusal::Path(Step::Sneak) => "sneak",
            Refusal::Path(Step::Stairs) => "stairs",
        }
    }
}

impl Pace {
    fn words(self) -> &'static str {
        match self {
            Pace::Still => "still",
            Pace::Walk => "walk",
            Pace::Explore => "explore",
            Pace::Retreat { never_fight: true } => "never-fight retreat",
            Pace::Retreat { never_fight: false } => "retreat",
            Pace::Escape => "escape",
            Pace::Fight => "fight",
            Pace::Step(_) => "step",
        }
    }
}

/// The policy's answer for a tick.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verdict {
    Sprint,
    Refused(Refusal),
    /// Not a move that asks: no forward, a fight, or no wish.
    NotAsked,
}

/// Food points a sprint may spend before the food would go under `hungry_below`: the points
/// over the line, plus saturation, which the exhaustion takes first (`FoodData.tick`: a point of
/// saturation per 4.0, the last fraction a whole one). Unknown food is none.
pub fn points_to_spend(food: Option<i32>, saturation: Option<f32>, hungry_below: i32) -> i32 {
    let Some(f) = food else {
        return 0;
    };
    #[allow(clippy::cast_possible_truncation)]
    let sat = saturation.map_or(0, |s| s.max(0.0).ceil() as i32);
    (f - hungry_below).max(0) + sat
}

/// The policy: does this tick's move sprint? `c` is the controls the task made, its `sprint`
/// the wish.
pub fn judge(w: &World, hungry_below: i32, pace: Pace, c: &Controls) -> Verdict {
    let Some(body) = w.body else {
        return Verdict::NotAsked;
    };
    if !c.forward {
        return Verdict::NotAsked;
    }
    let budgeted = match pace {
        Pace::Still | Pace::Fight => return Verdict::NotAsked,
        Pace::Step(s) => return Verdict::Refused(Refusal::Path(s)),
        Pace::Walk | Pace::Explore | Pace::Escape => {
            if !c.sprint {
                return Verdict::NotAsked;
            }
            true
        }
        // A retreat from a `never_fight` kind always asks.
        Pace::Retreat { never_fight } => {
            if !c.sprint && !never_fight {
                return Verdict::NotAsked;
            }
            !never_fight
        }
    };
    if c.sneak {
        return Verdict::Refused(Refusal::Path(Step::Sneak));
    }
    if body.in_water || body.eye_in_water {
        return Verdict::Refused(Refusal::Swim);
    }
    if let Some(r) = ground_ahead(w, &body, c.yaw) {
        return Verdict::Refused(r);
    }
    if !budgeted {
        return Verdict::Sprint;
    }
    if w.player
        .food
        .is_some_and(|f| f <= physics::SPRINT_FOOD_FLOOR)
    {
        return Verdict::Refused(Refusal::FoodFloor);
    }
    if points_to_spend(w.player.food, w.player.saturation, hungry_below) < 1 {
        return Verdict::Refused(Refusal::Budget);
    }
    Verdict::Sprint
}

/// What refuses a sprint on the ground round the feet and `LOOK_AHEAD` blocks along `yaw`.
/// Read only from the blocks the world holds, the walk's own classes (`path::Cells`).
pub fn ground_ahead(w: &World, body: &physics::Body, yaw: f64) -> Option<Refusal> {
    if body.in_lava {
        return Some(Refusal::Lava);
    }
    let mut cells = path::Cells::new(w);
    let feet = path::stand_cell(w, body.pos);
    let in_end = crate::end::in_end(w);
    let edge = |cells: &mut path::Cells, c: Cell| {
        if crate::walk::lava_edge(cells, c) {
            Some(Refusal::Lava)
        } else if in_end && crate::end::void_edge(w, c) {
            Some(Refusal::Drop)
        } else {
            None
        }
    };
    if let Some(r) = edge(&mut cells, feet) {
        return Some(r);
    }
    let rad = yaw.to_radians();
    let (dx, dz) = (-rad.sin(), rad.cos());
    for d in 1..=LOOK_AHEAD {
        let at = [
            body.pos[0] + dx * f64::from(d),
            body.pos[1],
            body.pos[2] + dz * f64::from(d),
        ];
        let (x, _, z) = path::feet_cell(at);
        let c = (x, feet.1, z);
        if c == feet {
            continue;
        }
        let under = (c.0, c.1 - 1, c.2);
        // A solid cell at the feet is a step up (or a wall, which stops the sprint by collision):
        // its own top is the floor.
        let floor = match cells.at(c) {
            Class::Solid => None,
            Class::Clear => Some(cells.at(under)),
            Class::Lava => return Some(Refusal::Lava),
            Class::Water => return Some(Refusal::Water),
            Class::Awkward => return Some(Refusal::Footing),
            Class::Unknown => return Some(Refusal::Unseen),
        };
        match floor {
            None | Some(Class::Solid) => {}
            Some(Class::Clear) => return Some(Refusal::Drop),
            Some(Class::Lava) => return Some(Refusal::Lava),
            Some(Class::Water) => return Some(Refusal::Water),
            Some(Class::Awkward) => return Some(Refusal::Footing),
            Some(Class::Unknown) => return Some(Refusal::Unseen),
        }
        if let Some(r) = edge(&mut cells, c) {
            return Some(r);
        }
    }
    None
}

/// The pace gate's state for the task in hand: which task it is for, what it has noted, and the
/// last verdict (held while the body is in the air: a sprint's flag holds through a jump, and
/// the ground ahead is read from the ground). Run totals for the case file.
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct PaceState {
    pub serial: u64,
    pub begun: bool,
    pub refused: Vec<Refusal>,
    pub last: bool,
    pub sprint_ticks: u64,
    pub refused_ticks: u64,
    /// No sprint is asked before this tick: the last refusal's hold (`HOLD_TICKS`).
    #[serde(default)]
    pub held_until: u64,
    /// Where the body was when the sprint was last stopped or refused in this task: no sprint
    /// is asked again within `CLEAR_BLOCKS` of it.
    #[serde(default)]
    pub stopped_at: Option<[f64; 3]>,
    /// Sprints stopped in this task (each doubles the hold), and sprints started again after one.
    #[serde(default)]
    pub stops: u32,
    #[serde(default)]
    pub restarts: u32,
}

impl PaceState {
    /// A stop or a refusal at `pos`: the hold, doubled for each sprint already stopped in the
    /// task when this one ends a sprint.
    fn stop(&mut self, tick: u64, pos: Option<[f64; 3]>) {
        if self.last {
            self.stops += 1;
        }
        let doublings = self.stops.saturating_sub(1).min(MAX_DOUBLINGS);
        self.held_until = tick + (HOLD_TICKS << doublings);
        self.stopped_at = pos.or(self.stopped_at);
    }

    /// Is a sprint held back at `pos` on `tick`?
    fn held(&self, tick: u64, pos: Option<[f64; 3]>) -> bool {
        let near = self.stopped_at.zip(pos).is_some_and(|(a, b)| {
            ((a[0] - b[0]).powi(2) + (a[2] - b[2]).powi(2)).sqrt() < CLEAR_BLOCKS
        });
        tick < self.held_until || near
    }
}

impl Plan {
    /// The kind of move the task in hand makes this tick, and the blocks it has left to go.
    fn pace_of(&self, w: &World, c: &Controls) -> (Pace, Option<f64>) {
        let pos = w.body.map(|b| b.pos);
        let walk_left = |walk: &crate::walk::Walk| pos.map(|p| walk.left(p));
        // A task with no pace of its own moves only by a walk inside it: a plain walk when that
        // walk asked to sprint.
        let inner = if c.sprint { Pace::Walk } else { Pace::Still };
        match &self.task {
            Task::Idle => (Pace::Still, None),
            Task::ToLog(walk) => (walk.pace(Pace::Walk), walk_left(walk)),
            Task::Explore(walk) => (walk.pace(Pace::Explore), walk_left(walk)),
            Task::Defend(d) => match d.stance {
                crate::defend::Stance::Fight => (Pace::Fight, None),
                crate::defend::Stance::Retreat => {
                    let rules = self.table.as_ref().map(|t| &t.survival);
                    let never_fight = rules.is_some_and(|r| r.never_fight.contains(&d.kind));
                    let left = rules.zip(pos).map(|(r, p)| {
                        let (dx, dz) = (p[0] - d.last_seen[0], p[2] - d.last_seen[2]);
                        (r.flee_clear - (dx * dx + dz * dz).sqrt()).max(0.0)
                    });
                    let plain = Pace::Retreat { never_fight };
                    let pace = d.flee.as_ref().map_or(plain, |(walk, _)| walk.pace(plain));
                    (pace, left)
                }
            },
            Task::Escape(_) | Task::Swim(..) => (Pace::Escape, None),
            Task::Stairs(_) => (Pace::Step(Step::Stairs), None),
            Task::Dig(_)
            | Task::Craft(_)
            | Task::Place(_)
            | Task::Smelt(_)
            | Task::Redig(_)
            | Task::Scoop(_)
            | Task::Cast(_)
            | Task::Enter(_)
            | Task::Eat(_)
            | Task::Hunt(_)
            | Task::Throw(_)
            | Task::Dragon(_) => (inner, None),
        }
    }

    /// The gate: `c.sprint` as the policy answers, and the ledger's notes, one when a task
    /// begins to sprint and one for each reason it is refused.
    pub(crate) fn pace_gate(&mut self, w: &World, mut c: Controls) -> Controls {
        let tick = w.ticks;
        if self.pace.serial != self.task_serial {
            self.pace = PaceState {
                serial: self.task_serial,
                sprint_ticks: self.pace.sprint_ticks,
                refused_ticks: self.pace.refused_ticks,
                ..PaceState::default()
            };
        }
        let (pace, left) = self.pace_of(w, &c);
        let hungry_below = self.table.as_ref().map_or_else(
            || crate::make::Survival::default().hungry_below,
            |t| t.survival.hungry_below,
        );
        let airborne = w.body.is_some_and(|b| !b.on_ground && !b.in_water);
        let verdict = match judge(w, hungry_below, pace, &c) {
            // The ground ahead is read from the ground; in a jump the last answer holds.
            Verdict::Refused(Refusal::Drop) if airborne && self.pace.last => Verdict::Sprint,
            // Into a wall: the body stops the sprint; asking again would start it each tick.
            Verdict::Sprint
                if w.body
                    .is_some_and(|b| b.horizontal_collision && !b.minor_horizontal_collision) =>
            {
                Verdict::Refused(Refusal::Wall)
            }
            v => v,
        };
        let pos = w.body.map(|b| b.pos);
        // A refusal, or a sprint ended because the task stopped asking (no forward, a fight), is
        // a stop: held for a while and until the body is clear of where it was.
        match verdict {
            Verdict::Refused(_) => self.pace.stop(tick, pos),
            Verdict::NotAsked if self.pace.last => self.pace.stop(tick, pos),
            Verdict::NotAsked | Verdict::Sprint => {}
        }
        // A stop holds: no sprint asked until it is out, and nothing more noted.
        if verdict == Verdict::Sprint && self.pace.held(tick, pos) {
            c.sprint = false;
            self.pace.last = false;
            self.pace.refused_ticks += 1;
            return c;
        }
        let restart = verdict == Verdict::Sprint && !self.pace.last && self.pace.begun;
        c.sprint = verdict == Verdict::Sprint;
        self.pace.last = c.sprint;
        if restart {
            self.pace.restarts += 1;
            self.pace.stopped_at = None;
            self.note(
                tick,
                format!("sprint restarted, {}", self.pace.restarts),
                format!(
                    "{} ({}): {} stops in the task, the next hold {} ticks; at {:?}",
                    self.task.name(),
                    pace.words(),
                    self.pace.stops,
                    HOLD_TICKS << self.pace.stops.min(MAX_DOUBLINGS),
                    pos.map(path::feet_cell)
                ),
            );
        }
        let points = points_to_spend(w.player.food, w.player.saturation, hungry_below);
        let budget = f64::from(points) * METRES_PER_POINT;
        let left = left.map_or_else(|| "unknown".to_owned(), |l| format!("{l:.0} m"));
        match verdict {
            Verdict::NotAsked => {}
            Verdict::Sprint => {
                self.pace.sprint_ticks += 1;
                if !self.pace.begun {
                    self.pace.begun = true;
                    self.note(
                        tick,
                        format!("sprint begun, {}", pace.words()),
                        format!(
                            "{}: food {:?}, saturation {:?}, hungry_below {hungry_below}: budget {budget:.0} m, {left} left; {} sprint ticks in the run",
                            self.task.name(),
                            w.player.food,
                            w.player.saturation,
                            self.pace.sprint_ticks
                        ),
                    );
                }
            }
            Verdict::Refused(r) => {
                self.pace.refused_ticks += 1;
                if !self.pace.refused.contains(&r) {
                    self.pace.refused.push(r);
                    self.note(
                        tick,
                        format!("sprint refused, {}", r.words()),
                        format!(
                            "{} ({}): food {:?}, saturation {:?}, hungry_below {hungry_below}: budget {budget:.0} m, {left} left; at {:?}",
                            self.task.name(),
                            pace.words(),
                            w.player.food,
                            w.player.saturation,
                            w.body.map(|b| path::feet_cell(b.pos))
                        ),
                    );
                }
            }
        }
        c
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use data::BlockKind;
    use world::BlockPos;

    const HUNGRY: i32 = 14;

    fn put(w: &mut World, (x, y, z): Cell, b: BlockKind) {
        let pos = BlockPos::new(x, y, z);
        let state = b.info().map_or(0, |i| i.default_state);
        w.overlay.entry(pos.chunk()).or_default().insert(pos, state);
    }

    /// A stone floor at y 63 from (-4, -4) to (60, 60), air over it to y 66, the body at
    /// (0.5, 64, 0.5) on the ground facing south (+z), fed.
    fn flat() -> World {
        let mut w = World::new();
        for x in -4..=60 {
            for z in -4..=60 {
                put(&mut w, (x, 63, z), BlockKind::Stone);
                for y in 64..=66 {
                    put(&mut w, (x, y, z), BlockKind::Air);
                }
            }
        }
        w.body = Some(physics::Body {
            pos: [0.5, 64.0, 0.5],
            on_ground: true,
            ..physics::Body::default()
        });
        w.player.food = Some(20);
        w.player.saturation = Some(5.0);
        w
    }

    fn ahead() -> Controls {
        Controls {
            forward: true,
            yaw: 0.0,
            sprint: true,
            ..Controls::default()
        }
    }

    #[test]
    fn a_walk_on_flat_ground_sprints() {
        let w = flat();
        assert_eq!(judge(&w, HUNGRY, Pace::Walk, &ahead()), Verdict::Sprint);
        assert_eq!(judge(&w, HUNGRY, Pace::Explore, &ahead()), Verdict::Sprint);
        // No wish, no forward, a fight: nothing asked.
        let wish_off = Controls {
            sprint: false,
            ..ahead()
        };
        assert_eq!(judge(&w, HUNGRY, Pace::Walk, &wish_off), Verdict::NotAsked);
        let still = Controls {
            forward: false,
            ..ahead()
        };
        assert_eq!(judge(&w, HUNGRY, Pace::Walk, &still), Verdict::NotAsked);
        assert_eq!(judge(&w, HUNGRY, Pace::Fight, &ahead()), Verdict::NotAsked);
    }

    /// Food at hungry_below + 1 and no saturation, a long walk: the FoodData model (4.0 a point,
    /// 0.1 a metre of sprint) runs under the policy, which sprints at most the one point's 40
    /// metres (and a tick) and never takes the food under the line.
    #[test]
    fn a_long_walk_at_the_line_does_not_sprint_past_the_budget() {
        let mut w = flat();
        w.player.food = Some(HUNGRY + 1);
        w.player.saturation = Some(0.0);
        let step = 0.280_617;
        let (mut exhaustion, mut sprinted, mut walked) = (0.0f64, 0.0f64, 0.0f64);
        for _ in 0..4000 {
            match judge(&w, HUNGRY, Pace::Walk, &ahead()) {
                Verdict::Sprint => {
                    sprinted += step;
                    exhaustion += 0.1 * step;
                }
                Verdict::Refused(r) => {
                    assert_eq!(r, Refusal::Budget);
                    walked += 0.215_859;
                }
                Verdict::NotAsked => unreachable!("a walk asked"),
            }
            if exhaustion > 4.0 {
                exhaustion -= 4.0;
                w.player.food = w.player.food.map(|f| f - 1);
            }
            assert!(
                w.player.food.is_some_and(|f| f >= HUNGRY),
                "{:?}",
                w.player.food
            );
        }
        assert!(sprinted <= METRES_PER_POINT + step, "sprinted {sprinted}");
        assert!(sprinted >= METRES_PER_POINT - step, "sprinted {sprinted}");
        assert!(walked > 0.0);
        assert_eq!(w.player.food, Some(HUNGRY));
        // Saturation counts first: five points of it are 200 metres more.
        assert_eq!(points_to_spend(Some(HUNGRY), Some(5.0), HUNGRY), 5);
        assert_eq!(points_to_spend(Some(HUNGRY), Some(0.4), HUNGRY), 1);
        assert_eq!(points_to_spend(None, Some(5.0), HUNGRY), 0);
    }

    /// A creeper retreat sprints with food under hungry_below; another retreat does not; the
    /// body's own floor (6) still holds the retreat's sprint back (`physics::sprint_allowed`).
    #[test]
    fn a_creeper_retreat_sprints() {
        let mut w = flat();
        w.player.food = Some(10);
        w.player.saturation = Some(0.0);
        let never = Pace::Retreat { never_fight: true };
        let other = Pace::Retreat { never_fight: false };
        assert_eq!(judge(&w, HUNGRY, never, &ahead()), Verdict::Sprint);
        // It asks even when the step made no wish.
        let no_wish = Controls {
            sprint: false,
            ..ahead()
        };
        assert_eq!(judge(&w, HUNGRY, never, &no_wish), Verdict::Sprint);
        assert_eq!(
            judge(&w, HUNGRY, other, &ahead()),
            Verdict::Refused(Refusal::Budget)
        );
        w.player.food = Some(6);
        assert_eq!(
            judge(&w, HUNGRY, other, &ahead()),
            Verdict::Refused(Refusal::FoodFloor)
        );
        let body = w.body.unwrap_or_default();
        let c = Controls {
            sprint: true,
            ..ahead()
        };
        assert!(!physics::sprint_allowed(&body, w.player.food, &c));
    }

    #[test]
    fn a_lava_edge_refuses() {
        // Lava beside the floor of the cell ahead: along a lava edge.
        let mut w = flat();
        put(&mut w, (1, 63, 1), BlockKind::Lava);
        assert_eq!(
            judge(&w, HUNGRY, Pace::Walk, &ahead()),
            Verdict::Refused(Refusal::Lava)
        );
        // And for a creeper retreat too: never a death as a plan.
        assert_eq!(
            judge(&w, HUNGRY, Pace::Retreat { never_fight: true }, &ahead()),
            Verdict::Refused(Refusal::Lava)
        );
        // Lava two blocks ahead, the floor itself.
        let mut w = flat();
        put(&mut w, (0, 63, 2), BlockKind::Lava);
        assert_eq!(
            judge(&w, HUNGRY, Pace::Walk, &ahead()),
            Verdict::Refused(Refusal::Lava)
        );
        // Facing away from it, the walk sprints.
        let back = Controls {
            yaw: 180.0,
            ..ahead()
        };
        let mut w = flat();
        put(&mut w, (0, 63, 2), BlockKind::Lava);
        w.body = w.body.map(|b| physics::Body {
            pos: [0.5, 64.0, 10.5],
            ..b
        });
        assert_eq!(judge(&w, HUNGRY, Pace::Walk, &back), Verdict::Sprint);
    }

    #[test]
    fn a_drop_or_water_ahead_refuses() {
        let mut w = flat();
        put(&mut w, (0, 63, 2), BlockKind::Air);
        put(&mut w, (0, 62, 2), BlockKind::Air);
        assert_eq!(
            judge(&w, HUNGRY, Pace::Walk, &ahead()),
            Verdict::Refused(Refusal::Drop)
        );
        let mut w = flat();
        put(&mut w, (0, 63, 1), BlockKind::Water);
        assert_eq!(
            judge(&w, HUNGRY, Pace::Walk, &ahead()),
            Verdict::Refused(Refusal::Water)
        );
        let mut w = flat();
        w.body = w.body.map(|b| physics::Body {
            in_water: true,
            ..b
        });
        assert_eq!(
            judge(&w, HUNGRY, Pace::Escape, &ahead()),
            Verdict::Refused(Refusal::Swim)
        );
        // A step up ahead is not a drop.
        let mut w = flat();
        put(&mut w, (0, 64, 2), BlockKind::Stone);
        assert_eq!(judge(&w, HUNGRY, Pace::Walk, &ahead()), Verdict::Sprint);
    }

    #[test]
    fn a_bridge_or_sneak_path_refuses() {
        let w = flat();
        assert_eq!(
            judge(&w, HUNGRY, Pace::Step(Step::Bridge), &ahead()),
            Verdict::Refused(Refusal::Path(Step::Bridge))
        );
        assert_eq!(
            judge(&w, HUNGRY, Pace::Step(Step::Stairs), &ahead()),
            Verdict::Refused(Refusal::Path(Step::Stairs))
        );
        let sneak = Controls {
            sneak: true,
            ..ahead()
        };
        assert_eq!(
            judge(&w, HUNGRY, Pace::Walk, &sneak),
            Verdict::Refused(Refusal::Path(Step::Sneak))
        );
    }

    /// The gate on a planner: a walk that bridged this tick walks and notes why once; the same
    /// walk on plain ground sprints and notes it once.
    #[test]
    fn the_gate_notes_a_begun_sprint_and_a_refusal_once() {
        let mut w = flat();
        let mut plan = Plan::new(crate::Aim::Table);
        let mut walk = crate::walk::Walk::new(
            (0, 64, 40),
            path::Goal::Block { pos: (0, 64, 40) },
            crate::walk::Arrive::Goal,
            0,
        );
        walk.more.step = Some(Step::Bridge);
        plan.task = Task::ToLog(walk.clone());
        for _ in 0..3 {
            let c = plan.pace_gate(&w, ahead());
            assert!(!c.sprint);
        }
        walk.more.step = None;
        plan.task = Task::ToLog(walk);
        // The bridge's refusal holds `HOLD_TICKS`, and the body stands where it was refused: plain
        // ground walks until both are past.
        for t in 1..HOLD_TICKS {
            w.ticks = t;
            assert!(!plan.pace_gate(&w, ahead()).sprint, "held at {t}");
        }
        // Out, and clear of where it was refused.
        w.body = w.body.map(|b| physics::Body {
            pos: [0.5, 64.0, 0.5 + CLEAR_BLOCKS],
            ..b
        });
        for t in HOLD_TICKS..HOLD_TICKS + 3 {
            w.ticks = t;
            assert!(plan.pace_gate(&w, ahead()).sprint, "out at {t}");
        }
        let whats: Vec<&str> = plan.events.iter().map(|e| e.what.as_str()).collect();
        assert_eq!(whats, vec!["sprint refused, bridge", "sprint begun, walk"]);
        assert_eq!(plan.pace.sprint_ticks, 3);
        assert_eq!(plan.pace.refused_ticks, 3 + HOLD_TICKS - 1);
    }

    /// A straight walk of 40 blocks on flat ground, the walk, the gate and the body's physics run
    /// together: one start and at most one stop (CI 46120536785: a walk paused at each waypoint,
    /// and the body stopped and started its sprint every few ticks).
    #[test]
    fn a_straight_walk_starts_its_sprint_once() {
        let mut w = flat();
        let target = (0, 64, 40);
        let mut plan = Plan::new(crate::Aim::Table);
        let mut walk = crate::walk::Walk::new(
            target,
            path::Goal::Block { pos: target },
            crate::walk::Arrive::Goal,
            0,
        );
        assert!(walk.route_now(&w, 0));
        let (mut starts, mut stops, mut sprinted) = (0u32, 0u32, 0u64);
        let mut done = false;
        for t in 1..400u64 {
            w.ticks = t;
            let (c, status) = walk.step(&w, t);
            if status != crate::walk::TaskStatus::Running {
                done = status == crate::walk::TaskStatus::Done;
                break;
            }
            plan.task = Task::ToLog(walk.clone());
            let c = plan.pace_gate(&w, c);
            let Some(body) = w.body else { break };
            let next = {
                let solid = |x, y, z| w.solid(x, y, z);
                let fluid = |x, y, z| w.fluid(x, y, z);
                let surface = |x, y, z| w.surface(x, y, z);
                physics::step_fed(&solid, &fluid, &surface, &body, &c, w.player.food)
            };
            match (body.sprinting, next.sprinting) {
                (false, true) => starts += 1,
                (true, false) => stops += 1,
                (true, true) | (false, false) => {}
            }
            sprinted += u64::from(next.sprinting);
            w.body = Some(next);
        }
        assert!(done, "the walk arrives");
        assert_eq!(starts, 1, "{stops} stops, {sprinted} ticks sprinted");
        assert!(stops <= 1, "{stops} stops");
        assert!(sprinted > 60, "{sprinted} ticks sprinted");
    }

    /// Probe 46122310768: a walk stopped by the same cells again and again. A stop (a refusal,
    /// or the walk not asking while it sprinted) holds until the hold is out and the body is
    /// `CLEAR_BLOCKS` from where it stopped; each stop doubles the next hold; every restart is
    /// noted.
    #[test]
    fn a_stopped_sprint_waits_to_be_clear_and_each_restart_is_noted() {
        let mut w = flat();
        let mut plan = Plan::new(crate::Aim::Table);
        let walk = crate::walk::Walk::new(
            (0, 64, 40),
            path::Goal::Block { pos: (0, 64, 40) },
            crate::walk::Arrive::Goal,
            0,
        );
        plan.task = Task::ToLog(walk);
        let at = |w: &mut World, z: f64| {
            w.body = w.body.map(|b| physics::Body {
                pos: [0.5, 64.0, z],
                ..b
            });
        };
        let mut gate = |w: &mut World, t: u64, c: Controls| {
            w.ticks = t;
            plan.pace_gate(w, c).sprint
        };
        assert!(gate(&mut w, 1, ahead()));
        // The walk lets go of the key for a tick: a stop, though nothing refused.
        let off = Controls {
            forward: false,
            ..ahead()
        };
        assert!(!gate(&mut w, 2, off));
        // Long after the hold, but standing where it stopped: still held.
        assert!(!gate(&mut w, 200, ahead()));
        // Clear of it: started again, and noted.
        at(&mut w, 0.5 + CLEAR_BLOCKS);
        assert!(gate(&mut w, 201, ahead()));
        // The second stop holds twice as long, wherever the body is.
        assert!(!gate(&mut w, 202, off));
        at(&mut w, 20.5);
        assert!(!gate(&mut w, 202 + HOLD_TICKS, ahead()));
        assert!(gate(&mut w, 202 + 2 * HOLD_TICKS, ahead()));
        let whats: Vec<&str> = plan.events.iter().map(|e| e.what.as_str()).collect();
        assert_eq!(
            whats,
            vec![
                "sprint begun, walk",
                "sprint restarted, 1",
                "sprint restarted, 2"
            ]
        );
        assert_eq!(plan.pace.stops, 2);
    }

    /// The gate only ever writes `sprint`: every other key, the look and the pitch are the
    /// task's own, whatever the verdict (lead-56 on the planner test's walk under the log).
    #[test]
    fn the_gate_changes_nothing_but_the_sprint_key() {
        let cases: Vec<(World, Controls)> = vec![
            (flat(), ahead()),
            (
                flat(),
                Controls {
                    jump: true,
                    yaw: 37.0,
                    pitch: Some(12.0),
                    ..ahead()
                },
            ),
            (
                flat(),
                Controls {
                    sneak: true,
                    ..ahead()
                },
            ),
            (
                flat(),
                Controls {
                    forward: false,
                    ..ahead()
                },
            ),
            {
                let mut w = flat();
                put(&mut w, (0, 63, 2), BlockKind::Lava);
                (w, ahead())
            },
            {
                let mut w = flat();
                w.player.food = Some(HUNGRY);
                w.player.saturation = Some(0.0);
                (w, ahead())
            },
        ];
        let mut plan = Plan::new(crate::Aim::Table);
        plan.task = Task::ToLog(crate::walk::Walk::new(
            (0, 64, 40),
            path::Goal::Block { pos: (0, 64, 40) },
            crate::walk::Arrive::Goal,
            0,
        ));
        for (t, (mut w, c)) in cases.into_iter().enumerate() {
            for tick in 0..3u64 {
                w.ticks = 100 * t as u64 + tick;
                let out = plan.pace_gate(&w, c);
                assert_eq!(
                    Controls {
                        sprint: c.sprint,
                        ..out
                    },
                    c,
                    "case {t} tick {tick}"
                );
            }
        }
    }

    /// A walk to within reach of a block 3 over the floor: the route's goal (`Near`, height in
    /// its range) takes the cell under it, the arrival (`PICKUP_RISE` 2) never would; that end
    /// is refused and the route goes up the step beside it instead.
    #[test]
    fn a_route_end_the_arrival_refuses_is_planned_again() {
        let mut w = flat();
        put(&mut w, (1, 64, 10), BlockKind::Stone);
        for y in 65..=68 {
            for x in -1..=2 {
                for z in 8..=12 {
                    if (x, y, z) != (1, 64, 10) {
                        put(&mut w, (x, y, z), BlockKind::Air);
                    }
                }
            }
        }
        let mut walk = crate::walk::Walk::to_block((0, 67, 10), 0);
        assert!(walk.route_now(&w, 0));
        let end = walk.waypoints.as_ref().and_then(|v| v.last().copied());
        assert_eq!(end, Some((1, 65, 10)), "{:?}", walk.waypoints);
        assert!(walk.more.blocked.contains(&(0, 64, 10)));
        assert!(
            walk.more
                .said
                .iter()
                .any(|(w, _)| w.starts_with("walk: route end"))
        );
    }

    /// The constants are config/sprint.toml's `[policy]` (and its `[exhaustion]`, which they
    /// come from).
    #[test]
    fn the_policy_is_config_sprint_toml() {
        let t: toml::Table =
            toml::from_str(include_str!("../../../config/sprint.toml")).unwrap_or_default();
        let get = |table: &str, key: &str| {
            t.get(table)
                .and_then(|v| v.get(key))
                .and_then(|v| v.as_float().or_else(|| v.as_integer().map(|i| i as f64)))
        };
        assert_eq!(get("policy", "metres_per_point"), Some(METRES_PER_POINT));
        assert_eq!(get("policy", "look_ahead"), Some(f64::from(LOOK_AHEAD)));
        assert_eq!(get("policy", "hold_ticks"), Some(HOLD_TICKS as f64));
        assert_eq!(get("policy", "clear_blocks"), Some(CLEAR_BLOCKS));
        assert_eq!(
            get("policy", "max_doublings"),
            Some(f64::from(MAX_DOUBLINGS))
        );
        assert_eq!(
            get("policy", "lava_drop"),
            Some(f64::from(crate::walk::LAVA_DROP))
        );
        let per_point = get("exhaustion", "per_point").unwrap_or_default();
        let per_metre = get("exhaustion", "sprint_per_metre").unwrap_or(1.0);
        assert!((per_point / per_metre - METRES_PER_POINT).abs() < 1e-9);
        assert_eq!(
            get("start", "food_above"),
            Some(f64::from(physics::SPRINT_FOOD_FLOOR))
        );
    }
}
