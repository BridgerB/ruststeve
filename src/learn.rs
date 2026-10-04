//! Cycle-4 learning system (docs/ruststeve-loop-cycle4.md, Parts 4, 5 and 8).
//!
//! - **Attempt rows.** One JSON object per step attempt and per guarded-primitive call, appended to
//!   `data/attempts.jsonl` (the canonical file models read) and mirrored into race.db `attempts`
//!   on the race path. Schema: `docs/events-schema.md`.
//! - **Bandits.** `data/params.json` holds Beta posteriors per arm. At the start of an attempt each
//!   *enabled* parameter that applies to the step's skill draws its arm by Thompson sampling; the
//!   chosen arms ride in the row's `params` and are updated when the attempt ends. Hot-reloaded
//!   every attempt, so no rebuild changes a value.
//! - **Budgets and stall detection.** Each step has a budget (seconds) and a stall window: an
//!   attempt whose progress metric (a ratchet toward the postcondition) has not improved for the
//!   window, or that exceeds its budget, ends `timeout`, and the step's alternative counter bumps
//!   so the task changes something it controls (heading, site, band) on the next attempt.
//!
//! Context features are exposure-only (the no-X-ray rule restated for learning): nothing here may
//! read a block the bot has not seen exposed.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Mutex;
use std::time::Instant;

use serde_json::{json, Map, Value};

use crate::bot::Bot;
use crate::bot_utils::count_items;
use crate::types::GameState;

const PARAMS: &str = "data/params.json";

pub fn now_ms() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
}

fn env_or(k: &str, d: &str) -> String {
    std::env::var(k).ok().filter(|s| !s.is_empty()).unwrap_or_else(|| d.to_string())
}

fn source() -> &'static str {
    if std::env::var("RACE_GOAL").is_ok() {
        "race"
    } else if std::env::var("GYM").is_ok() {
        "gym"
    } else {
        "adhoc"
    }
}

/// The twelve skills of Part 3. Steps 1–5 of the table map onto the existing chain; the lava site
/// and the cast are one step (`build_nether_portal`) today, so both are tagged `portal`.
pub fn skill_of(step_id: &str) -> &'static str {
    match step_id {
        "gather_wood" | "craft_planks" | "craft_crafting_table" | "craft_sticks" | "craft_wooden_pickaxe" | "mine_stone"
        | "craft_stone_pickaxe" | "craft_stone_sword" => "wood_stone_tools",
        "mine_coal" | "mine_iron" => "iron",
        "craft_furnace" | "smelt_iron" | "craft_iron_pickaxe" | "craft_bucket" | "get_water_buckets" | "get_flint_and_steel"
        | "gather_build_blocks" => "kit_craft",
        "build_nether_portal" | "enter_nether" => "portal",
        "find_fortress" => "fortress",
        "kill_blaze" => "blaze_rods",
        "pearls" => "pearls",
        "stronghold" => "stronghold",
        "enter_end" => "end_entry",
        "crystals" => "crystals",
        "dragon" => "dragon",
        _ => "other",
    }
}

// ── params.json: budgets, stall windows, bandit arms ─────────────────────────────────────────

/// Starting budgets (seconds) per step, from Part 3's skill budgets, until a step has ten passes
/// and `params.json` carries its measured 80th percentile.
fn default_budget(step_id: &str) -> f64 {
    match step_id {
        "gather_wood" => 360.0,
        "mine_iron" => 1200.0,
        "mine_coal" => 600.0,
        "build_nether_portal" => 1500.0,
        "find_fortress" => 900.0,
        "kill_blaze" => 720.0,
        // A perch comes every few minutes; 600 s saw one (late10-v2h).
        "dragon" => 900.0,
        "crystals" => 1800.0,
        _ => 600.0,
    }
}

fn default_stall(step_id: &str) -> f64 {
    match step_id {
        // The descent and at-depth tunnel legitimately go minutes between frame/lava gains; the
        // ratchet includes depth, but the at-depth search is flat.
        "build_nether_portal" | "find_fortress" => 300.0,
        _ => 180.0,
    }
}

