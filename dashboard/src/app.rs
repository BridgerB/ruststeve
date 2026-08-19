use leptos::prelude::*;
use leptos_meta::{provide_meta_context, MetaTags, Stylesheet, Title};
use leptos_router::{
    components::{Route, Router, Routes, A},
    hooks::use_params_map,
    ParamSegment, StaticSegment,
};
use serde::{Deserialize, Serialize};

// The speedrun roadmap — used to name the current phase from the step count.
pub const PHASES: [&str; 19] = [
    "Gather Wood", "Planks", "Crafting Table", "Sticks", "Wood Pickaxe", "Mine Stone",
    "Stone Pickaxe", "Furnace", "Coal", "Iron Ore", "Smelt Iron", "Iron Pickaxe", "Buckets",
    "Water", "Food", "Flint & Steel", "Build Blocks", "Build Portal", "Enter Nether",
];

fn phase_tag(done: i32) -> (&'static str, &'static str) {
    match done {
        d if d >= 18 => ("NETHER", "var(--lava)"),
        d if d >= 17 => ("PORTAL", "var(--violet)"),
        d if d >= 12 => ("WATER", "var(--water)"),
        d if d >= 9 => ("IRON", "var(--sky)"),
        d if d >= 5 => ("STONE", "var(--stone)"),
        d if d >= 0 => ("WOOD", "var(--leaf)"),
        _ => ("START", "var(--muted)"),
    }
}

/// Viewer stream port from the trailing digits of the name — mirrors `viewer::port_for`
/// on the bot side (rust-race-003 → 4603, rust-gym-001 → 4601).
fn port_for(name: &str) -> u16 {
    let digits: String = name.chars().filter(|c| c.is_ascii_digit()).collect();
    4600 + digits.parse::<u16>().unwrap_or(0) % 100
}

#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq)]
pub struct Bot {
    pub name: String,
    pub port: u16,
    pub kind: String, // "race" | "gym" | "bot"
    pub step: String,
    pub done: i32,
    pub total: i32,
    pub health: f64,
    pub food: f64,
    pub x: i32,
    pub y: i32,
    pub z: i32,
    pub dim: String,
    pub alive: bool,
    pub portal_built: bool,
    pub pick: i32,
    pub iron: i32,
    pub diamonds: i32,
    pub water: i32,
    pub cobble: i32,
    pub logs: i32,
    pub age_s: i64, // seconds since the last telemetry tick (staleness)
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct Fleet {
    pub bots: Vec<Bot>,
    pub server_up: bool,
}

const DIR: &str = "/Users/bridger/Developer/mc/upstream/ruststeve";

fn now_s() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Compact "5m" / "2h" / "3d" from a seconds delta — used in tables/cards.
fn fmt_ago(secs: i64) -> String {
    let s = secs.max(0);
    if s < 90 {
        format!("{s}s")
    } else if s < 5400 {
        format!("{}m", s / 60)
    } else if s < 172800 {
        format!("{}h", s / 3600)
    } else {
        format!("{}d", s / 86400)
    }
}

/// Which bots are RUNNING right now, from `ps ewwx` env (MC_USERNAME + RACE_GOAL/GYM).
/// Shared by every "live" server fn (fleet/flow/map). Returns (name, kind).
#[cfg(feature = "ssr")]
fn running_bots() -> Vec<(String, String)> {
    let ps = std::process::Command::new("ps")
        .args(["ewwx"])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .unwrap_or_default();
    let mut running: Vec<(String, String)> = Vec::new();
    for line in ps.lines() {
        if !line.contains("target/release/ruststeve") || line.contains("SERVE=") {
            continue;
        }
        let name = line
            .split_whitespace()
            .find_map(|t| t.strip_prefix("MC_USERNAME="))
            .unwrap_or("")
            .to_string();
        if name.is_empty() {
            continue;
        }
        let kind = if line.contains("RACE_GOAL=") {
            "race"
        } else if line.contains("GYM=") {
            "gym"
        } else {
            "bot"
        };
        if !running.iter().any(|(n, _)| n == &name) {
            running.push((name, kind.to_string()));
        }
    }
    running
}

// ── server: which bots are RUNNING right now + their live state ───────────────
#[server(GetFleet, "/api")]
pub async fn get_fleet() -> Result<Fleet, ServerFnError> {
    // 1. Running bots: `ps ewwx` exposes each process's ENV (MC_USERNAME / RACE_GOAL / GYM),
    //    so we know a bot is alive AND its name (→ viewer port) — no stale finished cards.
    let running = running_bots();

    // 2. Enrich each with the latest telemetry tick (race bots write data/race.db).
    let now = now_s();
    let race = rusqlite::Connection::open(format!("{DIR}/data/race.db")).ok();
    let mut bots = Vec::new();
    for (name, kind) in running {
        let mut b = Bot {
            name: name.clone(),
            port: port_for(&name),
            kind,
            total: 19,
            alive: true,
            health: 20.0,
            food: 20.0,
            ..Default::default()
        };
        if let Some(conn) = &race {
            let _ = conn.query_row(
                "SELECT step_name, done, total, health, food, x, y, z, dim, alive, portal_built,
                        pick, iron_ingots, diamonds, water_buckets, cobblestone, logs, ts_ms
                 FROM ticks WHERE bot = ?1 ORDER BY ts_ms DESC LIMIT 1",
                [&name],
                |r| {
                    b.step = r.get::<_, Option<String>>(0)?.unwrap_or_default();
                    b.done = r.get(1)?;
                    b.total = r.get::<_, i32>(2)?.max(1);
                    b.health = r.get(3)?;
                    b.food = r.get(4)?;
                    b.x = r.get(5)?;
                    b.y = r.get(6)?;
                    b.z = r.get(7)?;
                    b.dim = r.get::<_, Option<String>>(8)?.unwrap_or_default();
                    b.alive = r.get::<_, i32>(9)? != 0;
                    b.portal_built = r.get::<_, i32>(10)? != 0;
                    b.pick = r.get(11)?;
                    b.iron = r.get(12)?;
                    b.diamonds = r.get(13)?;
                    b.water = r.get(14)?;
                    b.cobble = r.get(15)?;
                    b.logs = r.get(16)?;
                    let ts: i64 = r.get(17)?;
                    b.age_s = (now - ts / 1000).max(0);
                    Ok(())
                },
            );
        }
        bots.push(b);
    }
    // Furthest-along first; gym bots (no race telemetry) sink below by done=0.
    bots.sort_by(|a, b| b.done.cmp(&a.done).then(a.name.cmp(&b.name)));

    // "reachable" = the bots can actually connect: probe the MC port (the SSH tunnel to
    // the shared box listens on localhost:25565) rather than looking for a LOCAL server.jar.
    let server_up = std::net::TcpStream::connect_timeout(
        &"127.0.0.1:25565".parse().unwrap(),
        std::time::Duration::from_millis(400),
    )
    .is_ok();

    Ok(Fleet { bots, server_up })
}

// ── gym: per-step pass-rate across random terrain (data/gym.db) ──────────────
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct GymRow {
    pub slug: String,
    pub runs: i64,
    pub passes: i64,
    pub pass_pct: i64,
    pub avg_s: i64,
    pub last_fail_xyz: String,
}

#[server(GetGym, "/api")]
pub async fn get_gym() -> Result<Vec<GymRow>, ServerFnError> {
    let conn = rusqlite::Connection::open(format!("{DIR}/data/gym.db"))
        .map_err(|e| ServerFnError::new(e.to_string()))?;
    let mut stmt = conn
        .prepare(
            "SELECT slug, COUNT(*), SUM(pass),
                    CAST(ROUND(100.0*SUM(pass)/COUNT(*)) AS INT),
                    CAST(ROUND(AVG(duration_ms)/1000.0) AS INT)
             FROM gym_runs GROUP BY slug ORDER BY 4 ASC, slug ASC",
        )
        .map_err(|e| ServerFnError::new(e.to_string()))?;
    let base: Vec<GymRow> = stmt
        .query_map([], |r| {
            Ok(GymRow {
                slug: r.get(0)?,
                runs: r.get(1)?,
                passes: r.get(2)?,
                pass_pct: r.get(3)?,
                avg_s: r.get(4)?,
                last_fail_xyz: String::new(),
            })
        })
        .map_err(|e| ServerFnError::new(e.to_string()))?
        .filter_map(Result::ok)
        .collect();
    let mut out = Vec::new();
    for mut g in base {
        if let Ok((x, y, z)) = conn.query_row(
            "SELECT x,y,z FROM gym_runs WHERE slug=?1 AND pass=0 ORDER BY ts DESC LIMIT 1",
            [&g.slug],
            |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?, r.get::<_, i64>(2)?)),
        ) {
            g.last_fail_xyz = format!("{x},{y},{z}");
        }
        out.push(g);
    }
    Ok(out)
}

