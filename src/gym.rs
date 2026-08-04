//! GYM — run each speedrun sub-task in isolation across RANDOM real terrain and
//! record pass/duration/xyz to `data/gym.db`, so we can measure how each step
//! performs across the world and perfect the terrain-hard ones BEFORE chaining
//! them in a full race. Port of steve's gym (`src/lib/steve/gym/*`), extended
//! THROUGH the portal → nether (steve's gym stops at flint-and-steel).
//!
//! Everything is Rust: `GYM=<slug> GYM_TRIALS=<n>` loops N trials on ONE connected
//! bot (self-RCON `clear`/`give`/`spreadplayers` each trial, self-record), and
//! `GYM=report` prints the per-slug pass table from `gym.db`. No shell harness.
//!
//! RCON reaches the shared server over the SSH tunnel to `localhost:25575`
//! (`ssh -fN -L 25575:127.0.0.1:25575 bridger@144.24.32.76`). Never wipes the
//! world; forceloads are scoped to each trial's chunk and removed after.

use std::time::{Duration, Instant};

use rusqlite::{params, Connection};

use crate::bot::Bot;
use crate::bot_utils::count_items;
use crate::memory::WorldMemory;
use crate::rcon::{RconClient, RconOptions};
use crate::state::sync_from_bot;
use crate::types::GameState;

/// How a gym trial positions the bot before running the step.
#[derive(Clone, Copy)]
pub enum GymSetup {
    /// Random surface teleport (`spreadplayers` in 0..10k) — the terrain-variance test.
    RandomSurface,
}

pub struct GymStep {
    pub slug: &'static str,
    pub label: &'static str,
    pub order: i32,
    /// Items to `give` before the run (RCON `give` args, e.g. `"oak_log 8"`).
    pub prereq: &'static [&'static str],
    /// Pipeline step to execute each attempt (dispatched via `steps::execute_step`).
    pub step_id: &'static str,
    pub timeout_secs: u64,
    /// Pass check on the live bot; `None` → the `step_id`'s own `is_complete`.
    pub custom_pass: Option<fn(&Bot, &GameState) -> bool>,
    pub setup: GymSetup,
}

fn passes(step: &GymStep, bot: &Bot, s: &GameState) -> bool {
    if let Some(f) = step.custom_pass {
        return f(bot, s);
    }
    crate::steps::STEPS
        .iter()
        .find(|st| st.id == step.step_id)
        .map(|st| (st.is_complete)(s))
        .unwrap_or(false)
}

