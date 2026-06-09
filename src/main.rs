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

use rustcraft::bot::{Bot, BotEvent};
use rustcraft::protocol::ClientOptions;
use rustcraft::registry::{create_registry, BlockCollisionShapes, Registry};

mod bot_utils;
mod state;
mod steps;
mod tasks;
mod types;

use state::{is_dragon_dead, sync_from_bot};
use steps::{execute_step, get_next_step, progress};

fn env(key: &str, default: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| default.to_string())
}

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let host = env("MC_HOST", "localhost");
    let port: u16 = env("MC_PORT", "25565").parse().unwrap_or(25565);
    let username = env("MC_USERNAME", "Steve");
    let data_dir = env("STEVE_DATA", "../rustcraft/data");

    let registry = create_registry(&data_dir, "26.1.2").unwrap_or_else(|_| {
        eprintln!("(no registry at {data_dir} — run rustcraft's datagen; using empty registry)");
        Registry::build(
            vec![], vec![], vec![], vec![], vec![], vec![],
            BlockCollisionShapes::default(), HashMap::new(), "26.1.2",
        )
    });
    println!("registry: {} blocks, {} items", registry.blocks_array.len(), registry.items_array.len());

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
                let r = execute_step(&mut bot, step.id).await;
                println!("    {} — {}", if r.success { "ok" } else { "fail" }, r.message);
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
