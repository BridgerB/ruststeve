//! Issue 17 (docs/problems/endermen-leave.md), closed loop on a Nether cavern of the model
//! (`sim::nether::Stage`): with 7 blaze rods held the race no longer ends; the endermen task
//! takes over, notes its provisions and travels on; a ticking enderman within 32 is the stage, met
//! only once the bot is sealed in a 2-high pocket; one at 44 is walked to; a provoked one is
//! answered with the pocket, never a swing. The model's endermen only stand (or strike, for the
//! provoked case): E4's enderman model is not needed for these.

use data::{BlockKind, EntityKind, ItemKind};
use plan::{Aim, Plan, TaskRecord};
use sim::life::Brain;
use sim::nether::{LAVA_SEA, Stage};
use sim::{Config, Sim};

const CONFIG: Config = Config {
    view_distance: 8,
    simulation_distance: 3,
    peaceful: false,
};
/// The cavern's floor (two over the lava sea, as the walls are) and ceiling.
const FLOOR: i32 = LAVA_SEA + 2;
const CEILING: i32 = FLOOR + 15;
/// Iron helmet, chestplate and leggings.
const IRON: f32 = 13.0;

fn line(r: TaskRecord) -> String {
    match r {
        TaskRecord::Started {
            at,
            task,
            what,
            why,
            ..
        } => format!(
            "{at:>6} start {task}: {} ({})",
            what.unwrap_or_default(),
            why.unwrap_or_default()
        ),
        TaskRecord::Ended {
            at,
            task,
            ticks,
            outcome,
            why,
            ..
        } => format!(
            "{at:>6} end   {task} after {ticks}: {} ({})",
            outcome.unwrap_or_default(),
            why.unwrap_or_default()
        ),
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

/// The frontier kit (7 rods, an iron sword, two stone pickaxes, netherrack, the armour) on a
/// netherrack floor at (0, FLOOR + 1, 0), `aim`ed, with what `summon` adds, for `ticks`.
fn cavern(aim: Aim, ticks: u64, summon: impl Fn(&mut Sim)) -> Run {
    cavern_on(aim, ticks, false, summon)
}

/// The same, on a soul sand floor when `soul` (a soul sand valley's ground, a block seen).
fn cavern_on(aim: Aim, ticks: u64, soul: bool, summon: impl Fn(&mut Sim)) -> Run {
    let goals = concat!(env!("CARGO_MANIFEST_DIR"), "/../../config/goals.toml");
    let table = std::sync::Arc::new(bot::goals_table(goals).unwrap());
    let mut plan = Plan::with_table(aim, table);
    let mut stage = Stage::new(3, FLOOR, CEILING);
    stage.spawn((0, FLOOR + 1, 0));
    if soul {
        stage.soul_sand((-48, 63), (-48, 63), FLOOR);
    }
    let mut sim = Sim::new(CONFIG, stage.build().unwrap()).seeded(5);
    sim.survival.life.armor = IRON;
    sim.survival.give((ItemKind::IronSword, 1));
    sim.survival.give((ItemKind::StonePickaxe, 1));
    sim.survival.give((ItemKind::StonePickaxe, 1));
    sim.survival.give((ItemKind::BlazeRod, 7));
    sim.survival.give((ItemKind::Netherrack, 64));
    summon(&mut sim);
    let mut ledger = Vec::new();
    let run = bot::lockstep::run_acting(sim, "rs-ender", 5, ticks, 20, |w, _| {
        // The notes are the planner's events (the shell writes them to the ledger); the task
        // records carry only starts and ends.
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

/// The model's endermen (E4's `sim::enderman`, the jar's stare): placed with no loot.
fn summon_enderman(sim: &mut Sim, at: [f64; 3]) -> i32 {
    let id = sim.summon_enderman(at);
    if let Some(m) = sim.survival.life.mobs.get_mut(&id) {
        m.loot = false;
    }
    id
}

/// Whether any of the model's endermen was ever stared at (metadata 18) or angered.
fn stared_or_angry(r: &Run) -> Vec<(i32, bool, bool)> {
    r.run
        .sim
        .survival
        .life
        .mobs
        .keys()
        .filter_map(|id| r.run.sim.ender(*id).map(|e| (*id, e.stared_at, e.angry)))
        .filter(|(_, s, a)| *s || *a)
        .collect()
}

/// Every cell beside the feet and the head, and over the head, is a block: the pocket is shut.
fn sealed(r: &Run) -> bool {
    let b = r.run.world.body.unwrap();
    let (x, y, z) = (
        b.pos[0].floor() as i32,
        b.pos[1].floor() as i32,
        b.pos[2].floor() as i32,
    );
    let solid = |p: (i32, i32, i32)| {
        r.run
            .sim
            .state_at(p.0, p.1, p.2)
            .and_then(data::block_of_state)
            .is_some_and(|k| k != BlockKind::Air)
    };
    [(1, 0), (-1, 0), (0, 1), (0, -1)]
        .iter()
        .all(|(dx, dz)| solid((x + dx, y, z + dz)) && solid((x + dx, y + 1, z + dz)))
        && solid((x, y + 2, z))
}

/// The race no longer ends at the rods (17c): no spawner on record, so no leave walk; the endermen
/// task takes over, writes its provisions note and travels on.
#[test]
fn seven_rods_held_go_on_to_the_endermen() {
    let r = cavern(Aim::Race, 600, |_| {});
    let story = r.story();
    assert!(!r.plan.met, "not met at the rods:\n{story}");
    assert_eq!(r.has("rods met"), 0, "{story}");
    assert!(r.has("rods held: on to the endermen") >= 1, "{story}");
    assert!(r.has("note  provisions") >= 1, "{story}");
    assert!(
        r.has("pickaxe uses left 262") >= 1,
        "two fresh stone pickaxes:\n{story}"
    );
    assert!(
        r.has("search tunnel leg") + r.has("explore") > 0,
        "the heading's fallback travels on:\n{story}"
    );
    assert_eq!(r.run.world.inventory.count(ItemKind::BlazeRod), 7);
}

/// The stage and the safe hold (17c.1): a ticking enderman 20 off; the bot seals a 2-high pocket
/// round itself before the aim is met, and never stares at it.
#[test]
fn a_ticking_enderman_near_is_met_in_a_sealed_pocket() {
    let r = cavern(Aim::Race, 600, |sim| {
        summon_enderman(sim, [20.5, f64::from(FLOOR + 1), 0.5]);
    });
    let story = r.story();
    assert_eq!(stared_or_angry(&r), Vec::new(), "never stared at:\n{story}");
    assert!(r.has("endermen reached") >= 1, "{story}");
    assert!(r.has("pocket block") >= 5, "{story}");
    assert!(r.has("endermen met") >= 1, "{story}");
    assert!(r.plan.met, "{story}");
    assert!(sealed(&r), "the pocket is shut:\n{story}");
    assert_eq!(r.has("near miss"), 0, "{story}");
}

/// The approach (17e.1): a ticking enderman 44 off is walked to within 32.
#[test]
fn a_ticking_enderman_at_44_is_walked_to() {
    let r = cavern(Aim::Race, 1500, |sim| {
        summon_enderman(sim, [44.5, f64::from(FLOOR + 1), 0.5]);
    });
    let story = r.story();
    assert_eq!(
        stared_or_angry(&r),
        Vec::new(),
        "walked at, never stared at:\n{story}"
    );
    assert!(r.has("approach enderman") >= 1, "{story}");
    assert!(r.has("endermen reached") >= 1, "{story}");
    assert!(r.plan.met, "{story}");
}

/// A provoked enderman (17e.3): one angered (here set so in the model: it chases and strikes) is
/// answered with a sealed pocket, never a swing. The aim met in the Nether leaves nothing else to
/// do, so the threat is the only row.
#[test]
fn a_provoked_enderman_is_answered_by_the_pocket_never_a_swing() {
    let r = cavern(Aim::Nether, 600, |sim| {
        let id = summon_enderman(sim, [5.5, f64::from(FLOOR + 1), 0.5]);
        if let Some(m) = sim.survival.life.mobs.get_mut(&id)
            && let Brain::Enderman(e) = &mut m.brain
        {
            e.angry = true;
        }
    });
    let story = r.story();
    assert!(r.has("provoked enderman") >= 1, "{story}");
    assert!(r.has("pocket block") >= 1, "{story}");
    assert_eq!(r.has("defend Enderman"), 0, "{story}");
    assert_eq!(r.plan.swings_sent, 0, "never a swing:\n{story}");
    println!("health at the end {:?}", r.run.world.player.health);
}

/// The soul sand valley wait (the decision of 2026-11-24): standing on soul sand with no enderman
/// near, the bot builds a nook of held filler round itself, its open side not toward a ghast that
/// ticks 30 blocks east, and waits there with progress notes.
#[test]
fn in_a_soul_sand_valley_the_bot_waits_in_a_nook_open_away_from_a_ghast() {
    let r = cavern_on(Aim::Race, 1400, true, |sim| {
        sim.summon(EntityKind::Ghast, [30.5, f64::from(FLOOR + 6), 0.5]);
    });
    let story = r.story();
    assert!(r.has("rods held: on to the endermen") >= 1, "{story}");
    assert!(r.has("nook at") >= 1, "{story}");
    assert_eq!(r.has("open to (1, 0)"), 0, "not toward the ghast:\n{story}");
    assert!(r.has("pocket block") >= 4, "{story}");
    assert!(r.has("waiting in the soul sand valley") >= 1, "{story}");
    assert!(!r.plan.met, "{story}");
    assert_eq!(
        r.has("search tunnel leg"),
        0,
        "no heading while it waits:\n{story}"
    );
}