/// The gym registry — the 19-step pipeline plus the portal→nether coverage. Prereqs
/// mirror `isolation-test.sh::setup_prereqs` / steve's `gym/registry.ts`: what the bot
/// would hold ENTERING that step. Random-terrain teleport is what exposes the
/// terrain-dependent failures a clean arena hides.
pub static GYM_STEPS: &[GymStep] = &[
    GymStep { slug: "gather_wood", label: "Gather Wood", order: 1, prereq: &[], step_id: "gather_wood", timeout_secs: 120, custom_pass: None, setup: GymSetup::RandomSurface },
    GymStep { slug: "craft_planks", label: "Craft Planks", order: 2, prereq: &["oak_log 8"], step_id: "craft_planks", timeout_secs: 60, custom_pass: None, setup: GymSetup::RandomSurface },
    GymStep { slug: "craft_crafting_table", label: "Craft Table", order: 3, prereq: &["oak_planks 8"], step_id: "craft_crafting_table", timeout_secs: 60, custom_pass: None, setup: GymSetup::RandomSurface },
    GymStep { slug: "craft_sticks", label: "Craft Sticks", order: 4, prereq: &["oak_planks 8"], step_id: "craft_sticks", timeout_secs: 60, custom_pass: None, setup: GymSetup::RandomSurface },
    GymStep { slug: "craft_wooden_pickaxe", label: "Craft Wooden Pickaxe", order: 5, prereq: &["oak_planks 8", "stick 8", "crafting_table 1"], step_id: "craft_wooden_pickaxe", timeout_secs: 60, custom_pass: None, setup: GymSetup::RandomSurface },
    GymStep { slug: "mine_stone", label: "Mine Cobblestone", order: 6, prereq: &["wooden_pickaxe 1"], step_id: "mine_stone", timeout_secs: 150, custom_pass: None, setup: GymSetup::RandomSurface },
    GymStep { slug: "craft_stone_pickaxe", label: "Craft Stone Pickaxe", order: 7, prereq: &["cobblestone 8", "stick 8", "crafting_table 1"], step_id: "craft_stone_pickaxe", timeout_secs: 60, custom_pass: None, setup: GymSetup::RandomSurface },
    GymStep { slug: "craft_stone_sword", label: "Craft Stone Sword", order: 8, prereq: &["cobblestone 4", "stick 4", "crafting_table 1"], step_id: "craft_stone_sword", timeout_secs: 60, custom_pass: None, setup: GymSetup::RandomSurface },
    GymStep { slug: "craft_furnace", label: "Craft Furnace", order: 9, prereq: &["cobblestone 16", "crafting_table 1"], step_id: "craft_furnace", timeout_secs: 60, custom_pass: None, setup: GymSetup::RandomSurface },
    GymStep { slug: "mine_coal", label: "Mine Coal", order: 10, prereq: &["stone_pickaxe 1"], step_id: "mine_coal", timeout_secs: 320, custom_pass: None, setup: GymSetup::RandomSurface },
    // Gym tests the CAPABILITY (reach + mine iron), not the full race amount: the
    // pipeline is_complete wants 11 iron, unrealistic in one cold-spawn trial — steve's
    // gym used 3. The race accumulates the rest via repeated step runs.
    GymStep { slug: "mine_iron", label: "Mine Iron Ore", order: 11, prereq: &["stone_pickaxe 1"], step_id: "mine_iron", timeout_secs: 320, custom_pass: Some(|bot, _| count_items(bot, "raw_iron") + count_items(bot, "iron_ingot") >= 3), setup: GymSetup::RandomSurface },
    GymStep { slug: "smelt_iron", label: "Smelt Iron", order: 12, prereq: &["raw_iron 11", "coal 8", "furnace 1"], step_id: "smelt_iron", timeout_secs: 150, custom_pass: None, setup: GymSetup::RandomSurface },
    GymStep { slug: "craft_iron_pickaxe", label: "Craft Iron Pickaxe", order: 13, prereq: &["iron_ingot 3", "stick 2", "crafting_table 1"], step_id: "craft_iron_pickaxe", timeout_secs: 60, custom_pass: None, setup: GymSetup::RandomSurface },
    GymStep { slug: "craft_bucket", label: "Craft Buckets", order: 14, prereq: &["iron_ingot 6", "crafting_table 1"], step_id: "craft_bucket", timeout_secs: 60, custom_pass: None, setup: GymSetup::RandomSurface },
    GymStep { slug: "get_water_buckets", label: "Fill Water Buckets", order: 15, prereq: &["bucket 2"], step_id: "get_water_buckets", timeout_secs: 90, custom_pass: None, setup: GymSetup::RandomSurface },
    GymStep { slug: "get_flint_and_steel", label: "Get Flint and Steel", order: 16, prereq: &["iron_ingot 2", "crafting_table 1"], step_id: "get_flint_and_steel", timeout_secs: 260, custom_pass: None, setup: GymSetup::RandomSurface },
    GymStep { slug: "gather_build_blocks", label: "Gather Build Blocks", order: 17, prereq: &["stone_pickaxe 1"], step_id: "gather_build_blocks", timeout_secs: 120, custom_pass: None, setup: GymSetup::RandomSurface },
    // ── portal → nether (steve's gym lacks these) ──────────────────────────────
    // reach_lava: the terrain-hard descent→exposed-source→drop-to-source+1→scoop.
    // Runs the portal step but passes the moment a lava bucket is filled.
    GymStep { slug: "reach_lava", label: "Reach + Scoop Lava", order: 18, prereq: &["iron_pickaxe 1", "bucket 3", "water_bucket 1", "cobblestone 64"], step_id: "build_nether_portal", timeout_secs: 800, custom_pass: Some(|bot, _| count_items(bot, "lava_bucket") >= 1), setup: GymSetup::RandomSurface },
    // Capstone: full portal kit, random terrain, pass = we're in the Nether.
    GymStep { slug: "to_nether", label: "Portal → Nether (capstone)", order: 19, prereq: &["iron_pickaxe 1", "bucket 2", "water_bucket 1", "flint_and_steel 1", "cobblestone 64"], step_id: "build_nether_portal", timeout_secs: 1500, custom_pass: Some(|_, s| s.world.in_nether()), setup: GymSetup::RandomSurface },
];

