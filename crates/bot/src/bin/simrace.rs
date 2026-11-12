//! simrace: the sim race (brain/ruststeve/design/server-model.md). Many runs of the bot against
//! the server model, in lockstep and in parallel, one generated world and seed per run, leaving
//! the same case and batch files as the real race with `source = sim`. A run is a pure function
//! of its world, seed and commit, so it is reproduced, never saved: no recordings.
//!
//! ```text
//! cargo run --release -p bot --bin simrace -- --runs 1000 --out target/simrace/lane-0 \
//!     [--lane 0] [--first-seed 0] [--cap 6000] [--walk square] [--run local-<ms>] [--threads N]
//! ```
//!
//! With `--landings-file F --worlds DIR --goal log:30` it runs the planner in each captured
//! landing instead (issue 09's measure, `bot::landings`), prints the measure and, with
//! `--check`, fails when a done-condition or the CPU budget does not hold:
//!
//! ```text
//! cargo run --release -p bot --bin simrace -- --landings-file config/landings-grid.toml \
//!     --worlds W --goal log:30 --ticks 3000 --want 200 --check [--report measure.md]
//! ```
//!
//! A sim number counts only beside a passing parity table for what it measures
//! (brain/ruststeve/decisions/measurement-rules.md).

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::process::ExitCode;

use bot::Script;
use results::config::RaceConfig;
use results::{Batch, Case, Coverage, Doing, Ending, FORMAT_VERSION, Row, Source};

/// The generated world's radius in columns: room for a 6,000-tick walk.
const RADIUS: i32 = 8;

struct Args {
    runs: u64,
    first_seed: u64,
    lane: u32,
    out: PathBuf,
    cap: u64,
    walk: Option<Script>,
    run: String,
    threads: u64,
    config: String,
    landings_file: Option<String>,
    worlds: PathBuf,
    goal: Option<plan::Aim>,
    ticks: u64,
    want: usize,
    check: bool,
    report: Option<PathBuf>,
    goals: String,
}

fn args() -> Result<Args, String> {
    let mut a = Args {
        runs: 100,
        first_seed: 0,
        lane: 0,
        out: PathBuf::from("target/simrace/lane-0"),
        cap: 6000,
        walk: None,
        run: format!("local-sim-{}", bot::now_ms()),
        threads: std::thread::available_parallelism().map_or(4, |n| n.get() as u64),
        config: "config/race.toml".to_owned(),
        landings_file: None,
        worlds: PathBuf::from("target/worlds"),
        goal: None,
        ticks: 3000,
        want: 200,
        check: false,
        report: None,
        goals: "config/goals.toml".to_owned(),
    };
    let mut it = std::env::args().skip(1);
    while let Some(flag) = it.next() {
        let v = it.next().ok_or_else(|| format!("{flag} needs a value"))?;
        let num = |v: &str| v.parse::<u64>().map_err(|e| format!("{flag}: {e}"));
        match flag.as_str() {
            "--runs" => a.runs = num(&v)?,
            "--first-seed" => a.first_seed = num(&v)?,
            "--lane" => a.lane = u32::try_from(num(&v)?).map_err(|e| e.to_string())?,
            "--out" => a.out = PathBuf::from(v),
            "--cap" => a.cap = num(&v)?,
            "--walk" => a.walk = Some(Script::parse(&v).ok_or(format!("--walk: no script {v}"))?),
            "--run" => a.run = v,
            "--threads" => a.threads = num(&v)?.max(1),
            "--config" => a.config = v,
            "--landings-file" => a.landings_file = Some(v),
            "--worlds" => a.worlds = PathBuf::from(v),
            "--goal" => a.goal = Some(plan::Aim::parse(&v).ok_or(format!("--goal: no aim {v}"))?),
            "--ticks" => a.ticks = num(&v)?,
            "--want" => a.want = usize::try_from(num(&v)?).map_err(|e| e.to_string())?,
            "--check" => a.check = v == "true",
            "--report" => a.report = Some(PathBuf::from(v)),
            "--goals" => a.goals = v,
            other => return Err(format!("unknown flag {other}")),
        }
    }
    Ok(a)
}

