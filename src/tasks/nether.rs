//! Nether-fortress tasks — wander to a fortress, then hunt blazes at the spawner
//! for a blaze rod. Port of steve's `tasks/nether/main.ts` (`findFortress` /
//! `killBlazes`), hardened for real nether terrain (lava seas, fire damage).

use std::time::{Duration, Instant};

use crate::bot::{Bot, Face};
use crate::bot_utils::{collect_drops, count_items, select_item};
use crate::memory::WorldMemory;
use crate::types::{failure, success, StepResult};
use crate::vec3::{vec3, Vec3};

/// Nether-brick blocks that mark a fortress (scanned to detect arrival).
const FORTRESS_BLOCKS: [&str; 3] = ["nether_bricks", "nether_brick_fence", "nether_brick_stairs"];
/// Swords worst→best-last so we equip the best available.
const SWORDS: [&str; 6] =
    ["netherite_sword", "diamond_sword", "iron_sword", "stone_sword", "golden_sword", "wooden_sword"];
/// Which +X/-X way we're currently sweeping for the fortress (fortresses align on X).
/// Persists across the per-tick task calls so we don't thrash direction every tick.
static WANDER_DIR: std::sync::Mutex<i32> = std::sync::Mutex::new(1);

fn name_at(bot: &Bot, x: i32, y: i32, z: i32) -> String {
    bot.block_at(x, y, z).map(|b| b.name).unwrap_or_else(|| "air".into())
}
fn is_lava(n: &str) -> bool {
    n == "lava" || n == "flowing_lava"
}
fn is_air(n: &str) -> bool {
    n == "air" || n == "cave_air" || n == "void_air"
}
fn feet_y(bot: &Bot) -> i32 {
    (bot.entity.position.y - 0.5).floor() as i32
}
fn horiz_dist(a: Vec3, b: Vec3) -> f64 {
    ((a.x - b.x).powi(2) + (a.z - b.z).powi(2)).sqrt()
}

/// The nearest fortress (nether-brick) block within `dist`, by a direct block scan — NOT
/// `find_block`, which requires line-of-sight to an EXPOSED block (anti-X-ray) and so returns
/// None for the very floor the bot stands on (confirmed live: dead-centre on an 11x11 brick
/// floor, find_block=None). "Am I at a fortress?" just needs brick nearby, no LOS.
fn see_fortress(bot: &Bot, dist: i32) -> Option<(i32, i32, i32)> {
    let p = bot.entity.position;
    let (ox, oy, oz) = (p.x.floor() as i32, p.y.floor() as i32, p.z.floor() as i32);
    for r in 0..=dist {
        for dx in -r..=r {
            for dz in -r..=r {
                // Only the shell at radius r (nearest-first) — skip the interior already scanned.
                if dx.abs() != r && dz.abs() != r {
                    continue;
                }
                for dy in -4..=4 {
                    let (x, y, z) = (ox + dx, oy + dy, oz + dz);
                    if FORTRESS_BLOCKS.contains(&name_at(bot, x, y, z).as_str()) {
                        return Some((x, y, z));
                    }
                }
            }
        }
    }
    None
}

/// Eat to recover health/hunger so natural regen stays on between hits. Minimal
/// port of the portal cast's `eat_if_hurt` (blazes + nether fire chew health fast).
async fn eat_if_hurt(bot: &mut Bot<'_>) {
    if bot.health >= 17.0 {
        return;
    }
    const FOODS: [&str; 8] =
        ["cooked_beef", "cooked_porkchop", "cooked_mutton", "cooked_chicken", "bread", "golden_apple", "beef", "porkchop"];
    let Some(food) = FOODS.iter().find(|f| count_items(bot, f) > 0) else {
        return;
    };
    if !select_item(bot, food).await.unwrap_or(false) {
        return;
    }
    for _ in 0..4 {
        if bot.health >= 18.0 {
            break;
        }
        let p = bot.entity.position;
        bot.look_at(vec3(p.x, p.y + 1.0, p.z + 3.0)); // look flat so we don't interact with a block
        bot.wait_ticks(2).await.ok();
        bot.activate_item().await.ok();
        bot.wait_ticks(40).await.ok();
    }
}

