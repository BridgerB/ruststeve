//! Issue 11b: from the `table` start's kit (a crafting table and four oak logs) on the plains
//! landing captured on the race seed, against the server model: place the table, craft a wooden
//! pickaxe at it, get cobblestone (exposed stone, or stairs down), then a stone pickaxe and a
//! sword, with no death and nothing the server refuses.

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
fn from_the_table_kit_to_stone_tools() {
    let goals = concat!(env!("CARGO_MANIFEST_DIR"), "/../../config/goals.toml");
    let table = std::sync::Arc::new(bot::goals_table(goals).unwrap());
    let mut p = Plan::with_table(Aim::StoneTools, table);
    let mut sim = Sim::new(CONFIG, plains()).seeded(1);
    sim.survival.give((data::ItemKind::CraftingTable, 1));
    sim.survival.give((data::ItemKind::OakLog, 4));
    let run = bot::lockstep::run_acting(sim, "rs-s", 1, 12000, 20, |w, _| {
        let c = p.step(w);
        (c, p.take_acts())
    });
    let w = &run.world;
    assert!(p.met, "not met: {:#?}", p.events);
    let held = plan::held(w);
    assert!(
        held.get(&data::ItemKind::StonePickaxe)
            .copied()
            .unwrap_or(0)
            >= 1
    );
    assert!(held.get(&data::ItemKind::StoneSword).copied().unwrap_or(0) >= 1);
    assert!(!run.died);
    assert!(run.sim.violations.is_empty(), "{:?}", run.sim.violations);
}
