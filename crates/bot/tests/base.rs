//! Issue 18d, the forest base closed loop (docs/problems/pearls-base.md): the bot's own world,
//! fed by the model in lockstep, in a Nether cavern laid as a warped forest (warped nylium, huge
//! fungus stems), with the development aim `base`. It digs stems by hand, places a table, smelts
//! its raw iron with its coal, makes the swords and pickaxes it is short of and a shield, and ends
//! with the kit held and its rods kept. Development evidence only (measurement-rules).

use data::{BlockKind, ItemKind};
use plan::{Aim, Plan, TaskRecord};
use sim::nether::{Biome, Stage};
use sim::{Config, Sim};

const CONFIG: Config = Config {
    view_distance: 8,
    simulation_distance: 3,
    peaceful: false,
};
const FLOOR: i32 = 40;
/// Iron helmet, chestplate and leggings.
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

    fn held(&self, k: ItemKind) -> i64 {
        self.run.world.inventory.count(k)
    }
}

/// A warped forest cavern: nylium floor, eight huge fungus stems five high round the spawn.
fn forest() -> Stage {
    let mut stage = Stage::new(3, FLOOR, FLOOR + 20);
    stage
        .biome((-48, 63), (-48, 63), Biome::WarpedForest)
        .fill(
            (-48, 63),
            (FLOOR, FLOOR),
            (-48, 63),
            BlockKind::WarpedNylium,
        )
        .spawn((0, FLOOR + 1, 0));
    for (x, z) in [
        (5, 0),
        (-6, 3),
        (2, -7),
        (-3, 8),
        (9, 6),
        (-9, -5),
        (12, -2),
        (0, 12),
    ] {
        stage.fill(
            (x, x),
            (FLOOR + 1, FLOOR + 5),
            (z, z),
            BlockKind::WarpedStem,
        );
        stage.fill(
            (x - 1, x + 1),
            (FLOOR + 6, FLOOR + 6),
            (z - 1, z + 1),
            BlockKind::WarpedWartBlock,
        );
    }
    stage
}

fn base_run(ticks: u64, kit: &[(ItemKind, i32)]) -> Run {
    let goals = concat!(env!("CARGO_MANIFEST_DIR"), "/../../config/goals.toml");
    let table = std::sync::Arc::new(bot::goals_table(goals).unwrap());
    let mut plan = Plan::with_table(Aim::Base, table);
    let mut sim = Sim::new(CONFIG, forest().build().unwrap()).seeded(7);
    sim.survival.life.armor = IRON;
    for k in kit {
        sim.survival.give(*k);
    }
    let mut ledger = Vec::new();
    let run = bot::lockstep::run_acting(sim, "rs-base", 7, ticks, 20, |w, _| {
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

/// The frontier's makings (race-45481633896 lane 3) with a stone sword (16 kills) for its worn
/// iron one: two stone pickaxes (262 uses: the pickaxe is skipped), a furnace, 7 raw iron, one
/// coal, 3 sticks, cobblestone, netherrack, 7 rods.
fn frontier_kit(coal: bool) -> Vec<(ItemKind, i32)> {
    let mut kit = vec![
        (ItemKind::StoneSword, 1),
        (ItemKind::StonePickaxe, 1),
        (ItemKind::StonePickaxe, 1),
        (ItemKind::Furnace, 1),
        (ItemKind::RawIron, 7),
        (ItemKind::Stick, 3),
        (ItemKind::Cobblestone, 64),
        (ItemKind::Cobblestone, 62),
        (ItemKind::Netherrack, 64),
        (ItemKind::BlazeRod, 7),
    ];
    if coal {
        kit.push((ItemKind::Coal, 1));
    }
    kit
}

/// With the coal: stems, the table, 7 ingots, an iron sword (16 + 35 kills), the shield; the
/// pickaxe and the stone swords skipped; the rods kept; the kit held.
#[test]
fn the_frontier_kit_makes_an_iron_sword_and_a_shield_in_the_forest() {
    let r = base_run(9000, &frontier_kit(true));
    let story = r.story();
    assert!(r.plan.met, "base met:\n{story}");
    assert!(r.has("base over: the kit is held") >= 1, "{story}");
    for s in [
        "wood",
        "table",
        "wood picks",
        "smelt",
        "iron sword",
        "shield",
    ] {
        assert_eq!(r.has(&format!("base: {s} done")), 1, "{s}:\n{story}");
    }
    assert_eq!(r.has("base: pickaxe skipped"), 1, "{story}");
    assert_eq!(r.has("base: stone swords skipped"), 1, "{story}");
    assert_eq!(r.has("gave up"), 0, "{story}");
    assert_eq!(r.held(ItemKind::IronSword), 1, "{story}");
    assert_eq!(r.held(ItemKind::Shield), 1, "{story}");
    assert_eq!(
        r.held(ItemKind::WoodenPickaxe),
        3,
        "the hunt's filler pickaxes:\n{story}"
    );
    assert_eq!(
        r.held(ItemKind::IronIngot),
        4,
        "7 less the sword and the shield:\n{story}"
    );
    assert_eq!(r.held(ItemKind::BlazeRod), 7, "{story}");
    assert_eq!(r.held(ItemKind::RawIron), 0, "{story}");
}

/// Lane 2's case: no coal (the raw iron kept), a shield held, no pickaxe: a stone sword (16 + 16
/// kills) and a stone pickaxe (131 uses) from the cobblestone.
#[test]
fn without_coal_the_base_makes_stone_tools() {
    let mut kit: Vec<(ItemKind, i32)> = frontier_kit(false)
        .into_iter()
        .filter(|(k, _)| *k != ItemKind::StonePickaxe)
        .collect();
    kit.push((ItemKind::Shield, 1));
    let r = base_run(9000, &kit);
    let story = r.story();
    assert!(r.plan.met, "base met:\n{story}");
    assert!(r.has("base over: the kit is held") >= 1, "{story}");
    assert_eq!(r.has("base: smelt skipped"), 1, "{story}");
    assert_eq!(r.has("base: iron sword skipped"), 1, "{story}");
    assert_eq!(r.has("base: stone swords done"), 1, "{story}");
    assert_eq!(r.has("base: pickaxe done"), 1, "{story}");
    assert_eq!(r.has("base: wood picks done"), 1, "{story}");
    assert_eq!(r.held(ItemKind::WoodenPickaxe), 3, "{story}");
    assert_eq!(r.held(ItemKind::StoneSword), 2, "{story}");
    assert_eq!(r.held(ItemKind::StonePickaxe), 1, "{story}");
    assert_eq!(r.held(ItemKind::RawIron), 7, "kept: {story}");
    assert_eq!(r.held(ItemKind::BlazeRod), 7, "{story}");
}
