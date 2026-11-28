//! race: one lane of the real race (brain/ruststeve/design/race-and-results.md). N bots, one
//! thread each, against one vanilla server; each gets the start's kit and place over RCON once
//! in the world, runs to its cap in server ticks, its death or a disconnect, and leaves a
//! recording and a case file; the lane leaves one batch file.
//!
//! ```text
//! RUSTSTEVE_RCON_PASSWORD=... cargo run -p bot --bin race -- --start spawn --bots 14 --lane 0 \
//!     --out target/race/lane-0 [--config config/race.toml] [--starts config/starts.toml] \
//!     [--host 127.0.0.1] [--port 25565] [--rcon-port 25575] [--run local-<ms>] \
//!     [--cap TICKS] [--wall-limit SECONDS] [--walk square] [--snapshot-at TICKS]
//! ```
//!
//! `--wall-limit` is the job's end, never a cap on behaviour: bots still running then end `Cut`
//! and are not counted.

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::JoinHandle;
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
    /// Keep each bot living after its aim is met, to the cap (issue 12e).
    hold_after_aim: bool,
    revive: bool,
    /// Take a snapshot once a bot has played this many server ticks (`--snapshot-at`).
    snapshot_at: Option<u64>,
    /// A snapshot's memory to seed every bot with (`--memory`).
    memory: Option<PathBuf>,
    /// The server's directory: its `world/` is what a snapshot copies.
    server_dir: PathBuf,
    walk: Option<bot::Script>,
    goal: Option<plan::Aim>,
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
        hold_after_aim: false,
        revive: false,
        snapshot_at: None,
        memory: None,
        server_dir: PathBuf::from("target/server"),
        walk: None,
        goal: None,
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
            "--hold-after-aim" => a.hold_after_aim = v == "true",
            "--revive" => a.revive = v == "true",
            "--snapshot-at" => a.snapshot_at = Some(num(&flag, &v)?),
            "--memory" => a.memory = Some(PathBuf::from(v)),
            "--server-dir" => a.server_dir = PathBuf::from(v),
            "--goal" => a.goal = Some(plan::Aim::parse(&v).ok_or(format!("--goal: no aim {v}"))?),
            "--walk" => {
                a.walk = Some(bot::Script::parse(&v).ok_or(format!("--walk: no script {v}"))?)
            }
            other => return Err(format!("unknown flag {other}")),
        }
    }
    Ok(a)
}

/// The race a snapshot comes from: its run, its lane, and whether it is a development race
/// (`bot::devstart`).
#[derive(Clone)]
struct From {
    run: String,
    lane: u32,
    dev: bool,
}

