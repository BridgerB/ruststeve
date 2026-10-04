//! Race telemetry → a single queryable SQLite DB (`data/race.db`), the source of
//! truth for both the live dashboard and post-run analysis. Replaces grepping 100+ MB
//! of stdout `race-<i>.log`.
//!
//! Three tables, all wall-clock (`ts_ms`) and tagged by `bot` so every bot in a race
//! lives in one place:
//!   - `ticks`    — a throttled per-iteration state snapshot (the timeline).
//!   - `events`   — rare typed events (death, milestone, win, bails, session).
//!   - `counters` — upsert-and-count for the repetitive stuff (the 247k "surfacing"
//!                  spam and the per-step failure messages collapse to one row + count).
//!
//! Attached only on the race path (`RACE_GOAL`/`RACE_DB` set) so gym/isolation modes and
//! `data/gym.db` / `data/sniff-*.db` / the per-bot `pois` table are untouched. Mirrors
//! the `GymStore`/`Sniffer` rusqlite patterns.

use rusqlite::{params, Connection};

use crate::types::GameState;

const DB_PATH: &str = "data/race.db";

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// One bot's writer into the shared race DB.
pub struct RaceLog {
    conn: Connection,
    bot: String,
    /// (step_id, x, y, z, done, health rounded) — a new `ticks` row is written only when
    /// this changes or the heartbeat elapses, so fail-spins collapse to ~1 row/5s.
    last_sig: Option<(String, i32, i32, i32, i32, i32)>,
    last_tick_ms: i64,
    last_done: i32,
}

impl RaceLog {
    pub fn open(bot: &str) -> RaceLog {
        let _ = std::fs::create_dir_all("data");
        let conn = Connection::open(DB_PATH).expect("open race db");
        conn.execute_batch(SCHEMA).expect("init race schema");
        let mut r = RaceLog { conn, bot: bot.to_string(), last_sig: None, last_tick_ms: 0, last_done: -1 };
        r.event("session", "start", None, "", None, None, None, None);
        r
    }

