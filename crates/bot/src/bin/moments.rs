//! moments: the parity table (brain/ruststeve/design/parity-table.md). The same recorded moments
//! run in real and against the server model, and their outcome classes compared field by field.
//!
//! ```text
//! # on a real server (same seed, peaceful, RCON on), after `capture` wrote the worlds:
//! RUSTSTEVE_RCON_PASSWORD=... cargo run -p bot --bin moments -- real --worlds W --out R
//! # against the model in the same captured worlds:
//! cargo run -p bot --bin moments -- sim --worlds W --out S
//! # the table (exit 1 under the tolerance):
//! cargo run -p bot --bin moments -- compare R S [--tolerance 48] [--report parity.md]
//! ```
//!
//! Shared flags: `--moments config/moments.toml --landings config/landings.toml
//! --config config/race.toml`; real also takes `--host --port --rcon-port --wave 10`.

use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::{Duration, Instant};

use bot::Script;
use bot::captured::CapturedWorld;
use results::Source;
use results::config::RaceConfig;
use results::parity::{Health, Landings, Moment, MomentResult, Moments, Outcome, compare, render};
use world::World;

/// Server ticks a start settles before the script, the same in real and sim (bot::run's).
const SETTLE: u64 = bot::SETTLE_TICKS;
/// The table's stated tolerance: all but 2 of the moments match in every field (48 of issue
/// 08's 50; 78 of 80 with issue 10b's dig and craft rows; 103 of 105 with issue 11a's stone and
/// table rows).
const TOLERANCE: usize = 103;

struct Args {
    cmd: String,
    rest: Vec<String>,
    worlds: PathBuf,
    out: PathBuf,
    moments: String,
    landings: String,
    config: String,
    host: String,
    port: u16,
    rcon_port: u16,
    wave: usize,
    tolerance: usize,
    report: Option<PathBuf>,
}

fn args() -> Result<Args, String> {
    let mut all = std::env::args().skip(1);
    let cmd = all.next().ok_or("usage: moments real|sim|compare ...")?;
    let mut a = Args {
        cmd,
        rest: Vec::new(),
        worlds: PathBuf::from("target/worlds"),
        out: PathBuf::from("target/moments"),
        moments: "config/moments.toml".to_owned(),
        landings: "config/landings.toml".to_owned(),
        config: "config/race.toml".to_owned(),
        host: "127.0.0.1".to_owned(),
        port: 25565,
        rcon_port: 25575,
        wave: 10,
        tolerance: TOLERANCE,
        report: None,
    };
    while let Some(flag) = all.next() {
        if !flag.starts_with("--") {
            a.rest.push(flag);
            continue;
        }
        let v = all.next().ok_or_else(|| format!("{flag} needs a value"))?;
        let num = |v: &str| v.parse::<u64>().map_err(|e| format!("{flag}: {e}"));
        match flag.as_str() {
            "--worlds" => a.worlds = PathBuf::from(v),
            "--out" => a.out = PathBuf::from(v),
            "--moments" => a.moments = v,
            "--landings" => a.landings = v,
            "--config" => a.config = v,
            "--host" => a.host = v,
            "--port" => a.port = u16::try_from(num(&v)?).map_err(|e| e.to_string())?,
            "--rcon-port" => a.rcon_port = u16::try_from(num(&v)?).map_err(|e| e.to_string())?,
            "--wave" => a.wave = usize::try_from(num(&v)?).map_err(|e| e.to_string())?,
            "--tolerance" => a.tolerance = usize::try_from(num(&v)?).map_err(|e| e.to_string())?,
            "--report" => a.report = Some(PathBuf::from(v)),
            other => return Err(format!("unknown flag {other}")),
        }
    }
    Ok(a)
}