fn read_params() -> Value {
    std::fs::read_to_string(PARAMS).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or(json!({}))
}

pub fn budget_s(step_id: &str) -> f64 {
    read_params()["budgets"][step_id].as_f64().unwrap_or_else(|| default_budget(step_id))
}

pub fn stall_s(step_id: &str) -> f64 {
    let p = read_params();
    p["stall_s"][step_id].as_f64().or_else(|| p["stall_s"]["default"].as_f64()).unwrap_or_else(|| default_stall(step_id))
}

/// Exclusive lock on params.json for read-modify-write across bots (create_new lock file; a lock
/// older than 5 s is stale and taken over).
fn with_params_lock<T>(f: impl FnOnce() -> T) -> T {
    let lock = format!("{PARAMS}.lock");
    for _ in 0..100 {
        if std::fs::OpenOptions::new().write(true).create_new(true).open(&lock).is_ok() {
            let out = f();
            let _ = std::fs::remove_file(&lock);
            return out;
        }
        if let Ok(m) = std::fs::metadata(&lock).and_then(|m| m.modified()) {
            if m.elapsed().map(|e| e.as_secs() >= 5).unwrap_or(false) {
                let _ = std::fs::remove_file(&lock);
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    f()
}

// Marsaglia–Tsang Gamma(shape, 1) and Beta(a, b) = X / (X + Y). Same algorithm as
// scripts/ml/bandit.ts; both are checked against mean(Beta(3,7)) ≈ 0.3.
fn std_normal(rng: &mut impl rand::Rng) -> f64 {
    let u1: f64 = rng.gen_range(f64::MIN_POSITIVE..1.0);
    let u2: f64 = rng.gen();
    (-2.0 * u1.ln()).sqrt() * (2.0 * std::f64::consts::PI * u2).cos()
}

pub fn gamma(shape: f64, rng: &mut impl rand::Rng) -> f64 {
    if shape < 1.0 {
        let u: f64 = rng.gen_range(f64::MIN_POSITIVE..1.0);
        return gamma(shape + 1.0, rng) * u.powf(1.0 / shape);
    }
    let d = shape - 1.0 / 3.0;
    let c = 1.0 / (9.0 * d).sqrt();
    loop {
        let x = std_normal(rng);
        let v = (1.0 + c * x).powi(3);
        if v <= 0.0 {
            continue;
        }
        let u: f64 = rng.gen_range(f64::MIN_POSITIVE..1.0);
        if u.ln() < 0.5 * x * x + d - d * v + d * v.ln() {
            return d * v;
        }
    }
}

pub fn beta(a: f64, b: f64, rng: &mut impl rand::Rng) -> f64 {
    let x = gamma(a, rng);
    let y = gamma(b, rng);
    x / (x + y)
}

/// Arms chosen for the current attempt: param name → arm key (as written in params.json).
static CHOSEN: Mutex<BTreeMap<String, String>> = Mutex::new(BTreeMap::new());

/// The current attempt's arm for `name`, if the bandit for it is enabled and applies to this
/// skill; otherwise `None` and the caller keeps its scripted default.
pub fn param(name: &str) -> Option<String> {
    CHOSEN.lock().unwrap().get(name).cloned()
}

pub fn param_i32(name: &str, default: i32) -> i32 {
    param(name).and_then(|v| v.parse().ok()).unwrap_or(default)
}

/// Thompson draw for every enabled parameter whose `skills` include `skill`.
fn draw_arms(skill: &str) -> BTreeMap<String, String> {
    let p = read_params();
    let mut rng = rand::thread_rng();
    let mut out = BTreeMap::new();
    if let Some(params) = p["params"].as_object() {
        for (name, spec) in params {
            if !spec["enabled"].as_bool().unwrap_or(false) {
                continue;
            }
            let applies = spec["skills"].as_array().map(|a| a.iter().any(|s| s.as_str() == Some(skill))).unwrap_or(false);
            if !applies {
                continue;
            }
            let Some(arms) = spec["arms"].as_object() else { continue };
            let best = arms
                .iter()
                .map(|(k, ab)| {
                    let a = ab[0].as_f64().unwrap_or(1.0).max(0.01);
                    let b = ab[1].as_f64().unwrap_or(1.0).max(0.01);
                    (k.clone(), beta(a, b, &mut rng))
                })
                .max_by(|x, y| x.1.total_cmp(&y.1));
            if let Some((k, _)) = best {
                out.insert(name.clone(), k);
            }
        }
    }
    out
}

/// Posterior update: `a += 1` on ok, `b += 1` otherwise, for each chosen arm.
fn update_arms(chosen: &BTreeMap<String, String>, ok: bool) {
    if chosen.is_empty() {
        return;
    }
    with_params_lock(|| {
        let mut p = read_params();
        for (name, arm) in chosen {
            if let Some(ab) = p["params"][name]["arms"][arm].as_array_mut() {
                let i = if ok { 0 } else { 1 };
                let v = ab.get(i).and_then(|v| v.as_f64()).unwrap_or(1.0) + 1.0;
                if let Some(slot) = ab.get_mut(i) {
                    *slot = json!(v);
                }
            }
        }
        if let Ok(s) = serde_json::to_string_pretty(&p) {
            let tmp = format!("{PARAMS}.tmp");
            if std::fs::write(&tmp, s).is_ok() {
                let _ = std::fs::rename(&tmp, PARAMS);
            }
        }
    });
}

// ── progress metric (Part 8): a ratchet toward the postcondition ─────────────────────────────

/// Higher is closer to the step's postcondition. Only quantities the step cannot raise without
/// advancing (not blocks dug, not distance walked). The portal folds in depth so the descent
/// counts until the lava band, then lava held, then frame obsidian.
pub fn progress(bot: &Bot, step_id: &str, s: &GameState) -> f64 {
    let iv = &s.inventory;
    match step_id {
        "gather_wood" => iv.logs as f64 + iv.planks as f64 / 4.0,
        "mine_stone" | "gather_build_blocks" => iv.cobblestone as f64,
        "mine_iron" => (iv.iron_ore + iv.iron_ingots) as f64,
        "mine_coal" => iv.coal as f64,
        "kill_blaze" => iv.blaze_rods as f64,
        "build_nether_portal" => {
            let obs = crate::tasks::portal::frame_obsidian_count(bot) as f64;
            let lava = count_items(bot, "lava_bucket") as f64;
            let depth = (64.0 - s.position.1).clamp(0.0, 114.0);
            obs * 1000.0 + lava * 50.0 + depth
        }
        _ => {
            (iv.logs + iv.planks + iv.sticks + iv.cobblestone + iv.coal + iv.iron_ore + iv.iron_ingots + iv.crafting_tables
                + iv.buckets + iv.water_buckets + iv.flint_and_steel + iv.iron_pickaxes) as f64
                + s.equipment.pickaxe_tier().rank() as f64 * 100.0
        }
    }
}

// ── alternatives (Part 8): what the task changes after a stall ───────────────────────────────

static ALT: [AtomicU32; 4] = [AtomicU32::new(0), AtomicU32::new(0), AtomicU32::new(0), AtomicU32::new(0)];

fn alt_slot(step_id: &str) -> usize {
    match step_id {
        "gather_wood" => 0,
        "mine_iron" | "mine_coal" | "mine_stone" => 1,
        "build_nether_portal" => 2,
        _ => 3,
    }
}

/// How many times this step's attempts were abandoned for no progress. Tasks read it to pick a
/// different heading / band / site (e.g. heading index = base + alternatives).
pub fn alternatives(step_id: &str) -> u32 {
    ALT[alt_slot(step_id)].load(Ordering::Relaxed)
}

fn bump_alternative(step_id: &str) {
    ALT[alt_slot(step_id)].fetch_add(1, Ordering::Relaxed);
}

/// What the next attempt changes after a budget/stall cut (logged with the cut, cycle 5 Phase 1 (iii)).
/// The portal has none: under 6b-safe the site/pool choice is frozen (decision 1).
pub fn alternative_taken(step_id: &str) -> &'static str {
    match step_id {
        "gather_wood" => "sweep heading +2",
        "mine_iron" => "strip and range direction +1",
        _ => "none (re-derive only)",
    }
}

// ── context features (exposure-only) ─────────────────────────────────────────────────────────

const HOSTILES: [&str; 14] = [
    "zombie", "skeleton", "creeper", "spider", "enderman", "witch", "drowned", "husk", "stray", "blaze", "ghast", "piglin_brute",
    "wither_skeleton", "magma_cube",
];

pub fn context(bot: &Bot, s: &GameState, mem_ore: i64) -> Value {
    let p = bot.entity.position;
    let hostiles = bot
        .entities
        .values()
        .filter(|e| e.name.as_deref().map(|n| HOSTILES.contains(&n.trim_start_matches("minecraft:"))).unwrap_or(false))
        .filter(|e| {
            let q = e.position;
            ((q.x - p.x).powi(2) + (q.y - p.y).powi(2) + (q.z - p.z).powi(2)).sqrt() <= 24.0
        })
        .count();
    json!({
        "y": s.position.1.floor() as i64,
        "health": s.health,
        "food": s.food,
        "time_of_day": bot.time.time_of_day,
        "dim": s.world.dimension,
        "cobble": s.inventory.cobblestone,
        "iron": s.inventory.iron_ore + s.inventory.iron_ingots,
        "pick_rank": s.equipment.pickaxe_tier().rank(),
        "buckets": s.inventory.buckets,
        "lava_buckets": count_items(bot, "lava_bucket"),
        "hostiles_seen": hostiles,
        "lava_cells_8": bot.find_exposed_blocks("lava", 8, 512).len(),
        "water_cells_8": bot.find_exposed_blocks("water", 8, 512).len(),
        "ore_sightings_memory": mem_ore,
        "in_water": crate::bot_utils::feet_in_water(bot),
        "head_in_water": crate::bot_utils::head_in_water(bot),
        "light": Value::Null,
        "air": Value::Null,
    })
}

// ── rows ─────────────────────────────────────────────────────────────────────────────────────

/// The per-process event file: `data/attempts/<run_id>-<bot>.jsonl`, one writer each. One shared
/// file with many appending processes interleaved rows (cycle 4: 118 of 27,011 lines unparseable).
/// `ATTEMPTS_DIR` overrides the directory (tests). The old `data/attempts.jsonl` stays read-only.
fn attempts_path(row: &Value) -> std::path::PathBuf {
    let dir = std::env::var("ATTEMPTS_DIR").ok().filter(|s| !s.is_empty()).unwrap_or_else(|| "data/attempts".to_string());
    let clean = |s: &str| s.chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '+' { c } else { '_' }).collect::<String>();
    let run = row.get("run_id").and_then(Value::as_str).unwrap_or("adhoc");
    let bot = row.get("bot").and_then(Value::as_str).unwrap_or("bot");
    std::path::Path::new(&dir).join(format!("{}-{}.jsonl", clean(run), clean(bot)))
}

/// Append one row to this process's event file (and race.db when attached by the caller). The row
/// goes out as a single write of one line.
pub fn write_row(row: &Value) {
    use std::io::Write;
    let path = attempts_path(row);
    if let Some(d) = path.parent() {
        let _ = std::fs::create_dir_all(d);
    }
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&path) {
        let _ = f.write_all(format!("{row}\n").as_bytes());
    }
}

