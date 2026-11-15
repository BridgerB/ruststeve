//! Issue 14b: a bucket filled at surface water, closed loop against the model on a flat world
//! (grass at y 63): the source seen with air over it is kept in memory's pool, walked to, and
//! scooped along the look; cave water under a ceiling is never kept
//! (brain/ruststeve/design/buckets-and-fluids.md).

use data::{BlockKind, ItemKind};
use plan::{Aim, Plan};
use sim::{Config, Sim, flat_world};

const CONFIG: Config = Config {
    view_distance: 2,
    simulation_distance: 2,
    peaceful: true,
};

fn state(k: BlockKind) -> u32 {
    k.info().unwrap().default_state
}

fn run(set: &[((i32, i32, i32), BlockKind)], aim: Aim, ticks: u64) -> (Plan, bot::lockstep::Run) {
    let mut m = flat_world(2, 63).unwrap();
    for ((x, y, z), k) in set {
        m.set_block(*x, *y, *z, state(*k)).unwrap();
    }
    m.spawn = [0.5, 64.0, 0.5];
    let goals = concat!(env!("CARGO_MANIFEST_DIR"), "/../../config/goals.toml");
    let table = std::sync::Arc::new(bot::goals_table(goals).unwrap());
    let mut p = Plan::with_table(aim, table);
    let mut sim = Sim::new(CONFIG, m);
    sim.survival.give((ItemKind::Bucket, 2));
    // fill-bucket holds a pickaxe, as the kit start does.
    sim.survival.give((ItemKind::StonePickaxe, 1));
    let run = bot::lockstep::run_acting(sim, "rs-scp", 1, ticks, 0, |w, _| {
        let c = p.step(w);
        (c, p.take_acts())
    });
    (p, run)
}

/// A pond 3 wide, a block deep, its middle `(x, 63, z)`.
fn pond(x: i32, z: i32) -> Vec<((i32, i32, i32), BlockKind)> {
    (x - 1..=x + 1)
        .flat_map(|a| (z - 1..=z + 1).map(move |b| ((a, 63, b), BlockKind::Water)))
        .collect()
}

#[test]
fn a_pond_ten_blocks_off_fills_a_bucket() {
    let (p, run) = run(&pond(10, 4), Aim::WaterBucket, 800);
    assert!(p.met, "not met: {:#?}", p.events);
    assert_eq!(plan::held(&run.world).get(&ItemKind::WaterBucket), Some(&1));
    assert!(run.sim.violations.is_empty(), "{:?}", run.sim.violations);
}

#[test]
fn the_portal_kit_keeps_one_bucket_empty() {
    let (p, run) = run(&pond(-8, -6), Aim::PortalKit, 800);
    assert!(p.met, "not met: {:#?}", p.events);
    let held = plan::held(&run.world);
    assert_eq!(held.get(&ItemKind::WaterBucket), Some(&1));
    assert_eq!(held.get(&ItemKind::Bucket), Some(&1));
}

#[test]
fn water_under_a_ceiling_is_never_kept() {
    // A pond roofed with stone at y 64: no air over any source.
    let mut set = pond(6, 0);
    set.extend((5..=7).flat_map(|a| (-1..=1).map(move |b| ((a, 64, b), BlockKind::Stone))));
    let (_, run) = run(&set, Aim::Hold, 60);
    let dim = run.world.dimension.as_ref().unwrap().name.clone();
    assert!(
        run.world
            .memory
            .sources_near(&dim, memory::Fluid::Water, 0, 0, 2)
            .is_empty(),
        "kept roofed water"
    );
}