/// A moment's start: the landing's captured world and the surface position to stand on.
fn start_of(
    a: &Args,
    landings: &Landings,
    m: &Moment,
) -> Result<(CapturedWorld, [f64; 3]), String> {
    let l = landings
        .landing
        .get(m.landing)
        .ok_or_else(|| format!("{}: no landing {}", m.id, m.landing))?;
    let world = CapturedWorld::read(&CapturedWorld::path(&a.worlds, m.landing))?;
    let (x, z) = (l.x + m.dx, l.z + m.dz);
    let model = world.model([0.0; 3])?;
    let y = model
        .surface(x, z)
        .ok_or_else(|| format!("{}: no surface at {x} {z}", m.id))?;
    Ok((world, [f64::from(x) + 0.5, y, f64::from(z) + 0.5]))
}

/// The outcome class of a finished run, from the bot's own world. Health is the lowest the
/// script saw, so when a peaceful heal lands (every 20 ticks) cannot flip it.
fn outcome(
    w: &World,
    teleports_at_script: Option<u64>,
    min_health: Option<f32>,
) -> Option<Outcome> {
    let at = teleports_at_script?;
    let body = w.body?;
    let health = match (w.death.is_some(), min_health.or(w.player.health)) {
        (true, _) => Health::Dead,
        (false, Some(h)) if h <= 0.0 => Health::Dead,
        (false, Some(h)) if h < 20.0 => Health::Hurt,
        _ => Health::Full,
    };
    let mut inventory: std::collections::BTreeMap<String, i32> = std::collections::BTreeMap::new();
    let name = |k: data::ItemKind| k.name().map_or_else(|| format!("{k:?}"), str::to_owned);
    let held = w
        .inventory
        .slots
        .iter()
        .filter(|(s, _)| **s >= 1)
        .map(|(_, st)| *st)
        .chain(w.inventory.carried);
    for (k, n) in held.filter(|(k, _)| !chance_drop(*k)) {
        *inventory.entry(name(k)).or_default() += n;
    }
    let grid = w
        .inventory
        .slots
        .iter()
        .filter(|(s, _)| **s <= 4)
        .map(|(_, (_, n))| *n)
        .chain(w.inventory.carried.map(|(_, n)| n))
        .sum();
    Some(Outcome {
        cell: body.pos.map(|v| v.floor() as i32),
        on_ground: body.on_ground,
        in_water: body.in_water,
        corrected: w.session.teleports > at,
        health,
        inventory,
        grid,
    })
}

/// Whether some block drops this item only by chance (a sapling from leaves, a seed from grass,
/// flint from gravel): the model's roll is its own seeded one, never vanilla's, and the trees a
/// fresh server grows on the same seed differ from run to run, so such an item is not part of
/// the inventory class. An item a recipe makes (sticks: a dead bush drops them by chance) stays
/// in: the craft rows count it (issue 11a found sticks left out).
fn chance_drop(k: data::ItemKind) -> bool {
    let crafted = data::generated::recipes::RECIPES
        .iter()
        .any(|r| r.result == k);
    !crafted
        && data::generated::loot::BLOCK_DROPS
            .iter()
            .flat_map(|b| b.tries)
            .any(|d| d.item == k && d.chance < 1.0)
}

/// The block cell the feet of a start stand in.
fn feet_cell(start: [f64; 3]) -> (i32, i32, i32) {
    (
        start[0].floor() as i32,
        (start[1] + 0.01).floor() as i32,
        start[2].floor() as i32,
    )
}

/// A moment's `give` (`oak_log 1`) as a stack.
fn give_of(g: &str) -> Option<(data::ItemKind, i32)> {
    let (item, count) = g.split_once(' ')?;
    Some((data::ItemKind::from_name(item)?, count.parse().ok()?))
}

fn save(out: &Path, r: &MomentResult) -> Result<(), String> {
    let tag = match r.source {
        Source::Real => "real",
        Source::Sim => "sim",
    };
    results::write(&out.join(format!("{}.{tag}.rsm", r.moment)), r)
}

