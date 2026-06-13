// The state/step model carries fields (sword, furnace, vitals, priority) used by
// later speedrun phases that aren't ported yet; allow them until then.
#![allow(dead_code)]

//! ruststeve — Ender Dragon speedrun bot, built on rustcraft. Single-bot tick
//! loop: sync state → pick the next incomplete step → execute it. Port of the
//! single-bot core of steve's `main.ts`.
//!
//! Env: MC_HOST, MC_PORT, MC_USERNAME, STEVE_DATA (registry dir, default
//! `../rustcraft/data`). Run: `cargo run`.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use rustcraft::bot::{Bot, BotEvent};
use rustcraft::protocol::ClientOptions;
use rustcraft::registry::{create_registry, BlockCollisionShapes, Registry};

mod bot_utils;
mod memory;
mod state;
mod steps;
mod survival;
mod tasks;
mod types;

use memory::WorldMemory;
use state::{is_dragon_dead, sync_from_bot};
use steps::{execute_step, get_next_step, progress};

fn env(key: &str, default: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| default.to_string())
}

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let host = env("MC_HOST", "localhost");
    let port: u16 = env("MC_PORT", "25565").parse().unwrap_or(25565);
    let username = env("MC_USERNAME", "ruststeve-001");
    let data_dir = env("STEVE_DATA", "../rustcraft/data");

    let registry = create_registry(&data_dir, "26.1.2").unwrap_or_else(|_| {
        eprintln!("(no registry at {data_dir} — run rustcraft's datagen; using empty registry)");
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

    println!("world ready — starting speedrun loop");
    let mut idle = 0;
    loop {
        // Let packets settle so inventory/position are current.
        bot.wait_ticks(6).await?;
        let state = sync_from_bot(&bot);

        if is_dragon_dead(&state) {
            println!("VICTORY — the Ender Dragon is dead!");
            bot.run_command("say I have slain the Ender Dragon!").await.ok();
            break;
        }

        // Died mid-run (drowned/lava/fall) — respawn (to our lane, if a per-player
        // spawnpoint was set) and retry instead of idling out of the race.
        if !state.alive {
            println!("died at {:?} — respawning", state.position);
            memory.log(
                "session",
                "death",
                &format!("{:.0},{:.0},{:.0}", state.position.0, state.position.1, state.position.2),
            );
            bot.respawn().await.ok();
            bot.wait_ticks(40).await.ok(); // let respawn + chunks settle
            continue;
        }

        // Survival reflexes run ABOVE the goal: if a hazard fired, handle it and
        // re-evaluate before doing any task this cycle.
        if survival::handle_survival(&mut bot, &mut memory).await {
            continue;
        }

        // Race finish line: stop as soon as we reach the goal tool.
        if let Ok(goal) = std::env::var("RACE_GOAL") {
            let reached = match goal.as_str() {
                "iron_pickaxe" => state.equipment.pickaxe_tier().rank() >= 3,
                "stone_pickaxe" => state.equipment.pickaxe_tier().rank() >= 2,
                "wooden_pickaxe" => state.equipment.pickaxe_tier().rank() >= 1,
                _ => false,
            };
            if reached {
                println!("RACE GOAL REACHED: {goal}");
                memory.log("race", "win", &goal);
                bot.run_command(&format!("say I reached {goal} — race done!")).await.ok();
                break;
            }
        }

        match get_next_step(&state) {
            Some(step) => {
                idle = 0;
                let (done, total) = progress(&state);
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
                let r = execute_step(&mut bot, step.id, &mut memory).await;
                memory.log("step", step.id, &format!("{} {}", if r.success { "ok" } else { "fail" }, r.message));
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