fn base_row(bot_name: &str, skill: &str, step_id: &str) -> Map<String, Value> {
    let mut m = Map::new();
    m.insert("run_id".into(), json!(env_or("RACE_ID", &env_or("GYM_RUN", "adhoc"))));
    m.insert("bot_impl".into(), json!("rs"));
    m.insert("build".into(), json!(env_or("BUILD", "dev")));
    m.insert("world_seed".into(), std::env::var("WORLD_SEED").ok().and_then(|s| s.parse::<i64>().ok()).map(Value::from).unwrap_or(Value::Null));
    m.insert("skill".into(), json!(skill));
    m.insert("step_id".into(), json!(step_id));
    m.insert("source".into(), json!(source()));
    m.insert("bot".into(), json!(bot_name));
    m
}

/// A guarded-primitive row (`lava_safe_move`, `leave_water`, `station_refill`, `tunnel_step`).
pub fn primitive_row(bot: &Bot, primitive: &str, step_id: &str, start: Instant, outcome: &str, reason: &str, extra: Value) {
    let mut m = base_row(&env_or("MC_USERNAME", "bot"), primitive, step_id);
    let p = bot.entity.position;
    m.insert("start_ms".into(), json!(now_ms() - start.elapsed().as_millis() as i64));
    m.insert("duration_s".into(), json!((start.elapsed().as_secs_f64() * 10.0).round() / 10.0));
    m.insert("outcome".into(), json!(outcome));
    m.insert("reason".into(), json!(reason));
    m.insert("death_cause".into(), Value::Null);
    m.insert("pos".into(), json!([p.x.floor() as i64, p.y.floor() as i64, p.z.floor() as i64]));
    m.insert("params".into(), json!({}));
    m.insert("context".into(), extra);
    write_row(&Value::Object(m));
}