// ── flow: a running bot's full step-ladder progress (data/race.db latest tick) ─
#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq)]
pub struct FlowBot {
    pub name: String,
    pub done: i32,
    pub step_id: String,
    pub alive: bool,
    pub age_s: i64,
    pub logs: i32,
    pub planks: i32,
    pub sticks: i32,
    pub cobble: i32,
    pub dirt: i32,
    pub coal: i32,
    pub iron_ore: i32,
    pub iron: i32,
    pub water: i32,
    pub buckets: i32,
    pub fas: i32,
    pub pick: i32,
    pub sword: i32,
    pub has_table: bool,
    pub has_furnace: bool,
    pub portal_built: bool,
    pub in_nether: bool,
}

#[server(GetFlow, "/api")]
pub async fn get_flow() -> Result<Vec<FlowBot>, ServerFnError> {
    let now = now_s();
    let race = rusqlite::Connection::open(format!("{DIR}/data/race.db")).ok();
    let mut out = Vec::new();
    for (name, kind) in running_bots() {
        if kind == "gym" {
            continue; // gym bots don't write per-step race telemetry
        }
        let mut b = FlowBot { name: name.clone(), alive: true, ..Default::default() };
        if let Some(conn) = &race {
            let _ = conn.query_row(
                "SELECT step_id, done, alive, logs, planks, sticks, cobblestone, dirt, coal,
                        iron_ore, iron_ingots, water_buckets, buckets, flint_and_steel, pick, sword,
                        has_table, has_furnace, portal_built, dim, ts_ms
                 FROM ticks WHERE bot=?1 ORDER BY ts_ms DESC LIMIT 1",
                [&name],
                |r| {
                    b.step_id = r.get::<_, Option<String>>(0)?.unwrap_or_default();
                    b.done = r.get(1)?;
                    b.alive = r.get::<_, i32>(2)? != 0;
                    b.logs = r.get(3)?;
                    b.planks = r.get(4)?;
                    b.sticks = r.get(5)?;
                    b.cobble = r.get(6)?;
                    b.dirt = r.get(7)?;
                    b.coal = r.get(8)?;
                    b.iron_ore = r.get(9)?;
                    b.iron = r.get(10)?;
                    b.water = r.get(11)?;
                    b.buckets = r.get(12)?;
                    b.fas = r.get(13)?;
                    b.pick = r.get(14)?;
                    b.sword = r.get(15)?;
                    b.has_table = r.get::<_, i32>(16)? != 0;
                    b.has_furnace = r.get::<_, i32>(17)? != 0;
                    b.portal_built = r.get::<_, i32>(18)? != 0;
                    b.in_nether = r.get::<_, Option<String>>(19)?.unwrap_or_default().contains("nether");
                    let ts: i64 = r.get(20)?;
                    b.age_s = (now - ts / 1000).max(0);
                    Ok(())
                },
            );
        }
        out.push(b);
    }
    out.sort_by(|a, b| b.done.cmp(&a.done).then(a.name.cmp(&b.name)));
    Ok(out)
}

// ── timeline: a bot's recent milestones / deaths / bails (data/race.db events) ─
#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq)]
pub struct TimelineEntry {
    pub kind: String, // milestone | death | stuck_bail
    pub label: String,
    pub detail: String,
    pub ago: String,
}

#[server(GetTimeline, "/api")]
pub async fn get_timeline(bot: String) -> Result<Vec<TimelineEntry>, ServerFnError> {
    let now = now_s();
    let conn = rusqlite::Connection::open(format!("{DIR}/data/race.db"))
        .map_err(|e| ServerFnError::new(e.to_string()))?;
    let mut out = Vec::new();
    if let Ok(mut s) = conn.prepare(
        "SELECT category, event, COALESCE(detail,''), ts_ms FROM events
         WHERE bot=?1 AND category IN ('milestone','death','stuck_bail')
         ORDER BY ts_ms DESC LIMIT 40",
    ) {
        if let Ok(rows) = s.query_map([&bot], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?, r.get::<_, i64>(3)?))
        }) {
            for (kind, label, detail, ts) in rows.flatten() {
                out.push(TimelineEntry { kind, label, detail, ago: fmt_ago(now - ts / 1000) });
            }
        }
    }
    Ok(out)
}

// ── map: top-down bot positions + aggregated POIs (memory dbs) ────────────────
#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq)]
pub struct MapBot {
    pub name: String,
    pub kind: String,
    pub x: i32,
    pub z: i32,
    pub done: i32,
    pub dim: String,
}
#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq)]
pub struct MapPoint {
    pub kind: String,
    pub x: i32,
    pub z: i32,
    pub n: i32,
}
#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq)]
pub struct MapData {
    pub bots: Vec<MapBot>,
    pub pois: Vec<MapPoint>,
}

#[server(GetMap, "/api")]
pub async fn get_map() -> Result<MapData, ServerFnError> {
    // Live bot positions from the latest overworld race tick.
    let race = rusqlite::Connection::open(format!("{DIR}/data/race.db")).ok();
    let mut bots = Vec::new();
    for (name, kind) in running_bots() {
        let mut mb = MapBot { name: name.clone(), kind, ..Default::default() };
        if let Some(conn) = &race {
            let _ = conn.query_row(
                "SELECT x, z, done, dim FROM ticks WHERE bot=?1 AND (x!=0 OR z!=0)
                 ORDER BY ts_ms DESC LIMIT 1",
                [&name],
                |r| {
                    mb.x = r.get(0)?;
                    mb.z = r.get(1)?;
                    mb.done = r.get(2)?;
                    mb.dim = r.get::<_, Option<String>>(3)?.unwrap_or_default();
                    Ok(())
                },
            );
        }
        bots.push(mb);
    }

    // POIs aggregated across every .memory-*.db, grid-bucketed (24-block cells) so the
    // scatter is legible; keep only the interesting kinds and cap to the densest cells.
    const KINDS: [&str; 8] =
        ["lava", "water", "diamond_ore", "iron_ore", "coal_ore", "gold_ore", "log", "crafting_table"];
    let mut agg: std::collections::HashMap<(String, i32, i32), i32> = std::collections::HashMap::new();
    if let Ok(rd) = std::fs::read_dir(DIR) {
        for e in rd.flatten() {
            let p = e.path();
            let fname = p.file_name().and_then(|s| s.to_str()).unwrap_or("");
            if !(fname.starts_with(".memory-") && fname.ends_with(".db")) {
                continue;
            }
            let Ok(conn) = rusqlite::Connection::open(&p) else { continue };
            let Ok(mut stmt) =
                conn.prepare("SELECT kind, x/24*24, z/24*24, COUNT(*) FROM pois GROUP BY kind, x/24, z/24")
            else {
                continue;
            };
            let Ok(rows) = stmt.query_map([], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, i32>(1)?, r.get::<_, i32>(2)?, r.get::<_, i32>(3)?))
            }) else {
                continue;
            };
            for (k, x, z, n) in rows.flatten() {
                if KINDS.contains(&k.as_str()) {
                    *agg.entry((k, x, z)).or_insert(0) += n;
                }
            }
        }
    }
    let mut pois: Vec<MapPoint> =
        agg.into_iter().map(|((kind, x, z), n)| MapPoint { kind, x, z, n }).collect();
    pois.sort_by(|a, b| b.n.cmp(&a.n));
    pois.truncate(2500);
    Ok(MapData { bots, pois })
}

// ── reports: where races die / stall, milestone funnel, cast breakdown ────────
#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq)]
pub struct Bar {
    pub label: String,
    pub value: i64,
}
#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct Reports {
    pub milestones: Vec<Bar>,
    pub stalls: Vec<Bar>,
    pub cast: Vec<Bar>,
    pub deaths: i64,
    pub bails: i64,
    pub gym: Vec<GymRow>,
}