fn env(key: &str, default: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| default.to_string())
}

// ── results store (data/gym.db) — mirrors memory.rs's rusqlite pattern ──────────

pub struct GymStore {
    conn: Connection,
}

impl GymStore {
    pub fn open() -> Self {
        let _ = std::fs::create_dir_all("data");
        let conn = Connection::open("data/gym.db").expect("open gym db");
        conn.execute_batch(
            "PRAGMA journal_mode=WAL;
             CREATE TABLE IF NOT EXISTS gym_runs(
                 id INTEGER PRIMARY KEY AUTOINCREMENT,
                 ts INTEGER NOT NULL,
                 slug TEXT NOT NULL,
                 pass INTEGER NOT NULL,
                 duration_ms INTEGER NOT NULL,
                 x INTEGER, y INTEGER, z INTEGER,
                 prereq TEXT, message TEXT
             );
             CREATE INDEX IF NOT EXISTS gym_runs_slug_ts ON gym_runs(slug, ts);",
        )
        .expect("init gym schema");
        GymStore { conn }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn record(
        &self,
        slug: &str,
        pass: bool,
        duration_ms: i64,
        x: i32,
        y: i32,
        z: i32,
        prereq: &[&str],
        message: &str,
    ) {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0);
        let msg: String = message.chars().take(400).collect();
        let _ = self.conn.execute(
            "INSERT INTO gym_runs(ts, slug, pass, duration_ms, x, y, z, prereq, message)
             VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9)",
            params![now, slug, pass as i32, duration_ms, x, y, z, prereq.join(", "), msg],
        );
    }
}

/// Print the per-slug pass table (the CLI companion to the dashboard).
pub fn report() {
    let store = GymStore::open();
    let mut stmt = store
        .conn
        .prepare(
            "SELECT slug, COUNT(*), CAST(ROUND(100.0*SUM(pass)/COUNT(*)) AS INT),
                    CAST(ROUND(AVG(duration_ms)/1000.0) AS INT)
             FROM gym_runs GROUP BY slug ORDER BY 3 ASC",
        )
        .expect("prepare report");
    let rows = stmt
        .query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, i64>(2)?,
                r.get::<_, i64>(3)?,
            ))
        })
        .expect("query report");
    println!("\n=== GYM report (lowest pass% first) ===");
    println!("{:<24} {:>5} {:>6} {:>7}", "slug", "runs", "pass%", "avg_s");
    for row in rows.flatten() {
        let (slug, runs, pct, avg_s) = row;
        println!("{slug:<24} {runs:>5} {pct:>5}% {avg_s:>6}s");
    }
    println!();
}

// ── the runner ──────────────────────────────────────────────────────────────

/// Run `trials` gym trials of `slug` on this bot, recording each to `gym.db`.
pub async fn run(
    bot: &mut Bot<'_>,
    memory: &mut WorldMemory,
    slug: &str,
    trials: u32,
) -> std::io::Result<()> {
    let Some(step) = GYM_STEPS.iter().find(|s| s.slug == slug) else {
        println!("GYM: unknown slug '{slug}'. Known: {}", GYM_STEPS.iter().map(|s| s.slug).collect::<Vec<_>>().join(", "));
        return Ok(());
    };
    let mut rcon = RconClient::connect(RconOptions {
        host: env("RCON_HOST", "localhost"),
        port: env("RCON_PORT", "25575").parse().unwrap_or(25575),
        password: env("RCON_PASS", "minecraft-test-rcon"),
        ..Default::default()
    })
    .await
    .map_err(|e| std::io::Error::other(format!("gym RCON connect failed: {e} (is the tunnel to 25575 up?)")))?;

    let name = bot.username().to_string();
    let _ = rcon.command(&format!("op {name}")).await; // so respawn/tp behave; harmless if already op
    let store = GymStore::open();

    for trial in 0..trials {
        println!("[gym] === {} trial {}/{} ===", step.slug, trial + 1, trials);
        run_one_trial(bot, memory, &mut rcon, &store, step).await;
    }
    Ok(())
}

