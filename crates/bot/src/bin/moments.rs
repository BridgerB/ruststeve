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
use results::parity::{
    Health, Landings, MobOutcome, Moment, MomentResult, Moments, Outcome, compare, render,
};
use world::World;

/// Server ticks a start settles before the script, the same in real and sim (bot::run's).
const SETTLE: u64 = bot::SETTLE_TICKS;
/// The table's stated tolerance: all but 2 of the moments match in every field (48 of issue
/// 08's 50; 78 of 80 with issue 10b's dig and craft rows; 103 of 105 with issue 11a's stone and
/// table rows; 117 of 119 with issue 12a's survival rows; 126 of 128 with 12b's escape rows; 135 of 137 with 13a's smelting rows; 150 of 152 with 14a's bucket rows; 156 of 158 with the sneak rows of client parity; 158 of 160 with its soul sand rows; 162 of 164 with issue 17's enderman rows; 168 of 170 with issue 18's burrow rows).
const TOLERANCE: usize = 168;
/// A placed mob that ends farther than this from where it was put moved (it ticked).
const MOB_MOVED: f64 = 0.5;
/// An enderman's creepy flag (`EnderMan.DATA_CREEPY`, entity data 17).
const CREEPY: u8 = 17;
/// Its stared-at flag (`EnderMan.DATA_STARED_AT`, entity data 18).
const STARED: u8 = 18;
/// A living entity's health (`LivingEntity.DATA_HEALTH_ID`, entity data 9).
const HEALTH: u8 = 9;
/// Server ticks between a trace's lines (issue 18: where a placed mob went, over time).
const TRACE_EVERY: u64 = 10;

/// Whether an entity's boolean data `i` is true.
fn flag(e: &world::Entity, i: u8) -> bool {
    matches!(
        e.metadata.get(&i),
        Some(protocol::metadata::MetaValue::Bool(true))
    )
}

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
    watch: &[(i32, i32, i32)],
    mobs: &[(String, [f64; 3])],
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
        food: w.player.food,
        kills: w.kills,
        blocks: watch
            .iter()
            .map(|&(x, y, z)| {
                w.block_at(world::BlockPos::new(x, y, z))
                    .and_then(data::BlockKind::name)
                    .unwrap_or("unknown")
                    .to_owned()
            })
            .collect(),
        mobs: mobs
            .iter()
            .map(|(kind, at)| mob_outcome(w, kind, *at))
            .collect(),
    })
}

/// A placed mob's outcome: the entity of its kind nearest where it was put.
fn mob_outcome(w: &World, kind: &str, at: [f64; 3]) -> MobOutcome {
    let d = |p: [f64; 3]| (0..3).map(|i| (p[i] - at[i]).powi(2)).sum::<f64>().sqrt();
    let found = w
        .entities
        .values()
        .filter(|e| e.kind.name() == Some(kind) && !e.dead)
        .min_by(|a, b| d(a.pos).total_cmp(&d(b.pos)));
    MobOutcome {
        kind: kind.to_owned(),
        seen: found.is_some(),
        moved: found.is_some_and(|e| d(e.pos) > MOB_MOVED),
        creepy: found.is_some_and(|e| flag(e, CREEPY)),
        stared: found.is_some_and(|e| flag(e, STARED)),
        cell: found.map(|e| e.pos.map(|v| v.floor() as i32)),
    }
}