/// Search for a nether fortress by sweeping along the X axis (fortresses generate in
/// X-aligned bands), stopping when nether brick comes into view. Hardened vs steve's
/// raw sprint: bridge cobble over lava gaps and bail off lava rather than walk in.
pub async fn find_fortress(bot: &mut Bot<'_>, _mem: &mut WorldMemory) -> StepResult {
    {
        let p = bot.entity.position;
        let (ox, oy, oz) = (p.x.floor() as i32, p.y.floor() as i32, p.z.floor() as i32);
        let feet = name_at(bot, ox, oy - 1, oz);
        let mut solid = 0;
        for dx in -5..=5 {
            for dy in -3..=3 {
                for dz in -5..=5 {
                    if !is_air(&name_at(bot, ox + dx, oy + dy, oz + dz)) {
                        solid += 1;
                    }
                }
            }
        }
        println!("    [dbg] find_fortress at ({:.1},{:.1},{:.1}) feet-block={feet} solid_in_11x7x11={solid} see_brick={:?}", p.x, p.y, p.z, see_fortress(bot, 8));
    }
    // Already see nether brick → NAVIGATE onto it (the pathfinder handles the rough nether
    // terrain the raw sprint can't). Being AT the fortress is what lets kill_blaze's spawner
    // scan reach the spawner; just spotting brick 60 blocks off isn't enough.
    if let Some(pos) = see_fortress(bot, 64) {
        let _ = bot.goto_near(pos.0, pos.1, pos.2, 3.0).await;
        return success(format!("reached fortress near {pos:?}"));
    }

    // PATHFIND toward the known fortress region if the harness hinted it (FORTRESS_HINT="x y z").
    // The raw sprint below can't cross nether terrain (cliffs/lava sea) — it wedges at the pad —
    // so the pathfinder is what actually delivers the bot into brick-scan range. Not "cheating"
    // the hunt: it only gets us to the fortress; finding the spawner + killing a blaze is the task.
    if let Ok(h) = std::env::var("FORTRESS_HINT") {
        let c: Vec<i32> = h.split_whitespace().filter_map(|s| s.parse().ok()).collect();
        if c.len() == 3 {
            let _ = bot.goto_near(c[0], c[1], c[2], 6.0).await;
            if let Some(pos) = see_fortress(bot, 64) {
                let _ = bot.goto_near(pos.0, pos.1, pos.2, 3.0).await;
                return success(format!("reached fortress near {pos:?}"));
            }
        }
    }

    let deadline = Instant::now() + Duration::from_secs(45);
    let dir = { *WANDER_DIR.lock().unwrap() };
    let start = bot.entity.position;
    bot.set_control_state("forward", true);
    bot.set_control_state("sprint", true);

    while Instant::now() < deadline {
        // Reached the fortress? Navigate onto it (see note above).
        if let Some(pos) = see_fortress(bot, 48) {
            bot.set_control_state("forward", false);
            bot.set_control_state("sprint", false);
            let _ = bot.goto_near(pos.0, pos.1, pos.2, 3.0).await;
            return success(format!("reached fortress near {pos:?}"));
        }
        // Keep heading along X toward the sweep direction.
        let p = bot.entity.position;
        bot.look_at(vec3(p.x + dir as f64 * 64.0, p.y, p.z));

        // Footing check: the cell we're about to step onto (one ahead in X, at foot level).
        let (fx, fz) = (p.x.floor() as i32, p.z.floor() as i32);
        let fy = feet_y(bot);
        let ahead = (fx + dir, fy - 1, fz);
        let ahead_name = name_at(bot, ahead.0, ahead.1, ahead.2);
        let below_name = name_at(bot, fx, fy - 1, fz);

        if is_lava(&below_name) {
            // Standing over lava — do NOT sprint deeper in; bail so the step retries alive.
            bot.set_control_state("forward", false);
            bot.set_control_state("sprint", false);
            return failure("wander hit lava underfoot — bailing");
        }
        if (is_lava(&ahead_name) || is_air(&ahead_name)) && count_items(bot, "cobblestone") > 0 {
            // Bridge the gap: pause, lay cobble on the ahead cell against our own floor, continue.
            bot.set_control_state("forward", false);
            if select_item(bot, "cobblestone").await.unwrap_or(false) {
                let face = if dir > 0 { Face::East } else { Face::West };
                let _ = bot.place_block(fx, fy - 1, fz, face).await; // places at (fx+dir, fy-1, fz)
                bot.wait_ticks(3).await.ok();
            }
            bot.set_control_state("forward", true);
        }
        bot.wait_ticks(20).await.ok(); // ~1s of travel between scans

        // Swept far enough this direction with no luck → flip the sweep for next time.
        if horiz_dist(bot.entity.position, start) > 180.0 {
            *WANDER_DIR.lock().unwrap() = -dir;
            break;
        }
    }
    bot.set_control_state("forward", false);
    bot.set_control_state("sprint", false);
    if let Some(pos) = see_fortress(bot, 64) {
        return success(format!("found fortress at {pos:?}"));
    }
    failure("no fortress found this sweep — will keep searching")
}

