//! The day clock, closed loop (`docs/problems/day-clock.md`): the model sends the time as a
//! server does, the bot's world keeps it, and the ledger says when night falls and day breaks.

use std::time::{Duration, Instant};

use observe::Fact;
use sim::{Config, Sim, flat_world};
use world::time::{DAWN, DUSK};

/// A bot standing in the model for `cap` server ticks from `day_time`.
fn stand(day_time: i64, cap: u64, tag: &str) -> bot::Outcome {
    let world = flat_world(3, 63).unwrap();
    let mut sim = Sim::new(
        Config {
            view_distance: 2,
            simulation_distance: 2,
            peaceful: false,
        },
        world,
    );
    sim.day_time = day_time;
    let dir = std::env::temp_dir().join(format!("day-clock-{tag}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let out = bot::run(bot::Opts {
        host: "model".to_owned(),
        port: 0,
        name: "rs-model".to_owned(),
        record: Some(dir.join("day.rsr")),
        script: None,
        script_ticks: None,
        aim: None,
        cap_ticks: Some(cap),
        deadline: Some(Instant::now() + Duration::from_secs(60)),
        start: None,
        verbose: false,
        relay: None,
        run_id: "test".to_owned(),
        lane: 0,
        start_name: "spawn".to_owned(),
        case_path: String::new(),
        model: Some(Box::new(sim)),
        hold_after_aim: false,
        revive: false,
        memory: None,
        on_stage: None,
        no_progress_ticks: None,
        snapshot_at: None,
        eyes: None,
        progress: None,
    })
    .unwrap();
    std::fs::remove_dir_all(&dir).unwrap();
    out
}

#[test]
fn night_falls_in_the_ledger() {
    let out = stand(DUSK - 300, 600, "dusk");
    let t = out.world.day_time().expect("the model sent the time");
    assert!(world::time::is_night(t), "past dusk: {t}");
    assert_eq!(out.world.is_night(), Some(true));
    let nights: Vec<i64> = out
        .facts
        .iter()
        .filter_map(|f| match f.fact {
            Fact::Night { day_time } => Some(day_time),
            _ => None,
        })
        .collect();
    assert_eq!(nights.len(), 1, "one nightfall: {nights:?}");
    // Told every 20 ticks: the fact lands within one sync of dusk.
    assert!((DUSK..DUSK + 20).contains(&nights[0]), "{nights:?}");
    assert!(!out.facts.iter().any(|f| matches!(f.fact, Fact::Day { .. })));
    let rows: Vec<i64> = out.senses.iter().filter_map(|r| r.day_time).collect();
    assert!(!rows.is_empty(), "the senses rows carry the day time");
    assert!(rows.windows(2).all(|w| w[0] <= w[1]), "{rows:?}");
}

#[test]
fn day_breaks_in_the_ledger() {
    let out = stand(DAWN - 300, 600, "dawn");
    assert_eq!(out.world.is_night(), Some(false));
    let days: Vec<i64> = out
        .facts
        .iter()
        .filter_map(|f| match f.fact {
            Fact::Day { day_time } => Some(day_time),
            _ => None,
        })
        .collect();
    assert_eq!(days.len(), 1, "one daybreak: {days:?}");
    assert!((DAWN..DAWN + 20).contains(&days[0]), "{days:?}");
    // Joined at night: said once, at the first sync.
    let nights = out
        .facts
        .iter()
        .filter(|f| matches!(f.fact, Fact::Night { .. }))
        .count();
    assert_eq!(nights, 1, "joined at night");
}