#[server(GetReports, "/api")]
pub async fn get_reports() -> Result<Reports, ServerFnError> {
    // Await the gym query FIRST so no (non-Send) rusqlite handle is held across it.
    let gym = get_gym().await.unwrap_or_default();
    let conn = rusqlite::Connection::open(format!("{DIR}/data/race.db"))
        .map_err(|e| ServerFnError::new(e.to_string()))?;
    let bars = |sql: &str| -> Vec<Bar> {
        conn.prepare(sql)
            .and_then(|mut s| {
                let rows = s
                    .query_map([], |r| Ok(Bar { label: r.get(0)?, value: r.get(1)? }))?
                    .filter_map(Result::ok)
                    .collect::<Vec<_>>();
                Ok(rows)
            })
            .unwrap_or_default()
    };
    // Milestone funnel — ordered along the speedrun ladder, not by count.
    const MS_ORDER: [&str; 14] = [
        "Gather Wood", "Craft Planks", "Craft Sticks", "Craft Wooden Pickaxe", "Mine Cobblestone",
        "Craft Stone Pickaxe", "Craft Furnace", "Mine Coal", "Mine Iron Ore", "Smelt Iron",
        "Craft Buckets", "Fill Water Buckets", "Get Flint and Steel", "Build Nether Portal",
    ];
    let mut milestones = bars("SELECT event, COUNT(*) FROM events WHERE category='milestone' GROUP BY event");
    milestones.sort_by_key(|b| MS_ORDER.iter().position(|m| *m == b.label).unwrap_or(99));
    let stalls =
        bars("SELECT event, SUM(count) FROM counters WHERE category='step_fail' GROUP BY event ORDER BY 2 DESC LIMIT 12");
    let cast =
        bars("SELECT event, SUM(count) FROM counters WHERE category='cast' GROUP BY event ORDER BY 2 DESC LIMIT 12");
    let deaths = conn
        .query_row("SELECT COUNT(*) FROM events WHERE category='death'", [], |r| r.get(0))
        .unwrap_or(0);
    let bails = conn
        .query_row("SELECT COUNT(*) FROM events WHERE category='stuck_bail'", [], |r| r.get(0))
        .unwrap_or(0);
    Ok(Reports { milestones, stalls, cast, deaths, bails, gym })
}

// ── gym slug detail: recent runs + pass history (data/gym.db) ────────────────
#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq)]
pub struct GymRun {
    pub pass: bool,
    pub secs: i64,
    pub x: i32,
    pub y: i32,
    pub z: i32,
    pub msg: String,
    pub ago: String,
}
#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct GymDetail {
    pub slug: String,
    pub runs: i64,
    pub passes: i64,
    pub pass_pct: i64,
    pub avg_s: i64,
    pub recent: Vec<GymRun>,
    pub history: Vec<i64>, // oldest→newest pass(1)/fail(0), for the strip
}