/// Nearest LIVE blaze (id + position), skipping any in `skip` (ghosts/unreachable ones we've
/// already flailed at). A blaze the bot swung a full set at without killing is almost certainly a
/// stale entity the server already removed (missed entity_remove) sitting on top of us — it would
/// otherwise be picked as "nearest" every time and starve the real blazes of attacks.
fn nearest_blaze(bot: &Bot, skip: &std::collections::HashSet<i32>, max_range: f64) -> Option<(i32, Vec3)> {
    let blaze_id = bot.registry.entities_by_name.get("blaze").map(|d| d.id);
    let me = bot.entity.position;
    bot.entities
        .values()
        .filter(|e| e.is_valid && !skip.contains(&e.id) && (e.entity_type == blaze_id || e.name.as_deref() == Some("blaze")))
        // Only blazes we can actually reach — chasing a stray 70 blocks off (a blaze that spawned
        // elsewhere / escaped) walked the bot out over the lava and into the void (died at y-631).
        .filter(|e| horiz_dist(e.position, me) <= max_range)
        .map(|e| (e.id, e.position))
        .min_by(|a, b| {
            horiz_dist(a.1, me).partial_cmp(&horiz_dist(b.1, me)).unwrap_or(std::cmp::Ordering::Equal)
        })
}

