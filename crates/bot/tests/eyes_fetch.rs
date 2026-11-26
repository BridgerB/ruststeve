//! Issue 19c, a dropped eye at rest high (docs/problems/eyes-throw.md): Q4's overland races lost
//! both throws a bot made to drops resting 9 up in a canopy. The bot's own world, fed by the model
//! in lockstep: an eye on leaves 9 over the feet and 3 east; the plan walks under it, pillars up
//! with held cobblestone, picks it up, and digs the pillar back down. Development evidence only
//! (measurement-rules).

use data::{BlockKind, ItemKind};
use plan::{Aim, Plan, Task, TaskRecord};
use sim::{Config, Sim, flat_world};

const CONFIG: Config = Config {
    view_distance: 4,
    simulation_distance: 3,
    peaceful: true,
};
/// Grass at 64: the feet at 65.
const GROUND: i32 = 64;
const FEET_Y: f64 = 65.0;

fn state(k: BlockKind) -> u32 {
    k.info().map_or(0, |i| i.default_state)
}

fn line(r: TaskRecord) -> String {
    match r {
        TaskRecord::Started { at, task, what, .. } => {
            format!("{at:>6} start {task}: {}", what.unwrap_or_default())
        }
        TaskRecord::Ended {
            at, task, outcome, ..
        } => format!("{at:>6} end   {task}: {}", outcome.unwrap_or_default()),
        TaskRecord::Note { at, what, why } => format!("{at:>6} note  {what} ({why})"),
    }
}

fn held_eyes(w: &world::World) -> i32 {
    w.inventory
        .slots
        .values()
        .filter(|(k, _)| *k == ItemKind::EnderEye)
        .map(|(_, n)| n)
        .sum()
}

struct Fetch {
    ledger: Vec<String>,
    top: f64,
    run: bot::lockstep::Run,
    plan: Plan,
    started: bool,
}

impl Fetch {
    fn has(&self, what: &str) -> bool {
        self.ledger.iter().any(|l| l.contains(what))
    }

    fn story(&self) -> String {
        self.ledger.join("\n")
    }

    /// Fetched, the pillar dug, back on the ground with the eye.
    fn assert_fetched(&self) {
        let story = self.story();
        assert!(self.started, "the eye was never seen\n{story}");
        assert!(self.has("eye at rest high: fetch it"), "{story}");
        assert!(self.has("eye fetch: pillar up"), "{story}");
        // Never a pillar begun further off than the pickup reaches (race 45740169100 lane 0: one
        // begun 3.2 off waited 600 ticks with 0 blocks up).
        for l in self
            .ledger
            .iter()
            .filter(|l| l.contains("eye fetch: pillar up"))
        {
            let off: f64 = l
                .rsplit("; ")
                .next()
                .and_then(|s| s.strip_suffix(" off)"))
                .and_then(|s| s.parse().ok())
                .unwrap_or(f64::MAX);
            assert!(off <= 1.2, "a pillar begun {off} off: {l}\n{story}");
        }
        assert!(self.has("eye fetched from a canopy"), "{story}");
        assert!(self.has("eye fetch: back down"), "{story}");
        assert!(!self.has("given up"), "{story}");
        assert!(
            self.top >= 72.0,
            "the pillar rose only to {}\n{story}",
            self.top
        );
        assert_eq!(held_eyes(&self.run.world), 1, "{story}");
        let feet = self.run.world.body.unwrap().pos;
        assert!(feet[1] < FEET_Y + 0.5, "not back down: {feet:?}\n{story}");
        assert!(matches!(self.plan.task, Task::Idle), "{story}");
        assert_eq!(self.plan.eyes.kept, 1, "{story}");
    }
}