/// One open step attempt, owned by the step loop.
pub struct Attempt {
    pub step_id: String,
    pub skill: &'static str,
    start: Instant,
    start_ms: i64,
    start_pos: [i64; 3],
    chosen: BTreeMap<String, String>,
    context: Value,
    best: f64,
    best_at: Instant,
    deaths_at_start: u32,
    last_fail: String,
    same_fail: u32,
}

/// Why the loop should end the current attempt now (and start a fresh one of the same step).
pub enum Verdict {
    Continue,
    Stall(String),
}

impl Attempt {
    pub fn start(bot: &Bot, step_id: &str, s: &GameState, mem_ore: i64) -> Attempt {
        let skill = skill_of(step_id);
        let chosen = draw_arms(skill);
        *CHOSEN.lock().unwrap() = chosen.clone();
        let p = s.position;
        let a = Attempt {
            step_id: step_id.to_string(),
            skill,
            start: Instant::now(),
            start_ms: now_ms(),
            start_pos: [p.0.floor() as i64, p.1.floor() as i64, p.2.floor() as i64],
            chosen,
            context: context(bot, s, mem_ore),
            best: progress(bot, step_id, s),
            best_at: Instant::now(),
            deaths_at_start: bot.deaths,
            last_fail: String::new(),
            same_fail: 0,
        };
        a.checkpoint();
        a
    }