fn real(a: &Args) -> Result<(), String> {
    let moments = Moments::read(&a.moments)?;
    let landings = Landings::read(&a.landings)?;
    let password =
        std::env::var("RUSTSTEVE_RCON_PASSWORD").map_err(|_| "set RUSTSTEVE_RCON_PASSWORD")?;
    let nonce = bot::now_ms() & 0xffff;
    std::fs::create_dir_all(&a.out).map_err(|e| e.to_string())?;
    for (w, wave) in moments.moment.chunks(a.wave.max(1)).enumerate() {
        println!("moments real: wave {w}, {} moments", wave.len());
        let mut handles = Vec::new();
        for (i, m) in wave.iter().enumerate() {
            let (_, start) = start_of(a, &landings, m)?;
            let script = Script::parse(&m.script)
                .ok_or_else(|| format!("{}: no script {}", m.id, m.script))?;
            // A fresh player per run: the server saves a player's place, health and effects by
            // name, so a reused name would start where the last run left it.
            let name = format!("rs-m{:02}-{:04x}", w * a.wave + i, nonce);
            let rcon_addr = format!("{}:{}", a.host, a.rcon_port);
            let password = password.clone();
            let tp = format!("tp {name} {} {} {}", start[0], start[1], start[2]);
            // The model has no air supply yet (issue 12, survival): a real bot waiting under
            // water for its chunks would drown on a clock the model does not keep, so the real
            // side breathes. Stated in the report; drowning joins the table with issue 12.
            let breathe = format!("effect give {name} minecraft:water_breathing 600 0 true");
            let gives: Vec<String> = m
                .give
                .iter()
                .map(|g| format!("give {name} minecraft:{g}"))
                .chain(m.set.iter().map(|(b, dx, dy, dz)| {
                    let (x, y, z) = feet_cell(start);
                    format!("setblock {} {} {} minecraft:{b}", x + dx, y + dy, z + dz)
                }))
                .collect();
            let opts = bot::Opts {
                host: a.host.clone(),
                port: a.port,
                name: name.clone(),
                record: Some(a.out.join(format!("{}.real.rsr", m.id))),
                script: Some(script),
                script_ticks: Some(m.ticks),
                aim: None,
                cap_ticks: None,
                deadline: Some(Instant::now() + Duration::from_secs(300)),
                start: Some(Box::new(move || {
                    let mut r = rcon::Rcon::connect(&rcon_addr, &password)?;
                    r.command(&breathe)?;
                    r.command(&tp)?;
                    // A setblock right after the teleport can land before the chunk loads
                    // ("That position is not loaded") and do nothing: try again until it does.
                    gives.iter().try_for_each(|g| {
                        for _ in 0..80 {
                            let reply = r.command(g)?;
                            if !reply.contains("not loaded") {
                                return Ok(());
                            }
                            std::thread::sleep(Duration::from_millis(250));
                        }
                        Err(format!("{g}: never loaded"))
                    })
                })),
                verbose: false,
                relay: None,
                run_id: "parity".to_owned(),
                lane: 0,
                start_name: m.id.clone(),
                case_path: String::new(),
                model: None,
            };
            let id = m.id.clone();
            handles.push((id, std::thread::spawn(move || bot::run(opts))));
            std::thread::sleep(Duration::from_millis(500));
        }
        for (id, h) in handles {
            let r = match h.join() {
                Ok(Ok(o)) => MomentResult {
                    moment: id,
                    source: Source::Real,
                    commit: record::COMMIT.to_owned(),
                    outcome: outcome(&o.world, o.teleports_at_script, o.min_health_in_script),
                    corrections: o
                        .teleports_at_script
                        .map_or(0, |t| o.world.session.teleports.saturating_sub(t)),
                    ending: o.ending.name().to_owned(),
                    final_pos: o.world.body.map(|b| b.pos),
                },
                Ok(Err(e)) => MomentResult {
                    moment: id,
                    source: Source::Real,
                    commit: record::COMMIT.to_owned(),
                    outcome: None,
                    corrections: 0,
                    ending: format!("never connected: {e}"),
                    final_pos: None,
                },
                Err(_) => return Err("a bot thread panicked".to_owned()),
            };
            save(&a.out, &r)?;
        }
    }
    Ok(())
}

