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
    /// A controlled water pool in a fixed arena — the water-escape test. Geometry via env:
    /// WATER_HALF (pool half-width → bank distance), WATER_DEPTH, WATER_SUBMERGE (start depth
    /// below the surface), WATER_CAP=1 (solid ceiling over the bot). Drops the bot submerged.
    WaterPool,
    /// Teleport the bot to a REAL underground lava pool: spreadplayers to random surface, scan
    /// the loaded chunks for exposed lava at depth with a safe cave stand beside it, and tp the
    /// bot there. Isolates the portal CAST (dig out a cast site → infinite water → obsidian frame
    /// → light → enter) from the descent, which is the easy/solved part. The real test of the bot.
    LavaPool,
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
    // Water escape — not a pipeline step; a focused drill for `leave_water`. Seeded via
    // WaterPool (WATER_HALF/DEPTH/SUBMERGE/CAP). Pass = fully out of water (neither head nor
    // feet submerged). Given cobblestone so the escape can pillar up out of open water.
    GymStep { slug: "leave_water", label: "Leave Water", order: 0, prereq: &["cobblestone 64"], step_id: "leave_water", timeout_secs: 90, custom_pass: Some(|bot, _| !crate::bot_utils::head_in_water(bot) && !crate::bot_utils::feet_in_water(bot)), setup: GymSetup::WaterPool },
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
    GymStep { slug: "reach_lava", label: "Reach + Scoop Lava", order: 18, prereq: &["iron_pickaxe 1", "bucket 3", "water_bucket 1", "cobblestone 200", "cooked_beef 8"], step_id: "build_nether_portal", timeout_secs: 800, custom_pass: Some(|bot, _| count_items(bot, "lava_bucket") >= 1 || !bot.find_blocks("obsidian", 8, 1).is_empty()), setup: GymSetup::RandomSurface },
    // Capstone: full portal kit, random terrain, pass = we're in the Nether.
    GymStep { slug: "to_nether", label: "Portal → Nether (capstone)", order: 19, prereq: &["iron_pickaxe 1", "bucket 12", "water_bucket 1", "flint_and_steel 1", "cobblestone 200", "cooked_beef 8"], step_id: "build_nether_portal", timeout_secs: 2700, custom_pass: Some(|_, s| s.world.in_nether()), setup: GymSetup::RandomSurface },
    // The FOCUSED portal drill: spawn fully kitted on REAL random terrain (spreadplayers,
    // ~0..10k) — NOT the seeded arena — and build + light + ENTER a portal on whatever lava
    // the world offers. This is exactly the race's failing case (bots arrive at Build Portal
    // fully supplied, then can't cast over the deep sea). Pass = in the nether.
    GymStep { slug: "portal", label: "Build + Enter Portal (wild)", order: 20, prereq: &["iron_pickaxe 1", "bucket 3", "water_bucket 2", "flint_and_steel 1", "cobblestone 128", "cooked_beef 16"], step_id: "build_nether_portal", timeout_secs: 720, custom_pass: Some(|_, s| s.world.in_nether()), setup: GymSetup::RandomSurface },
    // The operator's design: tp the bot to a REAL underground lava pool with the full kit (2 water
    // buckets → infinite source), then it must dig out a safe cast site, build the obsidian frame,
    // light it, and enter. Isolates the CAST at a real pool from the (easy, solved) descent.
    GymStep { slug: "pool", label: "Underground Pool → Nether", order: 21, prereq: &["iron_pickaxe 1", "bucket 3", "water_bucket 2", "flint_and_steel 1", "cobblestone 200", "cooked_beef 16"], step_id: "build_nether_portal", timeout_secs: 1500, custom_pass: Some(|_, s| s.world.in_nether()), setup: GymSetup::LavaPool },
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
    // Portal steps: record the SEEDED lava pool in memory so prepare_cast_site's memory-first
    // path walks straight to it (as it would in a real run after mining recorded exposed lava),
    // instead of a racy find_fluid scan that missed it and triggered a blind deep descent.
    if matches!(step.slug, "reach_lava" | "to_nether" | "pool") {
        // WEST EDGE of the lake (lake is gx+4..gx+14). This is a pure SAFETY NET: with the
        // adjacent flush lake, prepare_cast_site's find_fluid finds it on the first scan and
        // never consults memory (exactly like isolation, which seeds no POI). Only if that
        // first scan races empty does the memory-first path use this — and it must point at the
        // lake EDGE so the bot approaches from stone and anchors the frame at the edge, NOT
        // mid-lake (a centre POI made the bot stand IN lava and die).
        memory.record(
            crate::memory::PoiKind::Lava,
            (gx + 4, gy - 1, gz),
            crate::memory::PoiStatus::Available,
        );
    }
    // The `pool` slug now uses the SAME prepare path as the (proven) to_nether/DEEPSEA arena: a
    // memory POI at the pool's west edge (seeded above) + find_fluid, with FORCED_LAVA CLEARED.
    // Setting FORCED_LAVA made prepare take a different branch that anchored the frame OVER the pool
    // edge (bot at feet -39 over lava → `pillar1 FAIL`), while DEEPSEA's find_fluid+anchor path builds
    // 10/10. The generated pool is the DEEPSEA geometry, so the DEEPSEA path is what it wants.
    crate::tasks::portal::set_forced_lava(None);
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
            // Fixed-arena steps: a mid-build death respawns at WORLD SPAWN (no bed), stranding the
            // bot thousands of blocks from the arena+lava so it can never recover — it burns the
            // rest of the budget building futile frames on bare terrain. RCON-tp it back onto the
            // arena pad so the retry has lava in reach.
            if matches!(step.slug, "reach_lava" | "to_nether") {
                let _ = rcon.command(&format!("tp {name} {gx} {gy} {gz}")).await;
                pump_teleport(bot, gx, gz).await;
            }
            continue;
        }
        // Water-escape drill: drive `leave_water` directly (the unit under test) rather than
        // through the survival preempt, so we measure IT. The loop's top pass-check (fully out
        // of water) ends the trial; the 90s timeout bounds a failure.
        if step.slug == "leave_water" {
            attempts += 1;
            crate::bot_utils::leave_water(bot, 40).await;
            last_msg = format!("escaping — head_in_water={} feet_in_water={} y={:.0}", crate::bot_utils::head_in_water(bot), crate::bot_utils::feet_in_water(bot), bot.entity.position.y);
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
        // to_nether's step is build_nether_portal, which only builds+LIGHTS the portal
        // ("nether portal cast & lit") — it never walks in. The pass is in_nether, so we must
        // also run the separate enter_nether step once the portal exists, or it can NEVER pass.
        if (step.slug == "to_nether" || step.slug == "portal") && sync_from_bot(bot).world.portal_built {
            let rem = deadline.saturating_duration_since(Instant::now());
            let _ = tokio::time::timeout(rem, crate::steps::execute_step(bot, "enter_nether", memory)).await;
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
    // Random center in the EXPLORED land region near the race lanes (x~680, z~350-550):
    // a 9000-block range dropped trials into open ocean, where spreadplayers can't find a
    // surface ("too many entities for space") and lands the bot on the fallback arena, so
    // to_nether never exercised the real deep-descent-to-natural-lava path. This band is
    // solid terrain with lava the bots already mapped at the y-54 band.
    let cx = rand::Rng::gen_range(&mut rand::thread_rng(), 300..1400);
    let cz = rand::Rng::gen_range(&mut rand::thread_rng(), 300..1400);
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
                // maxRange 24 was too tight — spreadplayers errored "too many entities for
                // space" whenever the exact center was unsuitable. A wider range lets it find
                // a valid surface nearby; spread distance is moot for a single bot.
                let resp = rcon
                    .command(&format!("spreadplayers {cx} {cz} 8 200 false {name}"))
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
            // Anchor the spawnpoint at the landing spot so a mid-task death (e.g. a lava nick
            // during the portal cast) respawns the bot right here, not at world-spawn thousands
            // of blocks away where it can never recover — mirrors the race's lane spawnpoint.
            let p = bot.entity.position;
            let _ = rcon
                .command(&format!("spawnpoint {name} {} {} {}", p.x.floor() as i32, p.y.floor() as i32 + 1, p.z.floor() as i32))
                .await;
        }
        GymSetup::WaterPool => {
            // A contained water pool in a fixed arena, geometry from env — reproducible so we
            // can iterate leave_water against pond/lake/ocean/capped deterministically instead
            // of waiting for a random ocean spawn.
            let fx: i32 = std::env::var("GYM_ARENA_X").ok().and_then(|s| s.parse().ok()).unwrap_or(600);
            let fz: i32 = std::env::var("GYM_ARENA_Z").ok().and_then(|s| s.parse().ok()).unwrap_or(600);
            let fy: i32 = 72; // water surface level
            let half: i32 = std::env::var("WATER_HALF").ok().and_then(|s| s.parse().ok()).unwrap_or(12);
            let depth: i32 = std::env::var("WATER_DEPTH").ok().and_then(|s| s.parse().ok()).unwrap_or(6);
            let submerge: i32 = std::env::var("WATER_SUBMERGE").ok().and_then(|s| s.parse().ok()).unwrap_or(1);
            let cap = std::env::var("WATER_CAP").is_ok();
            let _ = rcon.command(&format!("forceload add {} {} {} {}", fx - half - 4, fz - half - 4, fx + half + 4, fz + half + 4)).await;
            // Clear the column above (leftover blocks/pillars from a prior trial), then a solid
            // stone shell, then carve the water pool inside it (open surface at fy).
            let _ = rcon.command(&format!("fill {} {} {} {} {} {} minecraft:air", fx - half - 2, fy + 1, fz - half - 2, fx + half + 2, fy + 24, fz + half + 2)).await;
            let _ = rcon.command(&format!("fill {} {} {} {} {} {} minecraft:stone", fx - half - 1, fy - depth - 1, fz - half - 1, fx + half + 1, fy, fz + half + 1)).await;
            let _ = rcon.command(&format!("fill {} {} {} {} {} {} minecraft:water", fx - half, fy - depth, fz - half, fx + half, fy, fz + half)).await;
            if cap {
                // A solid ceiling one block above the surface — the "dug a staircase, water
                // flooded in, capped above" case where the bot must dig up to escape.
                let _ = rcon.command(&format!("fill {} {} {} {} {} {} minecraft:stone", fx - half, fy + 1, fz - half, fx + half, fy + 1, fz + half)).await;
            }
            let gy = fy - submerge;
            let _ = rcon.command(&format!("tp {name} {fx} {gy} {fz}")).await;
            let _ = rcon.command(&format!("spawnpoint {name} {fx} {} {fz}", fy + 2)).await;
            pump_teleport(bot, fx, fz).await;
            bot.wait_ticks(10).await.ok();
            let p = bot.entity.position;
            return (p.x.floor() as i32, p.y.floor() as i32, p.z.floor() as i32, fx, fz);
        }
        GymSetup::LavaPool => {
            // GENERATE a clean, guaranteed-scoopable underground pool at a RANDOM deep location and
            // tp the bot to it. Searching NATURAL pools was intractable: their scoopable source sits
            // at a variable, often-unreachable depth vs any safe stand (e.g. source at y-55 under a
            // stand at y-35, through lava the bot can't dig) → endless "NO scoopable source" / far
            // scoops. The operator's intent is "bot stands at a SAFE underground lava pool, now build
            // the cast" — reaching the pool is explicitly "the easy part, not the point". So deliver
            // that state deterministically: this is the exact DEEPSEA rock-pool arena (which the cast
            // PASSES on — flush air-topped source in solid rock + a clear stand), placed at a random
            // deep spot instead of the fixed arena. The CAST is what's tested.
            let px = rand::Rng::gen_range(&mut rand::thread_rng(), 500..9500);
            let pz = rand::Rng::gen_range(&mut rand::thread_rng(), 500..9500);
            let py: i32 = rand::Rng::gen_range(&mut rand::thread_rng(), -50..-30); // deep underground band
            let _ = rcon.command(&format!("forceload add {} {} {} {}", px - 24, pz - 24, px + 24, pz + 24)).await;
            // EXACTLY the DEEPSEA rock-pool arena dimensions (which builds 10/10 + enters reliably),
            // just at a random location. Do NOT diverge — every geometry tweak (roomier/taller pocket)
            // re-introduced per-run drift. Solid stone box, then a 3-tall air pocket; the frame's top
            // rows dig UP into the ceiling as the cast builds, exactly like the proven arena.
            let _ = rcon.command(&format!("fill {} {} {} {} {} {} minecraft:stone", px - 10, py - 8, pz - 8, px + 16, py + 6, pz + 8)).await;
            let _ = rcon.command(&format!("fill {} {} {} {} {} {} minecraft:air", px - 2, py, pz - 3, px + 9, py + 2, pz + 3)).await;
            // 5x5 flush SOURCE pool at floor level (py-1), 4 blocks +X of the stand. 25 sources so
            // scooping 10+ times (sources are consumed per scoop) never depletes it mid-frame.
            let _ = rcon.command(&format!("fill {} {} {} {} {} {} minecraft:lava", px + 4, py - 1, pz - 2, px + 8, py - 1, pz + 2)).await;
            // Bot stands at (px,py,pz): feet at py on the stone floor (py-1), the pool source at py-1
            // is FLUSH (one below the feet) with air above — a shallow horizontal scoop. Spawnpoint
            // here so any mishap-death respawns in the arena, not at world spawn.
            let _ = rcon.command(&format!("spawnpoint {name} {px} {py} {pz}")).await;
            let _ = rcon.command(&format!("tp {name} {px} {py} {pz}")).await;
            pump_teleport(bot, px, pz).await;
            bot.wait_ticks(20).await.ok();
            // Guard the deep-tp death/rubber-band (respawn lands at the spawnpoint = the stand).
            if bot.health <= 0.0 {
                let _ = bot.respawn().await;
                bot.wait_ticks(20).await.ok();
            }
            let too_far = {
                let p = bot.entity.position;
                ((px as f64 - p.x).powi(2) + (pz as f64 - p.z).powi(2)).sqrt() >= 24.0
            };
            if too_far {
                let _ = rcon.command(&format!("tp {name} {px} {py} {pz}")).await;
                pump_teleport(bot, px, pz).await;
                bot.wait_ticks(20).await.ok();
            }
            // READBACK-SETTLE: the RCON fill + tp to a FAR fresh location can leave the bot's CLIENT
            // view of the just-filled box still streaming, so early block_at queries race and report a
            // just-placed cup wall as not-solid (`cup=SS_SS`) — the cast then never seals the cup and
            // stalls at 1/10 (while the ALWAYS-loaded fixed arena builds 10/10). Poll until the filled
            // stone box AND the lava pool read back correctly (not air) before starting the trial.
            let solid_now = |bot: &Bot, x: i32, y: i32, z: i32| {
                bot.block_at(x, y, z).map(|b| !b.name.is_empty() && !b.name.ends_with("air")).unwrap_or(false)
            };
            let lava_now = |bot: &Bot, x: i32, y: i32, z: i32| {
                bot.block_at(x, y, z).map(|b| b.name.contains("lava")).unwrap_or(false)
            };
            // Check only cells that are actually FILLED (never the carved air pocket px-2..px+9 ×
            // py..py+2 × pz-3..pz+3): the stone floor (py-1), the stone ceiling (py+3), and the pool.
            let mut settled = false;
            for _ in 0..80 {
                let box_ok = solid_now(bot, px, py - 1, pz)          // floor under the stand
                    && solid_now(bot, px + 6, py - 2, pz)            // floor under the pool
                    && solid_now(bot, px, py + 3, pz)                // ceiling above the 3-tall pocket
                    && solid_now(bot, px - 5, py, pz);              // west wall of the box
                let pool_ok = lava_now(bot, px + 4, py - 1, pz) && lava_now(bot, px + 6, py - 1, pz);
                if box_ok && pool_ok {
                    settled = true;
                    break;
                }
                bot.wait_ticks(5).await.ok();
            }
            println!("[gym] LavaPool: generated + tp'd to pool stand ({px},{py},{pz}) settled={settled}");
            let p = bot.entity.position;
            return (p.x.floor() as i32, p.y.floor() as i32, p.z.floor() as i32, px, pz);
        }
    }
    // Portal steps (reach_lava/to_nether) run WITHOUT the mining phase that, in a real run,
    // leaves exposed lava in the bot's memory (mine_ore records find_exposed_blocks). Without
    // it, the trial is dominated by a blind ~120-block descent to the deep sea — that tests
    // MINING's job, not the portal's, and never passes. Match the real post-mining state: drop
    // an exposed surface lava pool a few blocks from the bot so the CAST (the portal step's
    // actual work: scoop → platform → 10-obsidian frame → light → enter) is what's exercised.
    if matches!(step.slug, "reach_lava" | "to_nether") {
        // FIXED FLAT ARENA. The per-spawn synthetic pad was flaky on uneven/high terrain (the
        // cast stalled at 2-5/10 depending on the spawn). The cast is TERRAIN-INDEPENDENT — it
        // PASSED in the flat isolation arena — and the terrain-hard part (reaching lava) is
        // MINING's job, recorded in memory. So tp to a fixed cleared arena and build a
        // guaranteed-flat stone pad + a contained lava pool east of the frame footprint. This
        // makes reach_lava/to_nether reliably exercise the CAST (scoop→platform→frame→light→enter).
        // Arena origin — configurable via GYM_ARENA_X/GYM_ARENA_Z so PARALLEL gym bots each run
        // in their own arena (each ~48 wide; space them ≥64 apart, e.g. 600/700/800/900). This
        // lets several bots measure the to_nether pass-rate concurrently — one 15-min trial at a
        // time is far too slow to iterate on reliability. Default 600 keeps single-bot behaviour.
        let fx: i32 = std::env::var("GYM_ARENA_X").ok().and_then(|s| s.parse().ok()).unwrap_or(600);
        let fy: i32 = 72;
        let fz: i32 = std::env::var("GYM_ARENA_Z").ok().and_then(|s| s.parse().ok()).unwrap_or(600);
        let _ = rcon.command(&format!("forceload add {} {} {} {}", fx - 24, fz - 24, fx + 24, fz + 24)).await;
        // Clear the whole arena volume (prior trials' obsidian/portal/frame would leave
        // portal_built stale-true and clutter the cast site).
        let _ = rcon.command(&format!("fill {} {} {} {} {} {} minecraft:air", fx - 4, fy, fz - 8, fx + 20, fy + 22, fz + 8)).await;
        let _ = rcon.command(&format!("fill {} {} {} {} {} {} minecraft:air replace minecraft:obsidian", fx - 4, fy - 3, fz - 8, fx + 20, fy + 22, fz + 8)).await;
        // DEEP-SEA VARIANT (GYM_DEEPSEA=1): reproduce the RACE's cast-over-lava fire so a fix
        // can be VALIDATED. The default flat-pad seed below is solid ground beside a contained
        // lake (→ 9-10/10, and the frame anchors AWAY from lava so it never platforms over it),
        // which is why the gym passes while the race gets ~1/10. Here a deep lava sea (fy-1..fy-5)
        // fills the arena with only a small 3x3 solid spawn island; the 4-wide frame footprint
        // extends OVER the lava, forcing the bot to platform over it and take fire — exactly the
        // real deep sea. If a fix makes to_nether pass HERE, it should fix the race.
        if std::env::var("GYM_DEEPSEA").is_ok() {
            // ROCK POOL (repurposed from the pathological open-sea): a SOLID STONE block with a small
            // carved lava SOURCE pool + an air stand beside it — mimics a real underground pool
            // EMBEDDED IN ROCK so the fire-safe chamber dig + refill iterate fast & deterministically
            // (the open sea had no rock → unwinnable fire death). Bot in the air pocket at (fx,fy),
            // pool 4 blocks +X, solid rock all around for the chamber.
            let _ = rcon.command(&format!("fill {} {} {} {} {} {} minecraft:stone", fx - 10, fy - 8, fz - 8, fx + 16, fy + 6, fz + 8)).await;
            // Air pocket is intentionally 3-TALL + TIGHT in z (fz-3..fz+3): the cast DIGS UP into the
            // stone ceiling as it builds the upper frame rows, and the tight +Z wall CONSTRAINS the bot
            // to the frame stand so it can't drift (widening it to fz+7 let the bot wander to z=602.6 →
            // `centered=false` even on the bottom row). This is the documented working DEEPSEA geometry
            // (memory 18bl passed 10/10+enter). Do NOT widen/heighten it — that oscillates the failure.
            // +Z extent must include the cast's STAGING cell (stand_z+3 ≈ fz+4) or the bot stages IN
            // the stone wall and suffocates/POS-FAILs (`z=604.2, off=2.99` at fz+3). fz+5 fits staging
            // with 1 cell margin — NOT wider (fz+7 let it drift off the stand → centered=false). This
            // is the sweet spot between "tight = suffocate at staging" and "wide = drift, no center".
            let _ = rcon.command(&format!("fill {} {} {} {} {} {} minecraft:air", fx - 2, fy, fz - 3, fx + 9, fy + 2, fz + 5)).await; // bot pocket + cave over the pool
            // 5x5 flush SOURCE pool (25 sources) — big enough that scooping 10+ times doesn't
            // DEPLETE it (lava sources are consumed per scoop, unlike water; a 9-source pool ran dry
            // mid-frame → "fill lava: all rounds failed"). Real underground pools are usually larger.
            let _ = rcon.command(&format!("fill {} {} {} {} {} {} minecraft:lava", fx + 4, fy - 1, fz - 2, fx + 8, fy - 1, fz + 2)).await;
            let _ = rcon.command(&format!("tp {name} {fx} {fy} {fz}")).await;
            let _ = rcon.command(&format!("spawnpoint {name} {fx} {fy} {fz}")).await;
            pump_teleport(bot, fx, fz).await;
            bot.wait_ticks(10).await.ok();
            let p = bot.entity.position;
            return (p.x.floor() as i32, p.y.floor() as i32, p.z.floor() as i32, fx, fz);
        }
        // 2-deep flat stone pad.
        let _ = rcon.command(&format!("fill {} {} {} {} {} {} minecraft:stone", fx - 4, fy - 2, fz - 8, fx + 20, fy - 1, fz + 8)).await;
        // FLUSH 11x11 lava lake at floor level (fy-1), EAST of the bot at (fx,fy,fz) — the EXACT
        // arena isolation-test.sh's build_nether_portal uses, which reaches 10/10+lit reliably.
        // The bot stands 4 blocks west of the lake edge on solid stone; prepare_cast_site
        // approaches to "2 away" (stays on stone), scoops the edge, and anchors the frame there.
        // Earlier gym divergences broke this: a small 7-wide pool with a CENTRE POI made the bot
        // stand IN lava and die; a too-wide lake made the frame anchor over lava (POS FAIL).
        let _ = rcon.command(&format!("fill {} {} {} {} {} {} minecraft:lava", fx + 4, fy - 1, fz - 7, fx + 14, fy - 1, fz + 7)).await;
        // Put the bot on the pad (RCON runs as console/op) and set its spawnpoint HERE, so a
        // mishap-death (lava tick) respawns it back in the arena instead of world spawn (where
        // there's no lava → "NO scoopable" for the rest of the trial). Matches isolation line 137.
        let _ = rcon.command(&format!("tp {name} {fx} {fy} {fz}")).await;
        let _ = rcon.command(&format!("spawnpoint {name} {fx} {fy} {fz}")).await;
        pump_teleport(bot, fx, fz).await;
        bot.wait_ticks(10).await.ok();
        let p = bot.entity.position;
        return (p.x.floor() as i32, p.y.floor() as i32, p.z.floor() as i32, fx, fz);
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
