//! Issue 14c: the portal cast closed loop against the model (brain/ruststeve/design/portal-cast.md):
//! the `kit` start's items on a flat world (grass at y 63) with a stone cliff, a lava pool sunk
//! in the ground in front of it and a pond aside; the planner fills its water bucket, chooses a
//! site beside the lava seen exposed, cuts the frame into the cliff, casts the ten cells, opens
//! the inside and lights it.

use data::{BlockKind, ItemKind};
use plan::{Aim, Plan};
use sim::{Config, Sim, flat_world};
use world::BlockPos;

const CONFIG: Config = Config {
    view_distance: 3,
    simulation_distance: 3,
    peaceful: true,
};

fn state(k: BlockKind) -> u32 {
    k.info().unwrap().default_state
}

fn kit() -> Vec<(ItemKind, i32)> {
    vec![
        (ItemKind::IronPickaxe, 1),
        (ItemKind::Bucket, 2),
        (ItemKind::FlintAndSteel, 1),
        (ItemKind::StonePickaxe, 1),
        (ItemKind::Stick, 8),
        (ItemKind::Cobblestone, 16),
    ]
}

fn world() -> sim::ModelWorld {
    let mut m = flat_world(3, 63).unwrap();
    // A stone plateau x -16..16, y 64..74, z -16..16, a corridor three wide and three high cut
    // into it from its north edge (x -1..1, y 64..66, z -16..8).
    for x in -16..=16 {
        for y in 64..=74 {
            for z in -16..=16 {
                let corridor = (-1..=1).contains(&x) && (64..=66).contains(&y) && z <= 8;
                if !corridor {
                    m.set_block(x, y, z, state(BlockKind::Stone)).unwrap();
                }
            }
        }
    }
    // The lava pool: eighteen sources sunk in the corridor floor at its end (a cell spends one).
    for x in -1..=1 {
        for z in 3..=8 {
            m.set_block(x, 63, z, state(BlockKind::Lava)).unwrap();
        }
    }
    // The pond outside, three by three.
    for x in 12..=14 {
        for z in -24..=-22 {
            m.set_block(x, 63, z, state(BlockKind::Water)).unwrap();
        }
    }
    m.spawn = [0.5, 64.0, -20.5];
    m
}

fn run(ticks: u64) -> (Plan, bot::lockstep::Run) {
    let m = world();
    let goals = concat!(env!("CARGO_MANIFEST_DIR"), "/../../config/goals.toml");
    let table = std::sync::Arc::new(bot::goals_table(goals).unwrap());
    let mut p = Plan::with_table(Aim::Portal, table);
    let mut sim = Sim::new(CONFIG, m).seeded(5);
    for g in kit() {
        sim.survival.give(g);
    }
    let run = bot::lockstep::run_acting(sim, "rs-cast", 1, ticks, 0, |w, _| {
        let c = p.step(w);
        (c, p.take_acts())
    });
    (p, run)
}

#[test]
fn the_kit_casts_and_lights_a_portal_beside_lava() {
    let (p, run) = run(24_000);
    let tail: Vec<_> = p.events.iter().rev().take(25).collect();
    assert!(
        p.met,
        "not met; last events: {tail:#?}; task {:?}",
        p.task.name()
    );
    let inside = p.portal.unwrap();
    assert_eq!(
        run.world
            .block_at(BlockPos::new(inside.0, inside.1, inside.2)),
        Some(BlockKind::NetherPortal)
    );
    assert!(!run.died, "died: {tail:#?}");
    assert!(run.sim.violations.is_empty(), "{:?}", run.sim.violations);
}
