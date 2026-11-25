//! Issue 19, the eye of ender in the closed loop (docs/problems/eyes-model.md): the bot's own
//! world, fed by the model in lockstep, on flat ground with the race seed's stronghold told to the
//! model. The scripted bot of parity row y02 (`fetch:<tick>`) throws one eye looking up, reads
//! its flight from the entity packets, and walks to the eye if it drops. Development evidence
//! only (measurement-rules): the parity table says whether the model agrees with the server.

use data::{EntityKind, ItemKind};
use sim::{Config, Sim};

const CONFIG: Config = Config {
    view_distance: 4,
    simulation_distance: 3,
    peaceful: true,
};
const GROUND: i32 = 64;
/// RCON `locate structure minecraft:stronghold` from the race seed's landings (probe
/// 45685968956).
const STRONGHOLD: [f64; 3] = [1648.0, 0.0, -16.0];
const THROW_AT: u64 = 20;

fn eye_sim(seed: u64) -> Sim {
    let world = sim::flat_world(4, GROUND).unwrap();
    let mut sim = Sim::new(CONFIG, world)
        .seeded(seed)
        .with_stronghold(STRONGHOLD);
    sim.survival.give((ItemKind::EnderEye, 2));
    sim
}

fn held_eyes(w: &world::World) -> i32 {
    w.inventory
        .slots
        .values()
        .filter(|(k, _)| *k == ItemKind::EnderEye)
        .map(|(_, n)| n)
        .sum()
}

/// The eye's places as the bot's world was told them, the spawn first.
fn run_fetch(seed: u64) -> (bot::lockstep::Run, Vec<[f64; 3]>) {
    let script = bot::Script::Fetch {
        at: THROW_AT,
        home: None,
    };
    let mut places: Vec<[f64; 3]> = Vec::new();
    let run = bot::lockstep::run_acting(eye_sim(seed), "rs-eye", seed, 20 + 400, 20, |w, t| {
        if let Some(e) = w
            .entities
            .values()
            .find(|e| e.kind == EntityKind::EyeOfEnder)
            && places.last() != Some(&e.pos)
        {
            places.push(e.pos);
        }
        (script.steer(t, w), script.acts(t, w))
    });
    (run, places)
}

#[test]
fn the_bot_reads_the_line_within_a_degree_and_fetches_a_dropped_eye() {
    let mut fetched = 0;
    for seed in 1..=6 {
        let (run, places) = run_fetch(seed);
        assert_eq!(run.sim.eyes_thrown, 1, "seed {seed}: thrown");
        assert!(places.len() >= 10, "seed {seed}: {} places", places.len());
        let a = places[0];
        let b = places
            .iter()
            .copied()
            .max_by(|x, y| {
                let h = |p: [f64; 3]| (p[0] - a[0]).hypot(p[2] - a[2]);
                h(*x).total_cmp(&h(*y))
            })
            .unwrap();
        let heading = (b[2] - a[2]).atan2(b[0] - a[0]);
        let bearing = (STRONGHOLD[2] - a[2]).atan2(STRONGHOLD[0] - a[0]);
        let err = (heading - bearing).to_degrees().abs();
        assert!(
            err < 0.1,
            "seed {seed}: the read line is off by {err} degrees"
        );
        let dropped = run.sim.eyes_dropped == 1;
        let left = run
            .sim
            .survival
            .items
            .values()
            .any(|i| i.stack.0 == ItemKind::EnderEye);
        assert!(!left, "seed {seed}: a dropped eye left on the ground");
        let held = held_eyes(&run.world);
        assert_eq!(held, if dropped { 2 } else { 1 }, "seed {seed}");
        fetched += i32::from(dropped);
    }
    assert!(fetched >= 1, "no seed dropped its eye");
}
