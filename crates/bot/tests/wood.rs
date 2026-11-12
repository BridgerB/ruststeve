//! Issue 10c: the bot gets wood and crafts in the 2x2 grid against the server model, on the
//! plains landing captured on the race seed: from nothing to four sticks and a crafting table,
//! every log dug and every click answered by the model's server rules (crates/sim/src/survival.rs).

use plan::{Aim, Plan};
use protocol::chunk::Column;
use sim::{Config, ModelColumn, ModelWorld, Sim};

const CONFIG: Config = Config {
    view_distance: 8,
    simulation_distance: 3,
    peaceful: true,
};

fn plains() -> ModelWorld {
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../plan/tests/fixtures");
    let mut columns = std::collections::BTreeMap::new();
    for e in std::fs::read_dir(dir).unwrap().flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        let Some(rest) = name.strip_prefix("plains_") else {
            continue;
        };
        let p: Vec<i32> = rest
            .trim_end_matches(".bin")
            .split('_')
            .map(|v| v.parse().unwrap())
            .collect();
        let column = Column::decode(&std::fs::read(e.path()).unwrap()).unwrap();
        columns.insert(
            (p[0], p[1]),
            ModelColumn::generated(p[0], p[1], column).unwrap(),
        );
    }
    ModelWorld {
        columns,
        spawn: [160.5, 67.0, 0.5],
        dimension: "minecraft:overworld".to_owned(),
        dimension_type: 0,
    }
}

#[test]
fn the_bot_makes_the_kit_from_nothing() {
    let goals = concat!(env!("CARGO_MANIFEST_DIR"), "/../../config/goals.toml");
    let table = std::sync::Arc::new(bot::goals_table(goals).unwrap());
    let mut p = Plan::with_table(Aim::Table, table);
    let run = bot::lockstep::run_acting(
        Sim::new(CONFIG, plains()).seeded(1),
        "rs-w",
        1,
        6000,
        20,
        |w, _| {
            let c = p.step(w);
            (c, p.take_acts())
        },
    );
    let w = &run.world;
    assert!(p.met, "not met: {:#?}", p.events);
    let held = plan::held(w);
    assert!(
        held.get(&data::ItemKind::CraftingTable)
            .copied()
            .unwrap_or(0)
            >= 1
    );
    assert!(held.get(&data::ItemKind::Stick).copied().unwrap_or(0) >= 4);
    assert_eq!(w.stage_reached, Some(world::Stage::Table));
    assert!(
        p.digs >= 2 && p.crafts >= 4,
        "digs {} crafts {}",
        p.digs,
        p.crafts
    );
    assert_eq!(run.sim.corrections, 0);
    assert!(run.sim.violations.is_empty(), "{:?}", run.sim.violations);
    assert_eq!(w.clicks_sent, w.clicks_answered, "every click answered");
}
