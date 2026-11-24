//! Issue 16: the model's Nether (crates/sim/src/nether.rs, docs/problems/nether-testbed.md): the
//! dimension a stage logs in as, Nether lava's delay, reach and slope search against the
//! Overworld's (`LavaFluid` in the 26.1.2 jar), and magma's burn (`MagmaBlock.stepOn`).

use data::BlockKind;
use protocol::{C2s, S2c, decode};
use sim::nether::{NETHER, Stage};
use sim::{Config, In, ModelWorld, Sim, Wire, flat_world};

const CONFIG: Config = Config {
    view_distance: 2,
    simulation_distance: 2,
    peaceful: false,
};

fn packets(out: Vec<Wire>) -> Vec<S2c> {
    out.iter()
        .filter_map(|w| decode(w.state, w.id, &w.body).ok())
        .collect()
}

fn send(sim: &mut Sim, p: C2s) -> Vec<S2c> {
    packets(sim.step(&In::Packet(p)).unwrap())
}

fn tick(sim: &mut Sim) -> Vec<S2c> {
    packets(sim.step(&In::Tick).unwrap())
}

fn state(k: BlockKind) -> u32 {
    k.info().unwrap().default_state
}

/// The model joined and standing at the world's spawn; with it, what the join sent.
fn joined(world: ModelWorld) -> (Sim, Vec<S2c>) {
    let mut sim = Sim::new(CONFIG, world).seeded(3);
    for p in protocol::session::opening(775, "model", 0, "rs-test") {
        send(&mut sim, p);
    }
    send(&mut sim, C2s::LoginAcknowledged);
    send(&mut sim, C2s::SelectKnownPacks);
    let join = send(&mut sim, C2s::FinishConfiguration);
    send(&mut sim, C2s::AcceptTeleportation { teleport_id: 1 });
    send(&mut sim, C2s::MovePlayerStatusOnly { flags: 1 });
    (sim, join)
}

fn kind(sim: &Sim, x: i32, y: i32, z: i32) -> Option<BlockKind> {
    sim.state_at(x, y, z).and_then(data::block_of_state)
}

/// The lava's amount at a cell (8 a source or falling, 7 to 1 flowing), 0 where there is none.
fn lava(sim: &Sim, x: i32, y: i32, z: i32) -> i32 {
    sim.state_at(x, y, z)
        .and_then(sim::fluid::fluid_of)
        .filter(|f| f.kind == sim::fluid::Kind::Lava)
        .map_or(0, |f| f.amount)
}

/// A Nether cavern (floor 40, ceiling 60) and a flat overworld (ground 63), the player well away
/// from (0, 0); with each, the height of the cells over the floor.
fn nether() -> (Sim, i32) {
    let mut stage = Stage::new(1, 40, 60);
    stage.spawn((12, 41, 12));
    (joined(stage.build().unwrap()).0, 41)
}

fn overworld() -> (Sim, i32) {
    let mut w = flat_world(1, 63).unwrap();
    w.spawn = [12.5, 64.0, 12.5];
    (joined(w).0, 64)
}

#[test]
fn a_stage_logs_in_as_the_nether_with_its_sixteen_sections() {
    let mut stage = Stage::new(1, 40, 60);
    stage
        .lava_lake((2, 4), (2, 4), 40, 2)
        .soul_sand((-3, -1), (0, 2), 40)
        .bricks((6, 8), (41, 44), (6, 6));
    let (sim, join) = joined(stage.build().unwrap());
    let login = join.iter().find_map(|p| match p {
        S2c::Login {
            dimension,
            dimension_type,
            ..
        } => Some((dimension.clone(), *dimension_type)),
        _ => None,
    });
    // The synced registry's dimension types are sorted by name: the Nether is the fourth.
    assert_eq!(login, Some((NETHER.to_owned(), 3)));
    assert!(sim.world().fast_lava());
    let column = join
        .iter()
        .find_map(|p| match p {
            S2c::LevelChunkWithLight { x: 0, z: 0, column } => Some(column.clone()),
            _ => None,
        })
        .unwrap();
    assert_eq!((column.min_y, column.sections.len()), (0, 16));
    assert_eq!(kind(&sim, 0, 0, 0), Some(BlockKind::Bedrock));
    assert_eq!(kind(&sim, 0, 40, 5), Some(BlockKind::Netherrack));
    assert_eq!(kind(&sim, 0, 41, 5), Some(BlockKind::Air));
    assert_eq!(kind(&sim, 0, 60, 5), Some(BlockKind::Netherrack));
    assert_eq!(kind(&sim, 0, 127, 5), Some(BlockKind::Bedrock));
    assert_eq!(kind(&sim, 0, 128, 5), Some(BlockKind::Air));
    assert_eq!(kind(&sim, 3, 39, 3), Some(BlockKind::Lava));
    assert_eq!(lava(&sim, 3, 40, 3), 8);
    assert_eq!(kind(&sim, 3, 38, 3), Some(BlockKind::Netherrack));
    assert_eq!(kind(&sim, -2, 40, 1), Some(BlockKind::SoulSand));
    assert_eq!(kind(&sim, 7, 43, 6), Some(BlockKind::NetherBricks));
    assert!(!joined(flat_world(1, 63).unwrap()).0.world().fast_lava());
}

