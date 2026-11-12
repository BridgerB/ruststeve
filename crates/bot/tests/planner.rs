//! Issue 09a: the planner against the server model. It reaches a log 30+ blocks away on real
//! captured terrain; over generated worlds it never churns (a chosen task is not over after one
//! tick) and no walk goes longer than its patience without getting closer.

use plan::walk::PATIENCE_TICKS;
use plan::{Aim, Plan};
use protocol::chunk::Column;
use sim::{Config, ModelColumn, ModelWorld, Sim};

const CONFIG: Config = Config {
    view_distance: 8,
    simulation_distance: 3,
    peaceful: true,
};

/// The plains landing from plan's fixtures (5x5 columns captured on the race seed).
fn plains() -> ModelWorld {
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../plan/tests/fixtures");
    let mut columns = std::collections::BTreeMap::new();
    for e in std::fs::read_dir(dir).unwrap().flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        let Some(rest) = name.strip_prefix("plains_") else {
            continue;
        };
        let p: Vec<i32> = rest
            .trim_end_matches(".bin")
            .split('_')
            .map(|v| v.parse().unwrap())
            .collect();
        let column = Column::decode(&std::fs::read(e.path()).unwrap()).unwrap();
        columns.insert(
            (p[0], p[1]),
            ModelColumn::generated(p[0], p[1], column).unwrap(),
        );
    }
    ModelWorld {
        columns,
        spawn: [160.5, 67.0, 0.5],
        dimension: "minecraft:overworld".to_owned(),
        dimension_type: 0,
    }
}

/// The churn invariant over a plan's events: no task starts and ends within one tick, except
/// the aim being met.
fn churned(p: &Plan) -> Option<String> {
    let starts = |w: &str| w.starts_with("walk to") || w.starts_with("explore toward");
    p.events.windows(2).find_map(|e| {
        let (a, b) = (&e[0], &e[1]);
        (starts(&a.what) && b.tick <= a.tick + 1 && !b.what.starts_with("reached"))
            .then(|| format!("{} at {} then {} at {}", a.what, a.tick, b.what, b.tick))
    })
}

#[test]
fn the_planner_reaches_a_log_thirty_blocks_out_on_real_terrain() {
    let mut p = Plan::new(Aim::ReachLog { min_distance: 30.0 });
    let run = bot::lockstep::run(Sim::new(CONFIG, plains()), "rs-p", 1, 3000, 20, |w, _| {
        p.step(w)
    });
    let at = run.world.body.unwrap().pos;
    let log = p
        .reached
        .unwrap_or_else(|| panic!("not reached: {:?}", p.events));
    let from_start =
        ((f64::from(log.0) + 0.5 - 160.5).powi(2) + (f64::from(log.2) + 0.5 - 0.5).powi(2)).sqrt();
    assert!(
        from_start >= 30.0,
        "the log {log:?} is {from_start:.1} from the start"
    );
    assert_eq!(run.world.session.teleports, 1, "no server correction");
    assert!(p.longest_stall <= PATIENCE_TICKS + 1);
    assert_eq!(churned(&p), None);
    let eye = [at[0], at[1] + physics::EYE_HEIGHT, at[2]];
    let c = [
        f64::from(log.0) + 0.5,
        f64::from(log.1) + 0.5,
        f64::from(log.2) + 0.5,
    ];
    let d = ((eye[0] - c[0]).powi(2) + (eye[1] - c[1]).powi(2) + (eye[2] - c[2]).powi(2)).sqrt();
    assert!(d <= plan::walk::REACH, "within reach: {d:.2}");
}

#[test]
fn over_generated_worlds_it_never_churns_and_never_stalls_past_its_patience() {
    for seed in 0..24u64 {
        let world = sim::terrain::world(seed, 6).unwrap();
        // No trees in generated worlds: the planner explores, which walks the most.
        let mut p = Plan::new(Aim::ReachLog { min_distance: 0.0 });
        let run = bot::lockstep::run(Sim::new(CONFIG, world), "rs-g", seed, 1500, 20, |w, _| {
            p.step(w)
        });
        assert_eq!(churned(&p), None, "seed {seed}");
        assert!(
            p.longest_stall <= PATIENCE_TICKS + 1,
            "seed {seed}: stall {}",
            p.longest_stall
        );
        assert_eq!(run.world.session.teleports, 1, "seed {seed}: corrections");
        assert!(p.walks >= 1, "seed {seed}: it walked");
    }
}

#[test]
fn aims_parse_and_name_back() {
    for s in ["log", "log:30"] {
        let a = Aim::parse(s).unwrap();
        assert_eq!(Aim::parse(&a.name()), Some(a));
    }
    assert_eq!(Aim::parse("tree"), None);
}