/// Steve's "run random for a while" sweep: each trial picks a RANDOM pipeline step.
/// Launch several of these as separate bots (`rust-gym-001..004`) for 4-at-a-time
/// coverage, run for a while, then `GYM=report` and FOCUS on the low-pass slugs.
/// Excludes the two expensive portal steps (`reach_lava` 600s / `to_nether` 900s) —
/// run those in dedicated focused batches so the sweep stays fast and broad.
pub async fn run_random(
    bot: &mut Bot<'_>,
    memory: &mut WorldMemory,
    trials: u32,
) -> std::io::Result<()> {
    let mut rcon = RconClient::connect(RconOptions {
        host: env("RCON_HOST", "localhost"),
        port: env("RCON_PORT", "25575").parse().unwrap_or(25575),
        password: env("RCON_PASS", "minecraft-test-rcon"),
        ..Default::default()
    })
    .await
    .map_err(|e| std::io::Error::other(format!("gym RCON connect failed: {e} (tunnel to 25575 up?)")))?;
    let name = bot.username().to_string();
    let _ = rcon.command(&format!("op {name}")).await;
    let store = GymStore::open();
    let pool: Vec<&GymStep> = GYM_STEPS.iter().filter(|s| s.timeout_secs <= 200).collect();
    for trial in 0..trials {
        let step = pool[rand::Rng::gen_range(&mut rand::thread_rng(), 0..pool.len())];
        println!("[gym] === random trial {}/{}: {} ===", trial + 1, trials, step.slug);
        run_one_trial(bot, memory, &mut rcon, &store, step).await;
    }
    Ok(())
}

/// One gym trial: setup (clear/give/random-tp) → run the step to pass/timeout →
/// record to gym.db → release the forceload.
async fn run_one_trial(
    bot: &mut Bot<'_>,
    memory: &mut WorldMemory,
    rcon: &mut RconClient,
    store: &GymStore,
    step: &GymStep,
) {
    let name = bot.username().to_string();
    // Each trial teleports far from the last — POIs from prior trials (a remembered
    // table/ore across the map) would send tasks walking 18s toward nothing. Fresh slate.
    memory.clear_pois();
    let (gx, gy, gz, cx, cz) = setup_trial(bot, rcon, &name, step).await;
    println!("[gym:{}] @ {gx},{gy},{gz} — running (timeout {}s)", step.slug, step.timeout_secs);

    let t0 = Instant::now();
    let deadline = t0 + Duration::from_secs(step.timeout_secs);
    let mut pass = false;
    let mut last_msg = String::new();
    let mut attempts = 0u32;
    loop {
        bot.wait_ticks(6).await.ok();
        let s = sync_from_bot(bot);
        if passes(step, bot, &s) {
            pass = true;
            break;
        }
        if Instant::now() > deadline {
            last_msg = format!("gym timeout ({attempts} attempts)");
            break;
        }
        if !s.alive {
            bot.respawn().await.ok();
            bot.wait_ticks(40).await.ok();
            continue;
        }
        if crate::survival::handle_survival(bot, memory).await {
            continue;
        }
        attempts += 1;
        // HARD timeout around the task: the deadline check above only fires BETWEEN
        // attempts, so a task that hangs internally would wedge the batch. Abort at
        // the remaining budget and record a FAIL. Next trial's setup resets state.
        let remaining = deadline.saturating_duration_since(Instant::now());
        match tokio::time::timeout(remaining, crate::steps::execute_step(bot, step.step_id, memory)).await {
            Ok(r) => last_msg = r.message,
            Err(_) => {
                last_msg = format!("gym timeout — task hung ({attempts} attempts)");
                break;
            }
        }
        if last_msg.contains("Broken pipe") || last_msg.contains("os error 32") || last_msg.contains("disconnect") {
            last_msg = format!("connection lost: {last_msg}");
            break;
        }
    }
    // Final pass re-check: a task can ACHIEVE the goal and then hang/timeout on a later
    // sub-step (e.g. craft_planks makes 16 planks in the bootstrap, then hangs on the slow
    // table-craft). The loop breaks via the timeout path without re-checking, recording a
    // false FAIL. Re-check the real inventory before recording so a met goal counts.
    if !pass {
        pass = passes(step, bot, &sync_from_bot(bot));
        if pass {
            last_msg = format!("goal met (post-timeout re-check); {last_msg}");
        }
    }
    let dur = t0.elapsed().as_millis() as i64;
    store.record(step.slug, pass, dur, gx, gy, gz, step.prereq, &last_msg);
    println!(
        "[gym:{}] {} {:.1}s @{gx},{gy},{gz} — {last_msg}",
        step.slug,
        if pass { "PASS" } else { "FAIL" },
        dur as f64 / 1000.0
    );
    // Release this trial's chunks (scoped — never `forceload remove all`, which would
    // nuke the other project's chunks on the shared box).
    clear_forceload(rcon, cx, cz).await;
}