/// One line of a trace: the bot's feet and health, and every enderman and item within 32 (its
/// id, feet, health, creepy and stared flags). Issue 18: a row that fails on the real server
/// says what the enderman did, over time.
fn trace_line(w: &World) -> Option<String> {
    let body = w.body?;
    let near = |p: [f64; 3]| (0..3).map(|i| (p[i] - body.pos[i]).powi(2)).sum::<f64>() <= 1024.0;
    let f = |p: [f64; 3]| format!("({:.2} {:.2} {:.2})", p[0], p[1], p[2]);
    let others: String = w
        .entities
        .iter()
        .filter(|(_, e)| {
            near(e.pos) && matches!(e.kind, data::EntityKind::Enderman | data::EntityKind::Item)
        })
        .map(|(id, e)| {
            let hp = match e.metadata.get(&HEALTH) {
                Some(protocol::metadata::MetaValue::Float(h)) => format!(" hp {h}"),
                _ => String::new(),
            };
            format!(
                " | {:?} {id} {}{hp}{}{}{}",
                e.kind,
                f(e.pos),
                if flag(e, CREEPY) { " creepy" } else { "" },
                if flag(e, STARED) { " stared" } else { "" },
                if e.dead { " dead" } else { "" }
            )
        })
        .collect();
    Some(format!(
        "t {} bot {} hp {:?} look {:.1} {:.1}{others}",
        w.ticks,
        f(body.pos),
        w.player.health,
        w.controls.yaw,
        w.controls.pitch.unwrap_or(w.player.pitch)
    ))
}

/// A real run's trace, folded from its recording: a line every `TRACE_EVERY` ticks.
fn trace_recording(path: &Path) -> Result<Vec<String>, String> {
    let (_, records) = record::Recording::read_all(path)?;
    let mut folder = record::Folder::new();
    let mut lines = Vec::new();
    let mut last = None;
    for rec in &records {
        match rec {
            record::Record::Event(e) => {
                folder.step(e)?;
            }
            record::Record::Checkpoint {
                event_index, world, ..
            } => {
                folder.take_start(*event_index, world);
            }
            _ => continue,
        }
        let t = folder.world.ticks;
        if t.is_multiple_of(TRACE_EVERY) && last != Some(t) {
            last = Some(t);
            lines.extend(trace_line(&folder.world));
        }
    }
    Ok(lines)
}

