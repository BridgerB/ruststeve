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
        if p.y >= 45.0 {
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
    let mut last_fail_msg = String::new();
    let mut same_fail = 0u32;
    let mut last_death: Option<(i32, i32, i32)> = None;
    let mut same_death = 0u32;
    loop {
        // Let packets settle so inventory/position are current.
        bot.wait_ticks(6).await?;
        let state = sync_from_bot(&bot);
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
            memory.race_death(&state);
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

        // Race finish line: stop as soon as we reach the goal tool.
        if let Ok(goal) = std::env::var("RACE_GOAL") {
            let reached = match goal.as_str() {
                "nether" => state.world.in_nether(),
                "iron_pickaxe" => state.equipment.pickaxe_tier().rank() >= 3,
                "stone_pickaxe" => state.equipment.pickaxe_tier().rank() >= 2,
                "wooden_pickaxe" => state.equipment.pickaxe_tier().rank() >= 1,
                _ => false,
            };
            if reached {
                println!("RACE GOAL REACHED: {goal}");
                memory.race_win(&goal);
                bot.run_command(&format!("say I reached {goal} — race done!")).await.ok();
                break;
            }
        }

        match next {
            Some(step) => {
                idle = 0;
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
                    "build_nether_portal" => Duration::from_secs(960),
                    _ => Duration::from_secs(330),
                };
                let r = match tokio::time::timeout(budget, execute_step(&mut bot, step.id, &mut memory)).await {
                    Ok(r) => r,
                    Err(_) => crate::types::failure(format!("{} exceeded {}s — re-deriving", step.id, budget.as_secs())),
                };
                memory.race_step_result(step.id, r.success, &r.message);
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
                // Stuck-guard: a step failing with the SAME message repeatedly is wedged with
                // ZERO progress. Two cases this catches: (1) a HALF-OPEN connection — the bot
                // was dropped from the world but the socket stays alive, so wait_ticks doesn't
                // error and find_block sees an empty local world → enter_nether spins "no portal
                // found to enter" forever offline (race.sh won't relaunch a live proc); (2) a
                // task that can't advance at this spot (mine "fail — mined 2/12 iron" frozen on
                // bad terrain). A step that IS progressing changes its message ("2/12"→"3/12"),
                // so identical repeats == no progress. Bail so race.sh relaunches us fresh (new
                // connection re-loads chunks → the built+lit portal is findable; new tp → new
                // terrain). Non-connection stalls never self-recovered before → whole race lost.
                if r.success {
                    same_fail = 0;
                } else if r.message == last_fail_msg {
                    same_fail += 1;
                    if same_fail >= 20 {
                        println!("stuck — same failure x{same_fail} ({m}) — stopping for a fresh relaunch");
                        memory.race_bail("stuck_bail", &state, &format!("{m} x{same_fail}"));
                        break;
                    }
                } else {
                    same_fail = 0;
                    last_fail_msg = r.message.clone();
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
    Ok(())
}
