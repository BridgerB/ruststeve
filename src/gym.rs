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
    /// The surface at a fixed (x, z): a known terrain case (e.g. race i5's treeless start).
    FixedSurface { x: i32, z: i32 },
    /// The End, skill 12's precondition: bot teleported in (which initialises the dragon fight and
    /// the exit fountain), crystals killed, a dragon present. Kit from the slug's prereq.
    EndDragon,
    /// The End with a dragon and a crystal on top of each of the ten towers (crystals gym).
    EndCrystals,
    /// A 1-wide × 2-tall × 6-long tunnel in solid stone at y 20 (the iron-band tunnels a race bot
    /// crafts in). Placed in the batch's gym region; spawnpoint in the tunnel.
    Tunnel,
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
    /// WATER GYM (cycle 2, Phase B): a stone-shelled pool built by RCON with per-slug geometry.
    /// `submerge` = start depth below the surface (negative = standing on the cap above the pool),
    /// `cap` = solid ceiling one above the surface. The slug runs a REAL step (not leave_water),
    /// with the gym's pre-step survival bypassed, so only the tick-driver breath watchdog saves it.
    Water { half: i32, depth: i32, submerge: i32, cap: bool, pocket: bool, buried: bool },
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

/// The step an attempt runs: the slug's own, or for `pipeline` slugs the step machine's choice.
fn step_for_attempt(step: &GymStep, bot: &Bot) -> &'static str {
    if step.step_id != "pipeline" {
        return step.step_id;
    }
    let id = crate::steps::get_next_step(&sync_from_bot(bot)).map(|s| s.id).unwrap_or("gather_wood");
    println!("[gym:{}] pipeline → {id}", step.slug);
    id
}

/// GYM_PARITY=1: run without the cycle-5 trial rules (same-failure 4× cut, respawn tp-back) so an arm
/// built on this tree is measured under the same harness as an arm built on an older tree (6b-head).
/// Respawns far from the landing are still counted.
fn parity() -> bool {
    std::env::var("GYM_PARITY").ok().as_deref() == Some("1")
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
/// Set by RandomSurface setup when every placement candidate (±480) was water or lava; the trial
/// then FAILs without running (an all-ocean region otherwise ran trials at sea).
static SETUP_NO_LAND: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);


