//! Issue 18, the burrow in the closed loop (docs/problems/pearls-model.md): the bot's own world,
//! fed by the model in lockstep, in a warped forest cavern with a level burrow one wide and two
//! high running east (B the back cell, M the mouth, O outside). The scripted bot of the parity
//! rows e04, e05 and e08 (`burrow:<mode>`) provokes from M, steps back to B and holds or
//! strikes. Development evidence only (measurement-rules): the race is the metric.

use sim::nether::{Biome, Stage};
use sim::{Config, Sim};

const CONFIG: Config = Config {
    view_distance: 8,
    simulation_distance: 3,
    peaceful: false,
};
const FLOOR: i32 = 40;
const FEET: i32 = FLOOR + 1;
/// B, M and O on x (z 0, the feet's level).
const B: i32 = 0;
const M: i32 = 1;
const O: i32 = 2;

/// A warped forest cavern with the burrow, the bot standing in M, an enderman 10 east of M.
fn forest_burrow(seed: u64) -> (Sim, i32) {
    let mut stage = Stage::new(4, FLOOR, FLOOR + 20);
    let stone = data::BlockKind::Stone;
    stage
        .biome((-64, 79), (-64, 79), Biome::WarpedForest)
        .fill((B - 1, B - 1), (FEET, FEET + 2), (-1, 1), stone)
        .fill((B, M), (FEET, FEET + 2), (-1, -1), stone)
        .fill((B, M), (FEET, FEET + 2), (1, 1), stone)
        .fill((B, M), (FEET + 2, FEET + 2), (0, 0), stone)
        .spawn((M, FEET, 0));
    let mut sim = Sim::new(CONFIG, stage.build().unwrap()).seeded(seed);
    let id = sim.summon_enderman([f64::from(M + 10) + 0.5, f64::from(FEET), 0.5]);
    (sim, id)
}

fn run(script: bot::Script, sim: Sim, ticks: u64) -> bot::lockstep::Run {
    bot::lockstep::run_acting(sim, "rs-burrow", 9, ticks, 20, |w, t| {
        (script.steer(t, w), script.acts(t, w))
    })
}

fn feet_cell(run: &bot::lockstep::Run) -> [i32; 3] {
    run.world
        .body
        .unwrap()
        .pos
        .map(|v| (v + 1e-7).floor() as i32)
}

#[test]
fn provoked_from_m_the_bot_steps_to_b_and_the_enderman_holds_at_o_with_no_hit() {
    for seed in 1..=3 {
        let (sim, id) = forest_burrow(seed);
        let run = run(bot::Script::Burrow(bot::BurrowMode::Hold), sim, 480);
        let e = run.sim.ender(id).unwrap();
        assert!(e.angry, "seed {seed}: provoked");
        assert_eq!(feet_cell(&run), [B, FEET, 0], "seed {seed}: in B");
        let at = run.sim.survival.life.mobs[&id].pos;
        assert_eq!(
            at.map(|v| (v + 1e-7).floor() as i32),
            [O, FEET, 0],
            "seed {seed}: at O"
        );
        assert_eq!(run.min_health_in_script, Some(20.0), "seed {seed}: hit");
        assert!(run.sim.survival.life.hurt_by_mob.is_empty());
    }
}

#[test]
fn struck_from_b_on_a_full_charge_the_enderman_dies_at_the_mouth() {
    let (mut sim, id) = forest_burrow(4);
    sim.survival.give((data::ItemKind::IronSword, 1));
    let run = run(bot::Script::Burrow(bot::BurrowMode::Strike), sim, 600);
    let m = run.sim.survival.life.mobs[&id];
    assert!(m.dead_at.is_some(), "alive at {} health", m.health);
    assert_eq!(run.world.kills, 1);
    assert_eq!(run.sim.ender(id).unwrap().teleports, 0);
    assert_eq!(feet_cell(&run), [B, FEET, 0]);
    assert_eq!(run.min_health_in_script, Some(20.0));
}