/// Copy the server's world (after `save-all flush`), the bot's memory and a `snap.toml` into
/// `<out>/<stage>/`: the moment a bot first reached the stage, for later attempts to resume from.
fn snapshot(
    up: &bot::StageUp,
    server_world: &std::path::Path,
    out: &std::path::Path,
    rcon_addr: &str,
    password: &str,
    from: &From,
) -> Result<(), String> {
    let From { run, lane, dev } = from;
    let (lane, dev) = (*lane, *dev);
    if password.is_empty() {
        return Err("no RCON password: the world cannot be saved first".to_owned());
    }
    let mut r = rcon::Rcon::connect(rcon_addr, password)?;
    let reply = r.command("save-all flush")?;
    println!("snapshot {}: save-all flush -> {reply}", up.stage.name());
    // A stage's snapshot is named by the stage; one on request or of a stuck spot by its kind.
    let kind = match up.kind {
        bot::SnapKind::Stage => None,
        bot::SnapKind::At(t) => Some(("at", t)),
        bot::SnapKind::Stuck => Some(("stuck", 0)),
    };
    let dir = out.join(match kind {
        None => up.stage.name().to_owned(),
        Some((k, t)) => bot::devstart::request_dir(k, t, &up.bot),
    });
    let files = results::snapshot::copy_dir(server_world, &dir.join("world"))?;
    results::snapshot::write_memory(&dir.join("memory.rsw"), &up.world.memory)?;
    // >>> eyes (Q3, issue 19c)
    if let Some(k) = &up.eyes {
        results::snapshot::write_eyes(&dir.join("eyes.rsw"), k)?;
    }
    // <<< eyes
    let snap = results::snapshot::Snap {
        stage: up.stage.name().to_owned(),
        bot: up.bot.clone(),
        uuid: results::snapshot::offline_uuid(&up.bot),
        server_ticks: up.world.session.server_ticks,
        commit: record::COMMIT.to_owned(),
        run: run.clone(),
        lane,
        pos: up.world.body.map_or([0.0; 3], |b| b.pos),
        dimension: up
            .world
            .dimension
            .as_ref()
            .map_or_else(String::new, |d| d.name.clone()),
        items: up
            .world
            .inventory
            .slots
            .iter()
            .filter(|(slot, _)| **slot >= 1)
            .map(|(_, s)| *s)
            .chain(up.world.inventory.carried)
            .fold(std::collections::BTreeMap::new(), |mut m, (item, n)| {
                let name = item
                    .name()
                    .map_or_else(|| format!("unknown:{}", item.id()), str::to_owned);
                *m.entry(name).or_default() += i64::from(n);
                m
            }),
    };
    let text = toml::to_string(&snap).map_err(|e| e.to_string())?;
    // A development race's snapshots are marked: never the frontier (bot::devstart).
    let text = if dev {
        bot::devstart::mark_dev(&text)
    } else {
        text
    };
    let text = match kind {
        Some((k, _)) => bot::devstart::mark_kind(&text, k),
        None => text,
    };
    std::fs::write(dir.join("snap.toml"), text).map_err(|e| e.to_string())?;
    println!(
        "snapshot {}{}: {} at {:?} in {}, {files} files",
        snap.stage,
        if dev { " (dev)" } else { "" },
        snap.bot,
        snap.pos,
        snap.dimension
    );
    Ok(())
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
    let needs_rcon = !start_commands(&start, "x", 0).is_empty();
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

    let memory = a
        .memory
        .as_deref()
        .map(results::snapshot::read_memory)
        .transpose()?;
    // >>> eyes (Q3, issue 19c)
    // The snapshot's `eyes.rsw` beside its memory; a missing file reads as the default.
    let eyes = a
        .memory
        .as_ref()
        .and_then(|m| m.parent())
        .map(|d| results::snapshot::read_eyes(&d.join("eyes.rsw")))
        .transpose()?;
    // <<< eyes
    // The snapshot taker: the first bot to reach each stage above the start's (one-bot-first).
    let (tx, rx) = std::sync::mpsc::channel::<bot::StageUp>();
    let snapper = {
        // Resuming: the snapshot's own stage is the floor, so the resumed bots' first tick (already
        // there) takes no second copy of it.
        let resumed_text = a
            .memory
            .as_ref()
            .and_then(|m| m.parent())
            .map(|d| d.join("snap.toml"))
            .and_then(|p| std::fs::read_to_string(p).ok());
        let resumed = resumed_text
            .as_deref()
            .and_then(|t| toml::from_str::<results::snapshot::Snap>(t).ok())
            .and_then(|snap| world::stage::Stage::from_name(&snap.stage));
        // A made-up start, or a resume of a snapshot that came from one: development only.
        let dev = bot::devstart::race_is_dev(&start.name, resumed_text.as_deref());
        if dev {
            println!(
                "race: a development start ({}): its snapshots are marked dev and can never be the frontier",
                start.name
            );
        }
        let from = resumed.or_else(|| world::stage::Stage::from_name(&start.stage));
        let server_world = a.server_dir.join("world");
        let out = a.out.join("snapshots");
        let rcon_addr = format!("{}:{}", a.host, a.rcon_port);
        let password = password.clone();
        let race = From {
            run: a.run.clone(),
            lane: a.lane,
            dev,
        };
        std::thread::spawn(move || {
            let mut best = from;
            let mut at_taken = false;
            for up in rx {
                match up.kind {
                    bot::SnapKind::Stage => {
                        if best.is_some_and(|b| up.stage <= b) {
                            continue;
                        }
                        best = Some(up.stage);
                    }
                    // One world for the lane: the first bot to play the ticks asked for.
                    bot::SnapKind::At(_) => {
                        if at_taken {
                            continue;
                        }
                        at_taken = true;
                    }
                    bot::SnapKind::Stuck => {}
                }
                if let Err(e) = snapshot(&up, &server_world, &out, &rcon_addr, &password, &race) {
                    eprintln!("snapshot {}: {e}", up.stage.name());
                }
            }
        })
    };

    let mut handles = Vec::new();
    for i in 0..bots {
        if i > 0 {
            std::thread::sleep(Duration::from_millis(config.race.join_stagger_ms));
        }
        let name = bot_name(a.lane, i);
        // The bot's number in its lane: the lane's bots share one server, so each takes its own
        // place, and every lane takes the same ones (paired lanes).
        let commands = start_commands(&start, &name, i as usize);
        let rcon_addr = format!("{}:{}", a.host, a.rcon_port);
        let password = password.clone();
        let watch_password = password.clone();
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
            script: a.walk,
            script_ticks: None,
            aim: a.goal,
            cap_ticks: Some(cap),
            deadline,
            start: apply,
            verbose: false,
            // The live view is local only: a runner never sets STEVE_RELAY.
            relay: std::env::var("STEVE_RELAY").ok().filter(|r| !r.is_empty()),
            run_id: a.run.clone(),
            lane: a.lane,
            start_name: start.name.clone(),
            case_path: a.out.join(format!("{name}.case.rsc")).display().to_string(),
            model: None,
            hold_after_aim: a.hold_after_aim,
            revive: a.revive,
            memory: memory.clone(),
            on_stage: Some(tx.clone()),
            no_progress_ticks: config.race.no_progress_ticks,
            progress: Some(a.out.join(format!("progress-{name}.txt"))),
            snapshot_at: a.snapshot_at,
            eyes: eyes.clone(), // >>> eyes (Q3, issue 19c)
            seed: Some(plan::lane_seed::derive(&a.run, a.lane, i)), // >>> lane seed (gen)
        };
        let watch = Watch::start(
            format!("{}:{}", a.host, a.rcon_port),
            watch_password,
            name.clone(),
        );
        handles.push((name, std::thread::spawn(move || bot::run(opts)), watch));
    }

    drop(tx);
    // Every bot's sender gone when its thread ends: the snapshots finish before the batch.
    if snapper.join().is_err() {
        eprintln!("race: the snapshot thread panicked");
    }
    let mut rows = Vec::new();
    let mut coverage = Coverage::default();
    let mut tick_rate = 0.0f64;
    for (name, h, watch) in handles {
        coverage.runs += 1;
        let outcome = match h.join() {
            Ok(Ok(o)) => o,
            Ok(Err(e)) => {
                // Never connected: still a run, with a case saying so.
                eprintln!("race: {name}: {e}");
                watch.finish(&name, None);
                rows.push(never_connected(a, &start.name, &name, &e, &mut coverage)?);
                continue;
            }
            Err(_) => {
                watch.finish(&name, None);
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
        // Server truth (issue 16e): the last minutes' samples and the end's, and what the bot
        // was blind to at the end.
        let truth_samples = watch.finish(&name, outcome.world.player.pos);
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
        for ((state, id), n) in &outcome.stats.unknown {
            *coverage
                .unknown_by_kind
                .entry(protocol::packet_name(*state, *id))
                .or_default() += n;
        }
        coverage.slots_truncated += w.inventory.truncated;
        for (k, n) in &w.inventory.truncated_by {
            *coverage.slots_truncated_by.entry(k.clone()).or_default() += n;
        }
        *coverage.metadata_truncated.get_or_insert(0) += w.metadata_unread.values().sum::<u64>();
        for (k, n) in &w.metadata_unread {
            *coverage.metadata_truncated_by.entry(k.clone()).or_default() += n;
        }
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
        let mut case = Case {
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
            swings_sent: outcome.plan.as_ref().map_or(0, |p| p.swings_sent),
            swings_landed: outcome.plan.as_ref().map_or(0, |p| p.swings_landed),
            health: w.player.health,
            pos: w.player.pos,
            dimension: w.dimension.as_ref().map(|d| d.name.clone()),
            server_ticks: ticks,
            wall_ms: outcome.stats.wall_ms,
            recording: outcome.recorded.then(|| recording.clone()),
            replay: format!("cargo run -p replay -- verify {recording}"),
            plan: outcome.plan.as_ref().map(|p| results::PlanSummary {
                met_at: outcome.aim_met_at,
                ..bot::plan_summary(p, &outcome.plan_tick_us, &outcome.plan_notes)
            }),
            stream: outcome.stream.map(|c| results::StreamCounts {
                sent: c.sent,
                dropped: c.dropped,
                failed: c.failed,
                connects: c.connects,
            }),
            story: None,
        };
        // The ledger (issue 16e): every task with its reason and outcome, beside the case.
        let ledger = results::Ledger {
            format_version: FORMAT_VERSION,
            run: a.run.clone(),
            bot: name.clone(),
            commit: record::COMMIT.to_owned(),
            tasks: outcome.ledger.clone(),
            dropped: 0,
            facts: outcome.facts.clone(),
        };
        // The same as text, to read without running anything (`results ledger` prints it too).
        if let Err(e) = std::fs::write(a.out.join(format!("{name}.ledger.txt")), ledger.render()) {
            eprintln!("race: {name}: the ledger text: {e}");
        }
        match results::write(&a.out.join(format!("{name}.ledger.rsl")), &ledger) {
            Ok(()) => {
                *coverage.ledgers.get_or_insert(0) += 1;
                *coverage.ledger_dropped.get_or_insert(0) += ledger.dropped;
            }
            Err(e) => eprintln!("race: {name}: the ledger: {e}"),
        }
        let senses = results::SensesFile {
            format_version: FORMAT_VERSION,
            run: a.run.clone(),
            bot: name.clone(),
            rows: outcome.senses.clone(),
        };
        if let Err(e) = std::fs::write(a.out.join(format!("{name}.senses.txt")), senses.render(10))
        {
            eprintln!("race: {name}: the senses text: {e}");
        }
        match results::write(&a.out.join(format!("{name}.senses.rss")), &senses) {
            Ok(()) => *coverage.senses.get_or_insert(0) += 1,
            Err(e) => eprintln!("race: {name}: the senses: {e}"),
        }
        match results::write(&a.out.join(format!("{name}.moments.rsm")), &outcome.moments) {
            Ok(()) => *coverage.moments.get_or_insert(0) += 1,
            Err(e) => eprintln!("race: {name}: the moments: {e}"),
        }
        if let Some(p) = &outcome.plan {
            *coverage.tasks_started.get_or_insert(0) += p.task_counts.started;
            *coverage.tasks_with_reason.get_or_insert(0) += p.task_counts.with_reason;
            *coverage.tasks_ended.get_or_insert(0) += p.task_counts.ended;
        }
        // The case tells the run's story from its ledger and senses, never cut.
        case.story = Some(results::story::story(&ledger, &outcome.senses));
        let believed: Vec<(String, [f64; 3])> = w
            .entities
            .values()
            .map(|e| (e.kind.name().unwrap_or("unknown").to_owned(), e.pos))
            .collect();
        let blind = truth_samples
            .last()
            .map(|s| results::truth::blind(s, &believed))
            .unwrap_or_default();
        if let Some(s) = case.story.as_mut() {
            s.blind.clone_from(&blind);
        }
        let truth = results::truth::TruthFile {
            format_version: FORMAT_VERSION,
            run: a.run.clone(),
            bot: name.clone(),
            samples: truth_samples,
            blind,
        };
        if !truth.samples.is_empty() {
            match results::write(&a.out.join(format!("{name}.truth.rst")), &truth) {
                Ok(()) => *coverage.truths.get_or_insert(0) += 1,
                Err(e) => eprintln!("race: {name}: the truth: {e}"),
            }
        } else {
            coverage.truths.get_or_insert(0);
        }
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
            std::slice::from_ref(&batch),
            results::observe::Observe::from_repo().as_ref(),
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
        aim: case.plan.as_ref().map(|p| (p.aim.clone(), p.reached)),
        pearls: Some(case.pearls_gained()),
        eyes: case.eyes(),
        end: case.end(),
        stage_ticks: case.stage_ticks(),
        aim_tick: case.aim_tick(),
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
        stream: None,
        plan: None,
        story: None,
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

/// Truth samples kept per bot: the last five minutes (issue 16e, phase D).
const TRUTH_KEPT: usize = 5;
/// Wall seconds between truth samples (a minute: the server ticks near 20 a second).
const TRUTH_EVERY_S: u64 = 60;

/// A bot's truth watcher: once a minute it asks the lane's own server, over RCON, what it holds
/// about the bot and the entities near it. Shell only: nothing it learns reaches the planner.
struct Watch {
    stop: Arc<AtomicBool>,
    thread: JoinHandle<VecDeque<results::truth::TruthSample>>,
    addr: String,
    password: String,
}

impl Watch {
    fn start(addr: String, password: String, bot: String) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let thread = {
            let (stop, addr, password) = (stop.clone(), addr.clone(), password.clone());
            std::thread::spawn(move || {
                let mut ring = VecDeque::new();
                let mut last = Instant::now();
                while !stop.load(Ordering::Relaxed) {
                    std::thread::sleep(Duration::from_secs(1));
                    if last.elapsed() < Duration::from_secs(TRUTH_EVERY_S) {
                        continue;
                    }
                    last = Instant::now();
                    let near = results::truth::near_command(&bot);
                    if let Ok(s) = truth_sample(&addr, &password, &near, &bot) {
                        ring.push_back(s);
                        if ring.len() > TRUTH_KEPT {
                            ring.pop_front();
                        }
                    }
                }
                ring
            })
        };
        Watch {
            stop,
            thread,
            addr,
            password,
        }
    }

    /// Stops the watcher and takes the end sample around the bot's last position (the bot has
    /// left the server, so its name no longer anchors the search).
    fn finish(self, bot: &str, pos: Option<[f64; 3]>) -> Vec<results::truth::TruthSample> {
        self.stop.store(true, Ordering::Relaxed);
        let mut samples: Vec<_> = self.thread.join().unwrap_or_default().into_iter().collect();
        if let Some(p) = pos {
            let near = format!(
                "execute positioned {} {} {} as @e[distance=..32,limit=12,sort=nearest,type=!player,type=!item,type=!experience_orb] run data get entity @s",
                p[0], p[1], p[2]
            );
            match truth_sample(&self.addr, &self.password, &near, bot) {
                Ok(s) => samples.push(s),
                Err(e) => eprintln!("race: {bot}: the end's truth: {e}"),
            }
        }
        samples
    }
}

/// One truth sample: the game time, the entities near, and the bot as the server holds it.
fn truth_sample(
    addr: &str,
    password: &str,
    near: &str,
    bot: &str,
) -> Result<results::truth::TruthSample, String> {
    let mut r = rcon::Rcon::connect(addr, password)?;
    let time = r.command("time query gametime")?;
    let at_tick = time
        .split_whitespace()
        .last()
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);
    let reply = r.command(near)?;
    let me = r.command(&results::truth::self_command(bot))?;
    let bot_entity = results::truth::parse_reply(&me).into_iter().next();
    Ok(results::truth::TruthSample {
        at_tick,
        entities: results::truth::parse_reply(&reply),
        bot: bot_entity,
        bot_items: results::truth::bot_items(&me),
        raw: format!("{reply}\n{me}"),
    })
}