pub static GYM_STEPS: &[GymStep] = &[
    // Water escape — not a pipeline step; a focused drill for `leave_water`. Seeded via
    // WaterPool (WATER_HALF/DEPTH/SUBMERGE/CAP). Pass = fully out of water (neither head nor
    // feet submerged). Given cobblestone so the escape can pillar up out of open water.
    GymStep { slug: "leave_water", label: "Leave Water", order: 0, prereq: &["cobblestone 64"], step_id: "leave_water", timeout_secs: 90, custom_pass: Some(|bot, _| !crate::bot_utils::head_in_water(bot) && !crate::bot_utils::feet_in_water(bot)), setup: GymSetup::WaterPool },
    // Water gym — five scenarios; pass = out of water (head and feet) at the end, `deaths=N` in
    // the gym.db message. Gate: 10/10 (each scenario ×2), zero drownings.
    GymStep { slug: "water_lake", label: "Water: tp into a lake", order: 0, prereq: &["cobblestone 64", "stone_pickaxe 1"], step_id: "gather_wood", timeout_secs: 120, custom_pass: Some(|bot, _| !crate::bot_utils::head_in_water(bot) && !crate::bot_utils::feet_in_water(bot)), setup: GymSetup::Water { half: 8, depth: 6, submerge: 3, cap: false, pocket: false, buried: false } },
    GymStep { slug: "water_cave", label: "Water: flooded capped cave", order: 0, prereq: &["cobblestone 64", "stone_pickaxe 1"], step_id: "gather_wood", timeout_secs: 120, custom_pass: Some(|bot, _| !crate::bot_utils::head_in_water(bot) && !crate::bot_utils::feet_in_water(bot)), setup: GymSetup::Water { half: 4, depth: 4, submerge: 2, cap: true, pocket: false, buried: false } },
    GymStep { slug: "water_aquifer", label: "Water: dig down into an aquifer", order: 0, prereq: &["stone_pickaxe 1"], step_id: "mine_stone", timeout_secs: 150, custom_pass: Some(|bot, _| !crate::bot_utils::head_in_water(bot) && !crate::bot_utils::feet_in_water(bot) && count_items(bot, "cobblestone") >= 6), setup: GymSetup::Water { half: 3, depth: 5, submerge: -2, cap: true, pocket: false, buried: false } },
    GymStep { slug: "water_shore", label: "Water: gather wood from a lake surface", order: 0, prereq: &["cobblestone 64", "stone_pickaxe 1"], step_id: "gather_wood", timeout_secs: 120, custom_pass: Some(|bot, _| !crate::bot_utils::head_in_water(bot) && !crate::bot_utils::feet_in_water(bot)), setup: GymSetup::Water { half: 8, depth: 4, submerge: 0, cap: false, pocket: false, buried: false } },
    // Roofed aquifer, wood stage: a capped pool whose only air is a side pocket through a 2-tall
    // opening at the pool FLOOR; no pickaxe, no blocks. Rising is the trap (race i4 06:09 drowning:
    // the bot floated to a stone roof and hand-dug it underwater). Pass = head in air, alive.
    GymStep { slug: "water_roofed", label: "Water: roofed aquifer, air pocket below-side", order: 0, prereq: &[], step_id: "gather_wood", timeout_secs: 90, custom_pass: Some(|bot, _| !crate::bot_utils::head_in_water(bot)), setup: GymSetup::Water { half: 2, depth: 5, submerge: 2, cap: true, pocket: true, buried: false } },
    // Portal DESCENT over an aquifer: the bot stands on the cap of a roofed 7×7×6 pool and runs the
    // portal step, whose first act is digging down. Pass = below the pool (y < 60) with zero deaths,
    // i.e. the shaft detoured through dry rock instead of sinking in (batch 1: y 30 drownings).
    GymStep { slug: "water_descent", label: "Water: portal descent over an aquifer", order: 0, prereq: &["iron_pickaxe 2", "bucket 5", "water_bucket 1", "flint_and_steel 1", "cobblestone 128", "cooked_beef 16"], step_id: "build_nether_portal", timeout_secs: 300, custom_pass: Some(|bot, _| bot.entity.position.y < 60.0), setup: GymSetup::Water { half: 3, depth: 5, submerge: -2, cap: true, pocket: false, buried: true } },
    GymStep { slug: "water_cave_iron", label: "Water: mine iron from a flooded cave", order: 0, prereq: &["cobblestone 64", "stone_pickaxe 1"], step_id: "mine_iron", timeout_secs: 150, custom_pass: Some(|bot, _| !crate::bot_utils::head_in_water(bot) && !crate::bot_utils::feet_in_water(bot)), setup: GymSetup::Water { half: 4, depth: 4, submerge: 1, cap: true, pocket: false, buried: false } },
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
    GymStep { slug: "craft_iron_pickaxe", label: "Craft Iron Pickaxes (2)", order: 13, prereq: &["iron_ingot 6", "stick 4", "crafting_table 1"], step_id: "craft_iron_pickaxe", timeout_secs: 60, custom_pass: None, setup: GymSetup::RandomSurface },
    // Table BOOTSTRAP from the race state: 3 spruce planks + spruce logs, no table, a step that needs
    // one. Race i4 bot 2 (44 min) and race i5 bot 5 (1 h+) looped "need a crafting table" holding
    // logs. Pass = an iron pickaxe crafted, which needs the table first.
    GymStep { slug: "table_bootstrap", label: "Table bootstrap (race state)", order: 0, prereq: &["stone_sword 1", "cobblestone 64", "stone_sword 1", "cobblestone 64", "raw_iron 5", "coal 8", "spruce_log 9", "spruce_planks 3", "iron_pickaxe 1", "iron_ingot 15"], step_id: "craft_iron_pickaxe", timeout_secs: 120, custom_pass: Some(|bot, _| count_items(bot, "iron_pickaxe") >= 2), setup: GymSetup::RandomSurface },
    // The deadlock case: 2 planks, so the step's stick craft leaves 0 and the table needs a full
    // log→plank conversion. One failed 2×2 conversion deadlocked the old code (race i5 bot 5).
    GymStep { slug: "table_bootstrap_short", label: "Table bootstrap, short on planks", order: 0, prereq: &["stone_sword 1", "cobblestone 64", "stone_sword 1", "cobblestone 64", "raw_iron 5", "coal 8", "spruce_log 9", "spruce_planks 2", "iron_pickaxe 1", "iron_ingot 15"], step_id: "craft_iron_pickaxe", timeout_secs: 120, custom_pass: Some(|bot, _| count_items(bot, "iron_pickaxe") >= 2), setup: GymSetup::Tunnel },
    // Treeless LAND start: a desert at (23904, 3680) (race i5's "treeless" lanes turned out to be ocean;
    // that was fixed in race-b placement). Pass = 6 logs.
    GymStep { slug: "gather_wood_treeless", label: "Gather wood, treeless start", order: 0, prereq: &[], step_id: "gather_wood", timeout_secs: 600, custom_pass: Some(|bot, _| crate::tasks::gather_wood::count_logs(bot) >= 6), setup: GymSetup::FixedSurface { x: 23904, z: 3680 } },
    GymStep { slug: "craft_bucket", label: "Craft Buckets", order: 14, prereq: &["iron_ingot 6", "crafting_table 1"], step_id: "craft_bucket", timeout_secs: 60, custom_pass: None, setup: GymSetup::RandomSurface },
    GymStep { slug: "get_water_buckets", label: "Fill Water Buckets", order: 15, prereq: &["bucket 2"], step_id: "get_water_buckets", timeout_secs: 90, custom_pass: None, setup: GymSetup::RandomSurface },
    GymStep { slug: "get_flint_and_steel", label: "Get Flint and Steel", order: 16, prereq: &["iron_ingot 2", "crafting_table 1"], step_id: "get_flint_and_steel", timeout_secs: 260, custom_pass: None, setup: GymSetup::RandomSurface },
    GymStep { slug: "gather_build_blocks", label: "Gather Build Blocks", order: 17, prereq: &["stone_pickaxe 1"], step_id: "gather_build_blocks", timeout_secs: 120, custom_pass: None, setup: GymSetup::RandomSurface },
    // ── portal → nether (steve's gym lacks these) ──────────────────────────────
    // reach_lava: the terrain-hard descent→exposed-source→drop-to-source+1→scoop.
    // Runs the portal step but passes the moment a lava bucket is filled.
    GymStep { slug: "reach_lava", label: "Reach + Scoop Lava", order: 18, prereq: &["iron_pickaxe 1", "bucket 3", "water_bucket 1", "cobblestone 200", "cooked_beef 8"], step_id: "build_nether_portal", timeout_secs: 800, custom_pass: Some(|bot, _| count_items(bot, "lava_bucket") >= 1 || !bot.find_blocks("obsidian", 8, 1).is_empty()), setup: GymSetup::RandomSurface },
    // Capstone: full portal kit, random terrain, pass = we're in the Nether.
    GymStep { slug: "to_nether", label: "Portal → Nether (capstone)", order: 19, prereq: &["iron_pickaxe 1", "bucket 5", "water_bucket 1", "flint_and_steel 1", "cobblestone 200", "cooked_beef 8"], step_id: "build_nether_portal", timeout_secs: 2700, custom_pass: Some(|_, s| s.world.in_nether()), setup: GymSetup::RandomSurface },
    // The FOCUSED portal drill: spawn fully kitted on REAL random terrain (spreadplayers,
    // ~0..10k) — NOT the seeded arena — and build + light + ENTER a portal on whatever lava
    // the world offers. This is exactly the race's failing case (bots arrive at Build Portal
    // fully supplied, then can't cast over the deep sea). Pass = in the nether.
    // Kit held at 6b's for the 6b vs 6c+lsm2 comparison (a kit change would confound it). Phase 2 switches
    // it to the race kit (decision 5: iron_pickaxe 2, bucket 4, water_bucket 1) once the comparison decides.
    GymStep { slug: "portal", label: "Build + Enter Portal (wild)", order: 20, prereq: &["iron_pickaxe 3", "bucket 5", "water_bucket 1", "flint_and_steel 1", "cobblestone 256", "cooked_beef 16", "stick 4", "crafting_table 1"], step_id: "build_nether_portal", timeout_secs: 2400, custom_pass: Some(|_, s| s.world.in_nether()), setup: GymSetup::RandomSurface },
    // The operator's design: tp the bot to a REAL underground lava pool with the full kit (2 water
    // buckets → infinite source), then it must dig out a safe cast site, build the obsidian frame,
    // light it, and enter. Isolates the CAST at a real pool from the (easy, solved) descent.
    // Cycle 4 Phase 1 iron reproduction: the smoke race bot rust-race-001's lane in the cycle-4 world (its two
    // mine_iron attempts stalled; it started with 376 stale iron POIs from the deleted world). Fresh memory
    // here (the gym clears it), the race kit for the step, pass = the full 22 iron (race target), 30 min.
    GymStep { slug: "iron_repro", label: "Iron, race lane (22 iron)", order: 0, prereq: &["stone_pickaxe 1", "cobblestone 16", "cooked_beef 8"], step_id: "mine_iron", timeout_secs: 1800, custom_pass: Some(|bot, _| count_items(bot, "raw_iron") + count_items(bot, "iron_ingot") + count_items(bot, "iron_ore") + count_items(bot, "deepslate_iron_ore") >= 22), setup: GymSetup::FixedSurface { x: 300900, z: 350 } },
    // Cycle 4 Part 6, skill 12: kitted teleport into the End, crystals gone, beds detonated at the perch.
    // Pass = the server has no ender dragon (RCON, ground truth), checked after the trial.
    GymStep { slug: "crystals", label: "End crystals (bow from the ground)", order: 0, prereq: &["bow 1", "arrow 64", "cooked_beef 16", "cobblestone 64", "water_bucket 1", "obsidian 32"], step_id: "crystals", timeout_secs: 1800, custom_pass: None, setup: GymSetup::EndCrystals },
    GymStep { slug: "dragon", label: "Dragon (beds, crystals gone)", order: 0, prereq: &["red_bed 16", "obsidian 32", "iron_sword 1", "cooked_beef 16", "cobblestone 128", "water_bucket 1"], step_id: "dragon", timeout_secs: 900, custom_pass: None, setup: GymSetup::EndDragon },
    // Cycle 5 Phase 3: the race from "wood and stone tools done" to an iron pickaxe, through the real
    // step machine (step_id "pipeline": each attempt runs get_next_step, as the race loop does). The
    // kit is the median race state at the first post-tools step (i6 + i100 archives, 5 bots: stone
    // pickaxe, ~12 logs, ~2 planks, 0 sticks, ~22 cobble, no sword/table/furnace/coal). Fresh terrain,
    // POIs cleared. Pass = the SERVER's count of iron pickaxes on the bot (`clear … 0`), not the client.
    GymStep { slug: "iron_from_surface", label: "Surface → iron pickaxe (step machine)", order: 0, prereq: &["stone_pickaxe 1", "oak_log 12", "oak_planks 2", "cobblestone 22"], step_id: "pipeline", timeout_secs: 1200, custom_pass: Some(|bot, _| count_items(bot, "iron_pickaxe") >= 1), setup: GymSetup::RandomSurface },
    GymStep { slug: "lava_safe_move", label: "lava_safe_move drill (pool arena)", order: 0, prereq: &["cobblestone 64", "cooked_beef 8", "iron_pickaxe 1"], step_id: "lsm_drill", timeout_secs: 400, custom_pass: Some(|_, _| crate::tasks::lava_move::DRILL_OK.load(std::sync::atomic::Ordering::Relaxed)), setup: GymSetup::LavaPool },
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
        // Every launch is recorded: `start` writes outcome='running', `finish` the terminal outcome.
        // A trial whose process is killed stays 'running' and the report counts it as a FAIL
        // (cycle 1 lost ~10 killed trials from gym.db, inflating the pass rate).
        let _ = conn.execute("ALTER TABLE gym_runs ADD COLUMN outcome TEXT", []);
        GymStore { conn }
    }

    pub fn start(&self, slug: &str, x: i32, y: i32, z: i32, prereq: &[&str]) -> i64 {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0);
        let _ = self.conn.execute(
            "INSERT INTO gym_runs(ts, slug, pass, duration_ms, x, y, z, prereq, message, outcome)
             VALUES(?1,?2,0,0,?3,?4,?5,?6,'',  'running')",
            params![now, slug, x, y, z, prereq.join(", ")],
        );
        self.conn.last_insert_rowid()
    }

    pub fn set_pos(&self, id: i64, x: i32, y: i32, z: i32) {
        let _ = self.conn.execute("UPDATE gym_runs SET x=?2, y=?3, z=?4 WHERE id=?1", params![id, x, y, z]);
    }

    pub fn finish(&self, id: i64, pass: bool, duration_ms: i64, outcome: &str, message: &str) {
        let msg: String = message.chars().take(400).collect();
        let _ = self.conn.execute(
            "UPDATE gym_runs SET pass=?2, duration_ms=?3, outcome=?4, message=?5 WHERE id=?1",
            params![id, pass as i32, duration_ms, outcome, msg],
        );
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
/// The trial in progress: (gym.db row id, slug, step_id, start). The SIGTERM handler reads it so a
/// trial I stop leaves an `aborted` row with a reason, never a `running` one (cycle 5: 111 of 208
/// cycle-4 gym rows were left `running` by my own relaunches).
static CURRENT_TRIAL: std::sync::Mutex<Option<(i64, &'static str, &'static str, Instant)>> = std::sync::Mutex::new(None);

/// Install the SIGTERM handler once: record the current trial as `aborted` (gym.db + event log),
/// then exit. The launchers stop a batch with SIGTERM, and use -9 only as a fallback.
fn install_abort_handler() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        tokio::spawn(async {
            let Ok(mut sig) = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) else {
                return;
            };
            sig.recv().await;
            if let Some((id, slug, step_id, t0)) = CURRENT_TRIAL.lock().unwrap().take() {
                let reason = "aborted: stopped by the operator (SIGTERM)";
                GymStore::open().finish(id, false, t0.elapsed().as_millis() as i64, "aborted", reason);
                let e = |k: &str| std::env::var(k).unwrap_or_default();
                crate::learn::write_row(&serde_json::json!({
                    "run_id": e("GYM_RUN"), "bot_impl": "rs", "build": e("BUILD"), "bot": e("MC_USERNAME"),
                    "world_seed": e("WORLD_SEED").parse::<i64>().ok(), "skill": crate::learn::skill_of(step_id), "step_id": step_id,
                    "source": "gym", "gym_slug": slug, "start_ms": crate::learn::now_ms() - t0.elapsed().as_millis() as i64,
                    "duration_s": t0.elapsed().as_secs_f64(), "outcome": "aborted", "reason": reason, "deaths": 0, "gym_id": id,
                }));
                println!("[gym:{slug}] ABORTED after {:.0}s (SIGTERM) — row {id} recorded as aborted", t0.elapsed().as_secs_f64());
            }
            std::process::exit(0);
        });
    });
}

