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
    GymStep { slug: "reach_lava", label: "Reach + Scoop Lava", order: 18, prereq: &["iron_pickaxe 1", "bucket 3", "water_bucket 1", "cobblestone 200"], step_id: "build_nether_portal", timeout_secs: 800, custom_pass: Some(|bot, _| count_items(bot, "lava_bucket") >= 1 || !bot.find_blocks("obsidian", 8, 1).is_empty()), setup: GymSetup::RandomSurface },
    // Capstone: full portal kit, random terrain, pass = we're in the Nether.
    GymStep { slug: "to_nether", label: "Portal → Nether (capstone)", order: 19, prereq: &["iron_pickaxe 1", "bucket 2", "water_bucket 1", "flint_and_steel 1", "cobblestone 200"], step_id: "build_nether_portal", timeout_secs: 1500, custom_pass: Some(|_, s| s.world.in_nether()), setup: GymSetup::RandomSurface },
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
    if matches!(step.slug, "reach_lava" | "to_nether") {
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
        if step.slug == "to_nether" && sync_from_bot(bot).world.portal_built {
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
        // FIXED FLAT ARENA. The per-spawn synthetic pad was flaky on uneven/high terrain (the
        // cast stalled at 2-5/10 depending on the spawn). The cast is TERRAIN-INDEPENDENT — it
        // PASSED in the flat isolation arena — and the terrain-hard part (reaching lava) is
        // MINING's job, recorded in memory. So tp to a fixed cleared arena and build a
        // guaranteed-flat stone pad + a contained lava pool east of the frame footprint. This
        // makes reach_lava/to_nether reliably exercise the CAST (scoop→platform→frame→light→enter).
        const FX: i32 = 600;
        const FY: i32 = 72;
        const FZ: i32 = 600;
        let _ = rcon.command(&format!("forceload add {} {} {} {}", FX - 24, FZ - 24, FX + 24, FZ + 24)).await;
        // Clear the whole arena volume (prior trials' obsidian/portal/frame would leave
        // portal_built stale-true and clutter the cast site).
        let _ = rcon.command(&format!("fill {} {} {} {} {} {} minecraft:air", FX - 4, FY, FZ - 8, FX + 20, FY + 22, FZ + 8)).await;
        let _ = rcon.command(&format!("fill {} {} {} {} {} {} minecraft:air replace minecraft:obsidian", FX - 4, FY - 3, FZ - 8, FX + 20, FY + 22, FZ + 8)).await;
        // 2-deep flat stone pad.
        let _ = rcon.command(&format!("fill {} {} {} {} {} {} minecraft:stone", FX - 4, FY - 2, FZ - 8, FX + 20, FY - 1, FZ + 8)).await;
        // FLUSH 11x11 lava lake at floor level (FY-1), EAST of the bot at (FX,FY,FZ) — the EXACT
        // arena isolation-test.sh's build_nether_portal uses, which reaches 10/10+lit reliably.
        // The bot stands 4 blocks west of the lake edge on solid stone; prepare_cast_site
        // approaches to "2 away" (stays on stone), scoops the edge, and anchors the frame there.
        // Earlier gym divergences broke this: a small 7-wide pool with a CENTRE POI made the bot
        // stand IN lava and die; a too-wide lake made the frame anchor over lava (POS FAIL).
        let _ = rcon.command(&format!("fill {} {} {} {} {} {} minecraft:lava", FX + 4, FY - 1, FZ - 7, FX + 14, FY - 1, FZ + 7)).await;
        // Put the bot on the pad (RCON runs as console/op) and set its spawnpoint HERE, so a
        // mishap-death (lava tick) respawns it back in the arena instead of world spawn (where
        // there's no lava → "NO scoopable" for the rest of the trial). Matches isolation line 137.
        let _ = rcon.command(&format!("tp {name} {FX} {FY} {FZ}")).await;
        let _ = rcon.command(&format!("spawnpoint {name} {FX} {FY} {FZ}")).await;
        pump_teleport(bot, FX, FZ).await;
        bot.wait_ticks(10).await.ok();
        let p = bot.entity.position;
        return (p.x.floor() as i32, p.y.floor() as i32, p.z.floor() as i32, FX, FZ);
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
