//! Issue 13b: toward the iron kit against the model on a flat world with coal ore, iron ore and
//! gravel exposed at the surface (brain/ruststeve/design/fuel-and-smelt-needs.md): from a stone
//! start's kit (stone and wooden pickaxes, a sword, a table, logs, sticks, four cobblestone), the bot places the table and a
//! furnace, mines coal and iron, smelts the ingots, crafts an iron pickaxe and a bucket, digs flint
//! out of gravel and makes flint and steel, with no death and nothing the server refuses.

use data::{BlockKind, ItemKind};
use plan::{Aim, Plan};
use sim::{Config, Sim, flat_world};

const CONFIG: Config = Config {
    view_distance: 2,
    simulation_distance: 2,
    peaceful: true,
};

#[test]
fn from_the_stone_kit_to_the_iron_kit() {
    let mut m = flat_world(2, 63).unwrap();
    let st = |k: BlockKind| k.info().unwrap().default_state;
    // Low walls of ore standing on the ground east of the spawn, at the feet's height (a dig
    // walk stands beside a block at the feet's height or up to two above, never below).
    for x in 4..7 {
        m.set_block(x, 64, 3, st(BlockKind::CoalOre)).unwrap();
    }
    for x in 4..12 {
        m.set_block(x, 64, -3, st(BlockKind::IronOre)).unwrap();
    }
    for x in 4..8 {
        m.set_block(x, 64, 6, st(BlockKind::Gravel)).unwrap();
    }
    m.spawn = [0.5, 64.0, 0.5];
    let goals = concat!(env!("CARGO_MANIFEST_DIR"), "/../../config/goals.toml");
    let table = std::sync::Arc::new(bot::goals_table(goals).unwrap());
    let mut p = Plan::with_table(Aim::IronKit, table);
    let mut sim = Sim::new(CONFIG, m).seeded(5);
    // The `stone` start's kit (config/starts.toml): what an issue-12 bot holds after stone tools.
    for g in [
        (ItemKind::StonePickaxe, 1),
        (ItemKind::StoneSword, 1),
        (ItemKind::WoodenPickaxe, 1),
        (ItemKind::CraftingTable, 1),
        (ItemKind::OakLog, 3),
        (ItemKind::Stick, 3),
        (ItemKind::Cobblestone, 4),
    ] {
        sim.survival.give(g);
    }
    let mut seen = 0;
    let mut log: Vec<String> = Vec::new();
    let run = bot::lockstep::run_acting(sim, "rs-iron", 1, 20000, 20, |w, _| {
        let c = p.step(w);
        for e in p.events_since(seen) {
            log.push(format!("{} {} ({})", e.tick, e.what, e.why));
        }
        seen = p.event_count;
        (c, p.take_acts())
    });
    if std::env::var("IRON_LOG").is_ok() {
        eprintln!("{}", log.join("\n"));
    }
    let held = plan::held(&run.world);
    let has = |k| held.get(&k).copied().unwrap_or(0) >= 1;
    assert!(
        p.met,
        "not met; held {held:?}; failures {:?}; last events {:#?}",
        p.failures,
        p.events.iter().rev().take(25).collect::<Vec<_>>()
    );
    assert!(has(ItemKind::IronPickaxe) && has(ItemKind::Bucket) && has(ItemKind::FlintAndSteel));
    assert!(!run.died);
    assert!(run.sim.violations.is_empty(), "{:?}", run.sim.violations);
}

#[test]
fn flint_is_dug_out_of_held_gravel() {
    // No gravel in the world: the held gravel is put down and dug again until it drops flint.
    let mut m = flat_world(2, 63).unwrap();
    m.spawn = [0.5, 64.0, 0.5];
    let goals = concat!(env!("CARGO_MANIFEST_DIR"), "/../../config/goals.toml");
    let table = std::sync::Arc::new(bot::goals_table(goals).unwrap());
    let mut p = Plan::with_table(Aim::FlintAndSteel, table);
    let mut sim = Sim::new(CONFIG, m).seeded(11);
    for g in [
        (ItemKind::StonePickaxe, 1),
        (ItemKind::IronIngot, 1),
        (ItemKind::Gravel, 30),
    ] {
        sim.survival.give(g);
    }
    let run = bot::lockstep::run_acting(sim, "rs-flint", 1, 12000, 20, |w, _| {
        let c = p.step(w);
        (c, p.take_acts())
    });
    let held = plan::held(&run.world);
    assert!(
        p.met,
        "not met; held {held:?}; failures {:?}; last events {:#?}",
        p.failures,
        p.events.iter().rev().take(12).collect::<Vec<_>>()
    );
    assert!(held.get(&ItemKind::FlintAndSteel).copied().unwrap_or(0) >= 1);
    assert!(
        p.events.iter().any(|e| e.what.starts_with("redig")),
        "{:#?}",
        p.events
    );
    assert!(run.sim.violations.is_empty(), "{:?}", run.sim.violations);
}