/// `LavaFluid.getTickDelay` (10 with fast lava, else 30) and `getDropOff` (1, else 2): a source
/// on a flat floor reaches the next cell on its tenth tick in the Nether and stops 7 cells out;
/// in the Overworld on its thirtieth, and 3 cells out.
#[test]
fn nether_lava_flows_a_block_in_ten_ticks_and_seven_blocks_out() {
    let (mut sim, y) = nether();
    sim.setblock((0, y, 0), state(BlockKind::Lava)).unwrap();
    for _ in 0..9 {
        tick(&mut sim);
    }
    assert_eq!(lava(&sim, 1, y, 0), 0, "a cell before the tenth tick");
    tick(&mut sim);
    assert_eq!(lava(&sim, 1, y, 0), 7, "a cell on the tenth tick");
    for _ in 0..20 {
        tick(&mut sim);
    }
    assert_eq!((lava(&sim, 3, y, 0), lava(&sim, 4, y, 0)), (5, 0));
    for _ in 0..200 {
        tick(&mut sim);
    }
    let along: Vec<i32> = (0..=8).map(|x| lava(&sim, x, y, 0)).collect();
    assert_eq!(along, [8, 7, 6, 5, 4, 3, 2, 1, 0]);
    // Each step out costs one, whichever way: three east and two south is five from the source.
    assert_eq!(lava(&sim, 3, y, 2), 3);
}

#[test]
fn overworld_lava_flows_a_block_in_thirty_ticks_and_three_blocks_out() {
    let (mut sim, y) = overworld();
    sim.setblock((0, y, 0), state(BlockKind::Lava)).unwrap();
    for _ in 0..29 {
        tick(&mut sim);
    }
    assert_eq!(lava(&sim, 1, y, 0), 0, "a cell before the thirtieth tick");
    tick(&mut sim);
    assert_eq!(lava(&sim, 1, y, 0), 6, "a cell on the thirtieth tick");
    for _ in 0..240 {
        tick(&mut sim);
    }
    let along: Vec<i32> = (0..=4).map(|x| lava(&sim, x, y, 0)).collect();
    assert_eq!(along, [8, 6, 4, 2, 0]);
}

/// `LavaFluid.getSlopeFindDistance` (4 with fast lava, else 2): a hole four cells east of the
/// source is found in the Nether, so the lava goes east alone; in the Overworld the search stops
/// short of it and the lava goes all four ways.
#[test]
fn nether_lava_finds_a_hole_four_cells_off() {
    let sides = |sim: &Sim, y: i32| {
        [(1, 0), (-1, 0), (0, 1), (0, -1)].map(|(dx, dz)| i32::from(lava(sim, dx, y, dz) > 0))
    };
    let (mut sim, y) = nether();
    sim.setblock((4, y - 1, 0), state(BlockKind::Air)).unwrap();
    sim.setblock((0, y, 0), state(BlockKind::Lava)).unwrap();
    for _ in 0..10 {
        tick(&mut sim);
    }
    assert_eq!(sides(&sim, y), [1, 0, 0, 0], "the Nether: east alone");
    let (mut sim, y) = overworld();
    sim.setblock((4, y - 1, 0), state(BlockKind::Air)).unwrap();
    sim.setblock((0, y, 0), state(BlockKind::Lava)).unwrap();
    for _ in 0..30 {
        tick(&mut sim);
    }
    assert_eq!(sides(&sim, y), [1, 1, 1, 1], "the Overworld: every side");
}

