//! Nether-fortress tasks — wander to a fortress, then hunt blazes at the spawner
//! for a blaze rod. Port of steve's `tasks/nether/main.ts` (`findFortress` /
//! `killBlazes`), hardened for real nether terrain (lava seas, fire damage).

use std::time::{Duration, Instant};

use crate::bot::Bot;
use crate::bot_utils::{collect_drops, count_items, select_item};
use crate::memory::WorldMemory;
use crate::tasks::portal::{dig_at, is_solid as is_solid_name, place_cobble, walk_to_xz};
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

/// The nearest block named in `names` anywhere in the LOADED chunks — no line of sight, every
/// height, ±view-distance (96 blocks at Server B's view-distance 6). Cheap: a palette check per
/// 16³ section first, a cell scan only inside sections that hold one of the states
/// (docs/nether-design.md, Idea B — a 192-block-wide fortress search swath from inside a tunnel).
pub(crate) fn loaded_block_nearest(bot: &Bot, names: &[&str]) -> Option<(i32, i32, i32)> {
    // Exposed blocks only (a fortress has thousands; a spawner sits in open air) — same honesty
    // rule as blockSeen, no X-ray through netherrack.
    // Above the lava sea (y 31) only: harness run 3 picked fortress brick at (-180,16,240) — 15 blocks
    // BELOW the sea surface — over a reachable fortress at y≈60.
    bot.find_loaded_nearest(names, (32, 127))
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
/// Consecutive X-sweep flips with no step taken (reset on any successful step).
static FLIPS: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

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
    if let Some(pos) = see_fortress(bot, 12) {
        let _ = bot.goto_near(pos.0, pos.1, pos.2, 3.0).await;
        return success(format!("reached fortress near {pos:?}"));
    }

    // The fortress may already sit in the LOADED chunks (±96 blocks, any height) with no line of
    // sight — cliffs and netherrack hide it from `see_fortress`. Aim the sweep at it and navigate.
    if let Some(pos) = see_fortress(bot, 64).or_else(|| loaded_block_nearest(bot, &FORTRESS_BLOCKS)) {
        let p = bot.entity.position;
        let d = horiz_dist(vec3(pos.0 as f64 + 0.5, pos.1 as f64, pos.2 as f64 + 0.5), p);
        println!("    [dbg] fortress brick in loaded chunks at {pos:?} ({d:.0} blocks) — navigating");
        *WANDER_DIR.lock().unwrap() = if pos.0 as f64 >= p.x { 1 } else { -1 };
        // ENCLOSED approach while far: tunnel toward the brick (dominant axis each step) instead of a
        // pathfinder walk over open terrain — harness 08:52: one bot fell from the open route into the
        // lava sea at the SAME spot (-11.5,28,182) three times (ghast knockback over the sea).
        // Fortress BELOW (blaze_rod gym, cycle 6: brick at y 51 under a y 98 landing, 0 horizontal blocks
        // away): the horizontal tunnel below stops at once and the pathfinder cannot dig down 47 blocks, so
        // the step failed 4× and the trial ended. Dig down to the brick's level first (the portal descent:
        // never onto or beside lava, never into an air pocket), then tunnel across.
        if pos.1 + 3 < crate::tasks::portal::feet_y(bot) {
            let from = crate::tasks::portal::feet_y(bot);
            for _ in 0..4 {
                crate::tasks::portal::descend_to_y(bot, pos.1 + 1).await;
                if crate::tasks::portal::feet_y(bot) <= pos.1 + 3 {
                    break;
                }
            }
            println!("    [dbg] fortress below: descended {from} -> {} toward brick y {}", crate::tasks::portal::feet_y(bot), pos.1);
        }
        let deadline = Instant::now() + Duration::from_secs(45);
        let mut blocked = 0;
        while Instant::now() < deadline {
            let q = bot.entity.position;
            let (ddx, ddz) = (pos.0 as f64 + 0.5 - q.x, pos.2 as f64 + 0.5 - q.z);
            if (ddx * ddx + ddz * ddz).sqrt() <= 12.0 {
                break;
            }
            let (sx, sz) = if ddx.abs() >= ddz.abs() { (ddx.signum() as i32, 0) } else { (0, ddz.signum() as i32) };
            if nether_tunnel_step(bot, sx, sz).await {
                blocked = 0;
            } else {
                blocked += 1;
                if blocked >= 4 {
                    break; // fall back to the pathfinder below
                }
            }
        }
        let _ = bot.goto_near(pos.0, pos.1, pos.2, 3.0).await;
        if see_fortress(bot, 16).is_some() {
            return success(format!("reached fortress near {pos:?}"));
        }
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

    // TUNNEL along ±X at the current height (docs/nether-design.md, Idea A) instead of sprinting
    // across the surface: the sprint walked off ledges into the lava sea (harness run 2: 7 deaths in
    // 90 s). Each step caps any lava touching the next 1×2 cells with cobble, fills a missing floor,
    // digs, and steps in; the loaded-chunk scan (Idea B) watches a ±96-block swath the whole time.
    let deadline = Instant::now() + Duration::from_secs(45);
    let dir = { *WANDER_DIR.lock().unwrap() };
    let start = bot.entity.position;
    let mut stuck = 0;
    while Instant::now() < deadline {
        if let Some(pos) = see_fortress(bot, 48).or_else(|| loaded_block_nearest(bot, &FORTRESS_BLOCKS)) {
            // Never an open-route walk from here (the lava-sea falls): the next call approaches
            // enclosed via the branch above.
            return failure(format!("fortress in view at {pos:?} — enclosed approach next"));
        }
        if bot.health < 12.0 {
            eat_if_hurt(bot).await;
        }
        let (fx, fy, fz) = (bot.entity.position.x.floor() as i32, feet_y(bot), bot.entity.position.z.floor() as i32);
        if !nether_tunnel_step(bot, dir, 0).await {
            stuck += 1;
            // Blocked both ways along X (race i11 rust-race-004 sat in its Nether arrival portal at (4236,89,78):
            // the frame's obsidian on ±X, never dug, and the sweep flipped between them forever). After the first
            // flip, step out along ±Z before sweeping X again.
            if stuck >= 2 && FLIPS.load(std::sync::atomic::Ordering::Relaxed) >= 1 {
                for dz in [1, -1] {
                    if nether_tunnel_step(bot, 0, dz).await && nether_tunnel_step(bot, 0, dz).await {
                        println!("    [dbg] nether: X blocked both ways — stepped out along z {dz}");
                        FLIPS.store(0, std::sync::atomic::Ordering::Relaxed);
                        stuck = 0;
                        break;
                    }
                }
            }
            if stuck >= 4 {
                *WANDER_DIR.lock().unwrap() = -dir;
                FLIPS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                return failure(format!("nether tunnel stuck at ({fx},{fy},{fz}) dir={dir} — flipping"));
            }
        } else {
            stuck = 0;
            FLIPS.store(0, std::sync::atomic::Ordering::Relaxed);
        }
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

/// One 1×2 tunnel step in direction (dx,dz) at the current height: cap any lava touching the next
/// two cells and their floor with cobble, fill a missing floor, dig, walk in. True if the bot moved.
async fn nether_tunnel_step(bot: &mut Bot<'_>, dx: i32, dz: i32) -> bool {
    if bot.health < 12.0 {
        eat_if_hurt(bot).await;
    }
    let p0 = bot.entity.position;
    // Feet from y + 0.01: standing on a block top reads 83.9999…, and floor() put the feet one block low, so
    // the step tested the FLOOR row as "ahead". At race i11 rust-race-004's Nether arrival that row was the
    // portal frame's bottom obsidian on both ±X: "blocked … ahead=obsidian head=air", forever.
    let (fx, fy, fz) = (p0.x.floor() as i32, (p0.y + 0.01).floor() as i32, p0.z.floor() as i32);
    let (ax, az) = (fx + dx, fz + dz);
    let cells = [(ax, fy, az), (ax, fy + 1, az)];
    for c in cells.iter().copied().chain(std::iter::once((ax, fy - 1, az))) {
        for (ox, oy, oz) in [(dx, 0, dz), (0, 1, 0), (0, -1, 0), (dz, 0, dx), (-dz, 0, -dx)] {
            let n = (c.0 + ox, c.1 + oy, c.2 + oz);
            if is_lava(&name_at(bot, n.0, n.1, n.2)) && crate::tasks::portal::scaffold_count(bot) > 0 {
                place_cobble(bot, n).await;
            }
        }
    }
    if !is_solid_name(&name_at(bot, ax, fy - 1, az)) && crate::tasks::portal::scaffold_count(bot) > 0 {
        place_cobble(bot, (ax, fy - 1, az)).await;
    }
    for c in cells {
        let n = name_at(bot, c.0, c.1, c.2);
        if !is_air(&n) && !is_lava(&n) {
            dig_at(bot, c.0, c.1, c.2).await;
        }
    }
    walk_to_xz(bot, ax as f64 + 0.5, az as f64 + 0.5, 0.3, 30).await;
    let p1 = bot.entity.position;
    let moved = (p1.x - p0.x).abs() + (p1.z - p0.z).abs() >= 0.5;
    if !moved {
        // Why a step failed (harness run 5: a bot sat at (80.7,62,147.7) with an unfloored cave ahead).
        println!(
            "    [dbg] nether step ({dx},{dz}) blocked at ({fx},{fy},{fz}): ahead={} head={} floor={} cobble={}",
            name_at(bot, ax, fy, az), name_at(bot, ax, fy + 1, az), name_at(bot, ax, fy - 1, az), count_items(bot, "cobblestone")
        );
    }
    moved
}

/// Nearest LIVE blaze (id + position), skipping any in `skip` (ghosts/unreachable ones we've
/// already flailed at). A blaze the bot swung a full set at without killing is almost certainly a
/// stale entity the server already removed (missed entity_remove) sitting on top of us — it would
/// otherwise be picked as "nearest" every time and starve the real blazes of attacks.
/// The nearest non-blaze hostile within `range` (horizontal): the fortress and wastes mobs that killed blaze
/// hunters in the cycle-6 gym.
fn nearest_hostile(bot: &Bot, range: f64) -> Option<(i32, String)> {
    const HOSTILE: [&str; 6] = ["wither_skeleton", "skeleton", "hoglin", "magma_cube", "zombified_piglin", "piglin_brute"];
    let me = bot.entity.position;
    bot.entities
        .values()
        .filter(|e| e.is_valid && horiz_dist(e.position, me) <= range && (e.position.y - me.y).abs() < 3.0)
        .filter_map(|e| e.name.as_deref().filter(|n| HOSTILE.contains(n)).map(|n| (e.id, n.to_string(), horiz_dist(e.position, me))))
        .min_by(|a, b| a.2.total_cmp(&b.2))
        .map(|(id, n, _)| (id, n))
}

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
        if bot.health < 12.0 {
            bot.set_control_state("back", true);
            bot.wait_ticks(12).await.ok();
            bot.set_control_state("back", false);
            eat_if_hurt(bot).await;
        }

        // DEFEND first (cycle 6 blaze_rod gym: 3 of 10 deaths to wither skeletons the hunt never fought).
        // Any other hostile within 4 blocks gets the sword until it dies or backs off.
        if let Some((hid, name)) = nearest_hostile(bot, 4.0) {
            println!("    [dbg] defending against {name} {hid}");
            for _ in 0..12 {
                let Some(hp) = bot.entities.get(&hid).map(|e| e.position) else { break };
                if horiz_dist(hp, bot.entity.position) > 4.5 {
                    break;
                }
                bot.look_at(vec3(hp.x, hp.y + 1.0, hp.z));
                if bot.attack(hid).await.is_err() {
                    break;
                }
                let _ = bot.wait_real_ms(450).await;
            }
            continue;
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
                // Break off mid-fight when hurt: the health check only ran BETWEEN engagements, and
                // harness run 3 logged `engagement … end: hp=0 killed=true swings=8` — the blaze died
                // and so did the bot (fireball impacts; fire resistance only stops the burn).
                if bot.health < 10.0 {
                    println!("    [dbg] hp {:.0} mid-fight — breaking off to heal", bot.health);
                    break;
                }
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
            // Combat trace (harness run 2 died beside the spawner with fire resistance and no hp log).
            {
                let p = bot.entity.position;
                let mut near: Vec<String> = bot
                    .entities
                    .values()
                    .filter(|e| ((e.position.x - p.x).powi(2) + (e.position.y - p.y).powi(2) + (e.position.z - p.z).powi(2)).sqrt() < 16.0)
                    .filter_map(|e| e.entity_type.and_then(|t| bot.registry.entities_by_id.get(&t)).map(|d| d.name.clone()))
                    .filter(|n| n != "item" && n != "experience_orb" && n != "player")
                    .collect();
                near.sort();
                near.dedup();
                println!("    [dbg] engagement {id} end: hp={:.0} killed={killed} swings={in_reach_swings} near={near:?}", bot.health);
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

        // No blaze in view. If the spawner is known (loaded-chunk scan; fortress spawners are all
        // blaze spawners) and we're not at it yet, PATHFIND there — the spawner is where blazes
        // appear, camping elsewhere on the fortress waits forever.
        if let Some(sp) = loaded_block_nearest(bot, &["spawner"]) {
            let d = horiz_dist(vec3(sp.0 as f64 + 0.5, sp.1 as f64, sp.2 as f64 + 0.5), bot.entity.position);
            if d > 6.0 {
                println!("    [dbg] no blaze in view — spawner at {sp:?} ({d:.0} away), approaching");
                let _ = bot.goto_near(sp.0, sp.1, sp.2, 4.0).await;
                // Pathfinder couldn't route through the fortress (harness run 4: two bots re-logged
                // "spawner … 50 / 91 away, approaching" for minutes without closing in) → dig toward
                // it with the enclosed tunnel step (dominant axis per step), 10 steps per pass.
                let q = bot.entity.position;
                let (ddx, ddz) = (sp.0 as f64 + 0.5 - q.x, sp.2 as f64 + 0.5 - q.z);
                if (ddx * ddx + ddz * ddz).sqrt() > 6.0 {
                    for _ in 0..10 {
                        let q = bot.entity.position;
                        let (ddx, ddz) = (sp.0 as f64 + 0.5 - q.x, sp.2 as f64 + 0.5 - q.z);
                        if (ddx * ddx + ddz * ddz).sqrt() <= 6.0 {
                            break;
                        }
                        let (sx, sz) = if ddx.abs() >= ddz.abs() { (ddx.signum() as i32, 0) } else { (0, ddz.signum() as i32) };
                        if !nether_tunnel_step(bot, sx, sz).await {
                            break;
                        }
                    }
                }
                continue;
            }
        }
        // No spawner in view either: EXPLORE ALONG THE FORTRESS — walk to the farthest loaded
        // fortress brick within ~60 blocks, which loads new chunks further along the structure.
        // Holding still at the first brick seen waited forever (harness 09:00: two bots idle at
        // x≈4 while the spawner sat at x=129, beyond the 96-block view).
        if loaded_block_nearest(bot, &["spawner"]).is_none() {
            let p = bot.entity.position;
            let far = bot
                .find_blocks("nether_bricks", 64, 256)
                .into_iter()
                .filter(|b| {
                    let d = ((b.0 as f64 + 0.5 - p.x).powi(2) + (b.2 as f64 + 0.5 - p.z).powi(2)).sqrt();
                    // Walkway height only: pillars run down into the lava sea (run 3 picked (57,29,177)).
                    (30.0..=60.0).contains(&d) && (b.1 - p.y.floor() as i32).abs() <= 3
                })
                .max_by(|a, b| {
                    let da = (a.0 as f64 - p.x).powi(2) + (a.2 as f64 - p.z).powi(2);
                    let db = (b.0 as f64 - p.x).powi(2) + (b.2 as f64 - p.z).powi(2);
                    da.total_cmp(&db)
                });
            if let Some(b) = far {
                println!("    [dbg] no blaze, no spawner in view — exploring the fortress toward {b:?}");
                let _ = bot.goto_near(b.0, b.1 + 1, b.2, 3.0).await;
                continue;
            }
        }
        // Otherwise HOLD POSITION and wait for the spawner to produce more. Do NOT wander:
        // raw-walking "to explore" marched the bot off the fortress ledge into the lava/void (died
        // repeatedly, losing every fight). Standing still at the spawner is correct — blazes come to us.
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

/// PEARLS BY BARTER (cycle 6, Phase 4 baseline; no design beyond throw-gold-and-collect): walk to the
/// nearest piglin, look at it, toss ONE gold ingot, wait for the 6 s barter, then walk over every item
/// within 8 blocks so its drop is picked up. Repeats until `target` pearls or no gold. Piglins attack a
/// player without gold armour, so the kit wears a golden helmet.
pub async fn barter_pearls(bot: &mut Bot<'_>, _mem: &mut WorldMemory, target: i32) -> StepResult {
    let deadline = Instant::now() + Duration::from_secs(150);
    let gold = bot.registry.items_by_name.get("gold_ingot").map(|d| d.id);
    let piglin = bot.registry.entities_by_name.get("piglin").map(|d| d.id);
    let item = bot.registry.entities_by_name.get("item").map(|d| d.id);
    let mut tossed = 0;
    while count_items(bot, "ender_pearl") < target && count_items(bot, "gold_ingot") > 0 && Instant::now() < deadline {
        let me = bot.entity.position;
        let Some((pid, pp)) = bot
            .entities
            .values()
            .filter(|e| e.is_valid && (e.entity_type == piglin || e.name.as_deref() == Some("piglin")) && horiz_dist(e.position, me) <= 32.0)
            .map(|e| (e.id, e.position))
            .min_by(|a, b| horiz_dist(a.1, me).total_cmp(&horiz_dist(b.1, me)))
        else {
            println!("    [pearls] no piglin within 32");
            return failure("no piglin within 32 blocks");
        };
        if horiz_dist(pp, me) > 3.0 {
            let _ = bot.goto_near(pp.x.floor() as i32, pp.y.floor() as i32, pp.z.floor() as i32, 2.5).await;
        }
        let Some(pp) = bot.entities.get(&pid).map(|e| e.position) else { continue };
        bot.look_at(vec3(pp.x, pp.y + 0.9, pp.z));
        bot.wait_ticks(2).await.ok();
        if let Some(g) = gold {
            let _ = bot.toss(g, 1).await;
            tossed += 1;
        }
        // The piglin walks to the ingot, inspects it ~6 s, then throws its barter item toward the player.
        bot.wait_ticks(160).await.ok();
        let me = bot.entity.position;
        let drops: Vec<Vec3> = bot
            .entities
            .values()
            .filter(|e| e.is_valid && (e.entity_type == item || e.name.as_deref() == Some("item")) && horiz_dist(e.position, me) <= 8.0)
            .map(|e| e.position)
            .collect();
        for d in drops {
            let _ = bot.goto_near(d.x.floor() as i32, d.y.floor() as i32, d.z.floor() as i32, 0.8).await;
        }
        println!("    [pearls] tossed {tossed}, pearls {}, gold left {}", count_items(bot, "ender_pearl"), count_items(bot, "gold_ingot"));
    }
    if count_items(bot, "ender_pearl") >= target {
        success(&format!("{} pearls after {tossed} ingots", count_items(bot, "ender_pearl")))
    } else {
        failure(&format!("{} pearls after {tossed} ingots", count_items(bot, "ender_pearl")))
    }
}