/// Reset the bot to a clean survival state, grant the step's prerequisites, and
/// random-teleport it to a real surface spot. Returns the resulting landing
/// `(gx,gy,gz)` plus the forceload center `(cx,cz)` so the caller can release it.
async fn setup_trial(
    bot: &mut Bot<'_>,
    rcon: &mut RconClient,
    name: &str,
    step: &GymStep,
) -> (i32, i32, i32, i32, i32) {
    // Random center well away from spawn/race lanes.
    let cx = rand::Rng::gen_range(&mut rand::thread_rng(), 500..9500);
    let cz = rand::Rng::gen_range(&mut rand::thread_rng(), 500..9500);
    // A DEAD bot can't be teleported by spreadplayers — it stays put, so several
    // trials in a row "run" at the same corpse spot ("0 attempts" timeouts, seen live
    // at 4822,5254 x3). Respawn first so every trial gets a fresh random location.
    // Respawn if dead OR fallen into the VOID. A trial's descent (or a physics glitch)
    // can drop the bot below the world; it then falls forever (seen at y=-11,192,350) and
    // every subsequent trial instant-fails at that phantom spot (5000+ wasted attempts).
    // If it's voided but still "alive", kill it first so respawn returns it to the surface.
    let alive = sync_from_bot(bot).alive;
    if !alive || bot.entity.position.y < -200.0 {
        if alive {
            let _ = rcon.command(&format!("kill {name}")).await;
            bot.wait_ticks(15).await.ok();
        }
        bot.respawn().await.ok();
        bot.wait_ticks(40).await.ok();
    }
    let before = bot.entity.position;

    let _ = rcon.command(&format!("gamemode survival {name}")).await;
    let _ = rcon.command(&format!("clear {name}")).await;
    for item in step.prereq {
        let _ = rcon.command(&format!("give {name} {item}")).await;
    }
    // Keep the trial's chunks resident so the task can act immediately; scoped +
    // removed after (never a world-wide forceload).
    let _ = rcon.command(&format!("forceload add {} {} {} {}", cx - 24, cz - 24, cx + 24, cz + 24)).await;

    // spreadplayers drops the bot on the top solid block near (cx,cz): no fall
    // damage, loads the chunks itself, real terrain. Retry once — the very first
    // trial after connect sometimes lands the command before the player is
    // teleportable, leaving the bot at spawn (bad terrain-variance data).
    match step.setup {
        GymSetup::RandomSurface => {
            for attempt in 0..2 {
                let resp = rcon
                    .command(&format!("spreadplayers {cx} {cz} 0 24 false {name}"))
                    .await
                    .unwrap_or_default();
                pump_teleport(bot, cx, cz).await;
                let p = bot.entity.position;
                let moved = ((p.x - before.x).powi(2) + (p.z - before.z).powi(2)).sqrt();
                if moved > 50.0 {
                    break;
                }
                if attempt == 0 {
                    println!("[gym] spreadplayers didn't move the bot (resp: {}), retrying", resp.trim());
                }
            }
        }
    }
    // Portal steps (reach_lava/to_nether) run WITHOUT the mining phase that, in a real run,
    // leaves exposed lava in the bot's memory (mine_ore records find_exposed_blocks). Without
    // it, the trial is dominated by a blind ~120-block descent to the deep sea — that tests
    // MINING's job, not the portal's, and never passes. Match the real post-mining state: drop
    // an exposed surface lava pool a few blocks from the bot so the CAST (the portal step's
    // actual work: scoop → platform → 10-obsidian frame → light → enter) is what's exercised.
    if matches!(step.slug, "reach_lava" | "to_nether") {
        let p = bot.entity.position;
        let (bx, by, bz) = (p.x.floor() as i32, p.y.floor() as i32, p.z.floor() as i32);
        // Build a clean, EXPOSED surface pool right next to the bot: stone base + rim, a
        // cleared air column above (so find_fluid sees the top face on any terrain), and a
        // lava source pool 3 east at foot level. Matches get_water_buckets' recessed pool.
        let _ = rcon // solid base/rim so the pool doesn't drain and there's a stand
            .command(&format!("fill {} {} {} {} {} {} minecraft:stone", bx + 1, by - 2, bz - 2, bx + 6, by - 1, bz + 2))
            .await;
        let _ = rcon // clear the air above the pool so the lava top is exposed
            .command(&format!("fill {} {} {} {} {} {} minecraft:air", bx + 2, by, bz - 1, bx + 5, by + 3, bz + 1))
            .await;
        let _ = rcon // the lava source pool, foot level (feet sit at by, so by-1 is scoopable-flush)
            .command(&format!("fill {} {} {} {} {} {} minecraft:lava", bx + 3, by - 1, bz - 1, bx + 5, by - 1, bz + 1))
            .await;
        bot.wait_ticks(20).await.ok();
    }
    let p = bot.entity.position;
    (p.x.floor() as i32, p.y.floor() as i32, p.z.floor() as i32, cx, cz)
}

