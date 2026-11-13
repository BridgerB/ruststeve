//! Issue 12c: the `eat` and `hunt` always-rows against the model on a flat world, on Normal (on
//! peaceful the food never drops): hungry with bread held, the bot eats it; hungry with nothing,
//! it kills a cow with its sword, picks up the beef and eats it raw
//! (brain/ruststeve/design/food-and-hunting.md).

use data::{EntityKind, ItemKind};
use plan::{Aim, Plan};
use sim::{Config, Sim, flat_world};

const CONFIG: Config = Config {
    view_distance: 2,
    simulation_distance: 2,
    peaceful: false,
};

fn hold_plan() -> Plan {
    let goals = concat!(env!("CARGO_MANIFEST_DIR"), "/../../config/goals.toml");
    let table = std::sync::Arc::new(bot::goals_table(goals).unwrap());
    Plan::with_table(Aim::Hold, table)
}

fn hungry_sim(food: i32, give: &[(ItemKind, i32)]) -> Sim {
    let mut m = flat_world(2, 63).unwrap();
    m.spawn = [0.5, 64.0, 0.5];
    let mut sim = Sim::new(CONFIG, m).seeded(7);
    sim.survival.life.food = food;
    sim.survival.life.saturation = 0.0;
    for g in give {
        sim.survival.give(*g);
    }
    sim
}

#[test]
fn hungry_with_bread_it_eats() {
    let mut p = hold_plan();
    let sim = hungry_sim(10, &[(ItemKind::Bread, 3)]);
    let run = bot::lockstep::run_acting(sim, "rs-eat", 1, 400, 20, |w, _| {
        let c = p.step(w);
        (c, p.take_acts())
    });
    let w = &run.world;
    assert!(p.meals >= 1, "no meal: {:#?}", p.events);
    let food = w.player.food.unwrap_or(0);
    assert!(food >= 14, "food {food}: {:#?}", p.events);
    assert!(plan::held(w).get(&ItemKind::Bread).copied().unwrap_or(0) < 3);
    assert!(run.sim.violations.is_empty(), "{:?}", run.sim.violations);
}

#[test]
fn hungry_with_nothing_it_hunts_a_cow_and_eats_the_beef() {
    let mut p = hold_plan();
    let mut sim = hungry_sim(10, &[(ItemKind::StoneSword, 1)]);
    sim.summon(EntityKind::Cow, [6.5, 64.0, 3.5]);
    let run = bot::lockstep::run_acting(sim, "rs-hunt", 1, 1200, 20, |w, _| {
        let c = p.step(w);
        (c, p.take_acts())
    });
    let w = &run.world;
    assert!(p.kills >= 1, "no kill: {:#?}", p.events);
    assert!(p.meals >= 1, "no meal: {:#?}", p.events);
    assert!(
        w.player.food.unwrap_or(0) > 10,
        "food {:?}: {:#?}",
        w.player.food,
        p.events
    );
    assert!(!run.died);
    assert!(run.sim.violations.is_empty(), "{:?}", run.sim.violations);
}