fn one(a: &Args, config: sim::Config, seed: u64) -> Result<Case, String> {
    let world = sim::terrain::world(seed, RADIUS).map_err(|e| e.to_string())?;
    let script = a.walk;
    let run = bot::lockstep::run(
        sim::Sim::new(config, world),
        &format!("rs-s{seed}"),
        seed,
        a.cap,
        bot::SETTLE_TICKS,
        |_, t| script.map_or_else(physics::Controls::default, |s| s.controls(t)),
    );
    let w = &run.world;
    let ending = match (&w.death, &run.sim.closed) {
        (Some(d), _) => Ending::Death {
            killer: bot::killer(d),
        },
        (None, Some(why)) => Ending::Kicked {
            reason: why.clone(),
        },
        (None, None) => Ending::Cap,
    };
    let doing = match script {
        Some(s) => Doing {
            task: format!("script {}", s.name()),
            why: "scripted harness until the planner (issue 09)".to_owned(),
        },
        None => Doing {
            task: "idle".to_owned(),
            why: "no planner yet (issue 09)".to_owned(),
        },
    };
    Ok(Case {
        format_version: FORMAT_VERSION,
        run: a.run.clone(),
        bot: format!("sim-{seed}"),
        commit: record::COMMIT.to_owned(),
        lane: a.lane,
        start: "spawn".to_owned(),
        source: Source::Sim,
        stage_reached: w.stage_reached,
        ending,
        death_text: w.death.as_ref().map(|d| d.text.clone()),
        first_hit: w.hits.first().cloned(),
        hits: w.hits.clone(),
        doing,
        items_gained: BTreeMap::new(),
        items_lost: BTreeMap::new(),
        swings_sent: 0,
        swings_landed: 0,
        health: w.player.health,
        pos: w.body.map(|b| b.pos),
        dimension: w.dimension.as_ref().map(|d| d.name.clone()),
        server_ticks: run.sim.ticks,
        wall_ms: 0,
        recording: None,
        replay: format!(
            "cargo run --release -p bot --bin simrace -- --runs 1 --first-seed {seed} --cap {} (commit {})",
            a.cap,
            record::COMMIT
        ),
        stream: None,
        plan: None,
    })
}

fn simrace(a: &Args) -> Result<bool, String> {
    let (race, hash) = RaceConfig::read(&a.config)?;
    let config = sim::Config {
        view_distance: i32::try_from(race.server.view_distance).map_err(|e| e.to_string())?,
        simulation_distance: i32::try_from(race.server.simulation_distance)
            .map_err(|e| e.to_string())?,
        peaceful: race.server.difficulty == "peaceful",
    };
    std::fs::create_dir_all(&a.out).map_err(|e| e.to_string())?;
    let seeds: Vec<u64> = (a.first_seed..a.first_seed + a.runs).collect();
    let cases: Vec<Result<Case, String>> = std::thread::scope(|s| {
        let handles: Vec<_> = (0..a.threads)
            .map(|t| {
                let seeds = &seeds;
                s.spawn(move || {
                    seeds
                        .iter()
                        .enumerate()
                        .filter(|(i, _)| *i as u64 % a.threads == t)
                        .map(|(_, seed)| one(a, config, *seed))
                        .collect::<Vec<_>>()
                })
            })
            .collect();
        handles
            .into_iter()
            .flat_map(|h| h.join().unwrap_or_default())
            .collect()
    });
    let mut rows = Vec::new();
    let mut coverage = Coverage {
        runs: a.runs,
        ..Coverage::default()
    };
    for c in cases {
        let c = c?;
        let file = format!("{}.case.rsc", c.bot);
        results::write(&a.out.join(&file), &c)?;
        coverage.cases += 1;
        rows.push(Row {
            bot: c.bot.clone(),
            stage_reached: c.stage_reached,
            ending: c.ending.clone(),
            signature: c.signature(),
            server_ticks: c.server_ticks,
            wall_ms: 0,
            case: Some(file),
            recording: None,
            aim: None,
        });
    }
    rows.sort_by(|x, y| x.bot.cmp(&y.bot));
    let batch = Batch {
        format_version: FORMAT_VERSION,
        run: a.run.clone(),
        commit: record::COMMIT.to_owned(),
        lane: a.lane,
        start: "spawn".to_owned(),
        source: Source::Sim,
        config_hash: hash,
        bots: u32::try_from(a.runs).unwrap_or(u32::MAX),
        server: "model".to_owned(),
        server_tick_rate: 0.0,
        rows,
        coverage,
    };
    results::write(&a.out.join("batch.rsb"), &batch)?;
    print!(
        "{}",
        results::render_summary(
            &results::summarize(std::slice::from_ref(&batch)),
            std::slice::from_ref(&batch)
        )
    );
    Ok(batch.coverage.cases == batch.coverage.runs)
}