#[server(GetGymSlug, "/api")]
pub async fn get_gym_slug(slug: String) -> Result<GymDetail, ServerFnError> {
    let conn = rusqlite::Connection::open(format!("{DIR}/data/gym.db"))
        .map_err(|e| ServerFnError::new(e.to_string()))?;
    let (runs, passes, avg_s): (i64, i64, i64) = conn
        .query_row(
            "SELECT COUNT(*), COALESCE(SUM(pass),0),
                    CAST(ROUND(COALESCE(AVG(duration_ms),0)/1000.0) AS INT)
             FROM gym_runs WHERE slug=?1",
            [&slug],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .unwrap_or((0, 0, 0));
    let pass_pct = if runs > 0 { 100 * passes / runs } else { 0 };
    let now = now_s();
    let mut recent = Vec::new();
    if let Ok(mut s) = conn.prepare(
        "SELECT pass, duration_ms, x, y, z, COALESCE(message,''), ts
         FROM gym_runs WHERE slug=?1 ORDER BY ts DESC LIMIT 40",
    ) {
        if let Ok(rows) = s.query_map([&slug], |r| {
            Ok((
                r.get::<_, i32>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, i32>(2)?,
                r.get::<_, i32>(3)?,
                r.get::<_, i32>(4)?,
                r.get::<_, String>(5)?,
                r.get::<_, i64>(6)?,
            ))
        }) {
            for (pass, dur, x, y, z, msg, ts) in rows.flatten() {
                recent.push(GymRun {
                    pass: pass != 0,
                    secs: dur / 1000,
                    x,
                    y,
                    z,
                    msg,
                    ago: fmt_ago(now - ts / 1000),
                });
            }
        }
    }
    let history: Vec<i64> = recent.iter().rev().map(|r| i64::from(r.pass)).collect();
    Ok(GymDetail { slug, runs, passes, pass_pct, avg_s, recent, history })
}

// ── control: spawn / kill bots + RCON (SSR only) ─────────────────────────────
#[cfg(feature = "ssr")]
mod ctl {
    use super::DIR;
    use std::io::{Read, Write};
    use std::process::{Command, Stdio};
    use std::time::Duration;

    fn assets() -> String {
        format!("{DIR}/viewer/static/assets.json")
    }
    fn bin() -> String {
        format!("{DIR}/target/release/ruststeve")
    }

    /// Minimal RCON over the SSH tunnel (`127.0.0.1:25575`). Returns the reply body.
    pub fn rcon(cmd: &str) -> std::io::Result<String> {
        let mut s = std::net::TcpStream::connect("127.0.0.1:25575")?;
        s.set_read_timeout(Some(Duration::from_secs(3)))?;
        let frame = |id: i32, ty: i32, body: &str| -> Vec<u8> {
            let b = body.as_bytes();
            let len = (4 + 4 + b.len() + 2) as i32;
            let mut p = Vec::new();
            p.extend_from_slice(&len.to_le_bytes());
            p.extend_from_slice(&id.to_le_bytes());
            p.extend_from_slice(&ty.to_le_bytes());
            p.extend_from_slice(b);
            p.extend_from_slice(&[0, 0]);
            p
        };
        let read = |s: &mut std::net::TcpStream| -> std::io::Result<String> {
            let mut lb = [0u8; 4];
            s.read_exact(&mut lb)?;
            let len = i32::from_le_bytes(lb).max(10) as usize;
            let mut buf = vec![0u8; len];
            s.read_exact(&mut buf)?;
            Ok(String::from_utf8_lossy(&buf[8..len - 2]).to_string())
        };
        s.write_all(&frame(1, 3, "minecraft-test-rcon"))?; // auth
        let _ = read(&mut s);
        s.write_all(&frame(2, 2, cmd))?;
        read(&mut s)
    }

    /// Spawn a detached ruststeve bot with the viewer on and the given extra env.
    pub fn spawn_bot(name: &str, envs: &[(&str, String)]) {
        let mut c = Command::new(bin());
        c.current_dir(DIR)
            .env("MC_HOST", "localhost")
            .env("MC_USERNAME", name)
            .env("RUST_VIEW", "1")
            .env("RUST_VIEW_ASSETS", assets())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        for (k, v) in envs {
            c.env(k, v);
        }
        let _ = c.spawn();
    }

    /// Kill every running bot whose MC_USERNAME starts with `prefix` (env from `ps ewwx`).
    pub fn kill_by(prefix: &str) {
        let ps = Command::new("ps")
            .args(["ewwx"])
            .output()
            .ok()
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .unwrap_or_default();
        let needle = format!("MC_USERNAME={prefix}");
        for line in ps.lines() {
            if line.contains("target/release/ruststeve") && line.contains(&needle) {
                if let Some(pid) = line.split_whitespace().next() {
                    let _ = Command::new("kill").args(["-9", pid]).output();
                }
            }
        }
    }
}

#[server(StartRace, "/api")]
pub async fn start_race(n: i32) -> Result<(), ServerFnError> {
    let n = n.clamp(1, 8);
    // Fresh standings: reset the race telemetry DB before the new bots attach.
    for suf in ["", "-wal", "-shm"] {
        let _ = std::fs::remove_file(format!("{DIR}/data/race.db{suf}"));
    }
    let _ = ctl::rcon("gamerule keep_inventory true");
    for i in 1..=n {
        let name = format!("rust-race-{i:03}");
        let _ = ctl::rcon(&format!("op {name}"));
        ctl::spawn_bot(&name, &[("RACE_GOAL".into(), "nether".into())]);
    }
    Ok(())
}

#[server(StopRace, "/api")]
pub async fn stop_race() -> Result<(), ServerFnError> {
    ctl::kill_by("rust-race-");
    Ok(())
}

#[server(StartGym, "/api")]
pub async fn start_gym(slug: String, trials: i32, count: i32, mode: String) -> Result<(), ServerFnError> {
    let count = count.clamp(1, 6);
    let trials = trials.clamp(1, 50);
    let _ = ctl::rcon("gamerule keep_inventory true");
    for i in 1..=count {
        let name = format!("rust-gym-{i:03}");
        let _ = ctl::rcon(&format!("op {name}"));
        let mut envs: Vec<(&str, String)> =
            vec![("GYM", slug.clone()), ("GYM_TRIALS", trials.to_string())];
        match mode.as_str() {
            "deepsea" => envs.push(("GYM_DEEPSEA", "1".into())),
            "arena" => {
                envs.push(("GYM_ARENA_X", (600 + (i as i32) * 70).to_string()));
                envs.push(("GYM_ARENA_Z", "600".into()));
            }
            _ => {}
        }
        ctl::spawn_bot(&name, &envs);
    }
    Ok(())
}

#[server(StopGym, "/api")]
pub async fn stop_gym() -> Result<(), ServerFnError> {
    ctl::kill_by("rust-gym-");
    Ok(())
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct WorldInfo {
    pub up: bool,
    pub players: Vec<String>,
    pub max_players: i32,
    pub keep_inventory: bool,
    pub version: String,
}

#[server(WorldStatus, "/api")]
pub async fn world_status() -> Result<WorldInfo, ServerFnError> {
    let up = std::net::TcpStream::connect_timeout(
        &"127.0.0.1:25565".parse().unwrap(),
        std::time::Duration::from_millis(500),
    )
    .is_ok();
    let mut info = WorldInfo { up, version: "26.1.2".into(), max_players: 100, ..Default::default() };
    if up {
        if let Ok(list) = ctl::rcon("list") {
            // "There are 1 of a max of 100 players online: rust-race-001"
            if let Some(m) = list.split("max of ").nth(1).and_then(|s| s.split(' ').next()) {
                if let Ok(n) = m.parse() {
                    info.max_players = n;
                }
            }
            if let Some((_, names)) = list.split_once("online:") {
                info.players = names
                    .split(',')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect();
            }
        }
        if let Ok(ki) = ctl::rcon("gamerule keep_inventory") {
            info.keep_inventory = ki.contains("true");
        }
    }
    Ok(info)
}

/// Restart the SHARED Minecraft world (systemd `minecraft-server` on the box) and re-apply
/// keep_inventory. Guarded behind a confirm in the UI — this also affects the steve project.
#[server(RestartWorld, "/api")]
pub async fn restart_world() -> Result<(), ServerFnError> {
    let _ = std::process::Command::new("ssh")
        .args([
            "-o",
            "ConnectTimeout=15",
            "bridger@144.24.32.76",
            "sudo systemctl restart minecraft-server",
        ])
        .output();
    std::thread::sleep(std::time::Duration::from_secs(8));
    let _ = ctl::rcon("gamerule keep_inventory true");
    Ok(())
}

// ── UI ──────────────────────────────────────────────────────────────────────
pub fn shell(options: LeptosOptions) -> impl IntoView {
    view! {
        <!DOCTYPE html>
        <html lang="en">
            <head>
                <meta charset="utf-8"/>
                <meta name="viewport" content="width=device-width, initial-scale=1"/>
                <AutoReload options=options.clone() />
                <HydrationScripts options/>
                <MetaTags/>
            </head>
            <body>
                <App/>
                // Binds steve's Babylon viewer.js onto each bot card's <canvas>, pointing
                // its SSE at that bot's per-bot stream port. Kept out of the hydrated tree so
                // the WebGL contexts persist across the client-side stat polling.
                <script type="module" src="/mount.js"></script>
            </body>
        </html>
    }
}

#[component]
pub fn App() -> impl IntoView {
    provide_meta_context();
    view! {
        <Stylesheet id="leptos" href="/pkg/dashboard.css"/>
        <Title text="ruststeve"/>
        <Router>
            <Routes fallback=|| "Page not found.".into_view()>
                <Route path=StaticSegment("") view=Overview/>
                <Route path=StaticSegment("race") view=RacePage/>
                <Route path=StaticSegment("gym") view=GymPage/>
                <Route path=(StaticSegment("gym"), ParamSegment("slug")) view=GymSlugPage/>
                <Route path=StaticSegment("map") view=MapPage/>
                <Route path=StaticSegment("world") view=WorldPage/>
                <Route path=StaticSegment("reports") view=ReportsPage/>
                <Route path=StaticSegment("flow") view=FlowPage/>
            </Routes>
        </Router>
    }
}

/// Cheap liveness probe for the sidebar pill — is the MC port (tunnel) reachable?
#[server(Ping, "/api")]
pub async fn ping() -> Result<bool, ServerFnError> {
    Ok(std::net::TcpStream::connect_timeout(
        &"127.0.0.1:25565".parse().unwrap(),
        std::time::Duration::from_millis(400),
    )
    .is_ok())
}

/// App chrome: persistent left sidebar + a content slot. Every page wraps in this.
#[component]
fn Shell(here: &'static str, children: Children) -> impl IntoView {
    let up = RwSignal::new(false);
    Effect::new(move |_| {
        let poll = move || {
            leptos::task::spawn_local(async move {
                if let Ok(u) = ping().await {
                    up.set(u);
                }
            });
        };
        poll();
        set_interval(poll, std::time::Duration::from_secs(4));
    });
    let cls = move |id: &'static str| if here == id { "navlink active" } else { "navlink" };
    view! {
        <div class="app">
            <aside class="side">
                <div class="side-brand">"rust"<span class="dot">"·"</span>"steve"</div>
                <nav class="side-nav">
                    <A href="/" attr:class=cls("overview")><span class="navicon">"▦"</span>"Overview"</A>
                    <A href="/race" attr:class=cls("race")><span class="navicon">"🏁"</span>"Race"</A>
                    <A href="/gym" attr:class=cls("gym")><span class="navicon">"🏋"</span>"Gym"</A>
                    <A href="/map" attr:class=cls("map")><span class="navicon">"🗺"</span>"Map"</A>
                    <A href="/world" attr:class=cls("world")><span class="navicon">"🌍"</span>"World"</A>
                    <A href="/reports" attr:class=cls("reports")><span class="navicon">"📊"</span>"Reports"</A>
                    <A href="/flow" attr:class=cls("flow")><span class="navicon">"🔀"</span>"Flow"</A>
                </nav>
                <div class="side-foot">
                    <span class="pulse" class:on=move || up.get()></span>
                    {move || if up.get() { "server up" } else { "server down" }}
                </div>
            </aside>
            <main class="content">{children()}</main>
        </div>
    }
}

fn page_head(title: &'static str, sub: &'static str) -> impl IntoView {
    view! { <div class="head"><div><h1>{title}</h1><p class="sub">{sub}</p></div></div> }
}

/// A responsive grid of live bot cards. `kind` filters ("" = all; "race"/"gym"). Stable
/// per-index signals so each card's 3D <canvas> survives stat polling (never remount it).
#[component]
fn Fleet(#[prop(default = "")] kind: &'static str) -> impl IntoView {
    let bots: RwSignal<Vec<RwSignal<Bot>>> = RwSignal::new(Vec::new());
    Effect::new(move |_| {
        let poll = move || {
            leptos::task::spawn_local(async move {
                let Ok(f) = get_fleet().await else { return };
                let list: Vec<Bot> = f
                    .bots
                    .into_iter()
                    .filter(|b| kind.is_empty() || b.kind == kind)
                    .collect();
                let same = bots.with_untracked(|b| {
                    b.len() == list.len()
                        && b.iter().zip(&list).all(|(s, n)| s.with_untracked(|x| x.name == n.name))
                });
                if same {
                    bots.with_untracked(|sigs| {
                        for (sig, b) in sigs.iter().zip(list.into_iter()) {
                            sig.set(b);
                        }
                    });
                } else {
                    bots.set(list.into_iter().map(RwSignal::new).collect());
                }
            });
        };
        poll();
        set_interval(poll, std::time::Duration::from_secs(2));
    });
    view! {
        {move || {
            if bots.with(|b| b.is_empty()) {
                view! { <div class="empty">"No bots running."</div> }.into_any()
            } else {
                view! {
                    <div class="fleet">
                        <For
                            each=move || bots.get().into_iter().enumerate()
                            key=|(i, _)| *i
                            children=move |(_, sig)| view! { <BotCard bot=sig/> }
                        />
                    </div>
                }.into_any()
            }
        }}
    }
}

#[component]
fn Overview() -> impl IntoView {
    view! { <Shell here="overview"><div class="wrap">
        {page_head("Overview", "Live first-person view of every running bot — race and gym.")}
        <Fleet/>
    </div></Shell> }
}

#[component]
fn RaceControls() -> impl IntoView {
    let n = RwSignal::new(3);
    let busy = RwSignal::new(false);
    let start = move |_| {
        let n = n.get_untracked();
        busy.set(true);
        leptos::task::spawn_local(async move {
            let _ = start_race(n).await;
            busy.set(false);
        });
    };
    let stop = move |_| {
        leptos::task::spawn_local(async move {
            let _ = stop_race().await;
        });
    };
    view! {
        <div class="ctl">
            <label>"Bots"</label>
            <input class="num" type="number" min="1" max="8" prop:value=move || n.get()
                on:input=move |e| n.set(event_target_value(&e).parse().unwrap_or(3)) />
            <button class="btn go" on:click=start disabled=move || busy.get()>
                {move || if busy.get() { "Starting…" } else { "▶ Start race" }}
            </button>
            <button class="btn stop" on:click=stop>"■ Stop all"</button>
        </div>
    }
}

#[component]
fn GymControls() -> impl IntoView {
    let slug = RwSignal::new("pool".to_string());
    let trials = RwSignal::new(5);
    let count = RwSignal::new(1);
    let mode = RwSignal::new("random".to_string());
    let start = move |_| {
        let (s, t, c, m) = (slug.get_untracked(), trials.get_untracked(), count.get_untracked(), mode.get_untracked());
        leptos::task::spawn_local(async move {
            let _ = start_gym(s, t, c, m).await;
        });
    };
    let stop = move |_| {
        leptos::task::spawn_local(async move {
            let _ = stop_gym().await;
        });
    };
    view! {
        <div class="ctl">
            <label>"Step"</label>
            <input class="txt" type="text" prop:value=move || slug.get()
                on:input=move |e| slug.set(event_target_value(&e)) list="slugs" />
            <label>"trials"</label>
            <input class="num" type="number" min="1" max="50" prop:value=move || trials.get()
                on:input=move |e| trials.set(event_target_value(&e).parse().unwrap_or(5)) />
            <label>"bots"</label>
            <input class="num" type="number" min="1" max="6" prop:value=move || count.get()
                on:input=move |e| count.set(event_target_value(&e).parse().unwrap_or(1)) />
            <select class="sel" on:change=move |e| mode.set(event_target_value(&e))>
                <option value="random">"random"</option>
                <option value="arena">"arena"</option>
                <option value="deepsea">"deepsea"</option>
            </select>
            <button class="btn go" on:click=start>"▶ Start gym"</button>
            <button class="btn stop" on:click=stop>"■ Stop all"</button>
        </div>
    }
}

#[component]
fn RacePage() -> impl IntoView {
    view! { <Shell here="race"><div class="wrap">
        {page_head("Race", "Bots racing spawn → the nether.")}
        <RaceControls/>
        <Fleet kind="race"/>
    </div></Shell> }
}

/// Colour for a POI kind on the map.
fn poi_color(kind: &str) -> &'static str {
    match kind {
        "lava" => "var(--lava)",
        "water" => "var(--water)",
        "diamond_ore" => "var(--diamond)",
        "iron_ore" => "var(--iron)",
        "coal_ore" => "#5b6472",
        "gold_ore" => "var(--gold)",
        "log" => "var(--leaf)",
        "crafting_table" => "#a1887f",
        _ => "var(--muted)",
    }
}

#[component]
fn MapPage() -> impl IntoView {
    let data = RwSignal::new(MapData::default());
    Effect::new(move |_| {
        let poll = move || {
            leptos::task::spawn_local(async move {
                if let Ok(d) = get_map().await {
                    data.set(d);
                }
            });
        };
        poll();
        set_interval(poll, std::time::Duration::from_secs(3));
    });
    view! { <Shell here="map"><div class="wrap">
        {page_head("Map", "Top-down overworld — live bot positions and everything the bots have seen.")}
        <div class="maplegend">
            {["lava","water","diamond_ore","iron_ore","gold_ore","coal_ore","log","crafting_table"].into_iter()
                .map(|k| view! { <span class="lg"><span class="dotc" style=format!("background:{}", poi_color(k))></span>{k.replace('_'," ")}</span> })
                .collect_view()}
            <span class="lg"><span class="dotc botmk" style="background:var(--sky)"></span>"race bot"</span>
        </div>
        {move || {
            let d = data.get();
            if d.pois.is_empty() && d.bots.iter().all(|b| b.x == 0 && b.z == 0) {
                return view! { <div class="empty">"No map data yet — run a race, or POIs accumulate as bots explore."</div> }.into_any();
            }
            // Fit bounds to the BULK of the POIs (3rd–97th percentile) so a few far-flung
            // outlier sightings don't squash the dense activity into a corner; then always
            // expand to include every live bot (those matter most).
            let mut xs: Vec<i32> = d.pois.iter().map(|p| p.x).collect();
            let mut zs: Vec<i32> = d.pois.iter().map(|p| p.z).collect();
            xs.sort_unstable();
            zs.sort_unstable();
            let pct = |v: &Vec<i32>, p: f64| v.get(((v.len().saturating_sub(1)) as f64 * p) as usize).copied().unwrap_or(0);
            let (mut minx, mut maxx) = (pct(&xs, 0.03), pct(&xs, 0.97));
            let (mut minz, mut maxz) = (pct(&zs, 0.03), pct(&zs, 0.97));
            for b in d.bots.iter().filter(|b| !(b.x == 0 && b.z == 0)) {
                minx = minx.min(b.x); maxx = maxx.max(b.x);
                minz = minz.min(b.z); maxz = maxz.max(b.z);
            }
            let pad = (((maxx - minx).max(maxz - minz)) / 20).max(24);
            minx -= pad; maxx += pad; minz -= pad; maxz += pad;
            let (w, h) = ((maxx - minx).max(1), (maxz - minz).max(1));
            let vb = format!("{minx} {minz} {w} {h}");
            let r = (w.max(h) as f64 / 260.0).max(2.0); // point radius in world units
            let pois = d.pois.clone();
            let bots = d.bots.clone();
            view! {
                <div class="mapbox">
                    <svg viewBox=vb preserveAspectRatio="xMidYMid meet" class="mapsvg">
                        {pois.into_iter().map(|p| {
                            let rr = r * (1.0 + (p.n.min(40) as f64).sqrt() / 6.0);
                            view! { <circle cx=p.x as f64 cy=p.z as f64 r=rr fill=poi_color(&p.kind) opacity="0.7"/> }
                        }).collect_view()}
                        {bots.into_iter().filter(|b| !(b.x==0 && b.z==0)).map(|b| {
                            let col = if b.kind == "gym" { "var(--violet)" } else { "var(--sky)" };
                            let lx = b.x as f64 + r * 3.0;
                            view! {
                                <g>
                                    <circle cx=b.x as f64 cy=b.z as f64 r=r*2.5 fill="none" stroke=col stroke-width=r*0.6/>
                                    <circle cx=b.x as f64 cy=b.z as f64 r=r*1.2 fill=col/>
                                    <text x=lx y=b.z as f64 fill=col font-size=r*3.4 style="font-family:monospace">{b.name.clone()}</text>
                                </g>
                            }
                        }).collect_view()}
                    </svg>
                </div>
                <p class="sub">{format!("bounds x[{minx}..{maxx}] z[{minz}..{maxz}] · {} POI cells", d.pois.len())}</p>
            }.into_any()
        }}
    </div></Shell> }
}
#[component]
fn WorldPage() -> impl IntoView {
    let info = RwSignal::new(WorldInfo::default());
    let confirming = RwSignal::new(false);
    let restarting = RwSignal::new(false);
    Effect::new(move |_| {
        let poll = move || {
            leptos::task::spawn_local(async move {
                if let Ok(w) = world_status().await {
                    info.set(w);
                }
            });
        };
        poll();
        set_interval(poll, std::time::Duration::from_secs(3));
    });
    let do_restart = move |_| {
        confirming.set(false);
        restarting.set(true);
        leptos::task::spawn_local(async move {
            let _ = restart_world().await;
            restarting.set(false);
        });
    };
    view! { <Shell here="world"><div class="wrap">
        {page_head("World", "The shared Minecraft server — status and restart.")}

        <div class="grid cards">
            <div class="card">
                <div class="k">"Server"</div>
                <div class="v" style=move || if info.get().up { "color:var(--ok)" } else { "color:var(--bad)" }>
                    {move || if info.get().up { "Online" } else { "Offline" }}
                </div>
                <div class="tag">{move || format!("vanilla {}", info.get().version)}</div>
            </div>
            <div class="card">
                <div class="k">"Players"</div>
                <div class="v">{move || info.get().players.len().to_string()}
                    <small>{move || format!(" / {}", info.get().max_players)}</small></div>
            </div>
            <div class="card">
                <div class="k">"keep_inventory"</div>
                <div class="v" style=move || if info.get().keep_inventory { "color:var(--ok)" } else { "color:var(--gold)" }>
                    {move || if info.get().keep_inventory { "on" } else { "off" }}
                </div>
            </div>
        </div>

        <h2>"Online now"</h2>
        <div class="panel" style="padding:12px 16px">
            {move || {
                let ps = info.get().players;
                if ps.is_empty() {
                    view! { <span class="tag">"nobody online"</span> }.into_any()
                } else {
                    view! { <div class="chips">
                        <For each=move || ps.clone() key=|p| p.clone()
                            children=move |p| view! { <span class="chip mono">{p}</span> }/>
                    </div> }.into_any()
                }
            }}
        </div>

        <h2>"Danger zone"</h2>
        <div class="panel danger">
            <div>
                <div style="font-weight:700">"Restart world"</div>
                <div class="tag">"Restarts the shared server (also affects the steve project). Re-applies keep_inventory after."</div>
            </div>
            <button class="btn danger" on:click=move |_| confirming.set(true) disabled=move || restarting.get()>
                {move || if restarting.get() { "Restarting…" } else { "Restart world" }}
            </button>
        </div>

        {move || confirming.get().then(|| view! {
            <div class="modal-bg" on:click=move |_| confirming.set(false)>
                <div class="modal" on:click=|e| e.stop_propagation()>
                    <h3>"Restart the shared world?"</h3>
                    <p class="sub">"⚠ This box is SHARED with the steve project. Restarting kicks all players and resets the tick — steve's bots too. Continue?"</p>
                    <div class="modal-actions">
                        <button class="btn" on:click=move |_| confirming.set(false)>"Cancel"</button>
                        <button class="btn danger" on:click=do_restart>"Yes, restart"</button>
                    </div>
                </div>
            </div>
        })}
    </div></Shell> }
}
/// Pure-CSS horizontal bar chart (no JS). Each row: label · bar · value.
#[component]
fn Bars(rows: Vec<Bar>, #[prop(default = "var(--sky)")] color: &'static str) -> impl IntoView {
    let max = rows.iter().map(|r| r.value).max().unwrap_or(1).max(1);
    if rows.is_empty() {
        return view! { <div class="gymcap">"no data yet"</div> }.into_any();
    }
    view! {
        <div class="bars">
            {rows.into_iter().map(|r| {
                let w = (r.value * 100 / max).clamp(0, 100);
                view! {
                    <div class="brow">
                        <span class="blabel" title=r.label.clone()>{r.label.clone()}</span>
                        <div class="btrack"><span style=format!("width:{w}%;background:{color}")></span></div>
                        <span class="bval">{r.value.to_string()}</span>
                    </div>
                }
            }).collect_view()}
        </div>
    }.into_any()
}

#[component]
fn ReportsPage() -> impl IntoView {
    let rep = RwSignal::new(Reports::default());
    Effect::new(move |_| {
        let poll = move || {
            leptos::task::spawn_local(async move {
                if let Ok(r) = get_reports().await {
                    rep.set(r);
                }
            });
        };
        poll();
        set_interval(poll, std::time::Duration::from_secs(5));
    });
    view! { <Shell here="reports"><div class="wrap">
        {page_head("Reports", "Where races reach, where they die — straight off the raw telemetry.")}

        <div class="grid cards">
            <div class="card"><div class="k">"Deaths"</div>
                <div class="v" style="color:var(--bad)">{move || rep.get().deaths.to_string()}</div>
                <div class="tag">"logged across all runs"</div></div>
            <div class="card"><div class="k">"Stuck bails"</div>
                <div class="v" style="color:var(--gold)">{move || rep.get().bails.to_string()}</div>
                <div class="tag">"8-failure aborts"</div></div>
            <div class="card"><div class="k">"Gym steps tracked"</div>
                <div class="v">{move || rep.get().gym.len().to_string()}</div>
                <div class="tag">"see the Gym page"</div></div>
        </div>

        <h2>"Milestone funnel — how far runs get"</h2>
        <div class="panel" style="padding:14px 16px">
            {move || view! { <Bars rows=rep.get().milestones color="var(--leaf)"/> }}
        </div>

        <h2>"Where races stall — top failure reasons"</h2>
        <div class="panel" style="padding:14px 16px">
            {move || view! { <Bars rows=rep.get().stalls color="var(--lava)"/> }}
        </div>

        <h2>"Portal cast stages"</h2>
        <div class="panel" style="padding:14px 16px">
            {move || view! { <Bars rows=rep.get().cast color="var(--violet)"/> }}
        </div>

        <h2>"Gym pass-rate"</h2>
        <div class="panel">
            <For each=move || rep.get().gym key=|r| r.slug.clone()
                children=move |r| {
                    let pct = r.pass_pct;
                    let col = if pct >= 80 { "var(--leaf)" } else if pct >= 40 { "var(--gold)" } else { "var(--lava)" };
                    let slug = r.slug.clone();
                    view! {
                        <a class="gymrow" href=format!("/gym/{slug}")>
                            <span class="gslug">{r.slug.clone()}</span>
                            <div class="gbar"><span style=format!("width:{pct}%;background:{col}")></span></div>
                            <span class="gpct" style=format!("color:{col}")>{format!("{pct}%")}</span>
                            <span class="gmeta">{format!("{}/{}", r.passes, r.runs)}</span>
                            <span class="gmeta">{format!("{}s", r.avg_s)}</span>
                            <span class="gxyz"></span>
                        </a>
                    }
                }/>
        </div>
    </div></Shell> }
}
#[derive(Clone)]
struct FlowNode {
    name: &'static str,
    detail: String,
    pct: i32,
    state: &'static str, // "done" | "current" | "locked"
}

/// The 19-step ladder for one bot, each node with live sub-progress ("3/16 cobble")
/// and a state derived from the bot's active step_id (falling back to first-incomplete).
fn ladder(b: &FlowBot) -> Vec<FlowNode> {
    let mk = |name: &'static str, cur: i32, tgt: i32, unit: &str| (name, format!("{}/{} {unit}", cur.min(tgt), tgt), (cur * 100 / tgt.max(1)).clamp(0, 100), cur >= tgt);
    let bl = |name: &'static str, ok: bool| (name, (if ok { "ready ✓" } else { "—" }).to_string(), if ok { 100 } else { 0 }, ok);
    let gw_done = b.logs >= 6 || b.planks >= 16;
    let raw: Vec<(&'static str, String, i32, bool)> = vec![
        ("Gather Wood", if gw_done { "✓".into() } else { format!("{}/6 logs", b.logs.min(6)) }, if gw_done { 100 } else { (b.logs * 100 / 6).clamp(0, 100) }, gw_done),
        mk("Planks", b.planks, 8, "planks"),
        bl("Crafting Table", b.has_table),
        mk("Sticks", b.sticks, 4, "sticks"),
        bl("Wood Pickaxe", b.pick >= 1),
        mk("Mine Stone", b.cobble, 16, "cobble"),
        bl("Stone Pickaxe", b.pick >= 2),
        bl("Stone Sword", b.sword >= 1),
        bl("Furnace", b.has_furnace),
        mk("Coal", b.coal, 2, "coal"),
        mk("Iron Ore", b.iron_ore + b.iron, 12, "iron"),
        mk("Smelt Iron", b.iron, 11, "ingots"),
        bl("Iron Pickaxe", b.pick >= 3),
        mk("Buckets", b.buckets + b.water, 2, "buckets"),
        mk("Water", b.water, 1, "water"),
        bl("Flint & Steel", b.fas >= 1),
        mk("Build Blocks", b.cobble + b.dirt, 40, "blocks"),
        bl("Build Portal", b.portal_built),
        bl("Enter Nether", b.in_nether),
    ];
    let step_idx = match b.step_id.as_str() {
        "gather_wood" => 0, "craft_planks" => 1, "craft_crafting_table" => 2, "craft_sticks" => 3,
        "craft_wooden_pickaxe" => 4, "mine_stone" => 5, "craft_stone_pickaxe" => 6, "craft_stone_sword" => 7,
        "craft_furnace" => 8, "mine_coal" => 9, "mine_iron" => 10, "smelt_iron" => 11, "craft_iron_pickaxe" => 12,
        "craft_bucket" => 13, "get_water_buckets" => 14, "get_flint_and_steel" => 15, "gather_build_blocks" => 16,
        "build_nether_portal" => 17, "enter_nether" => 18, _ => usize::MAX,
    };
    let cur = if step_idx != usize::MAX { step_idx } else { raw.iter().position(|n| !n.3).unwrap_or(raw.len()) };
    raw.into_iter().enumerate().map(|(i, (name, detail, pct, _))| {
        let (state, detail, pct) = if i < cur { ("done", "✓".to_string(), 100) } else if i == cur { ("current", detail, pct) } else { ("locked", detail, pct) };
        FlowNode { name, detail, pct, state }
    }).collect()
}

/// A bot's recent activity log — milestones (green), deaths (red), stuck bails (gold).
#[component]
fn Timeline(entries: Vec<TimelineEntry>) -> impl IntoView {
    if entries.is_empty() {
        return view! { <div class="gymcap">"no events logged yet"</div> }.into_any();
    }
    view! {
        <div class="tl">
            {entries.into_iter().map(|e| {
                let (dot, label) = match e.kind.as_str() {
                    "death" => ("var(--bad)", "💀 died".to_string()),
                    "stuck_bail" => ("var(--gold)", format!("⚠ bail — {}", e.detail)),
                    _ => ("var(--leaf)", e.label.clone()),
                };
                view! {
                    <div class="tlrow">
                        <span class="tldot" style=format!("background:{dot}")></span>
                        <span class="tllabel">{label}</span>
                        <span class="tlago mono">{e.ago}</span>
                    </div>
                }
            }).collect_view()}
        </div>
    }.into_any()
}

#[component]
fn FlowPage() -> impl IntoView {
    let bots: RwSignal<Vec<FlowBot>> = RwSignal::new(Vec::new());
    let sel = RwSignal::new(String::new());
    let timeline: RwSignal<Vec<TimelineEntry>> = RwSignal::new(Vec::new());
    Effect::new(move |_| {
        let poll = move || {
            leptos::task::spawn_local(async move {
                let Ok(list) = get_flow().await else { return };
                // keep the selection valid; default to the furthest-along bot
                let names: Vec<String> = list.iter().map(|b| b.name.clone()).collect();
                sel.update(|s| { if s.is_empty() || !names.contains(s) { *s = names.first().cloned().unwrap_or_default(); } });
                bots.set(list);
            });
        };
        poll();
        set_interval(poll, std::time::Duration::from_secs(2));
    });
    // Refresh the selected bot's timeline (reads sel untracked so re-selection is picked up).
    Effect::new(move |_| {
        let poll = move || {
            let name = sel.get_untracked();
            if name.is_empty() { return; }
            leptos::task::spawn_local(async move {
                if let Ok(t) = get_timeline(name).await { timeline.set(t); }
            });
        };
        poll();
        set_interval(poll, std::time::Duration::from_secs(3));
    });
    let current = move || bots.get().into_iter().find(|b| b.name == sel.get());
    view! { <Shell here="flow"><div class="wrap">
        {page_head("Flow", "The speedrun ladder for a bot — what it's on now, and what unlocks next.")}
        {move || {
            let list = bots.get();
            if list.is_empty() {
                return view! { <div class="empty">"No race bots running — start a race to watch the ladder advance."</div> }.into_any();
            }
            view! {
                <div class="botpick">
                    <For each=move || bots.get() key=|b| b.name.clone()
                        children=move |b| {
                            let name = b.name.clone();
                            let n2 = name.clone();
                            let sel_now = move || sel.get() == n2;
                            view! { <button class="pick" class:on=sel_now
                                on:click=move |_| sel.set(name.clone())>
                                {b.name.clone()}<span class="pickd">{format!("{}/19", b.done)}</span>
                            </button> }
                        }/>
                </div>
            }.into_any()
        }}
        {move || current().map(|b| {
            let nodes = ladder(&b);
            let cur = nodes.iter().find(|n| n.state == "current").cloned();
            view! {
                <div class="flowhead">
                    {cur.map(|c| view! {
                        <div class="flowcur">
                            <span class="tag">"NOW"</span>
                            <span class="fcname">{c.name}</span>
                            <span class="fcdetail">{c.detail}</span>
                        </div>
                    })}
                    {(!b.alive).then(|| view! { <span class="chip dead">"💀 dead"</span> })}
                    {b.in_nether.then(|| view! { <span class="chip lava">"🔥 in the nether"</span> })}
                </div>
                <div class="flow">
                    {nodes.into_iter().enumerate().map(|(i, n)| {
                        let arrow = (i < 18).then(|| view! { <span class="farrow">"›"</span> });
                        view! {
                            <div class=format!("fnode {}", n.state)>
                                <span class="fnum">{format!("{:02}", i + 1)}</span>
                                <span class="fname">{n.name}</span>
                                <div class="ftrack"><span style=format!("width:{}%", n.pct)></span></div>
                                <span class="fdetail">{n.detail}</span>
                            </div>
                            {arrow}
                        }
                    }).collect_view()}
                </div>
            }
        })}
        {move || (!bots.get().is_empty()).then(|| view! {
            <h2>"Recent events"</h2>
            <div class="panel" style="padding:6px 4px">
                {move || view! { <Timeline entries=timeline.get()/> }}
            </div>
        })}
    </div></Shell> }
}
#[component]
fn GymSlugPage() -> impl IntoView {
    let params = use_params_map();
    let slug0 = params.get_untracked().get("slug").unwrap_or_default();
    let detail = RwSignal::new(GymDetail::default());
    let busy = RwSignal::new(false);
    {
        let slug = slug0.clone();
        Effect::new(move |_| {
            let slug = slug.clone();
            let poll = move || {
                let slug = slug.clone();
                leptos::task::spawn_local(async move {
                    if let Ok(d) = get_gym_slug(slug).await {
                        detail.set(d);
                    }
                });
            };
            poll();
            set_interval(poll, std::time::Duration::from_secs(4));
        });
    }
    let start = {
        let slug = slug0.clone();
        move |_| {
            let slug = slug.clone();
            busy.set(true);
            leptos::task::spawn_local(async move {
                let _ = start_gym(slug, 20, 1, "random".into()).await;
                busy.set(false);
            });
        }
    };
    let stop = move |_| {
        leptos::task::spawn_local(async move {
            let _ = stop_gym().await;
        });
    };
    let title = slug0.clone();
    view! { <Shell here="gym"><div class="wrap">
        <div class="head">
            <div>
                <p class="sub"><a href="/gym" style="color:var(--muted)">"← gym"</a></p>
                <h1 class="mono">{title}</h1>
            </div>
            <div class="ctl" style="margin:0">
                <button class="btn go" on:click=start disabled=move || busy.get()>
                    {move || if busy.get() { "Starting…" } else { "▶ Run 20" }}</button>
                <button class="btn stop" on:click=stop>"■ Stop"</button>
            </div>
        </div>

        <div class="grid cards">
            <div class="card"><div class="k">"Pass rate"</div>
                <div class="v" style=move || {
                    let p = detail.get().pass_pct;
                    format!("color:{}", if p >= 80 { "var(--leaf)" } else if p >= 40 { "var(--gold)" } else { "var(--lava)" })
                }>{move || format!("{}%", detail.get().pass_pct)}</div>
                <div class="tag">{move || format!("{} / {} runs", detail.get().passes, detail.get().runs)}</div></div>
            <div class="card"><div class="k">"Avg time"</div>
                <div class="v">{move || detail.get().avg_s.to_string()}<small>"s"</small></div></div>
            <div class="card"><div class="k">"Last 40"</div>
                <div class="strip">{move || detail.get().history.into_iter().map(|p|
                    view! { <span class="scell" style=format!("background:{}", if p == 1 { "var(--leaf)" } else { "var(--lava)" })></span> }
                ).collect_view()}</div>
                <div class="tag">"oldest → newest"</div></div>
        </div>

        <h2>"Live"</h2>
        <Fleet kind="gym"/>

        <h2>"Recent runs"</h2>
        <div class="panel">
            <div class="trow thead">
                <span>"result"</span><span>"time"</span><span>"tp to reproduce"</span><span>"note"</span><span class="tago">"when"</span>
            </div>
            {move || {
                let runs = detail.get().recent;
                if runs.is_empty() {
                    return view! { <div class="gymcap">"no runs yet — hit Run 20"</div> }.into_any();
                }
                view! { <div>
                    <For each=move || detail.get().recent.into_iter().enumerate() key=|(i, _)| *i
                        children=move |(_, r)| {
                            let tp = format!("/tp @s {} {} {}", r.x, r.y, r.z);
                            view! {
                                <div class="trow">
                                    <span>{if r.pass { view! { <span class="chip" style="color:var(--leaf)">"PASS"</span> }.into_any() } else { view! { <span class="chip" style="color:var(--lava)">"FAIL"</span> }.into_any() }}</span>
                                    <span class="mono">{format!("{}s", r.secs)}</span>
                                    <span class="mono tpcell" title="run this on the server to reproduce the terrain">{tp}</span>
                                    <span class="tnote" title=r.msg.clone()>{r.msg.clone()}</span>
                                    <span class="tago mono">{r.ago.clone()}</span>
                                </div>
                            }
                        }/>
                </div> }.into_any()
            }}
        </div>
    </div></Shell> }
}

#[component]
fn BotCard(bot: RwSignal<Bot>) -> impl IntoView {
    let port = bot.with_untracked(|b| b.port);
    view! {
        <div class="botcard" class:stale=move || { let b = bot.get(); b.kind == "race" && b.age_s > 15 }>
            // ── the live world (Babylon viewer, bound by /mount.js) ──
            <div class="v3d-wrap">
                <canvas class="v3d" data-vport=port.to_string()></canvas>
                <div class="v3d-top">
                    <span class="name">{move || bot.get().name}</span>
                    {move || {
                        let b = bot.get();
                        let (tag, col) = phase_tag(b.done);
                        view! { <span class="phase" style=format!("background:{col}")>{tag}</span> }
                    }}
                </div>
                <div class="v3d-bot">
                    {move || {
                        let b = bot.get();
                        if !b.alive {
                            view! { <span class="chip dead">"💀 dead"</span> }.into_any()
                        } else if b.dim.contains("nether") {
                            view! { <span class="chip lava">"🔥 nether"</span> }.into_any()
                        } else if b.kind == "race" && b.age_s > 45 {
                            // No fresh tick for ~9 loops → the bot is hung / not progressing.
                            view! { <span class="chip stuck">{format!("⚠ stuck {}", fmt_ago(b.age_s))}</span> }.into_any()
                        } else if b.kind == "gym" {
                            view! { <span class="chip gym">"gym"</span> }.into_any()
                        } else {
                            view! { <span class="chip">{move || format!("y {}", bot.get().y)}</span> }.into_any()
                        }
                    }}
                    {move || {
                        let b = bot.get();
                        (!(b.x == 0 && b.y == 0 && b.z == 0))
                            .then(|| view! { <span class="coords">{format!("{}, {}, {}", b.x, b.y, b.z)}</span> })
                    }}
                </div>
            </div>

            // Gym bots don't write race telemetry, so their per-step state is unknown — show
            // just the live view. Race bots get the full step / vitals / inventory readout.
            {move || {
                let is_race = bot.with(|b| b.kind != "gym");
                if is_race {
                    view! {
                        <div class="step">
                            <span class="step-name">{move || {
                                let b = bot.get();
                                if b.step.is_empty() { PHASES.get(b.done.max(0) as usize).copied().unwrap_or("—").to_string() } else { b.step.clone() }
                            }}</span>
                            <span class="frac">{move || format!("{}/{}", bot.get().done, bot.get().total)}</span>
                        </div>
                        <div class="prog">
                            <span style=move || {
                                let b = bot.get();
                                format!("width:{}%", (100 * b.done / b.total.max(1)).clamp(0, 100))
                            }></span>
                        </div>
                        <div class="vitals">
                            <div class="vbar">
                                <span class="vlabel">"HP"</span>
                                <div class="bar"><span class="hp" style=move || format!("width:{}%", (bot.get().health / 20.0 * 100.0).clamp(0.0, 100.0))></span></div>
                            </div>
                            <div class="vbar">
                                <span class="vlabel">"Food"</span>
                                <div class="bar"><span class="food" style=move || format!("width:{}%", (bot.get().food / 20.0 * 100.0).clamp(0.0, 100.0))></span></div>
                            </div>
                        </div>
                        <div class="inv">
                            <span class="chip">"⛏ " {move || bot.get().pick}</span>
                            <span class="chip iron">"iron " {move || bot.get().iron}</span>
                            {move || (bot.get().diamonds > 0).then(|| view! { <span class="chip dia">"💎 " {bot.get().diamonds}</span> })}
                            <span class="chip water">"💧 " {move || bot.get().water}</span>
                            <span class="chip">"🪨 " {move || bot.get().cobble}</span>
                            {move || bot.get().portal_built.then(|| view! { <span class="chip lava">"portal ✓"</span> })}
                        </div>
                    }.into_any()
                } else {
                    view! { <div class="gymcap">"Gym trial — watching the cast live"</div> }.into_any()
                }
            }}
        </div>
    }
}

#[component]
fn GymPage() -> impl IntoView {
    let rows: RwSignal<Vec<GymRow>> = RwSignal::new(Vec::new());
    Effect::new(move |_| {
        let poll = move || {
            leptos::task::spawn_local(async move {
                if let Ok(r) = get_gym().await {
                    rows.set(r);
                }
            });
        };
        poll();
        set_interval(poll, std::time::Duration::from_secs(5));
    });

    view! { <Shell here="gym"><div class="wrap">
        {page_head("Gym", "Per-step pass-rate across random terrain — lowest first is the next thing to perfect.")}
        <GymControls/>
        <datalist id="slugs">
            <For each=move || rows.get() key=|r| r.slug.clone()
                children=move |r| view! { <option value=r.slug.clone()></option> }/>
        </datalist>
        <Fleet kind="gym"/>
        <h2>"Steps"</h2>
        <div class="panel">
            <For
                each=move || rows.get()
                key=|r| r.slug.clone()
                children=move |r| {
                    let pct = r.pass_pct;
                    let col = if pct >= 80 { "var(--leaf)" } else if pct >= 40 { "var(--gold)" } else { "var(--lava)" };
                    let slug = r.slug.clone();
                    view! {
                        <a class="gymrow" href=format!("/gym/{slug}")>
                            <span class="gslug">{r.slug.clone()}</span>
                            <div class="gbar"><span style=format!("width:{pct}%;background:{col}")></span></div>
                            <span class="gpct" style=format!("color:{col}")>{format!("{pct}%")}</span>
                            <span class="gmeta">{format!("{}/{}", r.passes, r.runs)}</span>
                            <span class="gmeta">{format!("{}s", r.avg_s)}</span>
                            <span class="gxyz" title="most recent failure — tp here to reproduce">{r.last_fail_xyz.clone()}</span>
                        </a>
                    }
                }
            />
        </div>
    </div></Shell> }
}