    /// Throttled state snapshot. Writes a `ticks` row only on a real change or the 5s
    /// heartbeat; emits a `milestone` event when the completed-step count rises.
    pub fn tick(&mut self, s: &GameState, step_id: Option<&str>, step_name: Option<&str>, done: i32, total: i32) {
        let (x, y, z) = (s.position.0.floor() as i32, s.position.1.floor() as i32, s.position.2.floor() as i32);
        let sig = (step_id.unwrap_or("").to_string(), x, y, z, done, s.health.round() as i32);
        let now = now_ms();
        if self.last_sig.as_ref() != Some(&sig) || now - self.last_tick_ms >= 5000 {
            let inv = &s.inventory;
            let _ = self.conn.execute(
                "INSERT INTO ticks(ts_ms,bot,dim,step_id,step_name,done,total,health,food,x,y,z,alive,portal_built,
                    pick,sword,has_table,has_furnace,logs,planks,sticks,cobblestone,dirt,coal,iron_ore,iron_ingots,
                    diamonds,food_items,crafting_tables,buckets,water_buckets,flint,flint_and_steel)
                 VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19,?20,?21,?22,?23,?24,?25,?26,?27,?28,?29,?30,?31,?32,?33)",
                params![
                    now, self.bot, s.world.dimension, step_id, step_name, done, total, s.health, s.food, x, y, z,
                    s.alive as i32, s.world.portal_built as i32,
                    s.equipment.pickaxe_tier().rank(), s.equipment.sword.map(|t| t.rank()).unwrap_or(0),
                    s.equipment.has_crafting_table as i32, s.equipment.has_furnace as i32,
                    inv.logs, inv.planks, inv.sticks, inv.cobblestone, inv.dirt, inv.coal, inv.iron_ore, inv.iron_ingots,
                    inv.diamonds, inv.food, inv.crafting_tables, inv.buckets, inv.water_buckets, inv.flint, inv.flint_and_steel,
                ],
            );
            self.last_sig = Some(sig);
            self.last_tick_ms = now;
        }
        // Milestone edge: record each newly-completed level with its wall-clock time.
        if done > self.last_done {
            self.event("milestone", step_name.unwrap_or(""), step_id, "", Some((x, y, z)), Some(s.health), Some(s.food), Some(done as i64));
            self.last_done = done;
        }
    }

    /// One discrete typed event row.
    #[allow(clippy::too_many_arguments)]
    pub fn event(
        &mut self,
        category: &str,
        event: &str,
        step_id: Option<&str>,
        detail: &str,
        pos: Option<(i32, i32, i32)>,
        health: Option<f64>,
        food: Option<f64>,
        n: Option<i64>,
    ) {
        let (x, y, z) = match pos {
            Some((x, y, z)) => (Some(x), Some(y), Some(z)),
            None => (None, None, None),
        };
        let d: String = detail.chars().take(400).collect();
        let _ = self.conn.execute(
            "INSERT INTO events(ts_ms,bot,category,event,step_id,detail,x,y,z,health,food,n)
             VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12)",
            params![now_ms(), self.bot, category, event, step_id, d, x, y, z, health, food, n],
        );
    }

    /// Upsert-and-count. This is where the repetitive task logs and step failures go, so
    /// 247k "surfacing" or 1884 "need a crafting table" become one row with a `count`.
    pub fn count(&mut self, category: &str, event: &str, step_id: &str, sample: &str) {
        let ev: String = event.chars().take(200).collect();
        let sm: String = sample.chars().take(200).collect();
        let now = now_ms();
        let _ = self.conn.execute(
            "INSERT INTO counters(bot,category,event,step_id,count,first_ts_ms,last_ts_ms,sample)
             VALUES(?1,?2,?3,?4,1,?5,?5,?6)
             ON CONFLICT(bot,category,event,step_id) DO UPDATE
               SET count=count+1, last_ts_ms=excluded.last_ts_ms, sample=excluded.sample",
            params![self.bot, category, ev, step_id, now, sm],
        );
    }

    pub fn death(&mut self, s: &GameState) {
        let pos = (s.position.0.floor() as i32, s.position.1.floor() as i32, s.position.2.floor() as i32);
        self.event("death", "died", None, "", Some(pos), Some(s.health), Some(s.food), None);
    }

    /// A step returned. Successes are timed via the `tick` milestone edge; failures go to
    /// `counters` keyed by the message so "top failures per step" is a GROUP BY.
    pub fn step_result(&mut self, step_id: &str, success: bool, message: &str) {
        if !success {
            self.count("step_fail", message, step_id, message);
        }
    }

    /// Mirror of one data/attempts.jsonl row (cycle 4 event log) for SQL queries.
    pub fn attempt(&mut self, row: &serde_json::Value) {
        let _ = self.conn.execute(
            "INSERT INTO attempts(ts_ms,bot,run_id,build,skill,step_id,outcome,reason,duration_s,deaths,json)
             VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11)",
            params![
                now_ms(), self.bot, row["run_id"].as_str(), row["build"].as_str(), row["skill"].as_str(),
                row["step_id"].as_str(), row["outcome"].as_str(), row["reason"].as_str(),
                row["duration_s"].as_f64(), row["deaths"].as_i64(), row.to_string()
            ],
        );
    }

    pub fn win(&mut self, goal: &str) {
        self.event("win", goal, None, "", None, None, None, None);
    }

    pub fn bail(&mut self, kind: &str, s: &GameState, detail: &str) {
        let pos = (s.position.0.floor() as i32, s.position.1.floor() as i32, s.position.2.floor() as i32);
        self.event(kind, "bail", None, detail, Some(pos), Some(s.health), Some(s.food), None);
    }
}

/// Schema + views. Window-function views need SQLite ≥3.25 (rusqlite's bundled build is
/// fine; a stale CLI `sqlite3` may not have them — the Rust report doesn't rely on them).
const SCHEMA: &str = "
PRAGMA journal_mode=WAL;
PRAGMA synchronous=NORMAL;
PRAGMA busy_timeout=5000;

