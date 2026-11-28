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
            v => v,
        };
        c.sprint = verdict == Verdict::Sprint;
        self.pace.last = c.sprint;
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
        let w = flat();
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
        for _ in 0..3 {
            let c = plan.pace_gate(&w, ahead());
            assert!(c.sprint);
        }
        let whats: Vec<&str> = plan.events.iter().map(|e| e.what.as_str()).collect();
        assert_eq!(whats, vec!["sprint refused, bridge", "sprint begun, walk"]);
        assert_eq!(plan.pace.sprint_ticks, 3);
        assert_eq!(plan.pace.refused_ticks, 3);
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
