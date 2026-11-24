//! Issue 13a: the smelt task against the model on a flat world with a furnace two blocks east
//! (brain/ruststeve/design/smelting.md): fuel first and never more than the batch burns, the
//! input, the results shift-clicked out as they cook.

use data::{BlockKind, ItemKind};
use plan::smelt::{Smelt, SmeltStatus};
use sim::{Config, Sim, flat_world};

const CONFIG: Config = Config {
    view_distance: 2,
    simulation_distance: 2,
    peaceful: true,
};

fn smelt(give: &[(ItemKind, i32)], count: i32) -> (bot::lockstep::Run, Option<SmeltStatus>) {
    smelt_for(give, count, 1200)
}

fn smelt_for(
    give: &[(ItemKind, i32)],
    count: i32,
    ticks: u64,
) -> (bot::lockstep::Run, Option<SmeltStatus>) {
    let mut m = flat_world(2, 63).unwrap();
    m.set_block(2, 64, 0, BlockKind::Furnace.info().unwrap().default_state)
        .unwrap();
    m.spawn = [0.5, 64.0, 0.5];
    let mut sim = Sim::new(CONFIG, m);
    for g in give {
        sim.survival.give(*g);
    }
    let recipe = data::generated::recipes::SMELTING
        .iter()
        .find(|r| r.id == "iron_ingot_from_smelting_raw_iron")
        .or_else(|| {
            data::generated::recipes::SMELTING
                .iter()
                .find(|r| r.result == ItemKind::IronIngot)
        })
        .unwrap();
    let mut task: Option<Smelt> = None;
    let mut ended = None;
    let run = bot::lockstep::run_acting(sim, "rs-smelt", 1, ticks, 20, |w, t| {
        if ended.is_some() {
            return (physics::Controls::default(), Vec::new());
        }
        let s = task.get_or_insert_with(|| Smelt::new(w, (2, 64, 0), recipe, count, t).unwrap());
        let (c, act, status) = s.step(w, t);
        if status != SmeltStatus::Running {
            ended = Some(status);
        }
        (c, act.into_iter().collect())
    });
    (run, ended)
}

fn held(run: &bot::lockstep::Run, item: ItemKind) -> i32 {
    plan::held(&run.world).get(&item).copied().unwrap_or(0)
}

#[test]
fn coal_smelts_three_raw_iron() {
    let (run, ended) = smelt(&[(ItemKind::RawIron, 3), (ItemKind::Coal, 2)], 3);
    assert_eq!(ended, Some(SmeltStatus::Done));
    assert_eq!(held(&run, ItemKind::IronIngot), 3);
    // One coal burns 1600 ticks: the second is kept.
    assert_eq!(held(&run, ItemKind::Coal), 1);
    assert!(run.sim.violations.is_empty(), "{:?}", run.sim.violations);
}

#[test]
fn planks_burn_only_what_the_batch_needs() {
    let (run, ended) = smelt(&[(ItemKind::RawIron, 3), (ItemKind::OakPlanks, 4)], 3);
    assert_eq!(ended, Some(SmeltStatus::Done));
    assert_eq!(held(&run, ItemKind::IronIngot), 3);
    // 600 ticks of cooking, 300 a plank: two burned, two kept.
    assert_eq!(held(&run, ItemKind::OakPlanks), 2);
    assert!(run.sim.violations.is_empty(), "{:?}", run.sim.violations);
}

/// Overworld-leg wall 4 (race 45343519424): five oak and twelve birch planks, and a batch of nine
/// that neither five alone covers. The first kind is loaded and burned, then the next: nine
/// results, seven planks burned in all, and no fuel clicked in and out.
#[test]
fn two_kinds_of_planks_fuel_one_batch() {
    let (run, ended) = smelt_for(
        &[
            (ItemKind::RawIron, 9),
            (ItemKind::OakPlanks, 5),
            (ItemKind::BirchPlanks, 12),
        ],
        9,
        2600,
    );
    assert_eq!(ended, Some(SmeltStatus::Done));
    assert_eq!(held(&run, ItemKind::IronIngot), 9);
    let planks = held(&run, ItemKind::OakPlanks) + held(&run, ItemKind::BirchPlanks);
    // Five oak burn 1,500 ticks: seven results and half of the eighth. Two are left to cook,
    // 400 ticks: two birch. Seven planks burned.
    assert_eq!(planks, 10);
    assert!(run.sim.violations.is_empty(), "{:?}", run.sim.violations);
}