CREATE TABLE IF NOT EXISTS ticks(
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  ts_ms INTEGER NOT NULL, bot TEXT NOT NULL, dim TEXT,
  step_id TEXT, step_name TEXT, done INTEGER, total INTEGER,
  health REAL, food REAL, x INTEGER, y INTEGER, z INTEGER,
  alive INTEGER, portal_built INTEGER, pick INTEGER, sword INTEGER,
  has_table INTEGER, has_furnace INTEGER,
  logs INTEGER, planks INTEGER, sticks INTEGER, cobblestone INTEGER, dirt INTEGER,
  coal INTEGER, iron_ore INTEGER, iron_ingots INTEGER, diamonds INTEGER, food_items INTEGER,
  crafting_tables INTEGER, buckets INTEGER, water_buckets INTEGER, flint INTEGER, flint_and_steel INTEGER
);
CREATE INDEX IF NOT EXISTS ticks_bot_ts   ON ticks(bot, ts_ms);
CREATE INDEX IF NOT EXISTS ticks_bot_step ON ticks(bot, step_id, ts_ms);

CREATE TABLE IF NOT EXISTS events(
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  ts_ms INTEGER NOT NULL, bot TEXT NOT NULL,
  category TEXT NOT NULL, event TEXT NOT NULL, step_id TEXT, detail TEXT,
  x INTEGER, y INTEGER, z INTEGER, health REAL, food REAL, n INTEGER
);
CREATE INDEX IF NOT EXISTS events_bot_ts ON events(bot, ts_ms);
CREATE INDEX IF NOT EXISTS events_cat    ON events(category, event);