fn sim(a: &Args) -> Result<(), String> {
    let moments = Moments::read(&a.moments)?;
    let landings = Landings::read(&a.landings)?;
    let (race, _) = RaceConfig::read(&a.config)?;
    let config = sim::Config {
        view_distance: i32::try_from(race.server.view_distance).map_err(|e| e.to_string())?,
        simulation_distance: i32::try_from(race.server.simulation_distance)
            .map_err(|e| e.to_string())?,
        // The parity server runs peaceful (parity.yml): mobs join the table with issue 12.
        peaceful: true,
    };
    std::fs::create_dir_all(&a.out).map_err(|e| e.to_string())?;
    for (i, m) in moments.moment.iter().enumerate() {
        let (captured, start) = start_of(a, &landings, m)?;
        let script =
            Script::parse(&m.script).ok_or_else(|| format!("{}: no script {}", m.id, m.script))?;
        let mut world = captured.model(start)?;
        for (b, dx, dy, dz) in &m.set {
            let (x, y, z) = feet_cell(start);
            let state = data::BlockKind::from_name(b)
                .and_then(|k| k.info())
                .map(|i| i.default_state)
                .ok_or_else(|| format!("{}: no block {b}", m.id))?;
            world
                .set_block(x + dx, y + dy, z + dz, state)
                .map_err(|e| e.to_string())?;
        }
        let mut model = sim::Sim::new(config, world).seeded(i as u64 + 1);
        for g in &m.give {
            model
                .survival
                .give(give_of(g).ok_or_else(|| format!("{}: give {g}", m.id))?);
        }
        let run = bot::lockstep::run_acting(
            model,
            &format!("rs-m{i:02}"),
            i as u64 + 1,
            SETTLE + m.ticks,
            SETTLE,
            |w, t| (script.controls(t), script.acts(t, w)),
        );
        let r = MomentResult {
            moment: m.id.clone(),
            source: Source::Sim,
            commit: record::COMMIT.to_owned(),
            outcome: outcome(
                &run.world,
                run.teleports_at_script,
                run.min_health_in_script,
            ),
            corrections: run
                .teleports_at_script
                .map_or(0, |t| run.world.session.teleports.saturating_sub(t)),
            ending: if run.died { "death" } else { "cap" }.to_owned(),
            final_pos: run.world.body.map(|b| b.pos),
        };
        save(&a.out, &r)?;
    }
    println!("moments sim: {} moments", moments.moment.len());
    Ok(())
}

fn read_dir(dir: &Path) -> Result<Vec<MomentResult>, String> {
    let mut out = Vec::new();
    for e in std::fs::read_dir(dir)
        .map_err(|e| format!("{}: {e}", dir.display()))?
        .flatten()
    {
        let p = e.path();
        if p.extension().is_some_and(|x| x == "rsm") {
            out.push(results::read::<MomentResult>(&p)?);
        }
    }
    Ok(out)
}

fn compare_cmd(a: &Args) -> Result<bool, String> {
    let [real_dir, sim_dir] = a.rest.as_slice() else {
        return Err("compare <real dir> <sim dir>".to_owned());
    };
    let rows = compare(
        &read_dir(Path::new(real_dir))?,
        &read_dir(Path::new(sim_dir))?,
    );
    let (text, pass) = render(&rows, a.tolerance);
    print!("{text}");
    if let Some(p) = &a.report {
        std::fs::write(p, &text).map_err(|e| e.to_string())?;
    }
    Ok(pass)
}

fn main() -> ExitCode {
    let a = match args() {
        Ok(a) => a,
        Err(e) => {
            eprintln!("moments: {e}");
            return ExitCode::FAILURE;
        }
    };
    let r = match a.cmd.as_str() {
        "real" => real(&a).map(|()| true),
        "sim" => sim(&a).map(|()| true),
        "compare" => compare_cmd(&a),
        other => Err(format!("no command {other}: real, sim, compare")),
    };
    match r {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::FAILURE,
        Err(e) => {
            eprintln!("moments: {e}");
            ExitCode::FAILURE
        }
    }
}
