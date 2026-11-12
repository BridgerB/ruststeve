//! Issue 08b, done-condition 2's second half: the bot walks its staircase in 1,000 generated
//! worlds against the server model, in lockstep (no clock), and every run ends clean: placed, no
//! server correction, no violation, no decode error, nothing unknown but the batch starts, no
//! kick. The suite runs 50 (debug builds are slow); CI's `worlds` job runs 1,000 in release
//! (`RUSTSTEVE_WORLDS=1000`).

use bot::lockstep;
use sim::{Config, Sim};

const TICKS: u64 = 400;
const RADIUS: i32 = 5;
const CONFIG: Config = Config {
    view_distance: 4,
    simulation_distance: 3,
    peaceful: false,
};

/// A clean run: its final position and how far it got from the spawn.
fn one(seed: u64) -> Result<([f64; 3], f64), String> {
    let world = sim::terrain::world(seed, RADIUS).map_err(|e| e.to_string())?;
    let spawn = world.spawn;
    let run = lockstep::run(Sim::new(CONFIG, world), "rs-gen", seed, TICKS, 0, |_, t| {
        bot::walk_controls(t)
    });
    let w = &run.world;
    let body = w.body.ok_or("never placed")?;
    let mut problems = Vec::new();
    if run.sim.corrections > 0 {
        problems.push(format!("{} corrections", run.sim.corrections));
    }
    if !run.sim.violations.is_empty() {
        problems.push(format!("violations {:?}", run.sim.violations));
    }
    if run.decode_errors > 0 {
        problems.push(format!("{} decode errors", run.decode_errors));
    }
    if run.sim.closed.is_some() {
        problems.push(format!("kicked: {:?}", run.sim.closed));
    }
    if run.died {
        problems.push("died".to_owned());
    }
    if w.session.teleports != 1 {
        problems.push(format!("{} teleports", w.session.teleports));
    }
    if !problems.is_empty() {
        return Err(format!(
            "seed {seed} (spawn {spawn:?}): {}",
            problems.join("; ")
        ));
    }
    let walked = ((body.pos[0] - spawn[0]).powi(2) + (body.pos[2] - spawn[2]).powi(2)).sqrt();
    Ok((body.pos, walked))
}

#[test]
fn the_bot_walks_a_thousand_generated_worlds_clean() {
    let n: u64 = std::env::var("RUSTSTEVE_WORLDS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(50);
    let threads = std::thread::available_parallelism().map_or(4, |n| n.get()) as u64;
    let handles: Vec<_> = (0..threads)
        .map(|t| {
            std::thread::spawn(move || {
                (0..n)
                    .filter(|s| s % threads == t)
                    .map(one)
                    .collect::<Vec<_>>()
            })
        })
        .collect();
    let runs: Vec<_> = handles
        .into_iter()
        .flat_map(|h| h.join().unwrap())
        .collect();
    let failures: Vec<String> = runs.iter().filter_map(|r| r.clone().err()).collect();
    let mut walked: Vec<f64> = runs
        .iter()
        .filter_map(|r| r.as_ref().ok().map(|(_, w)| *w))
        .collect();
    walked.sort_by(f64::total_cmp);
    let median = walked.get(walked.len() / 2).copied().unwrap_or(0.0);
    println!(
        "{n} worlds: {} clean, walked median {median:.1} blocks, min {:.1}, max {:.1}",
        walked.len(),
        walked.first().copied().unwrap_or(0.0),
        walked.last().copied().unwrap_or(0.0)
    );
    // The staircase covers about 60 blocks on flat ground in 400 ticks; terrain stops some.
    assert!(
        median > 20.0,
        "bots did not really walk: median {median:.1}"
    );
    assert!(
        failures.is_empty(),
        "{} of {n} worlds failed:\n{}",
        failures.len(),
        failures
            .iter()
            .take(20)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n")
    );
}

#[test]
fn a_run_is_a_pure_function_of_its_seed() {
    for seed in [3, 77, 501] {
        assert_eq!(one(seed), one(seed), "seed {seed}");
    }
}
