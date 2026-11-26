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

#[test]
fn an_eye_resting_on_leaves_9_up_is_fetched_with_a_pillar_and_the_pillar_dug_back() {
    let mut m = flat_world(4, GROUND).unwrap();
    // A canopy 9 over the feet: oak leaves at y 73, x 2 to 4, z -1 to 1 (its top at 74).
    for x in 2..=4 {
        for z in -1..=1 {
            m.set_block(x, 73, z, state(BlockKind::OakLeaves)).unwrap();
        }
    }
    let mut sim = Sim::new(CONFIG, m).seeded(19);
    sim.survival.give((ItemKind::Cobblestone, 32));
    // The overland bot holds pickaxes: cobblestone by hand is about 200 ticks a block (CI run
    // 45727672998 ran the way down out of its 400 ticks without one).
    sim.survival.give((ItemKind::StonePickaxe, 1));
    // The eye falls onto the leaves and rests at y 74: 9 over the feet, 3 east.
    sim.place_item((ItemKind::EnderEye, 1), [3.5, 74.5, 0.5]);
    let mut plan = Plan::new(Aim::Hold);
    let mut ledger: Vec<String> = Vec::new();
    let mut started = false;
    let mut top = FEET_Y;
    let run = bot::lockstep::run_acting(sim, "rs-fetch", 19, 1_600, 20, |w, _| {
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
    let story = ledger.join("\n");
    let has = |what: &str| ledger.iter().any(|l| l.contains(what));
    assert!(started, "the eye was never seen\n{story}");
    assert!(has("eye at rest high: fetch it"), "{story}");
    assert!(has("eye fetch: pillar up"), "{story}");
    assert!(has("eye fetched from a canopy"), "{story}");
    assert!(has("eye fetch: back down"), "{story}");
    assert!(!has("given up"), "{story}");
    assert!(top >= 72.0, "the pillar rose only to {top}\n{story}");
    assert_eq!(held_eyes(&run.world), 1, "{story}");
    let feet = run.world.body.unwrap().pos;
    assert!(feet[1] < FEET_Y + 0.5, "not back down: {feet:?}\n{story}");
    assert!(matches!(plan.task, Task::Idle), "{story}");
    assert_eq!(plan.eyes.kept, 1, "{story}");
}