/// Pump network events after a teleport so chunks load and position is current.
async fn pump_teleport(bot: &mut Bot<'_>, cx: i32, cz: i32) {
    // WAIT for the teleport to actually take AND the destination terrain to load. A fixed
    // pump was too short: tasks started on a STALE/empty local world (block queries at the
    // bot's real position returned nothing), so the descent dug air/wrong cells and hung the
    // whole budget (seen live: descend_step operating on coords far from the trial spot).
    // Loop until the bot is near (cx,cz) AND solid terrain is loaded around it, or 12s.
    let start = Instant::now();
    loop {
        for _ in 0..15 {
            bot.drive_tick().await.ok();
        }
        let p = bot.entity.position;
        let (bx, by, bz) = (p.x.floor() as i32, p.y.floor() as i32, p.z.floor() as i32);
        let near = (bx - cx).abs() < 48 && (bz - cz).abs() < 48;
        let mut terrain = false;
        'scan: for dx in -1..=1 {
            for dz in -1..=1 {
                for dy in -5..=1 {
                    if bot.block_state_at(bx + dx, by + dy, bz + dz) != 0 {
                        terrain = true;
                        break 'scan;
                    }
                }
            }
        }
        if near && terrain {
            break;
        }
        if start.elapsed() > Duration::from_secs(12) {
            break;
        }
    }
    // Settle (land from the spreadplayers drop) before the task reads the world.
    for _ in 0..10 {
        bot.drive_tick().await.ok();
    }
}

/// Remove a trial's forceload (best-effort; called by the runner between trials via
/// the next setup's re-add, but exposed for explicit cleanup).
pub async fn clear_forceload(rcon: &mut RconClient, cx: i32, cz: i32) {
    let _ = rcon.command(&format!("forceload remove {} {} {} {}", cx - 24, cz - 24, cx + 24, cz + 24)).await;
}