    /// After every step call: ratchet the progress metric and apply the stall, budget and
    /// same-failure rules (Part 8).
    pub fn observe(&mut self, bot: &Bot, s: &GameState, success: bool, message: &str) -> Verdict {
        let now = progress(bot, &self.step_id, s);
        if now > self.best + 1e-9 {
            self.best = now;
            self.best_at = Instant::now();
        }
        if success {
            self.same_fail = 0;
        } else if message == self.last_fail {
            self.same_fail += 1;
        } else {
            self.last_fail = message.to_string();
            self.same_fail = 1;
        }
        self.checkpoint();
        let stall = stall_s(&self.step_id);
        if self.best_at.elapsed().as_secs_f64() > stall {
            return Verdict::Stall(format!("no progress in {:.0} s", stall));
        }
        let budget = budget_s(&self.step_id);
        if self.start.elapsed().as_secs_f64() > budget {
            return Verdict::Stall(format!("over budget {:.0} s", budget));
        }
        if self.same_fail >= 4 {
            return Verdict::Stall(format!("same failure 4x: {}", self.last_fail));
        }
        Verdict::Continue
    }

    /// Close the attempt: write the row, update the chosen arms, and on a timeout bump the step's
    /// alternative so the next attempt changes something it controls.
    pub fn finish(self, bot: &Bot, outcome: &str, reason: &str, race: Option<&mut crate::telemetry::RaceLog>) {
        let deaths = bot.deaths.saturating_sub(self.deaths_at_start);
        let mut m = base_row(&env_or("MC_USERNAME", "bot"), self.skill, &self.step_id);
        m.insert("start_ms".into(), json!(self.start_ms));
        m.insert("duration_s".into(), json!((self.start.elapsed().as_secs_f64() * 10.0).round() / 10.0));
        m.insert("outcome".into(), json!(outcome));
        m.insert("reason".into(), json!(reason));
        m.insert("deaths".into(), json!(deaths));
        m.insert("death_cause".into(), if deaths > 0 { json!("see server log") } else { Value::Null });
        m.insert("pos".into(), json!(self.start_pos));
        m.insert("progress_best".into(), json!(self.best));
        m.insert("params".into(), json!(self.chosen));
        m.insert("context".into(), self.context.clone());
        if outcome == "timeout" {
            // The cut and the alternative the next attempt takes, in the same row.
            m.insert("alternative".into(), json!({"index": alternatives(&self.step_id) + 1, "takes": alternative_taken(&self.step_id)}));
            println!("    cut: {} after {:.0} s ({reason}) → next attempt: {}", self.step_id, self.start.elapsed().as_secs_f64(), alternative_taken(&self.step_id));
        }
        let row = Value::Object(m);
        write_row(&row);
        if let Some(r) = race {
            r.attempt(&row);
        }
        update_arms(&self.chosen, outcome == "ok");
        if outcome == "timeout" {
            bump_alternative(&self.step_id);
        }
        let _ = std::fs::remove_file(checkpoint_path());
    }