pub async fn run(
    bot: &mut Bot<'_>,
    memory: &mut WorldMemory,
    slug: &str,
    trials: u32,
) -> std::io::Result<()> {
    install_abort_handler();
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
    let _ = rcon_driving(bot, &mut rcon, &format!("op {name}")).await; // so respawn/tp behave; harmless if already op
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
    install_abort_handler();
    let mut rcon = RconClient::connect(RconOptions {
        host: env("RCON_HOST", "localhost"),
        port: env("RCON_PORT", "25575").parse().unwrap_or(25575),
        password: env("RCON_PASS", "minecraft-test-rcon"),
        ..Default::default()
    })
    .await
    .map_err(|e| std::io::Error::other(format!("gym RCON connect failed: {e} (tunnel to 25575 up?)")))?;
    let name = bot.username().to_string();
    let _ = rcon_driving(bot, &mut rcon, &format!("op {name}")).await;
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
    // A stale persisted frame anchor from an earlier trial/run would make build_nether_portal
    // "tp back to resume" a frame thousands of blocks away. Each trial is a fresh frame.
    crate::tasks::portal::clear_frame_anchor();
    // Start row BEFORE setup: a bot that dies or disconnects in setup still leaves a `running`
    // row (counted as killed) — cycle-2 water batch 2 lost one launch that way.
    let run_id = store.start(step.slug, 0, 0, 0, step.prereq);
    *CURRENT_TRIAL.lock().unwrap() = Some((run_id, step.slug, step.step_id, Instant::now()));
    crate::learn::gym_begin(step.step_id);
    crate::tasks::lava_move::DRILL_OK.store(false, std::sync::atomic::Ordering::Relaxed); // per-trial, never inherited
    *crate::tasks::lava_move::DRILL_ANCHOR.lock().unwrap() = None;
    *crate::tasks::end::CRYSTAL_MISSES.lock().unwrap() = None;
    crate::tasks::end::CLIMBED.lock().unwrap().clear();
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
    store.set_pos(run_id, gx, gy, gz);
    let deaths0 = bot.deaths;
    let deaths0_global = crate::bot::DEATH_COUNT.load(std::sync::atomic::Ordering::Relaxed);

    let t0 = Instant::now();
    let deadline = t0 + Duration::from_secs(step.timeout_secs);
    let mut pass = false;
    let mut last_msg = String::new();
    let mut attempts = 0u32;
    let mut harness_events = 0u32;
    let (mut same_fail, mut last_fail_key) = (0u32, String::new());
    // Setup found no land within ±480 (all-ocean region): FAIL the trial without running it.
    let no_land = SETUP_NO_LAND.swap(false, std::sync::atomic::Ordering::Relaxed);
    if no_land {
        last_msg = "setup: no land within ±480 — trial skipped".into();
    }
    loop {
        if no_land {
            break;
        }
        bot.wait_ticks(6).await.ok();
        let s = sync_from_bot(bot);
        if passes(step, bot, &s) {
            pass = true;
            break;
        }
        if Instant::now() > deadline {
            last_msg = format!("gym timeout ({attempts} attempts) | last: {}", crate::tasks::portal::last_cast_line());
            break;
        }
        // Between attempts too: a step that returns at once after each death never let the in-step
        // watcher (1 s sleep) tick (batch 3 rust-gym-002: 234 deaths over 479 attempts).
        if bot.deaths - deaths0 >= 5 {
            last_msg = format!("death loop — {} deaths this trial | last: {}", bot.deaths - deaths0, crate::tasks::portal::last_cast_line());
            break;
        }
        if !s.alive {
            bot.respawn().await.ok();
            bot.wait_ticks(40).await.ok();
            // Respawn must land within 32 blocks of the trial's landing. The spawnpoint isn't always
            // honoured (steve saw the same on its server), and a bot respawned at world spawn burns
            // the rest of the budget on foreign terrain. Otherwise tp back and count a harness event.
            // (Fixed-arena steps always went back to the pad; this generalises it.)
            let p = bot.entity.position;
            let d = ((p.x - gx as f64).powi(2) + (p.z - gz as f64).powi(2)).sqrt();
            if d > 32.0 && parity() {
                harness_events += 1;
                println!("[gym:{}] HARNESS respawn {d:.0} blocks from the landing — parity mode, not moved", step.slug);
            } else if d > 32.0 || matches!(step.slug, "reach_lava" | "to_nether") {
                if d > 32.0 {
                    harness_events += 1;
                    println!("[gym:{}] HARNESS respawn {d:.0} blocks from the landing ({:.0},{:.0},{:.0}) — tp back", step.slug, p.x, p.y, p.z);
                }
                let _ = rcon_driving(bot, rcon, &format!("tp {name} {gx} {gy} {gz}")).await;
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
        // Water slugs test the tick-driver breath watchdog, so the between-steps survival rescue
        // (which would surface the bot before the step even starts) is bypassed for them.
        // …except after a BREATH ALARM, which is exactly when the race's main loop would run it
        // (the watchdog's jump can't beat a stone ceiling; the escalation's cap-dig must).
        if (!step.slug.starts_with("water_") || bot.breath_alarm) && crate::survival::handle_survival(bot, memory).await {
            continue;
        }
        attempts += 1;
        // HARD timeout around the task: the deadline check above only fires BETWEEN
        // attempts, so a task that hangs internally would wedge the batch. Abort at
        // the remaining budget and record a FAIL. Next trial's setup resets state.
        let remaining = deadline.saturating_duration_since(Instant::now());
        // DEATH-LOOP guard: 5 deaths in a trial ends it as a FAIL. A step that respawns internally
        // never returns, so the between-steps checks never ran: batch 2 rust-gym-003 died 31× in
        // 7 min re-digging the same shaft into lava, and would have done so for its full 40 min.
        let trial_d0 = crate::bot::DEATH_COUNT.load(std::sync::atomic::Ordering::Relaxed).min(deaths0_global);
        let death_loop = async {
            loop {
                tokio::time::sleep(Duration::from_secs(1)).await;
                if crate::bot::DEATH_COUNT.load(std::sync::atomic::Ordering::Relaxed) >= trial_d0 + 5 {
                    break;
                }
            }
        };
        tokio::select! {
            r = tokio::time::timeout(remaining, crate::steps::execute_step(bot, step_for_attempt(step, bot), memory)) => match r {
                Ok(r) => {
                    // The same failure four times in one trial ends it, failed, with that reason (steve
                    // lost a run to a 102-dispatch "pickaxe worn out" loop). Digits are ignored so a
                    // counter or coordinate in the message doesn't make repeats look different.
                    if !r.success {
                        let key: String = r.message.chars().filter(|c| !c.is_ascii_digit()).collect();
                        same_fail = if key == last_fail_key { same_fail + 1 } else { 1 };
                        last_fail_key = key;
                    } else {
                        same_fail = 0;
                    }
                    last_msg = r.message;
                    if same_fail >= 4 && !parity() {
                        last_msg = format!("same failure 4× in this trial: {last_msg}");
                        break;
                    }
                }
                Err(_) => {
                    last_msg = format!("gym timeout — task hung ({attempts} attempts) | last: {}", crate::tasks::portal::last_cast_line());
                    break;
                }
            },
            _ = death_loop => {
                last_msg = format!("death loop — 5 deaths this trial | last: {}", crate::tasks::portal::last_cast_line());
                break;
            }
        }
        // to_nether's step is build_nether_portal, which only builds+LIGHTS the portal
        // ("nether portal cast & lit") — it never walks in. The pass is in_nether, so we must
        // also run the separate enter_nether step once the portal exists, or it can NEVER pass.
        if matches!(step.slug, "to_nether" | "portal" | "pool") && sync_from_bot(bot).world.portal_built {
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
    // Crystals slug: pass = no end crystal left in the End (server ground truth).
    if step.slug == "crystals" {
        let left = rcon_driving(bot, rcon, "execute in minecraft:the_end if entity @e[type=minecraft:end_crystal]").await;
        pass = left.contains("failed");
        last_msg = format!("server crystal check: {} | {last_msg}", if pass { "none left" } else { "crystals remain" });
    }
    // iron_from_surface: ground truth from the server (the client's inventory can lag or ghost).
    if step.slug == "iron_from_surface" {
        let truth = rcon_driving(bot, rcon, &format!("clear {name} minecraft:iron_pickaxe 0")).await;
        let server_pass = truth.contains("Found");
        if pass != server_pass {
            last_msg = format!("client said {pass}, server said {server_pass} | {last_msg}");
        }
        pass = server_pass;
        last_msg = format!("server iron_pickaxe check: {} | {last_msg}", truth.trim());
    }
    // Dragon slug: ground truth from the server, never the bot's own view (cycle 4 Part 3).
    if step.slug == "dragon" {
        let alive = rcon_driving(bot, rcon, "execute in minecraft:the_end if entity @e[type=minecraft:ender_dragon]").await;
        pass = alive.contains("failed");
        last_msg = format!("server dragon check: {} | {last_msg}", if pass { "no dragon (killed)" } else { "dragon alive" });
    }
    let dur = t0.elapsed().as_millis() as i64;
    let deaths = bot.deaths - deaths0;
    // Water slugs gate on SURVIVAL: a drowning that respawned the bot onto dry land is not a
    // pass (cycle-2 batch 4: water_cave "PASS 95.8 s — deaths=1").
    let mut outcome = if pass { "pass" } else if last_msg.contains("timeout") { "timeout" } else { "fail" };
    if pass && deaths > 0 && step.slug.starts_with("water_") {
        pass = false;
        outcome = "died";
    }
    if harness_events > 0 {
        last_msg = format!("harness_events={harness_events} | {last_msg}");
    }
    last_msg = format!("deaths={deaths} | {last_msg}");
    // Finished normally: the abort handler must no longer claim this row.
    *CURRENT_TRIAL.lock().unwrap() = None;
    store.finish(run_id, pass, dur, outcome, &last_msg);
    // A setup skip (no land, etc.) measures nothing about the bot: `skipped`, excluded by compare/funnel.
    let row_outcome = if last_msg.contains("trial skipped") { "skipped" } else { match outcome { "pass" => "ok", "timeout" => "timeout", "died" => "death", _ => "failed" } };
    crate::learn::gym_row(bot, step.slug, step.step_id, t0, row_outcome, &last_msg, deaths);
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
    // FRESH REGION per gym batch (cycle 2): GYM_REGION=n moves the square n×3000 on X and to
    // z 3300..4400 so a batch never lands on frames/pads/holes an earlier batch left (the world
    // is never wiped). GYM_REGION=0 (unset) keeps the cycle-1 square.
    let region: i32 = env("GYM_REGION", "0").parse().unwrap_or(0);
    let (ox, oz) = if region > 0 { (region * 3000, 3000) } else { (0, 0) };
    let cx = ox + rand::Rng::gen_range(&mut rand::thread_rng(), 300..1400);
    let cz = oz + rand::Rng::gen_range(&mut rand::thread_rng(), 300..1400);
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
            let _ = rcon_driving(bot, rcon, &format!("kill {name}")).await;
            bot.wait_ticks(15).await.ok();
        }
        bot.respawn().await.ok();
        bot.wait_ticks(40).await.ok();
    }
    let before = bot.entity.position;

    let _ = rcon_driving(bot, rcon, &format!("gamemode survival {name}")).await;
    let _ = rcon_driving(bot, rcon, &format!("clear {name}")).await;
    for item in step.prereq {
        let _ = rcon_driving(bot, rcon, &format!("give {name} {item}")).await;
    }
    // Keep the trial's chunks resident so the task can act immediately; scoped +
    // removed after (never a world-wide forceload).
    let _ = rcon_driving(bot, rcon, &format!("forceload add {} {} {} {}", cx - 24, cz - 24, cx + 24, cz + 24)).await;

    // spreadplayers drops the bot on the top solid block near (cx,cz): no fall
    // damage, loads the chunks itself, real terrain. Retry once — the very first
    // trial after connect sometimes lands the command before the player is
    // teleportable, leaving the bot at spawn (bad terrain-variance data).
    match step.setup {
        GymSetup::RandomSurface => {
            // NO spreadplayers: it reads terrain up to 200 blocks around and generates any missing chunk
            // SYNCHRONOUSLY on the server thread. With every batch in a fresh, never-generated region and
            // five bots setting up together, one tick passed 60 s and the watchdog crashed Server B
            // (10:20:34 UTC, stack SpreadPlayersCommand.getSpawnY → ServerChunkCache.getChunk; batch 4
            // lost every bot). The forceload above generates (cx,cz)'s chunks on worker threads: wait
            // for them to load, then put the bot on the surface. A liquid surface (spreadplayers skipped
            // those) shifts the spot 64 blocks and retries.
            // Spiral of candidates 160 apart out to ±480: a straight 4×64 shift stayed inside one ocean
            // (batch 4 region 36: every try "under water", two trials started at sea).
            let offsets = [(0, 0), (160, 0), (0, 160), (-160, 0), (0, -160), (320, 0), (0, 320), (-320, 0), (0, -320), (480, 0), (0, 480), (-480, 0)];
            SETUP_NO_LAND.store(true, std::sync::atomic::Ordering::Relaxed);
            let mut fallback: Option<(i32, i32)> = None; // first candidate dry underfoot (shore allowed)
            for (attempt, &(ox, oz)) in offsets.iter().enumerate() {
                let (sx, sz) = (cx + ox, cz + oz);
                if attempt > 0 {
                    let _ = rcon_driving(bot, rcon, &format!("forceload add {} {} {} {}", sx - 8, sz - 8, sx + 8, sz + 8)).await;
                }
                for _ in 0..90 {
                    if rcon_driving(bot, rcon, &format!("execute if loaded {sx} 0 {sz}")).await.contains("passed") {
                        break;
                    }
                    bot.wait_ticks(20).await.ok();
                }
                let _ = rcon_driving(bot, rcon, &format!("execute positioned {sx} 0 {sz} positioned over motion_blocking_no_leaves run tp {name} ~ ~1 ~")).await;
                pump_teleport(bot, sx, sz).await;
                bot.wait_ticks(10).await.ok();
                let p = bot.entity.position;
                let moved = ((p.x - before.x).powi(2) + (p.z - before.z).powi(2)).sqrt();
                let under = bot.block_at(p.x.floor() as i32, p.y.floor() as i32 - 1, p.z.floor() as i32).map(|b| b.name.to_string()).unwrap_or_default();
                // No water within 3 blocks of the landing either: a beach cell passed the underfoot test
                // and the trial began at the sea's edge (batch 6: two bots in water from t=8 s).
                let (lx, ly, lz) = (p.x.floor() as i32, p.y.floor() as i32, p.z.floor() as i32);
                let shore = (-3..=3).any(|dx| {
                    (-3..=3).any(|dz| (-1..=1).any(|dy| bot.block_at(lx + dx, ly + dy, lz + dz).is_some_and(|b| b.name.contains("water"))))
                });
                if moved > 50.0 && !under.contains("water") && !under.contains("lava") && !shore {
                    SETUP_NO_LAND.store(false, std::sync::atomic::Ordering::Relaxed);
                    break;
                }
                if fallback.is_none() && moved > 50.0 && !under.contains("water") && !under.contains("lava") {
                    fallback = Some((sx, sz));
                }
                println!("[gym] surface placement at ({sx},{sz}) unusable (moved {moved:.0}, under {under}, shore {shore}) — shifting");
            }
            // No shore-free landing anywhere: use the first one that was at least dry underfoot rather than
            // skip the trial. Batch 8 skipped two trials in a row; a river or pond within 3 blocks is common.
            // Only real ocean (water underfoot at all 12) still skips.
            if SETUP_NO_LAND.load(std::sync::atomic::Ordering::Relaxed) {
                if let Some((fx2, fz2)) = fallback {
                    println!("[gym] no shore-free landing — using the first dry one at ({fx2},{fz2})");
                    let _ = rcon_driving(bot, rcon, &format!("execute positioned {fx2} 0 {fz2} positioned over motion_blocking_no_leaves run tp {name} ~ ~1 ~")).await;
                    pump_teleport(bot, fx2, fz2).await;
                    bot.wait_ticks(10).await.ok();
                    SETUP_NO_LAND.store(false, std::sync::atomic::Ordering::Relaxed);
                }
            }
            // Anchor the spawnpoint at the landing spot so a mid-task death (e.g. a lava nick
            // during the portal cast) respawns the bot right here, not at world-spawn thousands
            // of blocks away where it can never recover — mirrors the race's lane spawnpoint.
            // 3 blocks OFF the landing, on the surface: the portal descent digs its 1×1 shaft straight
            // down from the landing, so a spawnpoint there sits over an open shaft. Batch 4 rust-gym-003
            // respawned into its own shaft, fell 70 blocks (`desc y=56->48->37…` within a second) into
            // the lava that had flowed into its bottom, five times (death-loop FAIL at 216 s).
            let p = bot.entity.position;
            let (spx, spz) = (p.x.floor() as i32 + 3, p.z.floor() as i32);
            let _ = rcon_driving(bot, rcon, &format!("execute positioned {spx} 0 {spz} positioned over motion_blocking_no_leaves run spawnpoint {name} ~ ~ ~")).await;
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
            let _ = rcon_driving(bot, rcon, &format!("forceload add {} {} {} {}", fx - half - 4, fz - half - 4, fx + half + 4, fz + half + 4)).await;
            wait_area_loaded(bot, rcon, fx - half - 4, fz - half - 4, fx + half + 4, fz + half + 4).await;
            // Clear the column above (leftover blocks/pillars from a prior trial), then a solid
            // stone shell, then carve the water pool inside it (open surface at fy).
            let _ = rcon_driving(bot, rcon, &format!("fill {} {} {} {} {} {} minecraft:air", fx - half - 2, fy + 1, fz - half - 2, fx + half + 2, fy + 24, fz + half + 2)).await;
            let _ = rcon_driving(bot, rcon, &format!("fill {} {} {} {} {} {} minecraft:stone", fx - half - 1, fy - depth - 1, fz - half - 1, fx + half + 1, fy, fz + half + 1)).await;
            let _ = rcon_driving(bot, rcon, &format!("fill {} {} {} {} {} {} minecraft:water", fx - half, fy - depth, fz - half, fx + half, fy, fz + half)).await;
            if cap {
                // A solid ceiling one block above the surface — the "dug a staircase, water
                // flooded in, capped above" case where the bot must dig up to escape.
                let _ = rcon_driving(bot, rcon, &format!("fill {} {} {} {} {} {} minecraft:stone", fx - half, fy + 1, fz - half, fx + half, fy + 1, fz + half)).await;
            }
            let gy = fy - submerge;
            let _ = rcon_driving(bot, rcon, &format!("tp {name} {fx} {gy} {fz}")).await;
            let _ = rcon_driving(bot, rcon, &format!("spawnpoint {name} {fx} {} {fz}", fy + 2)).await;
            pump_teleport(bot, fx, fz).await;
            bot.wait_ticks(10).await.ok();
            let p = bot.entity.position;
            return (p.x.floor() as i32, p.y.floor() as i32, p.z.floor() as i32, fx, fz);
        }
        GymSetup::Water { half, depth, submerge, cap, pocket, buried } => {
            // FIXED arena region (WATER_REGION, default 4 = x 12600, known forested land), NOT the
            // batch's fresh GYM_REGION: the arena is rebuilt by fill every trial, so it can't carry
            // an earlier run's leftovers — only its SURROUNDINGS matter, and gather_wood needs trees
            // there. Region 9 (x 27600) had none: the shore bot explored 80 blocks into natural
            // water and timed out (batch 6: lake/cave/shore 0/3 vs 2/2 on region 7).
            let region: i32 = env("WATER_REGION", "4").parse().unwrap_or(4);
            // Bots in one batch share a region: offset each bot's arena by its number (rust-gym-00N)
            // so five water bots don't build into the same pool.
            let botn: i32 = name.trim_start_matches(|c: char| !c.is_ascii_digit()).parse().unwrap_or(0);
            let fx = 600 + region * 3000;
            let fz = 600 + if region > 0 { 3000 } else { 0 } + botn * 40;
            let fy: i32 = 72;
            let _ = rcon_driving(bot, rcon, &format!("forceload add {} {} {} {}", fx - half - 4, fz - half - 4, fx + half + 4, fz + half + 4)).await;
            wait_area_loaded(bot, rcon, fx - half - 4, fz - half - 4, fx + half + 4, fz + half + 4).await;
            let _ = rcon_driving(bot, rcon, &format!("fill {} {} {} {} {} {} minecraft:air", fx - half - 2, fy + 1, fz - half - 2, fx + half + 2, fy + 24, fz + half + 2)).await;
            let _ = rcon_driving(bot, rcon, &format!("fill {} {} {} {} {} {} minecraft:stone", fx - half - 1, fy - depth - 1, fz - half - 1, fx + half + 1, fy, fz + half + 1)).await;
            let _ = rcon_driving(bot, rcon, &format!("fill {} {} {} {} {} {} minecraft:water", fx - half, fy - depth, fz - half, fx + half, fy, fz + half)).await;
            if cap {
                let _ = rcon_driving(bot, rcon, &format!("fill {} {} {} {} {} {} minecraft:stone", fx - half, fy + 1, fz - half, fx + half, fy + 1, fz + half)).await;
            }
            if pocket {
                // Stone enclosure east of the pool, an air pocket inside it whose floor sits 2 below
                // the pool floor, and a 2-tall opening through the shared wall at the pool floor.
                // Water pours through and runs along the pocket floor; the air above it is the exit.
                let (wx, floor_y) = (fx + half + 1, fy - depth);
                let _ = rcon_driving(bot, rcon, &format!("fill {} {} {} {} {} {} minecraft:stone", wx, floor_y - 3, fz - 3, wx + 5, fy + 1, fz + 3)).await;
                let _ = rcon_driving(bot, rcon, &format!("fill {} {} {} {} {} {} minecraft:air", wx + 1, floor_y - 2, fz - 2, wx + 4, fy, fz + 2)).await;
                let _ = rcon_driving(bot, rcon, &format!("fill {} {} {} {} {} {} minecraft:air", wx, floor_y, fz, wx, floor_y + 1, fz)).await;
            }
            // BURIED: the pool sealed inside dry rock (a slab 7 thick over it and a 9-wide rock ring
            // around it), with the bot in a short 1×1 shaft above the pool: the real geometry of a
            // descent shaft heading into an aquifer. Rebuild the water after the rock fill.
            let mut gy = fy - submerge;
            if buried {
                let r = half + 9;
                let _ = rcon_driving(bot, rcon, &format!("fill {} {} {} {} {} {} minecraft:stone", fx - r, fy - depth - 2, fz - r, fx + r, fy + 7, fz + r)).await;
                let _ = rcon_driving(bot, rcon, &format!("fill {} {} {} {} {} {} minecraft:water", fx - half, fy - depth, fz - half, fx + half, fy, fz + half)).await;
                let _ = rcon_driving(bot, rcon, &format!("fill {} {} {} {} {} {} minecraft:air", fx, fy + 4, fz, fx, fy + 8, fz)).await;
                gy = fy + 4;
            }
            let _ = rcon_driving(bot, rcon, &format!("tp {name} {fx} {gy} {fz}")).await;
            let (sx, sy) = if buried { (fx, fy + 4) } else { (fx + half + 3, fy + 2) }; // never inside the buried slab
            let _ = rcon_driving(bot, rcon, &format!("spawnpoint {name} {sx} {sy} {fz}")).await;
            pump_teleport(bot, fx, fz).await;
            bot.wait_ticks(10).await.ok();
            let p = bot.entity.position;
            return (p.x.floor() as i32, p.y.floor() as i32, p.z.floor() as i32, fx, fz);
        }
        GymSetup::EndDragon | GymSetup::EndCrystals => {
            let end = |c: &str| format!("execute in minecraft:the_end run {c}");
            let _ = rcon_driving(bot, rcon, &end("forceload add -64 -64 64 64")).await;
            for _ in 0..60 {
                if rcon_driving(bot, rcon, "execute in minecraft:the_end if loaded 0 0 0").await.contains("passed") {
                    break;
                }
                bot.wait_ticks(20).await.ok();
            }
            // Arrive east of the fountain on the end stone; the first arrival starts the dragon fight.
            let _ = rcon_driving(bot, rcon, &format!("execute in minecraft:the_end positioned 8 0 0 positioned over motion_blocking run tp {name} ~ ~1 ~")).await;
            let _ = rcon_driving(bot, rcon, &format!("execute in minecraft:the_end positioned 8 0 0 positioned over motion_blocking run spawnpoint {name} ~ ~1 ~")).await;
            for _ in 0..60 {
                if bot.game.dimension.contains("the_end") {
                    break;
                }
                bot.wait_ticks(10).await.ok();
            }
            // Wait for the exit fountain (bedrock pillar at x=z=0), then remove the crystals and make
            // sure a dragon exists (a re-run after a kill needs one summoned).
            for _ in 0..60 {
                let probe = rcon_driving(bot, rcon, "execute in minecraft:the_end if block 0 64 0 minecraft:bedrock").await;
                let probe2 = rcon_driving(bot, rcon, "execute in minecraft:the_end if block 0 65 0 minecraft:bedrock").await;
                if probe.contains("passed") || probe2.contains("passed") {
                    break;
                }
                bot.wait_ticks(20).await.ok();
            }
            let _ = rcon_driving(bot, rcon, &end("kill @e[type=minecraft:end_crystal]")).await;
            if matches!(step.setup, GymSetup::EndCrystals) {
                // A crystal on top of each of the ten towers (vanilla spike layout: radius 42, i·36°);
                // `over motion_blocking` puts it on the tower's top (on a cage's roof for caged ones).
                for i in 0..10 {
                    let a = 2.0 * (-std::f64::consts::PI + std::f64::consts::PI / 10.0 * i as f64);
                    let (x, z) = ((42.0 * a.cos()).floor() as i32, (42.0 * a.sin()).floor() as i32);
                    let _ = rcon_driving(
                        bot,
                        rcon,
                        &format!("execute in minecraft:the_end positioned {x} 0 {z} positioned over motion_blocking run summon minecraft:end_crystal ~0.5 ~ ~0.5"),
                    )
                    .await;
                }
            }
            if rcon_driving(bot, rcon, "execute in minecraft:the_end if entity @e[type=minecraft:ender_dragon]").await.contains("failed") {
                let _ = rcon_driving(bot, rcon, &end("summon minecraft:ender_dragon 0 100 0")).await;
            }
            // Every trial fights a full-health dragon on clean ground: the dragon outlives a failed
            // trial (late4-v2b trial 1 left it at 48 / 200), and the last trial's pillar and bridge
            // (x 4..8 east of the fountain, above the y-62 island surface) would change the geometry.
            let _ = rcon_driving(bot, rcon, &end("data merge entity @e[type=minecraft:ender_dragon,limit=1] {Health:200f}")).await;
            // A /summon'ed dragon (re-run after a kill) has no fight manager and starts in phase 10
            // (hover): late17-v2o trial 2 hovered at (5, 100, 0) for 900 s, never perching. The holding
            // pattern (0) runs without a fight (crystal count 0) and lands like a normal dragon.
            let _ = rcon_driving(bot, rcon, &end("data merge entity @e[type=minecraft:ender_dragon,limit=1] {DragonPhase:0}")).await;
            let _ = rcon_driving(bot, rcon, &end("fill 4 63 -1 8 80 1 minecraft:air")).await;
            // Refill the crater earlier trials' beds blew east of the fountain (island top is y 62): the
            // crystals bot spawned at (8, 57, 0) in that pit and could not walk out to any tower
            // (late22-cry3 trial 1: "could not close" on every crystal, 0 kills).
            let _ = rcon_driving(bot, rcon, &end("fill 4 50 -4 10 62 4 minecraft:end_stone replace minecraft:air")).await;
            let who = bot.username().to_string();
            let _ = rcon_driving(bot, rcon, &end(&format!("tp {who} 8 63 0"))).await;
            bot.wait_ticks(40).await.ok();
            let p = bot.entity.position;
            println!("[gym] EndDragon: in {} at ({:.0},{:.0},{:.0}), crystals killed, dragon ensured", bot.game.dimension, p.x, p.y, p.z);
            return (p.x.floor() as i32, p.y.floor() as i32, p.z.floor() as i32, 0, 0);
        }
        GymSetup::FixedSurface { x, z } => {
            let _ = rcon_driving(bot, rcon, &format!("forceload add {} {} {} {}", x - 16, z - 16, x + 16, z + 16)).await;
            for _ in 0..90 {
                if rcon_driving(bot, rcon, &format!("execute if loaded {x} 0 {z}")).await.contains("passed") {
                    break;
                }
                bot.wait_ticks(20).await.ok();
            }
            let _ = rcon_driving(bot, rcon, &format!("execute positioned {x} 0 {z} positioned over motion_blocking_no_leaves run tp {name} ~ ~1 ~")).await;
            pump_teleport(bot, x, z).await;
            bot.wait_ticks(10).await.ok();
            let _ = rcon_driving(bot, rcon, &format!("execute positioned {} 0 {z} positioned over motion_blocking_no_leaves run spawnpoint {name} ~ ~ ~", x + 3)).await;
        }
        GymSetup::Tunnel => {
            // Race i5 bot 5 looped on the table craft at y 19–23 in its iron tunnels; the surface
            // reproduction passed 5/5, so recreate the tunnel itself. The setup's forceload (±24 around
            // cx, cz) is already in place; wait for it, then build.
            let ty = 20;
            for _ in 0..90 {
                if rcon_driving(bot, rcon, &format!("execute if loaded {cx} 0 {cz}")).await.contains("passed") {
                    break;
                }
                bot.wait_ticks(20).await.ok();
            }
            let _ = rcon_driving(bot, rcon, &format!("fill {} {} {} {} {} {} minecraft:stone", cx - 4, ty - 2, cz - 4, cx + 9, ty + 4, cz + 4)).await;
            let _ = rcon_driving(bot, rcon, &format!("fill {} {} {} {} {} {} minecraft:air", cx, ty, cz, cx + 5, ty + 1, cz)).await;
            let _ = rcon_driving(bot, rcon, &format!("tp {name} {cx} {ty} {cz}")).await;
            pump_teleport(bot, cx, cz).await;
            bot.wait_ticks(10).await.ok();
            let _ = rcon_driving(bot, rcon, &format!("spawnpoint {name} {} {ty} {cz}", cx + 2)).await;
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
            let _ = rcon_driving(bot, rcon, &format!("forceload add {} {} {} {}", px - 24, pz - 24, px + 24, pz + 24)).await;
            wait_area_loaded(bot, rcon, px - 24, pz - 24, px + 24, pz + 24).await;
            // EXACTLY the DEEPSEA rock-pool arena dimensions (which builds 10/10 + enters reliably),
            // just at a random location. Do NOT diverge — every geometry tweak (roomier/taller pocket)
            // re-introduced per-run drift. Solid stone box, then a 3-tall air pocket; the frame's top
            // rows dig UP into the ceiling as the cast builds, exactly like the proven arena.
            let _ = rcon_driving(bot, rcon, &format!("fill {} {} {} {} {} {} minecraft:stone", px - 10, py - 8, pz - 8, px + 16, py + 6, pz + 8)).await;
            let _ = rcon_driving(bot, rcon, &format!("fill {} {} {} {} {} {} minecraft:air", px - 2, py, pz - 3, px + 9, py + 2, pz + 3)).await;
            // 5x5 flush SOURCE pool at floor level (py-1), 4 blocks +X of the stand. 25 sources so
            // scooping 10+ times (sources are consumed per scoop) never depletes it mid-frame.
            let _ = rcon_driving(bot, rcon, &format!("fill {} {} {} {} {} {} minecraft:lava", px + 4, py - 1, pz - 2, px + 8, py - 1, pz + 2)).await;
            // Bot stands at (px,py,pz): feet at py on the stone floor (py-1), the pool source at py-1
            // is FLUSH (one below the feet) with air above — a shallow horizontal scoop. Spawnpoint
            // here so any mishap-death respawns in the arena, not at world spawn.
            let _ = rcon_driving(bot, rcon, &format!("spawnpoint {name} {px} {py} {pz}")).await;
            let _ = rcon_driving(bot, rcon, &format!("tp {name} {px} {py} {pz}")).await;
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
                let _ = rcon_driving(bot, rcon, &format!("tp {name} {px} {py} {pz}")).await;
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
        let _ = rcon_driving(bot, rcon, &format!("forceload add {} {} {} {}", fx - 24, fz - 24, fx + 24, fz + 24)).await;
        // Clear the whole arena volume (prior trials' obsidian/portal/frame would leave
        // portal_built stale-true and clutter the cast site).
        let _ = rcon_driving(bot, rcon, &format!("fill {} {} {} {} {} {} minecraft:air", fx - 4, fy, fz - 8, fx + 20, fy + 22, fz + 8)).await;
        let _ = rcon_driving(bot, rcon, &format!("fill {} {} {} {} {} {} minecraft:air replace minecraft:obsidian", fx - 4, fy - 3, fz - 8, fx + 20, fy + 22, fz + 8)).await;
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
            let _ = rcon_driving(bot, rcon, &format!("fill {} {} {} {} {} {} minecraft:stone", fx - 10, fy - 8, fz - 8, fx + 16, fy + 6, fz + 8)).await;
            // Air pocket is intentionally 3-TALL + TIGHT in z (fz-3..fz+3): the cast DIGS UP into the
            // stone ceiling as it builds the upper frame rows, and the tight +Z wall CONSTRAINS the bot
            // to the frame stand so it can't drift (widening it to fz+7 let the bot wander to z=602.6 →
            // `centered=false` even on the bottom row). This is the documented working DEEPSEA geometry
            // (memory 18bl passed 10/10+enter). Do NOT widen/heighten it — that oscillates the failure.
            // +Z extent must include the cast's STAGING cell (stand_z+3 ≈ fz+4) or the bot stages IN
            // the stone wall and suffocates/POS-FAILs (`z=604.2, off=2.99` at fz+3). fz+5 fits staging
            // with 1 cell margin — NOT wider (fz+7 let it drift off the stand → centered=false). This
            // is the sweet spot between "tight = suffocate at staging" and "wide = drift, no center".
            let _ = rcon_driving(bot, rcon, &format!("fill {} {} {} {} {} {} minecraft:air", fx - 2, fy, fz - 3, fx + 9, fy + 2, fz + 5)).await; // bot pocket + cave over the pool
            // 5x5 flush SOURCE pool (25 sources) — big enough that scooping 10+ times doesn't
            // DEPLETE it (lava sources are consumed per scoop, unlike water; a 9-source pool ran dry
            // mid-frame → "fill lava: all rounds failed"). Real underground pools are usually larger.
            let _ = rcon_driving(bot, rcon, &format!("fill {} {} {} {} {} {} minecraft:lava", fx + 4, fy - 1, fz - 2, fx + 8, fy - 1, fz + 2)).await;
            let _ = rcon_driving(bot, rcon, &format!("tp {name} {fx} {fy} {fz}")).await;
            let _ = rcon_driving(bot, rcon, &format!("spawnpoint {name} {fx} {fy} {fz}")).await;
            pump_teleport(bot, fx, fz).await;
            bot.wait_ticks(10).await.ok();
            let p = bot.entity.position;
            return (p.x.floor() as i32, p.y.floor() as i32, p.z.floor() as i32, fx, fz);
        }
        // 2-deep flat stone pad.
        let _ = rcon_driving(bot, rcon, &format!("fill {} {} {} {} {} {} minecraft:stone", fx - 4, fy - 2, fz - 8, fx + 20, fy - 1, fz + 8)).await;
        // FLUSH 11x11 lava lake at floor level (fy-1), EAST of the bot at (fx,fy,fz) — the EXACT
        // arena isolation-test.sh's build_nether_portal uses, which reaches 10/10+lit reliably.
        // The bot stands 4 blocks west of the lake edge on solid stone; prepare_cast_site
        // approaches to "2 away" (stays on stone), scoops the edge, and anchors the frame there.
        // Earlier gym divergences broke this: a small 7-wide pool with a CENTRE POI made the bot
        // stand IN lava and die; a too-wide lake made the frame anchor over lava (POS FAIL).
        let _ = rcon_driving(bot, rcon, &format!("fill {} {} {} {} {} {} minecraft:lava", fx + 4, fy - 1, fz - 7, fx + 14, fy - 1, fz + 7)).await;
        // Put the bot on the pad (RCON runs as console/op) and set its spawnpoint HERE, so a
        // mishap-death (lava tick) respawns it back in the arena instead of world spawn (where
        // there's no lava → "NO scoopable" for the rest of the trial). Matches isolation line 137.
        let _ = rcon_driving(bot, rcon, &format!("tp {name} {fx} {fy} {fz}")).await;
        let _ = rcon_driving(bot, rcon, &format!("spawnpoint {name} {fx} {fy} {fz}")).await;
        pump_teleport(bot, fx, fz).await;
        bot.wait_ticks(10).await.ok();
        let p = bot.entity.position;
        return (p.x.floor() as i32, p.y.floor() as i32, p.z.floor() as i32, fx, fz);
    }
    let p = bot.entity.position;
    (p.x.floor() as i32, p.y.floor() as i32, p.z.floor() as i32, cx, cz)
}

/// Pump network events after a teleport so chunks load and position is current.
/// Run one RCON command while DRIVING the bot's connection. `spreadplayers` into a fresh region
/// makes the server generate terrain and can take 40+ s; awaiting it bare starved the bot's
/// keep-alive (cycle 2: `LOOP STALL 46756 ms` → `CONNECTION LOST` at trial start in region 92).
/// Wait until the forceloaded area (corners + centre) is loaded before any RCON `fill` touches it.
/// Forceload tickets load and generate chunks asynchronously, but a `fill` on a chunk that isn't
/// loaded yet generates it synchronously on the server thread. On a fresh world, seven water slugs
/// filling at once made a single tick take 60 s, and the watchdog crashed Server B
/// (2026-10-03 08:36 UTC, server thread in ServerChunkCache.getChunk).
async fn wait_area_loaded(bot: &mut Bot<'_>, rcon: &mut RconClient, x0: i32, z0: i32, x1: i32, z1: i32) -> bool {
    let pts = [(x0, z0), (x0, z1), (x1, z0), (x1, z1), ((x0 + x1) / 2, (z0 + z1) / 2)];
    for _ in 0..90 {
        let mut all = true;
        for &(x, z) in &pts {
            if !rcon_driving(bot, rcon, &format!("execute if loaded {x} 0 {z}")).await.contains("passed") {
                all = false;
                break;
            }
        }
        if all {
            return true;
        }
        bot.wait_ticks(20).await.ok();
    }
    println!("[gym] area ({x0},{z0})..({x1},{z1}) not loaded after 90 s — filling anyway");
    false
}

async fn rcon_driving(bot: &mut Bot<'_>, rcon: &mut RconClient, cmd: &str) -> String {
    use std::future::Future;
    use std::task::Poll;
    let fut = rcon.command(cmd);
    tokio::pin!(fut);
    loop {
        // Poll the RCON future once without blocking; drive_tick always runs to completion
        // (never cancelled mid-packet, unlike a select! branch).
        let done = std::future::poll_fn(|cx| match fut.as_mut().poll(cx) {
            Poll::Ready(v) => Poll::Ready(Some(v)),
            Poll::Pending => Poll::Ready(None),
        })
        .await;
        if let Some(r) = done {
            return r.unwrap_or_default();
        }
        if matches!(bot.drive_tick().await, Ok(crate::bot::DriveStep::Disconnected) | Err(_)) {
            return String::new();
        }
    }
}

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
