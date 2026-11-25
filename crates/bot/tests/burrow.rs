//! Issue 18e and 18f (docs/problems/pearls-hunt.md), closed loop on the model's warped forest: a
//! hunting bot finds a burrow site by its feet, builds the shell from held cobblestone one block a
//! 10 ticks, and from the mouth provokes a calm ticking enderman in line until its creepy flag,
//! then steps back to B. Development evidence only (measurement-rules): the race is the metric.
//! The fight itself waits on agent P4's model (endermen walking to a mouth, crates/bot/tests/
//! pearls*.rs).

use data::{BlockKind, ItemKind};
use plan::burrow::Burrow;
use plan::{Aim, Plan, TaskRecord};
use sim::nether::{Biome, Stage};
use sim::{Config, Sim};

const CONFIG: Config = Config {
    view_distance: 8,
    simulation_distance: 3,
    peaceful: false,
};
const FLOOR: i32 = 40;
const IRON: f32 = 13.0;

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

struct Run {
    plan: Plan,
    run: bot::lockstep::Run,
    ledger: Vec<String>,
}

impl Run {
    fn has(&self, what: &str) -> usize {
        self.ledger.iter().filter(|l| l.contains(what)).count()
    }

    fn story(&self) -> String {
        self.ledger.join("\n")
    }
}

/// A warped forest cavern round the origin, the frontier's kit (an iron sword, two stone
/// pickaxes, cobblestone and netherrack, 7 rods, iron armour), the planner hunting.
fn forest(ticks: u64, summon: impl Fn(&mut Sim)) -> Run {
    forest_with(ticks, |_| {}, summon)
}

/// `forest`, with `lay` laying more of the stage first.
fn forest_with(ticks: u64, lay: impl Fn(&mut Stage), summon: impl Fn(&mut Sim)) -> Run {
    let goals = concat!(env!("CARGO_MANIFEST_DIR"), "/../../config/goals.toml");
    let table = std::sync::Arc::new(bot::goals_table(goals).unwrap());
    let mut plan = Plan::with_table(Aim::Race, table);
    plan.ender.left = true;
    plan.ender.phase = plan::endermen::EnderPhase::Hunt;
    // The base (P5's row 6) waits for stems the model forest has none of: over, the burrow next.
    plan.base.over = true;
    let mut stage = Stage::new(4, FLOOR, FLOOR + 20);
    stage
        .biome((-64, 79), (-64, 79), Biome::WarpedForest)
        .spawn((0, FLOOR + 1, 0));
    lay(&mut stage);
    let mut sim = Sim::new(CONFIG, stage.build().unwrap()).seeded(11);
    sim.survival.life.armor = IRON;
    sim.survival.give((ItemKind::IronSword, 1));
    sim.survival.give((ItemKind::StonePickaxe, 1));
    sim.survival.give((ItemKind::StonePickaxe, 1));
    sim.survival.give((ItemKind::BlazeRod, 7));
    sim.survival.give((ItemKind::Cobblestone, 64));
    sim.survival.give((ItemKind::Netherrack, 64));
    summon(&mut sim);
    let mut ledger = Vec::new();
    let run = bot::lockstep::run_acting(sim, "rs-pearls", 11, ticks, 20, |w, _| {
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
    Run { plan, run, ledger }
}

fn solid(r: &Run, c: [i32; 3]) -> bool {
    r.run
        .sim
        .state_at(c[0], c[1], c[2])
        .and_then(data::block_of_state)
        .is_some_and(|k| k != BlockKind::Air)
}

#[test]
fn a_hunting_bot_builds_its_burrow_by_its_feet() {
    let r = forest(600, |_| {});
    let story = r.story();
    assert_eq!(r.has("pearls hunt begins"), 1, "{story}");
    assert_eq!(r.has("burrow site at"), 1, "{story}");
    assert_eq!(r.has("burrow built at"), 1, "{story}");
    let b: Burrow = r.plan.pearls.burrow.unwrap();
    for c in b.shell() {
        assert!(solid(&r, c), "shell cell {c:?} in the model:\n{story}");
    }
    for c in [b.b, b.m, b.o] {
        assert!(!solid(&r, c), "{c:?} open:\n{story}");
    }
    assert!(r.plan.pearls.built);
    assert_eq!(r.run.world.player.health, Some(20.0), "no damage:\n{story}");
}

#[test]
fn a_calm_enderman_in_line_is_provoked_from_the_mouth() {
    // The site at the feet faces east (the first side tried on open floor): one 18 east, penned
    // two high so its eye shows over the pen (the model's endermen stroll: one at 24 had
    // wandered out of sight by the build's end).
    let pen = |st: &mut Stage| {
        st.fill(
            (17, 19),
            (FLOOR + 1, FLOOR + 2),
            (-1, 1),
            BlockKind::Netherrack,
        )
        .fill((18, 18), (FLOOR + 1, FLOOR + 3), (0, 0), BlockKind::Air);
    };
    let r = forest_with(1600, pen, |sim| {
        let id = sim.summon_enderman([18.5, f64::from(FLOOR + 1), 0.5]);
        if let Some(m) = sim.survival.life.mobs.get_mut(&id) {
            m.loot = false;
        }
    });
    let story = r.story();
    assert_eq!(r.has("burrow built at"), 1, "{story}");
    assert!(r.has("provoke enderman") >= 1, "{story}");
    assert!(r.has("provoked enderman") >= 1, "{story}");
    assert_eq!(r.has("near miss"), 0, "{story}");
    // The fight's outcome is printed for the model work; not yet asserted.
    println!("{story}");
}
