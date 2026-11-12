//! race: one lane of the real race (brain/ruststeve/design/race-and-results.md). N bots, one
//! thread each, against one vanilla server; each gets the start's kit and place over RCON once
//! in the world, runs to its cap in server ticks, its death or a disconnect, and leaves a
//! recording and a case file; the lane leaves one batch file.
//!
//! ```text
//! RUSTSTEVE_RCON_PASSWORD=... cargo run -p bot --bin race -- --start spawn --bots 14 --lane 0 \
//!     --out target/race/lane-0 [--config config/race.toml] [--starts config/starts.toml] \
//!     [--host 127.0.0.1] [--port 25565] [--rcon-port 25575] [--run local-<ms>] \
//!     [--cap TICKS] [--wall-limit SECONDS] [--walk square]
//! ```
//!
//! `--wall-limit` is the job's end, never a cap on behaviour: bots still running then end `Cut`
//! and are not counted.

use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::{Duration, Instant};

use results::config::{RaceConfig, Starts, start_commands};
use results::{Batch, Case, Coverage, FORMAT_VERSION, Row, Source};

struct Args {
    config: String,
    starts: String,
    start: String,
    bots: Option<u32>,
    lane: u32,
    out: PathBuf,
    host: String,
    port: u16,
    rcon_port: u16,
    run: String,
    cap: Option<u64>,
    wall_limit: Option<u64>,
    walk: bool,
}

fn args() -> Result<Args, String> {
    let mut a = Args {
        config: "config/race.toml".to_owned(),
        starts: "config/starts.toml".to_owned(),
        start: "spawn".to_owned(),
        bots: None,
        lane: 0,
        out: PathBuf::from("target/race/lane-0"),
        host: "127.0.0.1".to_owned(),
        port: 25565,
        rcon_port: 25575,
        run: format!("local-{}", bot::now_ms()),
        cap: None,
        wall_limit: None,
        walk: false,
    };
    let mut it = std::env::args().skip(1);
    let num = |flag: &str, v: &str| -> Result<u64, String> {
        v.parse().map_err(|e| format!("{flag}: {e}"))
    };
    while let Some(flag) = it.next() {
        let v = it.next().ok_or_else(|| format!("{flag} needs a value"))?;
        match flag.as_str() {
            "--config" => a.config = v,
            "--starts" => a.starts = v,
            "--start" => a.start = v,
            "--bots" => a.bots = Some(u32::try_from(num(&flag, &v)?).map_err(|e| e.to_string())?),
            "--lane" => a.lane = u32::try_from(num(&flag, &v)?).map_err(|e| e.to_string())?,
            "--out" => a.out = PathBuf::from(v),
            "--host" => a.host = v,
            "--port" => a.port = u16::try_from(num(&flag, &v)?).map_err(|e| e.to_string())?,
            "--rcon-port" => {
                a.rcon_port = u16::try_from(num(&flag, &v)?).map_err(|e| e.to_string())?;
            }
            "--run" => a.run = v,
            "--cap" => a.cap = Some(num(&flag, &v)?),
            "--wall-limit" => a.wall_limit = Some(num(&flag, &v)?),
            "--walk" => a.walk = v == "square",
            other => return Err(format!("unknown flag {other}")),
        }
    }
    Ok(a)
}

/// A bot's name: lane and index, within the 16 characters an offline name may have.
fn bot_name(lane: u32, i: u32) -> String {
    format!("rs-l{lane:02}-b{i:02}")
}