/// A canopy 9 over the feet: oak leaves at y 73 (its top at 74) over `xs` and z -1 to 1, the eye
/// let fall onto it at `(eye_x, 0.5)`; the plan fetches it from the start.
fn fetch(
    eye_x: f64,
    xs: std::ops::RangeInclusive<i32>,
    more: impl Fn(&mut sim::ModelWorld),
) -> Fetch {
    let mut m = flat_world(4, GROUND).unwrap();
    for x in xs {
        for z in -1..=1 {
            m.set_block(x, 73, z, state(BlockKind::OakLeaves)).unwrap();
        }
    }
    more(&mut m);
    let mut sim = Sim::new(CONFIG, m).seeded(19);
    sim.survival.give((ItemKind::Cobblestone, 32));
    // The overland bot holds pickaxes: cobblestone by hand is about 200 ticks a block (CI run
    // 45727672998 ran the way down out of its 400 ticks without one).
    sim.survival.give((ItemKind::StonePickaxe, 1));
    // The eye falls onto the leaves and rests at y 74: 9 over the feet.
    sim.place_item((ItemKind::EnderEye, 1), [eye_x, 74.5, 0.5]);
    let mut plan = Plan::new(Aim::Hold);
    let mut ledger: Vec<String> = Vec::new();
    let mut started = false;
    let mut top = FEET_Y;
    let run = bot::lockstep::run_acting(sim, "rs-fetch", 19, 2_000, 20, |w, _| {
        if let Some(b) = w.body {
            top = top.max(b.pos[1]);
        }
        if !started
            && let Some(id) = w
                .entities
                .iter()
                .find(|(_, e)| e.item().is_some_and(|(k, _)| k == ItemKind::EnderEye))
                .map(|(id, _)| *id)
            && let Some(t) = plan.fetch_eye_task(w, id, w.ticks)
        {
            plan.task = t;
            started = true;
        }
        let seen = plan.event_count;
        let c = plan.step(w);
        ledger.extend(
            plan.events_since(seen)
                .iter()
                .map(|e| format!("{:>6} note  {} ({})", e.tick, e.what, e.why)),
        );
        ledger.extend(plan.take_records().into_iter().map(line));
        (c, plan.take_acts())
    });
    Fetch {
        ledger,
        top,
        run,
        plan,
        started,
    }
}

#[test]
fn an_eye_resting_on_leaves_9_up_is_fetched_with_a_pillar_and_the_pillar_dug_back() {
    // 3 east of the feet.
    fetch(3.5, 2..=4, |_| {}).assert_fetched();
}

/// Q4's race 45732530352 lane 0: no walk within 4 of the drop (NoPath). A wall 2 high at x 2
/// across the whole loaded world, the eye on leaves 6 east beyond it: the fetch cuts through.
#[test]
fn an_eye_behind_a_wall_with_no_way_round_is_fetched_through_a_cut() {
    let f = fetch(6.5, 5..=7, |m| {
        for z in -80..=80 {
            for y in 65..=66 {
                m.set_block(2, y, z, state(BlockKind::Stone)).unwrap();
            }
        }
    });
    assert!(f.has("eye fetch: cut toward the drop"), "{}", f.story());
    f.assert_fetched();
}

/// Race 45740169100 lane 0: the cut's first step had no path (no floor under it). The wall test's
/// world with the ground just past the wall gone: the cut lays a floor there and steps on.
#[test]
fn a_cut_step_onto_a_missing_floor_lays_one() {
    let f = fetch(6.5, 5..=7, |m| {
        for z in -80..=80 {
            for y in 65..=66 {
                m.set_block(2, y, z, state(BlockKind::Stone)).unwrap();
            }
        }
        // The ground just past the wall gone (a pit 1 deep), where only the cut goes.
        for z in -80..=80 {
            m.set_block(3, 64, z, state(BlockKind::Air)).unwrap();
        }
    });
    assert!(f.has("eye fetch: cut toward the drop"), "{}", f.story());
    assert!(
        f.has("floors laid") && !f.has("(0 floors laid)"),
        "{}",
        f.story()
    );
    f.assert_fetched();
}

/// Race 45740169100 lane 0: a pillar begun 3.2 off the drop never reached the pickup. The ground
/// under the canopy is a block 2 high that no walk gets beside: the walk ends 3 off, the pillar is
/// refused there, and a cut takes it beside the column first.
#[test]
fn a_pillar_is_refused_3_off_and_a_cut_goes_beside_first() {
    let f = fetch(3.5, 2..=4, |m| {
        for x in 2..=4 {
            for z in -1..=1 {
                for y in 65..=66 {
                    m.set_block(x, y, z, state(BlockKind::Stone)).unwrap();
                }
            }
        }
    });
    assert!(f.has("eye fetch: cut toward the drop"), "{}", f.story());
    f.assert_fetched();
}
