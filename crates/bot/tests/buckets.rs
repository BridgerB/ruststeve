//! Issue 14a: looking, buckets and fluids against the model, closed loop on a flat world
//! (grass at y 63): a source scooped along the look, water poured and flowing, water over a lava
//! source making obsidian, and flint and steel lighting a frame
//! (brain/ruststeve/design/buckets-and-fluids.md).

use data::{BlockKind, ItemKind};
use plan::walk::look_at;
use sim::{Config, Sim, flat_world};
use world::{Act, BlockPos};

const CONFIG: Config = Config {
    view_distance: 2,
    simulation_distance: 2,
    peaceful: true,
};

fn state(k: BlockKind) -> u32 {
    k.info().unwrap().default_state
}

/// The bot at (0.5, 64, 0.5) holding `give`, with `set` laid; it looks at `at` from tick 20 and
/// sends `act` once at tick 30, then stands for `ticks`.
fn use_at(
    set: &[((i32, i32, i32), u32)],
    give: &[(ItemKind, i32)],
    at: [f64; 3],
    act: Act,
    ticks: u64,
) -> bot::lockstep::Run {
    let mut m = flat_world(2, 63).unwrap();
    for ((x, y, z), s) in set {
        m.set_block(*x, *y, *z, *s).unwrap();
    }
    m.spawn = [0.5, 64.0, 0.5];
    let mut sim = Sim::new(CONFIG, m);
    for g in give {
        sim.survival.give(*g);
    }
    let mut sent = false;
    bot::lockstep::run_acting(sim, "rs-bkt", 1, ticks, 0, move |w, t| {
        let Some(body) = w.body else {
            return (physics::Controls::default(), Vec::new());
        };
        let eye = [body.pos[0], body.pos[1] + physics::EYE_HEIGHT, body.pos[2]];
        let (yaw, pitch) = look_at(eye, at);
        let c = physics::Controls {
            yaw,
            pitch: (t >= 20).then_some(pitch),
            ..physics::Controls::default()
        };
        let acts = if t >= 30 && !sent {
            sent = true;
            vec![act]
        } else {
            Vec::new()
        };
        (c, acts)
    })
}

fn kind(run: &bot::lockstep::Run, x: i32, y: i32, z: i32) -> Option<BlockKind> {
    run.world.block_at(BlockPos::new(x, y, z))
}

fn held(run: &bot::lockstep::Run, item: ItemKind) -> i32 {
    plan::held(&run.world).get(&item).copied().unwrap_or(0)
}

fn water_source() -> u32 {
    state(BlockKind::Water)
}

fn lava_source() -> u32 {
    state(BlockKind::Lava)
}

#[test]
fn a_water_source_is_scooped_along_the_look() {
    let run = use_at(
        &[((2, 63, 0), water_source())],
        &[(ItemKind::Bucket, 1)],
        [2.5, 63.85, 0.5],
        Act::UseItem,
        60,
    );
    assert_eq!(
        held(&run, ItemKind::WaterBucket),
        1,
        "{:?}",
        plan::held(&run.world)
    );
    assert_eq!(held(&run, ItemKind::Bucket), 0);
    assert_eq!(kind(&run, 2, 63, 0), Some(BlockKind::Air));
    assert!(run.sim.violations.is_empty(), "{:?}", run.sim.violations);
}

#[test]
fn one_of_two_buckets_is_filled() {
    let run = use_at(
        &[((2, 63, 0), water_source())],
        &[(ItemKind::Bucket, 2)],
        [2.5, 63.85, 0.5],
        Act::UseItem,
        60,
    );
    assert_eq!(held(&run, ItemKind::WaterBucket), 1);
    assert_eq!(held(&run, ItemKind::Bucket), 1);
}

#[test]
fn water_poured_on_the_ground_flows() {
    let run = use_at(
        &[],
        &[(ItemKind::WaterBucket, 1)],
        [2.5, 64.0, 0.5],
        Act::UseItem,
        120,
    );
    assert_eq!(held(&run, ItemKind::Bucket), 1);
    assert_eq!(kind(&run, 2, 64, 0), Some(BlockKind::Water));
    // Flat ground: it spreads a block every 5 ticks, 7 out.
    assert_eq!(kind(&run, 5, 64, 0), Some(BlockKind::Water));
    assert_eq!(kind(&run, 2, 64, 3), Some(BlockKind::Water));
    assert_ne!(kind(&run, 10, 64, 0), Some(BlockKind::Water));
}

#[test]
fn water_over_a_lava_source_makes_obsidian() {
    // Lava sunk in the ground at (2, 63, 0), a stone wall behind the cell over it: the water is
    // poured against the wall's face, into the cell over the lava.
    let stone = state(BlockKind::Stone);
    let run = use_at(
        &[
            ((2, 63, 0), lava_source()),
            ((3, 64, 0), stone),
            ((3, 63, 0), stone),
        ],
        &[(ItemKind::WaterBucket, 1)],
        [3.0, 64.5, 0.5],
        Act::UseItem,
        60,
    );
    assert_eq!(kind(&run, 2, 63, 0), Some(BlockKind::Obsidian));
    assert_eq!(held(&run, ItemKind::Bucket), 1);
}

#[test]
fn lava_poured_into_a_hole_stays_a_source() {
    // A hole a block deep at (2, 63, 0), its sides the grass: the lava poured low against its
    // back wall, as the cast pours it (its floor is out of sight past the grass in front).
    let air = state(BlockKind::Air);
    let run = use_at(
        &[((2, 63, 0), air)],
        &[(ItemKind::LavaBucket, 1)],
        [3.0, 63.3, 0.5],
        Act::UseItem,
        80,
    );
    assert_eq!(held(&run, ItemKind::Bucket), 1);
    assert_eq!(kind(&run, 2, 63, 0), Some(BlockKind::Lava));
    assert_ne!(
        kind(&run, 3, 64, 0),
        Some(BlockKind::Lava),
        "lava climbed out of its hole"
    );
}

#[test]
fn flint_and_steel_lights_an_obsidian_frame() {
    // A frame in the x-y plane at z 3, its inside x 1..=2, y 65..=67.
    let obsidian = state(BlockKind::Obsidian);
    let mut set = vec![
        ((1, 64, 3), obsidian),
        ((2, 64, 3), obsidian),
        ((1, 68, 3), obsidian),
        ((2, 68, 3), obsidian),
    ];
    for y in 65..=67 {
        set.push(((0, y, 3), obsidian));
        set.push(((3, y, 3), obsidian));
    }
    let run = use_at(
        &set,
        &[(ItemKind::FlintAndSteel, 1)],
        [1.5, 65.0, 3.5],
        Act::UseOn {
            pos: BlockPos::new(1, 64, 3),
            face: 1,
        },
        60,
    );
    for (x, y) in [(1, 65), (2, 65), (1, 66), (2, 66), (1, 67), (2, 67)] {
        assert_eq!(
            kind(&run, x, y, 3),
            Some(BlockKind::NetherPortal),
            "({x}, {y})"
        );
    }
    assert!(run.sim.violations.is_empty(), "{:?}", run.sim.violations);
}

#[test]
fn a_portal_block_has_no_collision() {
    // v1's load-bearing fix: generated data gave portals a full cube.
    assert_eq!(
        data::generated::shapes::collision(BlockKind::NetherPortal),
        Some(data::types::Collision::Empty)
    );
}
