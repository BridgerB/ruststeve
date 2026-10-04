// The state/step model carries fields (sword, furnace, vitals, priority) used by
// later speedrun phases that aren't ported yet; allow them until then.

//! ruststeve — Ender Dragon speedrun bot, built on rustcraft. Single-bot tick
//! loop: sync state → pick the next incomplete step → execute it. Port of the
//! single-bot core of steve's `main.ts`.
//!
//! Env: MC_HOST, MC_PORT, MC_USERNAME, STEVE_DATA (registry dir, default
//! `rustcraft/data`). Run: `cargo run`.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use crate::bot::{Bot, BotEvent};
use crate::protocol::ClientOptions;
use crate::registry::{create_registry, BlockCollisionShapes, Registry};


use crate::memory::WorldMemory;
use crate::state::{is_dragon_dead, sync_from_bot};
use crate::steps::{execute_step, get_next_step, progress};

fn env(key: &str, default: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| default.to_string())
}

pub async fn run() -> std::io::Result<()> {
    // GYM=report: print the per-slug pass table from data/gym.db and exit (no bot).
    if std::env::var("GYM").as_deref() == Ok("report") {
        crate::gym::report();
        return Ok(());
    }
    // RACE_REPORT=1: print the post-run analysis from data/race.db and exit (no bot).
    if std::env::var("RACE_REPORT").is_ok() {
        crate::telemetry::report();
        return Ok(());
    }

    let host = env("MC_HOST", "localhost");
    let port: u16 = env("MC_PORT", "25565").parse().unwrap_or(25565);
    let username = env("MC_USERNAME", "ruststeve-001");
    let data_dir = env("STEVE_DATA", "data");

    let registry = create_registry(&data_dir, "26.1.2").unwrap_or_else(|_| {
        eprintln!("(no registry at {data_dir} — run datagen; using empty registry)");
        Registry::build(
            vec![], vec![], vec![], vec![], vec![], vec![],
            BlockCollisionShapes::default(), HashMap::new(), "26.1.2",
        )
    });
    println!("registry: {} blocks, {} items", registry.blocks_array.len(), registry.items_array.len());

    // Persistent world memory (SQLite), per bot so racing bots don't share a db.
    let mem_path = std::path::PathBuf::from(format!(".memory-{username}.db"));
    let mut memory = WorldMemory::open(&mem_path);
    println!("memory: {} POIs remembered (db {})", memory.len(), mem_path.display());
    // On the race path (RACE_GOAL/RACE_DB set), route telemetry into the shared, queryable
    // data/race.db. Gym/isolation never set these, so their per-bot events stay as-is.
    if std::env::var("RACE_GOAL").is_ok() || std::env::var("RACE_DB").is_ok() {
        memory.attach_race_log(&username);
    }
    crate::learn::close_orphan(memory.race_log());
    memory.log("session", "start", &format!("{host}:{port} as {username}"));

    println!("connecting to {host}:{port} as {username}…");
    let options = ClientOptions { host, port, username, access_token: None, uuid: None };
    let mut bot = Bot::connect(options, &registry).await?;

    // Wait for spawn + a few chunks so the world is queryable.
    let mut chunks = 0;
    loop {
        match bot.next_event().await? {
            Some(BotEvent::Spawn) => println!("spawned at {:?}", bot.entity.position),
            Some(BotEvent::ChunkLoad(..)) => {
                chunks += 1;
                if chunks >= 12 {
                    break;
                }
            }
            Some(BotEvent::Death) => {
                // Joined dead (e.g. suffocating at an underground logout spot) —
                // respawn (sends us to world spawn) so chunks + a live Spawn arrive.
                println!("died on join — respawning");
                memory.log("session", "respawn", "dead on join");
                bot.respawn().await.ok();
            }
            Some(BotEvent::Kicked(r)) => {
                println!("kicked: {r}");
                return Ok(());
            }
            None => {
                println!("disconnected before spawn");
                return Ok(());
            }
            _ => {}
        }
    }

    // Optional: teleport to a starting position (like steve's MCP spawnBot) so
    // the bot can be dropped at a real forest instead of a hazard spawn. Needs op.
    // MC_TP="x y z".
    if let Ok(tp) = std::env::var("MC_TP") {
        let parts: Vec<&str> = tp.split_whitespace().collect();
        if parts.len() == 3 {
            println!("teleporting to {tp} …");
            let me = bot.username().to_string();
            bot.run_command(&format!("tp {} {} {} {}", me, parts[0], parts[1], parts[2])).await.ok();
            for _ in 0..60 {
                bot.drive_tick().await.ok();
            }
            // reload chunks at the new location
            let mut c = 0;
            while c < 8 {
                if let Ok(Some(BotEvent::ChunkLoad(..))) = bot.next_event().await {
                    c += 1;
                }
            }
            println!("now at {:?}", bot.entity.position);
        }
    }

    // NETHER_START="x y z": drop the bot straight into the nether onto a launch pad (the
    // post-nether combat harness). The old launch-then-RCON-tp path RACED the bot's startup —
    // the bot spawned at a stale/high spawnpoint and started wandering before the tp landed.
    // Doing it here (bot is op'd, over the game connection) is deterministic: cross-dimension
    // tp + anchor the spawnpoint at the pad so deaths respawn back at the fight, not overworld.
    if let Ok(ns) = std::env::var("NETHER_START") {
        let parts: Vec<&str> = ns.split_whitespace().collect();
        if parts.len() == 3 {
            let me = bot.username().to_string();
            println!("nether-start: tp into the nether at {ns} …");
            bot.run_command(&format!("execute in minecraft:the_nether run tp {} {} {} {}", me, parts[0], parts[1], parts[2])).await.ok();
            bot.run_command(&format!("execute in minecraft:the_nether run spawnpoint {} {} {} {}", me, parts[0], parts[1], parts[2])).await.ok();
            // The cross-dimension tp streams the nether chunks with a DELAY — right after it the
            // bot's world is still empty (block_at = air), so if the step loop starts now it flails
            // in a void world and wanders off. WAIT until the world is actually populated under the
            // bot (a solid block below its feet) or ~20s, driving the tick loop to ingest chunks.
            let tx: i32 = parts[0].parse().unwrap_or(0);
            let tz: i32 = parts[2].parse().unwrap_or(0);
            let ty: i32 = parts[1].parse().unwrap_or(64);
            let loaded_deadline = Instant::now() + Duration::from_secs(20);
            loop {
                bot.drive_tick().await.ok();
                let feet_solid = bot
                    .block_at(tx, ty - 1, tz)
                    .map(|b| b.name != "air" && b.name != "cave_air" && b.name != "void_air")
                    .unwrap_or(false);
                if feet_solid || Instant::now() > loaded_deadline {
                    break;
                }
            }
            // Re-center: the desync during loading may have slid the bot off the target; put it back.
            bot.run_command(&format!("execute in minecraft:the_nether run tp {} {} {} {}", me, parts[0], parts[1], parts[2])).await.ok();
            for _ in 0..20 {
                bot.drive_tick().await.ok();
            }
            println!("nether-start: now at {:?} dim={} feet={:?}", bot.entity.position, bot.game.dimension, bot.block_at(tx, ty - 1, tz).map(|b| b.name));
        }
    }

    // Race positioning: hold here (alive, idle) so the orchestrator can teleport
    // us into our lane before we start gathering. RACE_HOLD=seconds.
    if let Ok(hold) = std::env::var("RACE_HOLD") {
        let secs: u64 = hold.parse().unwrap_or(0);
        println!("holding {secs}s for race positioning…");
        let until = Instant::now() + Duration::from_secs(secs);
        while Instant::now() < until {
            bot.drive_tick().await.ok();
        }
        println!("hold done — at {:?}", bot.entity.position);
    }

    // ── GYM MODE ─────────────────────────────────────────────────────────────
    // GYM=<slug> [GYM_TRIALS=n]: run one gym exercise across N random-terrain trials,
    // recording pass/duration/xyz to data/gym.db. Self-RCONs the teleport + prereqs.
    if let Ok(slug) = std::env::var("GYM") {
        let trials: u32 =
            std::env::var("GYM_TRIALS").ok().and_then(|s| s.parse().ok()).unwrap_or(1);
        if slug == "random" {
            // Steve's sweep: pick a random pipeline step each trial. Launch several
            // bots (rust-gym-001..004) for 4-at-a-time coverage.
            crate::gym::run_random(&mut bot, &mut memory, trials).await?;
        } else {
            crate::gym::run(&mut bot, &mut memory, &slug, trials).await?;
        }
        return Ok(());
    }

    // ── ISOLATION TEST MODE ──────────────────────────────────────────────────
    // STEVE_TEST=<step_id>: run ONLY that step (its prerequisites are given by the
    // test harness via RCON before the run) until the step's is_complete check
    // passes (PASS) or STEVE_TEST_SECS elapses (FAIL), then print a machine-readable
    // TEST RESULT line and exit. Lets any task be validated in isolation — no full
    // gather→…→goal chain needed to test one step.
    if let Ok(step_id) = std::env::var("STEVE_TEST") {
        let secs: u64 =
            std::env::var("STEVE_TEST_SECS").ok().and_then(|s| s.parse().ok()).unwrap_or(180);
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(secs);
        let Some(step) = crate::steps::STEPS.iter().find(|s| s.id == step_id) else {
            println!("TEST RESULT: FAIL {step_id} (unknown step)");
            return Ok(());
        };
        let mut attempts = 0;
        loop {
            bot.wait_ticks(6).await?;
            let state = sync_from_bot(&bot);
            if (step.is_complete)(&state) {
                println!("TEST RESULT: PASS {step_id} (after {attempts} attempts)");
                bot.run_command(&format!("say TEST PASS {step_id}")).await.ok();
                return Ok(());
            }
            if std::time::Instant::now() > deadline {
                println!("TEST RESULT: FAIL {step_id} (timeout, {attempts} attempts)");
                return Ok(());
            }
            if !state.alive {
                println!("[test {step_id}] died — respawning");
                bot.respawn().await.ok();
                bot.wait_ticks(40).await.ok();
                continue;
            }
            if crate::survival::handle_survival(&mut bot, &mut memory).await {
                continue;
            }
            attempts += 1;
            println!("[test {step_id}] attempt {attempts}");
            let r = crate::steps::execute_step(&mut bot, &step_id, &mut memory).await;
            println!("    {} — {}", if r.success { "ok" } else { "fail" }, r.message);
            if r.message.contains("Broken pipe")
                || r.message.contains("os error 32")
                || r.message.contains("disconnect")
            {
                println!("TEST RESULT: FAIL {step_id} (connection lost)");
                return Ok(());
            }
        }
    }

    println!("world ready — starting speedrun loop");

    // Anchor our own respawn point at this safe surface spot. A death mid-cast at the
    // deep lava (y≈-57) otherwise respawns at world-spawn / underground (y-43), where the
    // bot gets stuck re-gathering wood it can't find below ground. race.sh sets a lane
    // spawnpoint over RCON on relaunch — but when the server is restarting RCON is DOWN,
    // so that set silently fails and the bot runs with NO spawnpoint (observed: SpawnXYZ
    // empty, 75 deaths, manual tp resets). Setting it here goes over the GAME connection
    // (the bot is op'd), so it holds even when RCON is down. Guard on surface height so we
    // never anchor the spawnpoint underground (e.g. if world-ready fires after a deep
    // reconnect); if we're below ground, leave whatever spawnpoint exists.
    {
        let p = bot.entity.position;
        // Surface here is ~y55-75; the deep-lava cast deaths are at y<0. A y>=45 gate anchors
        // a surface/shallow spawnpoint the bot can recover from and never one at the lava.
        // Skip when NETHER_START set it already (it anchored the nether pad; re-anchoring here
        // in whatever dimension we're in would clobber it).
        if p.y >= 45.0 && std::env::var("NETHER_START").is_err() {
            let me = bot.username().to_string();
            bot.run_command(&format!(
                "spawnpoint {} {} {} {}",
                me,
                p.x.floor() as i32,
                p.y.floor() as i32,
                p.z.floor() as i32
            ))
            .await
            .ok();
            println!("set own spawnpoint at ({:.0},{:.0},{:.0})", p.x, p.y, p.z);
        }
    }

    let mut idle = 0;
    // Stuck-guard state: the last (done, inventory-sum, pickaxe-rank) seen on a failing step.
    // We bail when failures pile up with NO change to this — see the guard below.
    let mut last_prog_sig: (usize, i32, i32, i32) = (usize::MAX, -1, -1, -1);
    let mut same_fail = 0u32;
    let mut last_death: Option<(i32, i32, i32)> = None;
    let mut same_death = 0u32;
    let mut deaths_recorded = 0u32;
    let mut goal_reached = false;
    // Cycle 4: one open step attempt (event row, budget, stall detector). See learn.rs.
    let mut attempt: Option<crate::learn::Attempt> = None;
    loop {
        // Let packets settle so inventory/position are current.
        bot.wait_ticks(6).await?;
        let state = sync_from_bot(&bot);
        // DEATHS from the SDK counter (health-packet edge) — the single source. Steps and the
        // survival escape respawn internally, so polling `alive` here missed deaths (the gym saw
        // 0 while the server logged two drownings). Position = where the bot is now.
        while deaths_recorded < bot.deaths {
            memory.race_death(&state);
            deaths_recorded += 1;
        }
        // Telemetry: throttled state snapshot into data/race.db (no-op off the race path).
        let next = get_next_step(&state);
        let (done, total) = progress(&state);
        memory.race_tick(&state, next.map(|s| s.id), next.map(|s| s.name), done as i32, total as i32);

        if is_dragon_dead(&state) {
            println!("VICTORY — the Ender Dragon is dead!");
            bot.run_command("say I have slain the Ender Dragon!").await.ok();
            break;
        }

        // Died mid-run (drowned/lava/fall) — respawn (to our lane, if a per-player
        // spawnpoint was set) and retry instead of idling out of the race.
        if !state.alive {
            println!("died at {:?} — respawning", state.position);
            // (death recorded from the SDK counter at the top of the loop)
            // Respawn-loop guard: if we keep dying at the EXACT SAME spot, respawn isn't
            // escaping it — a hazardous spawnpoint or a respawn that lands right back in
            // lava/suffocation (rust-race-004 logged 563 deaths at one identical coord).
            // Bail after a few so race.sh relaunches us fresh (new connection + re-tp to
            // the lane + a fresh surface spawnpoint) instead of spinning the whole race.
            let dp = (state.position.0 as i32, state.position.1 as i32, state.position.2 as i32);
            if last_death == Some(dp) {
                same_death += 1;
                if same_death >= 5 {
                    println!("respawn loop — died {same_death}x at {dp:?}, stopping for a fresh relaunch");
                    memory.race_bail("respawn_bail", &state, &format!("{same_death}x at {dp:?}"));
                    break;
                }
            } else {
                same_death = 0;
                last_death = Some(dp);
            }
            bot.respawn().await.ok();
            bot.wait_ticks(40).await.ok(); // let respawn + chunks settle
            continue;
        }

        // Survival reflexes run ABOVE the goal: if a hazard fired, handle it and
        // re-evaluate before doing any task this cycle.
        if crate::survival::handle_survival(&mut bot, &mut memory).await {
            continue;
        }

        // Race finish line: record the win ONCE, then KEEP RUNNING the step machine. Exiting here
        // made the driver relaunch the bot back to its overworld lane — the one natural nether
        // arrival was killed seconds later ("WINNER … cleanup — killing bots") and every step past
        // the goal produced no data. RACE_STOP_ON_GOAL=1 restores the old stop-at-goal behaviour.
        if let Ok(goal) = std::env::var("RACE_GOAL") {
            let reached = match goal.as_str() {
                "nether" => state.world.in_nether(),
                "blaze" => state.inventory.blaze_rods >= 1,
                "iron_pickaxe" => state.equipment.pickaxe_tier().rank() >= 3,
                "stone_pickaxe" => state.equipment.pickaxe_tier().rank() >= 2,
                "wooden_pickaxe" => state.equipment.pickaxe_tier().rank() >= 1,
                _ => false,
            };
            if reached && !goal_reached {
                goal_reached = true;
                println!("RACE GOAL REACHED: {goal}");
                memory.race_win(&goal);
                bot.run_command(&format!("say I reached {goal}!")).await.ok();
                if std::env::var("RACE_STOP_ON_GOAL").is_ok() {
                    break;
                }
            }
        }

        // A different step was chosen: close the open attempt (ok if its step is now complete).
        if let Some(a) = attempt.take() {
            if next.map(|s| s.id) != Some(a.step_id.as_str()) {
                let done_now = crate::steps::STEPS.iter().find(|s| s.id == a.step_id).map(|s| (s.is_complete)(&state)).unwrap_or(false);
                let (o, why) = if done_now { ("ok", String::new()) } else { ("failed", format!("left for {}", next.map(|s| s.id).unwrap_or("none"))) };
                a.finish(&bot, o, &why, memory.race_log());
            } else {
                attempt = Some(a);
            }
        }
        match next {
            Some(step) => {
                idle = 0;
                if attempt.is_none() {
                    attempt = Some(crate::learn::Attempt::start(&bot, step.id, &state, memory.count(crate::memory::PoiKind::IronOre)));
                }
                println!(
                    "[{}] → {} ({done}/{total}) | logs={} planks={} sticks={} pick={:?}",
                    state.world.dimension, step.name,
                    state.inventory.logs, state.inventory.planks, state.inventory.sticks,
                    state.equipment.pickaxe_tier(),
                );
                memory.log(
                    "step",
                    step.id,
                    &format!(
                        "start {}/{} logs={} planks={} sticks={} cobble={} pick={:?} y={:.0}",
                        done, total, state.inventory.logs, state.inventory.planks, state.inventory.sticks,
                        state.inventory.cobblestone, state.equipment.pickaxe_tier(), state.position.1,
                    ),
                );
                // Bound every step in wall-clock: no task may hang the bot. A movement/dig
                // await can wedge indefinitely on nasty terrain (the gym saw mine_iron freeze
                // over water — "underwater — surfacing" then no progress to timeout); in a race
                // that freezes the bot forever, because the stuck-guard below only fires when a
                // step RETURNS. Force a return so the loop re-derives from fresh state. The
                // budget covers each step's own deadline plus slack; the portal cast legitimately
                // runs up to ~900s, everything else settles well under 300s.
                let budget = match step.id {
                    // The portal step does the ~100-block descent to lava (prepare, ~600s) AND
                    // the 10-block cast (~900s) in one call; 960s killed it mid-cast (×7 in the
                    // race). Give it room for both. A bad/uncastable site no longer wastes this —
                    // the cast's 150s wall-clock stall-bail retires the lava and re-sites fast.
                    // Descent (~600s) + the NATURAL 10-block cast (now up to 2700s — slower than the
                    // gym because each block fire-caps + refills from the sea). 3600s covers both so
                    // the outer watchdog doesn't kill a legit slow cast mid-frame (which orphans it).
                    "build_nether_portal" => Duration::from_secs(3600),
                    // Wandering to a fortress and camping the spawner are long by nature; the
                    // tasks self-time-box (45s / 150s) and return so the loop re-derives, but give
                    // the outer watchdog headroom so a single sweep/fight isn't cut short.
                    "find_fortress" | "kill_blaze" => Duration::from_secs(300),
                    _ => Duration::from_secs(330),
                };
                let r = match tokio::time::timeout(budget, execute_step(&mut bot, step.id, &mut memory)).await {
                    Ok(r) => r,
                    Err(_) => crate::types::failure(format!("{} exceeded {}s — re-deriving", step.id, budget.as_secs())),
                };
                memory.race_step_result(step.id, r.success, &r.message);
                {
                    let st = sync_from_bot(&bot);
                    let verdict = attempt.as_mut().map(|a| a.observe(&bot, &st, r.success, &r.message));
                    if let Some(crate::learn::Verdict::Stall(why)) = verdict {
                        println!("    stall: {} — {why}; next attempt changes heading/site/band", step.id);
                        memory.race_event("stall", step.id, Some(step.id), &why, &st, 1);
                        if let Some(a) = attempt.take() {
                            a.finish(&bot, "timeout", &why, memory.race_log());
                        }
                    }
                }
                // Breath watchdog accounting: every pre-emption (watchdog took jump) and alarm
                // (still under after 6 s → movement aborted) goes to race.db for the rate report.
                let pre = std::mem::take(&mut bot.breath_preempts);
                if pre > 0 || bot.breath_alarm {
                    let st = sync_from_bot(&bot);
                    memory.race_event("breath", if bot.breath_alarm { "alarm" } else { "preempt" }, Some(step.id), "", &st, pre as i64);
                }
                // Cast-internal deaths are now counted by the SDK (see the top of the loop); drain
                // the old counter so it doesn't grow.
                let _ = crate::tasks::portal::take_cast_deaths();
                println!("    {} — {}", if r.success { "ok" } else { "fail" }, r.message);
                // Connection lost (e.g. the server restarted out from under us): a
                // step that failed on a dead socket reports "Broken pipe"/os error 32,
                // which the craft path CATCHES — so without this the bot zombie-loops
                // forever on a dead connection (seen: 7877 broken-pipe craft failures)
                // and the race orchestrator never sees it exit to start a fresh round.
                // Bail so race-loop respawns us against the live server.
                let m = &r.message;
                if m.contains("disconnect")
                    || m.contains("Broken pipe")
                    || m.contains("os error 32")
                    || m.contains("Connection reset")
                {
                    println!("connection lost — stopping");
                    break;
                }
                // Stuck-guard: bail when a step keeps failing with NO PROGRESS. "Progress" =
                // the completed-step count OR the inventory OR the pickaxe tier changed since
                // the last failure. Keying on progress (not identical message text) is what
                // catches a bot that ALTERNATES failure messages with nothing changing — a
                // flaky container ping-ponging "result never appeared"/"would not open" (008's
                // 433-loop), a half-open connection spinning "no portal found" offline, or a
                // stalled cast — while still letting a slowly-advancing step continue (mine_iron
                // "2/12"→"5/12" bumps the inventory sum, so it resets). Bailing relaunches fresh
                // (new connection reloads chunks / a fresh table / new terrain).
                let iv = &state.inventory;
                let inv_sum = iv.logs + iv.planks + iv.sticks + iv.cobblestone + iv.dirt
                    + iv.coal + iv.iron_ore + iv.iron_ingots + iv.diamonds + iv.food
                    + iv.crafting_tables + iv.buckets + iv.water_buckets + iv.flint + iv.flint_and_steel;
                // Fold in the portal frame's obsidian count: placed obsidian lives in the WORLD, not
                // the inventory, so a slowly-advancing cast (2/10→3/10) otherwise looks like "no
                // progress" and trips the stuck-bail → relaunch → re-hunt lava → orphaned frame. This
                // makes each obsidian reset the fail counter, so the frame can grind over many step
                // re-derives (it persists in-world) without ever relaunching.
                let prog_sig = (done, inv_sum, state.equipment.pickaxe_tier().rank(), crate::tasks::portal::frame_obsidian_count(&bot));
                if r.success || prog_sig != last_prog_sig {
                    same_fail = 0;
                    last_prog_sig = prog_sig;
                } else {
                    same_fail += 1;
                    if same_fail >= 20 {
                        println!("stuck — {same_fail} failures with no progress ({m}) — stopping for a fresh relaunch");
                        memory.race_bail("stuck_bail", &state, &format!("{m} x{same_fail}"));
                        break;
                    }
                }
            }
            None => {
                idle += 1;
                if idle == 1 {
                    println!("(no available step — waiting)");
                }
                bot.wait_ticks(20).await?;
                if idle > 200 {
                    println!("idle too long — stopping");
                    break;
                }
            }
        }
    }
    if let Some(a) = attempt.take() {
        a.finish(&bot, "failed", "process exiting (bail, goal stop or lost connection)", memory.race_log());
    }
    Ok(())
}