/// Hunt blazes at the fortress spawner until we hold `target_rods` blaze rods.
/// Blazes fly + throw fire, so: equip a sword, close to melee, swing until the entity
/// despawns (kill), then collect the dropped rod; when none are around, camp the spawner.
pub async fn kill_blaze(bot: &mut Bot<'_>, _mem: &mut WorldMemory, target_rods: i32) -> StepResult {
    // Equip the best sword we have.
    for s in SWORDS {
        if count_items(bot, s) > 0 && select_item(bot, s).await.unwrap_or(false) {
            break;
        }
    }

    let deadline = Instant::now() + Duration::from_secs(150);
    let mut ghosts: std::collections::HashSet<i32> = std::collections::HashSet::new();
    while count_items(bot, "blaze_rod") < target_rods && Instant::now() < deadline {
        eat_if_hurt(bot).await;

        // Low health: back off the spawner a few blocks and heal before re-engaging.
        if bot.health < 6.0 {
            bot.set_control_state("back", true);
            bot.wait_ticks(12).await.ok();
            bot.set_control_state("back", false);
            eat_if_hurt(bot).await;
        }

        if let Some((id, bpos)) = nearest_blaze(bot, &ghosts, 12.0) {
            if horiz_dist(bpos, bot.entity.position) > 3.0 {
                let _ = bot.goto_near(bpos.x.floor() as i32, bpos.y.floor() as i32, bpos.z.floor() as i32, 2.0).await;
            }
            // STAY ON this blaze until it dies or leaves the arena. Blazes fly, so re-close the gap
            // before each swing; count only in-reach swings — a blaze that survives many of THOSE is
            // a ghost (blacklist), one that flew off is just re-scanned (don't blacklist a live one).
            let mut in_reach_swings = 0;
            let mut killed = false;
            let mut lost = 0;
            for _ in 0..40 {
                let Some(pos) = bot.entities.get(&id).map(|e| e.position) else {
                    killed = true;
                    break;
                };
                let d = ((pos.x - bot.entity.position.x).powi(2)
                    + (pos.y - bot.entity.position.y).powi(2)
                    + (pos.z - bot.entity.position.z).powi(2))
                .sqrt();
                if d > 4.2 {
                    // Out of melee — step toward it. Give up on THIS engagement if it keeps fleeing.
                    let _ = bot.goto_near(pos.x.floor() as i32, pos.y.floor() as i32, pos.z.floor() as i32, 2.0).await;
                    lost += 1;
                    if lost >= 4 {
                        break;
                    }
                    continue;
                }
                lost = 0;
                bot.look_at(vec3(pos.x, pos.y + 0.9, pos.z)); // aim at the blaze body (it hovers)
                if let Err(e) = bot.attack(id).await {
                    let msg = e.to_string();
                    // A dropped connection (Broken pipe) otherwise made the whole task spin on
                    // errors forever (574k in one run) instead of exiting so the watchdog can
                    // relaunch — propagate it as a disconnect the main loop bails on.
                    if msg.contains("Broken pipe") || msg.contains("os error 32") || msg.contains("reset") {
                        return failure(format!("connection lost during combat: {msg}"));
                    }
                    break;
                }
                if in_reach_swings == 0 {
                    println!("    [dbg] engaging blaze {id} dist={d:.1}");
                }
                in_reach_swings += 1;
                // 650ms of REAL time between swings: clears BOTH the mob's 10-tick (0.5s)
                // hurt-invulnerability AND the iron sword's ~12.5-tick full-charge cooldown, so
                // each swing lands full damage. Must be `wait_real_ms`, not `wait_ticks` — under
                // the combat packet flood `wait_ticks` collapsed to <2ms, bursting 40 swings into
                // 66ms so all but one hit an i-frame and the blaze never died (sniffed + confirmed).
                bot.wait_real_ms(650).await.ok();
            }
            if killed {
                println!("    ✔ blaze {id} killed ({in_reach_swings} hits) — rods now {}", count_items(bot, "blaze_rod"));
            } else if in_reach_swings >= 25 {
                // Took 25 in-reach swings without dying → stale/ghost entity; skip it next time.
                ghosts.insert(id);
                println!("    ✗ blaze {id} survived {in_reach_swings} in-reach swings — blacklisting (ghost)");
            }
            // Walk onto the drop so the rod is collected.
            let p = bot.entity.position;
            collect_drops(bot, p.x.floor() as i32, p.z.floor() as i32).await;
            continue;
        }

        // No blaze in view — HOLD POSITION and wait for the spawner to produce more. Do NOT
        // wander: raw-walking "to explore" marched the bot off the fortress ledge into the lava/
        // void (died repeatedly, losing every fight). Standing still at the spawner is correct —
        // blazes come to us.
        bot.set_control_state("forward", false);
        bot.set_control_state("sprint", false);
        bot.wait_ticks(20).await.ok();
    }

    let rods = count_items(bot, "blaze_rod");
    if rods >= target_rods {
        success(format!("killed a blaze — {rods} blaze rod(s)"))
    } else {
        failure(format!("no blaze rod yet ({rods}/{target_rods}) — will keep hunting"))
    }
}