    /// Persist the open attempt so a killed process's attempt is closed (as `failed`, "process
    /// ended") by the relaunch instead of vanishing — every launch must have a row (Part 10.7).
    fn checkpoint(&self) {
        let v = json!({
            "step_id": self.step_id, "skill": self.skill, "start_ms": self.start_ms, "pos": self.start_pos,
            "params": self.chosen, "context": self.context, "best": self.best, "updated_ms": now_ms(),
            "run_id": env_or("RACE_ID", &env_or("GYM_RUN", "adhoc")), "build": env_or("BUILD", "dev"),
        });
        let _ = std::fs::write(checkpoint_path(), v.to_string());
    }
}

fn checkpoint_path() -> String {
    format!("data/.attempt-{}.json", env_or("MC_USERNAME", "bot"))
}

/// At startup: an attempt checkpoint left by a killed process becomes a `failed` row.
pub fn close_orphan(race: Option<&mut crate::telemetry::RaceLog>) {
    let Ok(s) = std::fs::read_to_string(checkpoint_path()) else { return };
    let Ok(v) = serde_json::from_str::<Value>(&s) else { return };
    let step = v["step_id"].as_str().unwrap_or("?").to_string();
    let mut m = base_row(&env_or("MC_USERNAME", "bot"), skill_of(&step), &step);
    let start = v["start_ms"].as_i64().unwrap_or(0);
    let upd = v["updated_ms"].as_i64().unwrap_or(start);
    m.insert("run_id".into(), v["run_id"].clone());
    m.insert("build".into(), v["build"].clone());
    m.insert("start_ms".into(), json!(start));
    m.insert("duration_s".into(), json!(((upd - start) as f64 / 100.0).round() / 10.0));
    m.insert("outcome".into(), json!("failed"));
    m.insert("reason".into(), json!("process ended (killed or relaunched) mid-attempt"));
    m.insert("death_cause".into(), Value::Null);
    m.insert("pos".into(), v["pos"].clone());
    m.insert("progress_best".into(), v["best"].clone());
    m.insert("params".into(), v["params"].clone());
    m.insert("context".into(), v["context"].clone());
    let row = Value::Object(m);
    write_row(&row);
    if let Some(r) = race {
        r.attempt(&row);
    }
    let _ = std::fs::remove_file(checkpoint_path());
}