/// Where a moment's placed mobs go, absolute: the cell's middle on x and z.
fn mob_spots(start: [f64; 3], m: &Moment) -> Vec<(String, [f64; 3])> {
    let (x, y, z) = feet_cell(start);
    m.mobs
        .iter()
        .map(|(e, dx, dy, dz)| {
            (
                e.clone(),
                [
                    f64::from(x + dx) + 0.5,
                    f64::from(y + dy),
                    f64::from(z + dz) + 0.5,
                ],
            )
        })
        .collect()
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

/// A moment's watched cells, absolute.
fn watched(start: [f64; 3], m: &Moment) -> Vec<(i32, i32, i32)> {
    let (x, y, z) = feet_cell(start);
    m.watch
        .iter()
        .map(|(dx, dy, dz)| (x + dx, y + dy, z + dz))
        .collect()
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

/// A row's script, or the planner's aim for a `plan:<aim>` row (issue 12b: the escape rows run
/// the planner with `plan:hold`, the always-rows only).
fn script_or_aim(m: &Moment) -> Result<(Option<Script>, Option<plan::Aim>), String> {
    if let Some(aim) = m.script.strip_prefix("plan:") {
        let a = plan::Aim::parse(aim).ok_or_else(|| format!("{}: no aim {aim}", m.id))?;
        return Ok((None, Some(a)));
    }
    Script::parse(&m.script)
        .map(|s| (Some(s), None))
        .ok_or_else(|| format!("{}: no script {}", m.id, m.script))
}

/// A row's trace beside its result, `<id>.<tag>.trace.txt`.
fn write_trace(out: &Path, id: &str, tag: &str, lines: &[String]) -> Result<(), String> {
    std::fs::write(
        out.join(format!("{id}.{tag}.trace.txt")),
        lines.join("\n") + "\n",
    )
    .map_err(|e| e.to_string())
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
    // The peaceful rows first, then those with a difficulty, the server's set for them by RCON
    // and put back after (issue 17: endermen are removed on peaceful).
    let (plain, hard): (Vec<Moment>, Vec<Moment>) = moments
        .moment
        .iter()
        .cloned()
        .partition(|m| m.difficulty.is_none());
    let rcon_addr = format!("{}:{}", a.host, a.rcon_port);
    let groups = [(None, plain), (Some("normal"), hard)];
    let mut w = 0;
    for (difficulty, rows) in &groups {
        if rows.is_empty() {
            continue;
        }
        if let Some(d) = difficulty {
            let mut r = rcon::Rcon::connect(&rcon_addr, &password)?;
            println!(
                "moments real: difficulty {d}: {}",
                r.command(&format!("difficulty {d}"))?
            );
        }
        real_waves(a, &landings, rows, &password, nonce, &mut w)?;
        if difficulty.is_some() {
            let mut r = rcon::Rcon::connect(&rcon_addr, &password)?;
            r.command("difficulty peaceful")?;
        }
    }
    Ok(())
}

/// The real side's waves of `rows`, numbered on from `w`.
fn real_waves(
    a: &Args,
    landings: &Landings,
    rows: &[Moment],
    password: &str,
    nonce: u64,
    w: &mut usize,
) -> Result<(), String> {
    for wave in rows.chunks(a.wave.max(1)) {
        let w = {
            let n = *w;
            *w += 1;
            n
        };
        println!("moments real: wave {w}, {} moments", wave.len());
        let mut handles = Vec::new();
        for (i, m) in wave.iter().enumerate() {
            let (_, start) = start_of(a, landings, m)?;
            let (script, aim) = script_or_aim(m)?;
            // A fresh player per run: the server saves a player's place, health and effects by
            // name, so a reused name would start where the last run left it.
            let name = format!("rs-m{:02}-{:04x}", w * a.wave + i, nonce);
            let rcon_addr = format!("{}:{}", a.host, a.rcon_port);
            let password = password.to_owned();
            let tp = format!("tp {name} {} {} {}", start[0], start[1], start[2]);
            // The model has no air supply yet (issue 12, survival): a real bot waiting under
            // water for its chunks would drown on a clock the model does not keep, so the real
            // side breathes. Stated in the report; drowning joins the table with issue 12.
            let breathe = format!("effect give {name} minecraft:water_breathing 600 0 true");
            let breathe_on = m.breathe;
            let gives: Vec<String> = m
                .give
                .iter()
                .map(|g| format!("give {name} minecraft:{g}"))
                .chain(m.fill.iter().map(|(b, x1, y1, z1, x2, y2, z2)| {
                    let (x, y, z) = feet_cell(start);
                    format!(
                        "fill {} {} {} {} {} {} minecraft:{b}",
                        x + x1,
                        y + y1,
                        z + z1,
                        x + x2,
                        y + y2,
                        z + z2
                    )
                }))
                .chain(m.set.iter().map(|(b, dx, dy, dz)| {
                    let (x, y, z) = feet_cell(start);
                    format!("setblock {} {} {} minecraft:{b}", x + dx, y + dy, z + dz)
                }))
                .chain(m.effects.iter().map(|(e, secs, amp)| {
                    format!("effect give {name} minecraft:{e} {secs} {amp} true")
                }))
                .chain(m.summon.iter().map(|(e, dx, dy, dz)| {
                    let (x, y, z) = feet_cell(start);
                    format!(
                        "summon minecraft:{e} {} {} {} {{NoAI:1b,DeathLootTable:\"minecraft:empty\",PersistenceRequired:1b,Silent:1b}}",
                        f64::from(x + dx) + 0.5,
                        y + dy,
                        f64::from(z + dz) + 0.5
                    )
                }))
                .chain(mob_spots(start, m).into_iter().map(|(e, p)| {
                    // With its AI: kept, no loot unless the row keeps it (issue 17; 18's pearls).
                    let loot = if m.loot {
                        ""
                    } else {
                        "DeathLootTable:\"minecraft:empty\","
                    };
                    format!(
                        "summon minecraft:{e} {} {} {} {{{loot}PersistenceRequired:1b}}",
                        p[0], p[1], p[2]
                    )
                }))
                .chain(m.items.iter().map(|(item, dx, dy, dz)| {
                    let (x, y, z) = feet_cell(start);
                    format!(
                        "summon minecraft:item {} {} {} {{Item:{{id:\"minecraft:{item}\",count:1}},PickupDelay:0s}}",
                        f64::from(x + dx) + 0.5,
                        y + dy,
                        f64::from(z + dz) + 0.5
                    )
                }))
                .collect();
            let opts = bot::Opts {
                host: a.host.clone(),
                port: a.port,
                name: name.clone(),
                record: Some(a.out.join(format!("{}.real.rsr", m.id))),
                script,
                script_ticks: Some(m.ticks),
                aim,
                cap_ticks: None,
                deadline: Some(Instant::now() + Duration::from_secs(300)),
                start: Some(Box::new(move || {
                    let mut r = rcon::Rcon::connect(&rcon_addr, &password)?;
                    if breathe_on {
                        r.command(&breathe)?;
                    }
                    r.command(&tp)?;
                    // A setblock right after the teleport can land before the chunk loads
                    // ("That position is not loaded") and do nothing: try again until it does.
                    gives.iter().try_for_each(|g| {
                        for _ in 0..80 {
                            let reply = r.command(g)?;
                            // A summon into a chunk not loaded yet says "Invalid position". A fill is
                            // done only when the server says it filled ("Successfully filled", "No
                            // blocks were filled"): issue 18's e05 once ran with no arena built.
                            let filled = !g.starts_with("fill ") || reply.contains("filled");
                            if filled
                                && !reply.contains("not loaded")
                                && !reply.contains("Invalid position")
                            {
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
                hold_after_aim: false,
                revive: false,
                memory: None,
                on_stage: None,
                no_progress_ticks: None,
                snapshot_at: None,
                progress: None,
            };
            let id = m.id.clone();
            let watch = watched(start, m);
            let mobs = mob_spots(start, m);
            let traced = !m.mobs.is_empty() || !m.items.is_empty();
            handles.push((
                id,
                watch,
                mobs,
                traced,
                std::thread::spawn(move || bot::run(opts)),
            ));
            std::thread::sleep(Duration::from_millis(500));
        }
        for (id, watch, mobs, traced, h) in handles {
            let trace_id = id.clone();
            let r = match h.join() {
                Ok(Ok(o)) => MomentResult {
                    moment: id,
                    source: Source::Real,
                    commit: record::COMMIT.to_owned(),
                    outcome: outcome(
                        &o.world,
                        o.teleports_at_script,
                        o.min_health_in_script,
                        &watch,
                        &mobs,
                    ),
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
            if traced {
                let rsr = a.out.join(format!("{trace_id}.real.rsr"));
                let text = trace_recording(&rsr).unwrap_or_else(|e| vec![format!("no trace: {e}")]);
                write_trace(&a.out, &trace_id, "real", &text)?;
            }
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
    let table = std::sync::Arc::new(bot::goals_table("config/goals.toml")?);
    for (i, m) in moments.moment.iter().enumerate() {
        let (captured, start) = start_of(a, &landings, m)?;
        let (script, aim) = script_or_aim(m)?;
        let mut world = captured.model(start)?;
        let state_of = |b: &str| {
            data::BlockKind::from_name(b)
                .and_then(|k| k.info())
                .map(|i| i.default_state)
                .ok_or_else(|| format!("{}: no block {b}", m.id))
        };
        for (b, x1, y1, z1, x2, y2, z2) in &m.fill {
            let (x, y, z) = feet_cell(start);
            let state = state_of(b)?;
            for (cx, cy, cz) in (*x1.min(x2)..=*x1.max(x2)).flat_map(|cx| {
                (*y1.min(y2)..=*y1.max(y2))
                    .flat_map(move |cy| (*z1.min(z2)..=*z1.max(z2)).map(move |cz| (cx, cy, cz)))
            }) {
                world
                    .set_block(x + cx, y + cy, z + cz, state)
                    .map_err(|e| e.to_string())?;
            }
        }
        for (b, dx, dy, dz) in &m.set {
            let (x, y, z) = feet_cell(start);
            let state = state_of(b)?;
            world
                .set_block(x + dx, y + dy, z + dz, state)
                .map_err(|e| e.to_string())?;
        }
        // A row with a difficulty runs off peaceful (issue 17).
        let config = sim::Config {
            peaceful: m.difficulty.is_none(),
            ..config
        };
        let mut model = sim::Sim::new(config, world).seeded(i as u64 + 1);
        for g in &m.give {
            model
                .survival
                .give(give_of(g).ok_or_else(|| format!("{}: give {g}", m.id))?);
        }
        model.survival.life.water_breathing = m.breathe;
        for (e, secs, amp) in &m.effects {
            match e.as_str() {
                "hunger" => model.survival.life.hunger = Some((secs * 20, *amp)),
                "fire_resistance" => model.survival.life.fire_resistance = true,
                other => return Err(format!("{}: the model has no effect {other}", m.id)),
            }
        }
        for (e, dx, dy, dz) in &m.summon {
            let (x, y, z) = feet_cell(start);
            let kind = data::generated::entities::ENTITY_NAMES
                .iter()
                .position(|n| n == e)
                .and_then(|i| u32::try_from(i).ok())
                .map(data::EntityKind::from_id)
                .ok_or_else(|| format!("{}: no entity {e}", m.id))?;
            let id = model.summon(
                kind,
                [
                    f64::from(x + dx) + 0.5,
                    f64::from(y + dy),
                    f64::from(z + dz) + 0.5,
                ],
            );
            // Summoned with no loot, as the real side's `DeathLootTable:"minecraft:empty"`.
            if let Some(m) = model.survival.life.mobs.get_mut(&id) {
                m.loot = false;
            }
        }
        for (e, p) in mob_spots(start, m) {
            match e.as_str() {
                "enderman" => {
                    let id = model.summon_enderman(p);
                    if let Some(mob) = model.survival.life.mobs.get_mut(&id) {
                        mob.loot = m.loot;
                    }
                }
                other => return Err(format!("{}: the model places no {other} with its AI", m.id)),
            }
        }
        for (item, dx, dy, dz) in &m.items {
            let (x, y, z) = feet_cell(start);
            let kind = data::ItemKind::from_name(item)
                .ok_or_else(|| format!("{}: no item {item}", m.id))?;
            model.place_item(
                (kind, 1),
                [
                    f64::from(x + dx) + 0.5,
                    f64::from(y + dy),
                    f64::from(z + dz) + 0.5,
                ],
            );
        }
        let traced = !m.mobs.is_empty() || !m.items.is_empty();
        let mut trace = Vec::new();
        let mut planner = aim.map(|a| plan::Plan::with_table(a, table.clone()));
        let run = bot::lockstep::run_acting(
            model,
            &format!("rs-m{i:02}"),
            i as u64 + 1,
            SETTLE + m.ticks,
            SETTLE,
            |w, t| {
                if traced && w.ticks.is_multiple_of(TRACE_EVERY) {
                    trace.extend(trace_line(w));
                }
                match (&mut planner, script) {
                    (Some(p), _) => {
                        let c = p.step(w);
                        (c, p.take_acts())
                    }
                    (None, Some(s)) => (s.steer(t, w), s.acts(t, w)),
                    (None, None) => (physics::Controls::default(), Vec::new()),
                }
            },
        );
        if traced {
            write_trace(&a.out, &m.id, "sim", &trace)?;
        }
        let r = MomentResult {
            moment: m.id.clone(),
            source: Source::Sim,
            commit: record::COMMIT.to_owned(),
            outcome: outcome(
                &run.world,
                run.teleports_at_script,
                run.min_health_in_script,
                &watched(start, m),
                &mob_spots(start, m),
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