CREATE TABLE IF NOT EXISTS attempts(
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  ts_ms INTEGER NOT NULL, bot TEXT NOT NULL, run_id TEXT, build TEXT, skill TEXT, step_id TEXT,
  outcome TEXT, reason TEXT, duration_s REAL, deaths INTEGER, json TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS attempts_skill ON attempts(skill, outcome);

CREATE TABLE IF NOT EXISTS counters(
  bot TEXT NOT NULL, category TEXT NOT NULL, event TEXT NOT NULL, step_id TEXT NOT NULL DEFAULT '',
  count INTEGER NOT NULL DEFAULT 0, first_ts_ms INTEGER NOT NULL, last_ts_ms INTEGER NOT NULL, sample TEXT,
  UNIQUE(bot, category, event, step_id)
);

CREATE VIEW IF NOT EXISTS v_step_spans AS
WITH marked AS (
  SELECT bot, ts_ms, step_id,
    ROW_NUMBER() OVER (PARTITION BY bot ORDER BY ts_ms)
    - ROW_NUMBER() OVER (PARTITION BY bot, step_id ORDER BY ts_ms) AS grp
  FROM ticks WHERE step_id IS NOT NULL)
SELECT bot, step_id, MIN(ts_ms) AS start_ms, MAX(ts_ms) AS end_ms,
       (MAX(ts_ms)-MIN(ts_ms))/1000.0 AS secs
FROM marked GROUP BY bot, step_id, grp ORDER BY bot, start_ms;

CREATE VIEW IF NOT EXISTS v_top_fails AS
SELECT bot, step_id, event AS message, count
FROM counters WHERE category='step_fail' ORDER BY count DESC;

CREATE VIEW IF NOT EXISTS v_deaths AS
SELECT bot, x, y, z, COUNT(*) AS deaths, MIN(ts_ms) AS first_ms, MAX(ts_ms) AS last_ms
FROM events WHERE category='death' GROUP BY bot, x, y, z ORDER BY deaths DESC;

CREATE VIEW IF NOT EXISTS v_milestones AS
SELECT bot, n AS done, event AS milestone, MIN(ts_ms) AS first_ms
FROM events WHERE category='milestone' GROUP BY bot, n;
";

// ── RACE_REPORT=1 — the full post-run auto-report ────────────────────────────────

fn q_rows<T, F>(conn: &Connection, sql: &str, f: F) -> Vec<T>
where
    F: FnMut(&rusqlite::Row) -> rusqlite::Result<T>,
{
    match conn.prepare(sql) {
        Ok(mut stmt) => stmt.query_map([], f).map(|r| r.flatten().collect()).unwrap_or_default(),
        Err(_) => Vec::new(),
    }
}

fn mmss(ms: i64) -> String {
    let s = (ms / 1000).max(0);
    format!("{}:{:02}", s / 60, s % 60)
}

/// Print the whole post-run analysis from `data/race.db` and exit (no bot).
pub fn report() {
    let conn = match Connection::open(DB_PATH) {
        Ok(c) => c,
        Err(e) => {
            println!("no race.db ({e})");
            return;
        }
    };

    // Per-bot session start (for relative milestone times) + furthest step.
    let bots: Vec<(String, i64, i64, i64)> = q_rows(
        &conn,
        "SELECT t.bot, MAX(t.done), MAX(t.total),
                (SELECT MIN(ts_ms) FROM events e WHERE e.bot=t.bot AND category='session')
         FROM ticks t GROUP BY t.bot ORDER BY MAX(t.done) DESC",
        |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?, r.get::<_, i64>(2)?, r.get::<_, i64>(3).unwrap_or(0))),
    );

    println!("\n================ RACE REPORT ({DB_PATH}) ================");
    println!("\n── Furthest step per bot ──");
    for (bot, done, total, _) in &bots {
        println!("  {bot:<16} {done}/{total}");
    }

    println!("\n── Milestones reached (m:ss from each bot's start) ──");
    for (bot, _, _, start) in &bots {
        let ms: Vec<(i64, String, i64)> = q_rows(
            &conn,
            &format!("SELECT done, milestone, first_ms FROM v_milestones WHERE bot='{bot}' ORDER BY done"),
            |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?, r.get::<_, i64>(2)?)),
        );
        let line: Vec<String> = ms.iter().map(|(_, name, ts)| format!("{name} @{}", mmss(ts - start))).collect();
        println!("  {bot:<16} {}", line.join("  →  "));
    }

    println!("\n── Seconds per step (per bot) ──");
    for (bot, _, _, _) in &bots {
        let spans: Vec<(String, f64)> = q_rows(
            &conn,
            &format!("SELECT step_id, secs FROM v_step_spans WHERE bot='{bot}' AND step_id!='' ORDER BY start_ms"),
            |r| Ok((r.get::<_, String>(0)?, r.get::<_, f64>(1)?)),
        );
        let line: Vec<String> = spans.iter().map(|(id, s)| format!("{id} {s:.0}s")).collect();
        println!("  {bot:<16} {}", line.join("  "));
    }

    println!("\n── Deaths / respawn loops (coords with >1 death) ──");
    for (bot, x, y, z, n) in q_rows(&conn, "SELECT bot,x,y,z,deaths FROM v_deaths WHERE deaths>1 LIMIT 25", |r| {
        Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?, r.get::<_, i64>(2)?, r.get::<_, i64>(3)?, r.get::<_, i64>(4)?))
    }) {
        println!("  {bot:<16} {n:>5}x at ({x},{y},{z})");
    }

    println!("\n── Top failure messages per step ──");
    for (bot, step, msg, n) in q_rows(&conn, "SELECT bot,step_id,message,count FROM v_top_fails LIMIT 25", |r| {
        Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?, r.get::<_, i64>(3)?))
    }) {
        println!("  {bot:<16} {n:>7}x [{step}] {msg}");
    }

    println!("\n── Spin / stuck (repeated task actions, count>200) ──");
    for (bot, cat, ev, n) in q_rows(
        &conn,
        "SELECT bot,category,event,count FROM counters WHERE category!='step_fail' AND count>200 ORDER BY count DESC LIMIT 20",
        |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?, r.get::<_, i64>(3)?)),
    ) {
        println!("  {bot:<16} {n:>7}x {cat}/{ev}");
    }
    println!("\n(ad-hoc: sqlite3 {DB_PATH} — views v_step_spans, v_top_fails, v_deaths, v_milestones)\n");
}
