//! Issue 08a, done-condition 2's first half: the bot runs end to end against the server model,
//! in-process, with no Java server: it joins, is placed, loads the world, walks, and its
//! recording replays.

use std::time::{Duration, Instant};

use results::Ending;
use sim::{Config, Sim, flat_world};

#[test]
fn the_bot_joins_the_model_and_walks() {
    let world = flat_world(4, 63).unwrap();
    let sim = Sim::new(
        Config {
            view_distance: 3,
            simulation_distance: 3,
            peaceful: false,
        },
        world,
    );
    let dir = std::env::temp_dir().join(format!("model-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let rec = dir.join("model.rsr");
    let out = bot::run(bot::Opts {
        host: "model".to_owned(),
        port: 0,
        name: "rs-model".to_owned(),
        record: Some(rec.clone()),
        script: Some(bot::Script::WalkSquare),
        script_ticks: None,
        cap_ticks: Some(100),
        deadline: Some(Instant::now() + Duration::from_secs(30)),
        start: None,
        verbose: false,
        relay: None,
        run_id: "test".to_owned(),
        lane: 0,
        start_name: "spawn".to_owned(),
        case_path: String::new(),
        model: Some(Box::new(sim)),
    })
    .unwrap();
    let w = &out.world;
    assert_eq!(out.ending, Ending::Cap, "ran to its cap in server ticks");
    assert_eq!(w.session.teleports, 1, "no server corrections");
    assert_eq!(w.stage_reached, Some(world::Stage::InWorld));
    assert_eq!(
        w.dimension.as_ref().map(|d| d.name.as_str()),
        Some("minecraft:overworld")
    );
    assert_eq!(out.stats.decode_errors, 0);
    assert!(
        out.stats.unknown.values().sum::<u64>() <= 2,
        "only the batch starts are unmodelled: {:?}",
        out.stats.unknown
    );
    let body = w.body.unwrap();
    assert!(body.on_ground, "standing on the grass");
    assert!(
        (body.pos[1] - 64.0).abs() < 1e-9,
        "on the surface, y {}",
        body.pos[1]
    );
    let walked = ((body.pos[0] - 0.5).powi(2) + (body.pos[2] - 0.5).powi(2)).sqrt();
    assert!(walked > 5.0, "walked {walked:.1} blocks");
    assert!(out.recorded);
    let (_, records) = record::Recording::read_all(&rec).unwrap();
    let mut folder = record::Folder::new();
    let mut mismatches = 0;
    let mut pending: Option<Vec<record::EffectRecord>> = None;
    for r in records {
        match r {
            record::Record::Event(e) => pending = Some(folder.step(&e).unwrap()),
            record::Record::Effects(want) => {
                let got = pending.take();
                mismatches += usize::from(got.is_some_and(|g| g != want));
            }
            _ => {}
        }
    }
    assert_eq!(mismatches, 0, "the run replays exactly");
    std::fs::remove_dir_all(&dir).unwrap();
}