/// Issue 09's measure over captured landings.
fn landings(a: &Args, file: &str) -> Result<bool, String> {
    let (race, _) = RaceConfig::read(&a.config)?;
    let config = sim::Config {
        view_distance: i32::try_from(race.server.view_distance).map_err(|e| e.to_string())?,
        simulation_distance: i32::try_from(race.server.simulation_distance)
            .map_err(|e| e.to_string())?,
        peaceful: race.server.difficulty == "peaceful",
    };
    let aim = a.goal.ok_or("--landings-file needs --goal")?;
    let table = match aim {
        plan::Aim::Table => Some(std::sync::Arc::new(bot::goals_table(&a.goals)?)),
        plan::Aim::ReachLog { .. } => None,
    };
    let list = results::parity::Landings::read(file)?;
    let n = list.landing.len();
    let runs: Vec<bot::landings::LandingRun> = std::thread::scope(|s| {
        let handles: Vec<_> = (0..a.threads)
            .map(|t| {
                let list = &list;
                let table = table.clone();
                s.spawn(move || {
                    (0..n)
                        .filter(|i| *i as u64 % a.threads == t)
                        .map(|i| {
                            bot::landings::run_one(
                                &a.worlds,
                                list,
                                i,
                                config,
                                aim,
                                a.ticks,
                                table.clone(),
                            )
                        })
                        .collect::<Vec<_>>()
                })
            })
            .collect();
        let mut all: Vec<_> = handles
            .into_iter()
            .flat_map(|h| h.join().unwrap_or_default())
            .collect();
        all.sort_by_key(|r| r.index);
        all
    });
    let what = match aim {
        plan::Aim::ReachLog { min_distance } => format!("reach a log {min_distance}+ blocks away"),
        plan::Aim::Table => format!("hold a crafting table (the kit) within {} ticks", a.ticks),
    };
    let m = bot::landings::measure(
        &runs,
        &what,
        a.want,
        0.95,
        plan::walk::PATIENCE_TICKS.max(1200),
        race.budget.as_ref(),
    );
    print!("{}", m.text);
    if let Some(p) = &a.report {
        std::fs::write(p, &m.text).map_err(|e| e.to_string())?;
    }
    Ok(m.pass || !a.check)
}

fn main() -> ExitCode {
    let a = match args() {
        Ok(a) => a,
        Err(e) => {
            eprintln!("simrace: {e}");
            return ExitCode::FAILURE;
        }
    };
    let result = match a.landings_file.clone() {
        Some(f) => landings(&a, &f),
        None => simrace(&a),
    };
    match result {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => {
            eprintln!("simrace: a done-condition does not hold (or coverage under 1.0)");
            ExitCode::FAILURE
        }
        Err(e) => {
            eprintln!("simrace: {e}");
            ExitCode::FAILURE
        }
    }
}