fn race(a: &Args) -> Result<bool, String> {
    let (config, config_hash) = RaceConfig::read(&a.config)?;
    let starts = Starts::read(&a.starts)?;
    let start = starts
        .find(&a.start)
        .ok_or_else(|| format!("no start {} in {}", a.start, a.starts))?
        .clone();
    let password = std::env::var("RUSTSTEVE_RCON_PASSWORD").unwrap_or_default();
    let needs_rcon = !start_commands(&start, "x").is_empty();
    if needs_rcon && password.is_empty() {
        return Err(format!(
            "start {} needs RCON: set RUSTSTEVE_RCON_PASSWORD to the server's",
            start.name
        ));
    }
    let bots = a.bots.unwrap_or(config.race.bots_per_server);
    let cap = a.cap.or(start.cap_ticks).unwrap_or(config.race.cap_ticks);
    let deadline = a
        .wall_limit
        .map(|s| Instant::now() + Duration::from_secs(s));
    std::fs::create_dir_all(&a.out).map_err(|e| format!("{}: {e}", a.out.display()))?;
    println!(
        "race {} lane {}: {bots} bots on start {} (cap {cap} server ticks) at {}:{}, commit {}",
        a.run,
        a.lane,
        start.name,
        a.host,
        a.port,
        record::COMMIT
    );

    let mut handles = Vec::new();
    for i in 0..bots {
        if i > 0 {
            std::thread::sleep(Duration::from_millis(config.race.join_stagger_ms));
        }
        let name = bot_name(a.lane, i);
        let commands = start_commands(&start, &name);
        let rcon_addr = format!("{}:{}", a.host, a.rcon_port);
        let password = password.clone();
        let apply: Option<bot::ApplyStart> = (!commands.is_empty()).then(|| {
            Box::new(move || {
                let mut r = rcon::Rcon::connect(&rcon_addr, &password)?;
                for c in &commands {
                    let reply = r.command(c)?;
                    println!("rcon: {c} -> {reply}");
                }
                Ok(())
            }) as bot::ApplyStart
        });
        let opts = bot::Opts {
            host: a.host.clone(),
            port: a.port,
            name: name.clone(),
            record: Some(a.out.join(format!("{name}.rsr"))),
            walk: a.walk,
            cap_ticks: Some(cap),
            deadline,
            start: apply,
            verbose: false,
        };
        handles.push((name, std::thread::spawn(move || bot::run(opts))));
    }

    let mut rows = Vec::new();
    let mut coverage = Coverage::default();
    let mut tick_rate = 0.0f64;
    for (name, h) in handles {
        coverage.runs += 1;
        let outcome = match h.join() {
            Ok(Ok(o)) => o,
            Ok(Err(e)) => {
                // Never connected: still a run, with a case saying so.
                eprintln!("race: {name}: {e}");
                rows.push(never_connected(a, &start.name, &name, &e, &mut coverage)?);
                continue;
            }
            Err(_) => {
                rows.push(never_connected(
                    a,
                    &start.name,
                    &name,
                    "the bot thread panicked",
                    &mut coverage,
                )?);
                continue;
            }
        };
        let w = &outcome.world;
        let ticks = w
            .session
            .server_ticks
            .saturating_sub(outcome.stats.play_ticks_from);
        if outcome.stats.play_ms > 0 {
            tick_rate = tick_rate.max(ticks as f64 / (outcome.stats.play_ms as f64 / 1000.0));
        }
        coverage.events_dropped += outcome.stats.decode_errors;
        coverage.unknown_packets += outcome.stats.unknown.values().sum::<u64>();
        let recording = format!("{name}.rsr");
        if outcome.recorded {
            coverage.recordings += 1;
        }
        let end_tick = w.session.server_ticks;
        let hits: Vec<_> = w
            .hits
            .iter()
            .filter(|h| h.at_tick + world::HITS_KEPT_TICKS >= end_tick)
            .cloned()
            .collect();
        let (items_gained, items_lost) = outcome.items_changed();
        let case = Case {
            format_version: FORMAT_VERSION,
            run: a.run.clone(),
            bot: name.clone(),
            commit: record::COMMIT.to_owned(),
            lane: a.lane,
            start: start.name.clone(),
            source: Source::Real,
            stage_reached: w.stage_reached,
            ending: outcome.ending.clone(),
            death_text: w.death.as_ref().map(|d| d.text.clone()),
            first_hit: hits.first().cloned(),
            hits,
            doing: outcome.doing.clone(),
            items_gained,
            items_lost,
            swings_sent: 0,
            swings_landed: 0,
            health: w.player.health,
            pos: w.player.pos,
            dimension: w.dimension.as_ref().map(|d| d.name.clone()),
            server_ticks: ticks,
            wall_ms: outcome.stats.wall_ms,
            recording: outcome.recorded.then(|| recording.clone()),
            replay: format!("cargo run -p replay -- verify {recording}"),
        };
        let row = write_case(&a.out, &case, &mut coverage)?;
        rows.push(row);
    }
    let batch = Batch {
        format_version: FORMAT_VERSION,
        run: a.run.clone(),
        commit: record::COMMIT.to_owned(),
        lane: a.lane,
        start: start.name.clone(),
        source: Source::Real,
        config_hash,
        bots,
        server: format!("{}:{}", a.host, a.port),
        server_tick_rate: tick_rate,
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
    println!("server tick rate {tick_rate:.1} ticks/s");
    Ok(batch.coverage.cases == batch.coverage.runs)
}

/// Writes a case (binary and text) and returns its batch row.
fn write_case(dir: &Path, case: &Case, coverage: &mut Coverage) -> Result<Row, String> {
    let file = format!("{}.case.rsc", case.bot);
    results::write(&dir.join(&file), case)?;
    std::fs::write(dir.join(format!("{}.case.txt", case.bot)), case.render())
        .map_err(|e| e.to_string())?;
    coverage.cases += 1;
    Ok(Row {
        bot: case.bot.clone(),
        stage_reached: case.stage_reached,
        ending: case.ending.clone(),
        signature: case.signature(),
        server_ticks: case.server_ticks,
        wall_ms: case.wall_ms,
        case: Some(file),
        recording: case.recording.clone(),
    })
}

/// A run that never reached the server still ends with a case: `Lost`, nothing reached.
fn never_connected(
    a: &Args,
    start: &str,
    name: &str,
    why: &str,
    coverage: &mut Coverage,
) -> Result<Row, String> {
    let case = Case {
        format_version: FORMAT_VERSION,
        run: a.run.clone(),
        bot: name.to_owned(),
        commit: record::COMMIT.to_owned(),
        lane: a.lane,
        start: start.to_owned(),
        source: Source::Real,
        stage_reached: None,
        ending: results::Ending::Lost {
            why: why.to_owned(),
        },
        death_text: None,
        first_hit: None,
        hits: Vec::new(),
        doing: results::Doing {
            task: "connect".to_owned(),
            why: "never reached the server".to_owned(),
        },
        items_gained: Default::default(),
        items_lost: Default::default(),
        swings_sent: 0,
        swings_landed: 0,
        health: None,
        pos: None,
        dimension: None,
        server_ticks: 0,
        wall_ms: 0,
        recording: None,
        replay: "-".to_owned(),
    };
    write_case(&a.out, &case, coverage)
}

fn main() -> ExitCode {
    let a = match args() {
        Ok(a) => a,
        Err(e) => {
            eprintln!("race: {e}");
            return ExitCode::FAILURE;
        }
    };
    match race(&a) {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => {
            eprintln!("race: coverage under 1.0");
            ExitCode::FAILURE
        }
        Err(e) => {
            eprintln!("race: {e}");
            ExitCode::FAILURE
        }
    }
}