/// A lake at rest stays at rest; opening its bank lets it out at the Nether's pace.
#[test]
fn a_lake_flows_once_its_bank_is_dug() {
    let mut stage = Stage::new(1, 40, 60);
    // A lake sunk in the floor and a trench beside it, a netherrack bank between them.
    stage
        .lava_lake((0, 2), (0, 2), 40, 1)
        .air((4, 6), (40, 40), (1, 1))
        .spawn((12, 41, 12));
    let (mut sim, _) = joined(stage.build().unwrap());
    for _ in 0..40 {
        tick(&mut sim);
    }
    assert_eq!(lava(&sim, 4, 40, 1), 0, "a lake at rest ran");
    sim.setblock((3, 40, 1), state(BlockKind::Air)).unwrap();
    for _ in 0..45 {
        tick(&mut sim);
    }
    let trench: Vec<i32> = (3..=6).map(|x| lava(&sim, x, 40, 1)).collect();
    assert_eq!(trench, [7, 6, 5, 4]);
}

/// How many `hot_floor` hits the player takes in `ticks` standing at the spawn of `stage`.
fn burns(stage: &Stage, ticks: u32, set: impl Fn(&mut Sim)) -> usize {
    let (mut sim, _) = joined(stage.build().unwrap());
    set(&mut sim);
    let hot = data::generated::registries::DAMAGE_TYPE
        .iter()
        .position(|n| *n == "hot_floor")
        .unwrap() as i32;
    (0..ticks)
        .flat_map(|_| tick(&mut sim))
        .filter(|p| matches!(p, S2c::DamageEvent { source_type, .. } if *source_type == hot))
        .count()
}

/// `MagmaBlock.stepOn`: 1 a tick to a body on it that is not stepping carefully, which the 20
/// ticks of invulnerability make one hit each 10 ticks (the wiki's "every half second").
#[test]
fn magma_burns_the_feet_each_ten_ticks_unless_sneaking_or_fire_resistant() {
    let mut magma = Stage::new(1, 40, 60);
    magma.magma((0, 0), (0, 0), 40);
    assert_eq!(burns(&magma, 25, |_| {}), 3, "ticks 1, 11 and 21");
    assert_eq!(burns(&magma, 25, |s| s.survival.life.sneaking = true), 0);
    assert_eq!(
        burns(&magma, 25, |s| s.survival.life.fire_resistance = true),
        0
    );
    assert_eq!(burns(&Stage::new(1, 40, 60), 25, |_| {}), 0, "netherrack");
    // Magma beside the feet, not under them, does nothing.
    let mut beside = Stage::new(1, 40, 60);
    beside.magma((1, 1), (0, 0), 40);
    assert_eq!(burns(&beside, 25, |_| {}), 0);
}

/// Lava laid in a stage rests, as captured columns hold a lava fall still; `wake_fluids` sets
/// what can run running, and only in the chunks that tick: within the simulation distance (2
/// here) of the player. A spring four chunks off waits.
#[test]
fn woken_lava_runs_where_the_chunks_tick_and_waits_where_they_do_not() {
    let mut stage = Stage::new(5, 40, 60);
    stage
        .set((2, 41, 2), BlockKind::Lava)
        .set((70, 41, 2), BlockKind::Lava)
        .lava_lake((-20, -16), (-20, -16), 40, 2)
        .spawn((12, 41, 12));
    let (mut sim, _) = joined(stage.build().unwrap());
    for _ in 0..30 {
        tick(&mut sim);
    }
    assert_eq!(lava(&sim, 3, 41, 2), 0, "lava at rest ran");
    // The two springs; the lake sunk in the floor has nowhere to go and is left asleep.
    assert_eq!(sim.wake_fluids(), 2);
    for _ in 0..10 {
        tick(&mut sim);
    }
    assert_eq!((lava(&sim, 3, 41, 2), lava(&sim, 71, 41, 2)), (7, 0));
    for _ in 0..60 {
        tick(&mut sim);
    }
    assert_eq!((lava(&sim, 8, 41, 2), lava(&sim, 71, 41, 2)), (2, 0));
}
