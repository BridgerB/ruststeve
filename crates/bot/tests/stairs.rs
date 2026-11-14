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
    // A drop deeper than three blocks under the stair is refused ...
    let deep: Vec<_> = (58..=62).map(|y| ((1, y, 0), BlockKind::Air)).collect();
    let drop = stair(&world_with(&deep)).unwrap();
    assert!(drop.starts_with("no floor"), "{drop}");
    // ... a short one (a landing a block down) is taken (issue 13b).
    assert_eq!(stair(&world_with(&[((1, 62, 0), BlockKind::Air)])), None);
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
