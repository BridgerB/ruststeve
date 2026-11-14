//! Issue 11b: a stair is refused before anything is dug when a cell it opens holds water or lava,
//! lava is beside one, its column is not loaded, or no full block is under the new feet
//! (brain/ruststeve/design/stairs-down.md). Checked on a flat model world the bot has joined.

use data::BlockKind;
use plan::stairs::refusal;
use sim::{Config, Sim, flat_world};

const CONFIG: Config = Config {
    view_distance: 2,
    simulation_distance: 2,
    peaceful: true,
};

fn state(k: BlockKind) -> u32 {
    k.info().unwrap().default_state
}

/// The bot stands on the grass at (0.5, 64, 0.5); a stair east opens (1, 65, 0), (1, 64, 0) and
/// (1, 63, 0), with its floor at (1, 62, 0).
fn world_with(set: &[((i32, i32, i32), BlockKind)]) -> world::World {
    let mut m = flat_world(2, 63).unwrap();
    for ((x, y, z), k) in set {
        m.set_block(*x, *y, *z, state(*k)).unwrap();
    }
    let run = bot::lockstep::run(Sim::new(CONFIG, m), "rs-st", 1, 30, 20, |_, _| {
        physics::Controls::default()
    });
    run.world
}

fn stair(w: &world::World) -> Option<String> {
    refusal(w, &[(1, 65, 0), (1, 64, 0), (1, 63, 0)], (1, 62, 0))
}

#[test]
fn a_clear_stair_is_taken() {
    assert_eq!(stair(&world_with(&[])), None);
}

#[test]
fn water_lava_and_drops_refuse_a_stair() {
    let water = stair(&world_with(&[((1, 63, 0), BlockKind::Water)])).unwrap();
    assert!(water.starts_with("water"), "{water}");
    let lava_beside = stair(&world_with(&[((2, 63, 0), BlockKind::Lava)])).unwrap();
    assert!(lava_beside.starts_with("lava beside"), "{lava_beside}");
    let drop = stair(&world_with(&[((1, 62, 0), BlockKind::Air)])).unwrap();
    assert!(drop.starts_with("no floor"), "{drop}");
}

#[test]
fn an_unloaded_column_refuses_a_stair() {
    let w = world_with(&[]);
    let far = refusal(
        &w,
        &[(400, 65, 0), (400, 64, 0), (400, 63, 0)],
        (400, 62, 0),
    )
    .unwrap();
    assert!(far.starts_with("unloaded"), "{far}");
}

#[test]
fn a_staircase_boxed_in_by_water_fills_its_way_out() {
    use plan::stairs::{Stairs, StairsStatus};
    // Water where the first stair's new feet would be, on all four sides.
    let mut m = flat_world(2, 63).unwrap();
    for (x, z) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
        m.set_block(x, 63, z, state(BlockKind::Water)).unwrap();
    }
    m.spawn = [0.5, 64.0, 0.5];
    let mut sim = Sim::new(CONFIG, m);
    sim.survival.give((data::ItemKind::StonePickaxe, 1));
    sim.survival.give((data::ItemKind::Cobblestone, 16));
    let mut st = Stairs::new((1, 0), 0);
    let mut ended = None;
    let run = bot::lockstep::run_acting(sim, "rs-fill", 1, 2000, 20, |w, t| {
        if ended.is_some() {
            return (physics::Controls::default(), Vec::new());
        }
        let (c, act, status) = st.step(w, t);
        if status != StairsStatus::Running {
            ended = Some(status);
        }
        (c, act.into_iter().collect())
    });
    assert!(st.fills >= 1, "never filled: {st:#?}");
    assert!(
        st.stairs >= 1,
        "no stair cut after filling: {ended:?} {st:#?}"
    );
    assert!(run.sim.violations.is_empty(), "{:?}", run.sim.violations);
}
