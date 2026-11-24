//! Issue 17, the model's endermen closed loop (docs/problems/endermen-model.md): the bot's own
//! world, fed by the model in lockstep, against endermen placed in a Nether cavern laid as a
//! warped forest. Development evidence only (measurement-rules): the race is the metric.

use protocol::metadata::MetaValue;
use sim::nether::{Biome, Stage};
use sim::{Config, Sim};

/// The race's distances (config/race.toml): simulation 3, view 8.
const CONFIG: Config = Config {
    view_distance: 8,
    simulation_distance: 3,
    peaceful: false,
};
const FLOOR: i32 = 40;
const FEET: f64 = 41.0;
/// Where the enderman stands: 80 east of the start (chunk 5, two past the simulation
/// distance) and 10 to the side, so a walk east never looks it in the eye.
const ENDER: [f64; 3] = [80.5, FEET, 10.5];

/// A warped forest cavern 7 chunks each way round the origin, the bot at (0.5, 41, 0.5).
fn forest() -> Sim {
    let mut stage = Stage::new(7, FLOOR, FLOOR + 20);
    stage
        .biome((-112, 127), (-112, 127), Biome::WarpedForest)
        .spawn((0, FLOOR + 1, 0));
    // No natural spawns: these tests place their own (a peaceful config would also stop the
    // placed enderman's blow, which these never draw).
    Sim::new(CONFIG, stage.build().unwrap()).seeded(9)
}

/// The bot walks east (yaw -90) for `ticks`, with an enderman placed at `ENDER`.
fn walk_east(ticks: u64) -> (bot::lockstep::Run, i32) {
    let mut sim = forest();
    let id = sim.summon_enderman(ENDER);
    sim.survival.life.mobs.get_mut(&id).unwrap().loot = false;
    let script = bot::Script::Straight { yaw: -90.0 };
    let run = bot::lockstep::run(sim, "rs-ender", 9, ticks, 20, |_, t| script.controls(t));
    (run, id)
}

fn ender_in_world(run: &bot::lockstep::Run, id: i32) -> &world::Entity {
    run.world
        .entities
        .get(&id)
        .expect("the bot's world holds the enderman")
}

#[test]
fn the_bot_reads_the_warped_forest_at_its_feet_from_the_models_columns() {
    let (run, _) = walk_east(40);
    let body = run.world.body.unwrap();
    let (x, y, z) = (
        body.pos[0].floor() as i32,
        body.pos[1].floor() as i32,
        body.pos[2].floor() as i32,
    );
    assert_eq!(run.sim.world().biome_at(x, y, z), Some("warped_forest"));
    let column = run
        .world
        .columns
        .get(&world::ChunkPos {
            x: x >> 4,
            z: z >> 4,
        })
        .unwrap();
    let id = column
        .section_at(y)
        .unwrap()
        .biomes
        .get(sim::nether::biome_index(x, y, z))
        .unwrap();
    assert_eq!(Some(id), sim::nether::biome_id("warped_forest"));
}

#[test]
fn a_frozen_enderman_starts_ticking_when_the_bot_comes_within_the_simulation_distance() {
    // Early: the bot is still more than 3 chunks from the enderman's chunk; it has not ticked.
    let (early, id) = walk_east(60);
    let e = early.sim.ender(id).unwrap();
    assert!(!early.sim.ticking(ENDER), "frozen at the start");
    assert_eq!(e.ticked, 0, "a frozen enderman never ticks");
    assert_eq!(early.sim.survival.life.mobs[&id].pos, ENDER, "nor moves");
    // The same run on: the bot walks into range and it ticks.
    let (late, id) = walk_east(400);
    let body = late.world.body.unwrap();
    assert!(late.sim.ticking(late.sim.survival.life.mobs[&id].pos));
    let e = late.sim.ender(id).unwrap();
    assert!(e.ticked > 0, "it ticks once within 3 chunks");
    assert!(
        e.ticked < 400,
        "and only since: {} of 400 ticks, the bot at {:?}",
        e.ticked,
        body.pos
    );
}

#[test]
fn a_bot_walking_toward_a_placed_enderman_reaches_within_thirty_two_with_it_ticking() {
    let (run, id) = walk_east(400);
    let body = run.world.body.unwrap();
    let seen = ender_in_world(&run, id);
    let d = ((seen.pos[0] - body.pos[0]).powi(2)
        + (seen.pos[1] - body.pos[1]).powi(2)
        + (seen.pos[2] - body.pos[2]).powi(2))
    .sqrt();
    assert!(d <= 32.0, "within 32: {d:.1}, the bot at {:?}", body.pos);
    assert!(run.sim.ticking(run.sim.survival.life.mobs[&id].pos));
    // Walked by without a stare: neutral.
    let e = run.sim.ender(id).unwrap();
    assert!(!e.angry, "never looked in the eye");
    assert_eq!(run.world.session.teleports, 1, "no correction");
}

/// The bot stands and looks along (yaw, pitch) at an enderman 12 east.
fn stand_and_look(yaw: f32, pitch: f32) -> (bot::lockstep::Run, i32) {
    let mut sim = forest();
    let id = sim.summon_enderman([12.5, FEET, 0.5]);
    let script = bot::Script::Use {
        at: u64::MAX,
        yaw: f64::from(yaw),
        pitch,
    };
    let run = bot::lockstep::run(sim, "rs-stare", 9, 60, 20, |_, t| script.controls(t));
    (run, id)
}

/// The look from the bot's eye (feet 41 + 1.62) at the enderman's eye (feet 41 + 2.55), 12 east.
fn eye_line() -> (f32, f32) {
    let pitch = -((2.55_f64 - 1.62) / 12.0).atan().to_degrees();
    (-90.0, pitch as f32)
}

#[test]
fn a_staring_look_angers_the_enderman_and_the_bot_sees_it_creepy() {
    let (yaw, pitch) = eye_line();
    let (run, id) = stand_and_look(yaw, pitch);
    assert!(run.sim.ender(id).unwrap().angry);
    let seen = ender_in_world(&run, id);
    assert_eq!(
        seen.metadata.get(&sim::enderman::META_CREEPY),
        Some(&MetaValue::Bool(true)),
        "the bot's world shows it creepy"
    );
}

#[test]
fn a_look_off_its_eye_keeps_the_enderman_neutral() {
    let (yaw, pitch) = eye_line();
    // At its legs (the look 6 degrees under its eye): not a stare in 26.1.2, whose rule is
    // the eye alone.
    let (run, id) = stand_and_look(yaw, pitch + 6.0);
    assert!(!run.sim.ender(id).unwrap().angry);
    let seen = ender_in_world(&run, id);
    assert_ne!(
        seen.metadata.get(&sim::enderman::META_CREEPY),
        Some(&MetaValue::Bool(true))
    );
}