/// Gym trial start: draw this trial's bandit arms (the gym runs its own trial loop, not the step loop).
pub fn gym_begin(step_id: &str) {
    *CHOSEN.lock().unwrap() = draw_arms(skill_of(step_id));
}

/// Gym trial → one attempt row, and the posterior update for the arms gym_begin drew.
pub fn gym_row(bot: &Bot, slug: &str, step_id: &str, start: Instant, outcome: &str, reason: &str, deaths: u32) {
    let mut m = base_row(&env_or("MC_USERNAME", "bot"), skill_of(step_id), step_id);
    let p = bot.entity.position;
    m.insert("gym_slug".into(), json!(slug));
    m.insert("start_ms".into(), json!(now_ms() - start.elapsed().as_millis() as i64));
    m.insert("duration_s".into(), json!((start.elapsed().as_secs_f64() * 10.0).round() / 10.0));
    m.insert("outcome".into(), json!(outcome));
    m.insert("reason".into(), json!(reason));
    m.insert("deaths".into(), json!(deaths));
    m.insert("death_cause".into(), if deaths > 0 { json!("see server log") } else { Value::Null });
    m.insert("pos".into(), json!([p.x.floor() as i64, p.y.floor() as i64, p.z.floor() as i64]));
    let chosen = CHOSEN.lock().unwrap().clone();
    m.insert("params".into(), json!(chosen));
    m.insert("context".into(), Value::Null);
    write_row(&Value::Object(m));
    update_arms(&chosen, outcome == "ok");
}

#[cfg(test)]
mod tests {
    /// Decision 5 (cycle 5): two concurrent writers produce zero unparseable lines. Each process
    /// (here: each thread with its own bot name) gets its own file; every line parses.
    #[test]
    fn two_writers_zero_unparseable() {
        let dir = std::env::temp_dir().join(format!("attempts-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::env::set_var("ATTEMPTS_DIR", &dir);
        let writers: Vec<_> = ["bot-a", "bot-b"]
            .iter()
            .map(|bot| {
                let bot = bot.to_string();
                std::thread::spawn(move || {
                    for i in 0..2000 {
                        super::write_row(&serde_json::json!({ "run_id": "test-run", "bot": bot, "i": i, "pad": "x".repeat(200) }));
                    }
                })
            })
            .collect();
        for w in writers {
            w.join().unwrap();
        }
        std::env::remove_var("ATTEMPTS_DIR");
        let (mut good, mut bad) = (0, 0);
        for f in std::fs::read_dir(&dir).unwrap() {
            for line in std::fs::read_to_string(f.unwrap().path()).unwrap().lines() {
                if serde_json::from_str::<serde_json::Value>(line).is_ok() {
                    good += 1;
                } else {
                    bad += 1;
                }
            }
        }
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(bad, 0, "unparseable lines");
        assert_eq!(good, 4000);
    }

    /// Cross-implementation agreement with scripts/ml/bandit.ts: mean of 10,000 Beta(3,7) draws ≈ 0.3.
    #[test]
    fn beta_3_7_mean() {
        let mut rng = rand::thread_rng();
        let n = 10_000;
        let mean = (0..n).map(|_| super::beta(3.0, 7.0, &mut rng)).sum::<f64>() / n as f64;
        assert!((mean - 0.3).abs() < 0.02, "mean {mean}");
    }

    #[test]
    fn skills_map() {
        assert_eq!(super::skill_of("mine_iron"), "iron");
        assert_eq!(super::skill_of("build_nether_portal"), "portal");
        assert_eq!(super::skill_of("gather_wood"), "wood_stone_tools");
    }
}
