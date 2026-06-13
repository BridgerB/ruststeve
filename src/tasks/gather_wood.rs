//! Wood gathering — find the genuinely nearest log with an expanding-ring scan
//! (one block of radius at a time, scanning only the newly-added shell each
//! round, at ANY height), walk to it, chop the tree, collect the drops, repeat.
//! When no log is anywhere in view, run straight north into fresh terrain and
//! start the scan over.

use std::collections::HashSet;

use rustcraft::bot::{Bot, DriveStep};
use rustcraft::vec3::vec3;

use crate::bot_utils::can_reach;
use crate::memory::{PoiKind, PoiStatus, WorldMemory};
use crate::types::{failure, success, StepResult};

/// How far out (blocks) the ring scan expands before declaring "nothing in view".
/// Kept modest: a big scan is synchronous CPU that blocks the bot's network loop,
/// and with many bots running at once a too-large scan stalls keep-alive responses
/// and gets the bot kicked for "Timed out". Trees farther than this are reached by
/// exploring toward them, not by scanning further.
const MAX_RADIUS: i32 = 64;

/// Vertical half-band around the bot the column scan checks — "any reachable
/// height" (you can't walk to a trunk 100 blocks above), while keeping the scan
/// cheap instead of sweeping the whole 384-block column.
const SCAN_VERT: i32 = 48;

pub fn count_logs(bot: &Bot) -> i32 {
    bot.inventory.slots.iter().flatten().filter(|i| i.name.ends_with("_log")).map(|i| i.count).sum()
}

fn is_log_at(bot: &Bot, x: i32, y: i32, z: i32) -> bool {
    bot.block_at(x, y, z).map(|b| b.name.ends_with("_log")).unwrap_or(false)
}

/// Every block state id belonging to a `*_log` block (all species + axes), so the
/// ring scan can test columns by raw state id with no per-block allocation.
fn log_state_ids(bot: &Bot) -> HashSet<u32> {
    bot.registry
        .blocks_array
        .iter()
        .filter(|b| b.name.ends_with("_log"))
        .flat_map(|b| b.min_state_id..=b.max_state_id)
        .collect()
}

/// Lowest log y (trunk base) in the column at (x, z), scanning the whole loaded
/// height — i.e. a log at ANY height, not a narrow band. None if the column has
/// no log, or its trunk is remembered `Unreachable` (the persistent successor to
/// the old blacklist).
fn log_in_column(bot: &Bot, logs: &HashSet<u32>, mem: &WorldMemory, x: i32, z: i32) -> Option<i32> {
    let by = bot.entity.position.y.floor() as i32;
    let y0 = (by - SCAN_VERT).max(bot.game.min_y);
    let y1 = (by + SCAN_VERT).min(bot.game.min_y + bot.game.height);
    for y in y0..y1 {
        if logs.contains(&bot.block_state_at(x, y, z)) {
            // Trunk base found. If we've already learned this tree is unreachable,
            // skip the whole column so the scan moves to the next-nearest.
            if mem.is_unreachable((x, y, z)) {
                return None;
            }
            return Some(y);
        }
    }
    None
}

/// Expanding-ring search for the nearest log column. Grows the radius ONE block
/// at a time and scans only the perimeter cells added that round (Chebyshev
/// shell), so each ring is O(r) work, out to `MAX_RADIUS`. Returns the closest
/// log in the first ring that contains one — at whatever height it sits.
fn find_nearest_log(bot: &Bot, logs: &HashSet<u32>, mem: &WorldMemory) -> Option<(i32, i32, i32)> {
    let p = bot.entity.position;
    let (bx, bz) = (p.x.floor() as i32, p.z.floor() as i32);

    // r = 0: the bot's own column.
    if let Some(y) = log_in_column(bot, logs, mem, bx, bz) {
        return Some((bx, y, bz));
    }

    for r in 1..=MAX_RADIUS {
        let mut best: Option<(i32, i32, i32)> = None;
        let mut best_d = i64::MAX;
        // Newly-added shell only: the perimeter of the r-square. Top & bottom
        // edges (full width, four corners included), then left & right edges
        // (corners excluded). Each cell scans its full column for a log.
        let mut perimeter: Vec<(i32, i32)> = Vec::with_capacity((8 * r) as usize);
        for dx in -r..=r {
            perimeter.push((dx, -r));
            perimeter.push((dx, r));
        }
        for dz in -(r - 1)..=(r - 1) {
            perimeter.push((-r, dz));
            perimeter.push((r, dz));
        }
        for (dx, dz) in perimeter {
            if let Some(y) = log_in_column(bot, logs, mem, bx + dx, bz + dz) {
                let d = (dx as i64) * (dx as i64) + (dz as i64) * (dz as i64);
                if d < best_d {
                    best_d = d;
                    best = Some((bx + dx, y, bz + dz));
                }
            }
        }
        if best.is_some() {
            return best;
        }
    }
    None
}

fn in_liquid(bot: &Bot) -> bool {
    let p = bot.entity.position;
    bot.block_at(p.x.floor() as i32, p.y.floor() as i32, p.z.floor() as i32)
        .map(|b| b.name.contains("water") || b.name.contains("lava"))
        .unwrap_or(false)
}

/// Any lava within `r` blocks (horizontally, ±1 vertically) of the bot — keeps
/// the drop-collection walk from stepping into a lava pool.
fn lava_near(bot: &Bot, r: i32) -> bool {
    let p = bot.entity.position;
    let (bx, by, bz) = (p.x.floor() as i32, p.y.floor() as i32, p.z.floor() as i32);
    for dx in -r..=r {
        for dy in -1..=1 {
            for dz in -r..=r {
                if bot.block_at(bx + dx, by + dy, bz + dz).map(|b| b.name.contains("lava")).unwrap_or(false) {
                    return true;
                }
            }
        }
    }
    false
}

async fn escape_water(bot: &mut Bot<'_>) {
    for _ in 0..40 {
        if !in_liquid(bot) && bot.entity.on_ground {
            break;
        }
        bot.set_control_state("jump", true);
        bot.set_control_state("forward", true);
        bot.set_control_state("sprint", true);
        if bot.drive_tick().await.map(|s| matches!(s, DriveStep::Disconnected)).unwrap_or(true) {
            break;
        }
    }
    bot.clear_control_states();
}

/// Nearest log within chopping reach of the bot (small ±4 scan, sees trunks
/// through their own leaves). Used by `chop` to walk up a trunk it's standing at.
fn find_trunk_raw(bot: &Bot, blacklist: &HashSet<(i32, i32, i32)>) -> Option<(i32, i32, i32)> {
    let p = bot.entity.position;
    let (bx, by, bz) = (p.x.floor() as i32, p.y.floor() as i32, p.z.floor() as i32);
    let mut best = None;
    let mut best_key = f64::MAX;
    for dx in -4..=4 {
        for dz in -4..=4 {
            for dy in -3..=4 {
                let (x, y, z) = (bx + dx, by + dy, bz + dz);
                if blacklist.contains(&(x, y, z)) {
                    continue;
                }
                if is_log_at(bot, x, y, z) {
                    let below = if dy < -1 { 4.0 * (-dy - 1) as f64 } else { 0.0 };
                    let key = (dx * dx + dy * dy + dz * dz) as f64 + below;
                    if key < best_key {
                        best_key = key;
                        best = Some((x, y, z));
                    }
                }
            }
        }
    }
    best
}

/// Walk to and pick up every dropped item within range (after felling a tree).
async fn collect_all(bot: &mut Bot<'_>) {
    let item_type = bot.registry.entities_by_name.get("item").map(|d| d.id);
    for _ in 0..40 {
        let bp = bot.entity.position;
        let mut best: Option<(f64, f64, f64, f64)> = None;
        for e in bot.entities.values() {
            if item_type.is_some() && e.entity_type != item_type {
                continue;
            }
            let d = ((e.position.x - bp.x).powi(2) + (e.position.z - bp.z).powi(2)).sqrt();
            if d < 10.0 && best.as_ref().map(|b| d < b.3).unwrap_or(true) {
                best = Some((e.position.x, e.position.y, e.position.z, d));
            }
        }
        let Some((ix, iy, iz, d)) = best else { break };
        if d < 0.6 {
            bot.wait_ticks(2).await.ok();
            continue;
        }
        if lava_near(bot, 2) {
            break;
        }
        bot.look_at(vec3(ix, iy, iz));
        bot.set_control_state("forward", true);
        if bot.drive_tick().await.map(|s| matches!(s, DriveStep::Disconnected)).unwrap_or(true) {
            break;
        }
    }
    bot.clear_control_states();
    bot.wait_ticks(4).await.ok();
}

/// Fell the tree the bot is standing at: break every reachable trunk log (block
/// prediction turns each to air, so `find_trunk_raw` walks up the trunk), then
/// collect the drops. Returns how many logs were gained.
async fn chop(bot: &mut Bot<'_>) -> i32 {
    let start = count_logs(bot);
    let mut trunk_bl: HashSet<(i32, i32, i32)> = HashSet::new();
    let mut idle = 0;
    for _ in 0..60 {
        let Some((tx, ty, tz)) = find_trunk_raw(bot, &trunk_bl) else { break };
        let center = vec3(tx as f64 + 0.5, ty as f64 + 0.5, tz as f64 + 0.5);
        let dist_before = bot.entity.position.distance(center);
        let progress;
        match bot.dig_toward(tx, ty, tz).await {
            Ok(true) => progress = true,
            Ok(false) => {
                if lava_near(bot, 2) {
                    break;
                }
                let above = ty as f64 > bot.entity.position.y + 0.5;
                bot.look_at(center);
                bot.set_control_state("forward", true);
                bot.set_control_state("jump", above);
                bot.wait_ticks(4).await.ok();
                bot.clear_control_states();
                progress = bot.entity.position.distance(center) < dist_before - 0.2;
            }
            Err(_) => break,
        }
        if progress {
            idle = 0;
        } else {
            idle += 1;
            if idle > 6 {
                trunk_bl.insert((tx, ty, tz));
                idle = 0;
            }
        }
    }
    collect_all(bot).await;
    count_logs(bot) - start
}

/// Max blocks the bot will wander from where it started gathering before it
/// turns back — a roam budget so it never marches off into ungenerated chunks
/// and gets disconnected.
const ROAM_LIMIT: f64 = 240.0;

/// Explore for fresh terrain when nothing's in view. ROTATES direction each
/// sweep (south first — forests trend +Z here) instead of marching one fixed way
/// off the map, and stays within a roam budget of `home`: once it's wandered too
/// far it heads BACK toward home (known-traversable ground) rather than walking
/// until the server kicks it. Short pathable hops; stops if a hop is blocked.
async fn explore(bot: &mut Bot<'_>, sweep: u32, home: (i32, i32)) {
    let p = bot.entity.position;
    let (px, pz) = (p.x.floor() as i32, p.z.floor() as i32);
    let dist_home = (((px - home.0) as f64).powi(2) + ((pz - home.1) as f64).powi(2)).sqrt();
    // 8 compass directions, SOUTH (+Z) first.
    let dirs = [(0, 1), (1, 1), (1, 0), (1, -1), (0, -1), (-1, -1), (-1, 0), (-1, 1)];
    let (dx, dz) = if dist_home > ROAM_LIMIT {
        ((home.0 - px).signum(), (home.1 - pz).signum()) // too far — head home
    } else {
        dirs[(sweep as usize) % dirs.len()]
    };
    let what = if dist_home > ROAM_LIMIT { "returning toward home" } else { "exploring" };
    println!("    wood: {what} dir=({dx},{dz}) {dist_home:.0} from home");
    // ~48 blocks this sweep (3 hops), then let the caller re-scan.
    for _ in 0..3 {
        let p = bot.entity.position;
        let tx = p.x.floor() as i32 + dx * 16;
        let tz = p.z.floor() as i32 + dz * 16;
        let reached = bot.goto_xz(tx, tz, 3.0).await.unwrap_or(false);
        if in_liquid(bot) {
            escape_water(bot).await;
        }
        if !reached {
            break; // blocked — rescan from here
        }
    }
}

/// Walk a couple of short hops toward (tx, tz) to close distance to a far tree —
/// cheap and network-pumping — before we try to actually path to and fell it.
async fn move_toward(bot: &mut Bot<'_>, tx: i32, tz: i32) {
    for _ in 0..2 {
        let p = bot.entity.position;
        let (dx, dz) = (tx as f64 - p.x, tz as f64 - p.z);
        let len = (dx * dx + dz * dz).sqrt();
        if len < 2.0 {
            break;
        }
        let step = 16.0_f64.min(len);
        let nx = (p.x + dx / len * step).floor() as i32;
        let nz = (p.z + dz / len * step).floor() as i32;
        let reached = bot.goto_xz(nx, nz, 3.0).await.unwrap_or(false);
        if in_liquid(bot) {
            escape_water(bot).await;
        }
        if !reached {
            break;
        }
    }
}

pub async fn gather_wood(bot: &mut Bot<'_>, target: i32, mem: &mut WorldMemory) -> StepResult {
    bot.wait_ticks(4).await.ok();
    mem.log("gather_wood", "begin", &format!("target={target} have={}", count_logs(bot)));
    let logs = log_state_ids(bot);
    let home = {
        let p = bot.entity.position;
        (p.x.floor() as i32, p.z.floor() as i32) // anchor we roam around / return to
    };
    let mut sweeps = 0u32; // consecutive explore sweeps with no wood found
    let mut cycles = 0; // total walk-and-chop cycles (safety bound)

    while count_logs(bot) < target && cycles < 200 {
        cycles += 1;
        // Pump the network every cycle so keep-alive is always answered even
        // between the synchronous scan / pathfinding work below (otherwise the
        // server kicks us for "Timed out").
        bot.wait_ticks(1).await.ok();
        if in_liquid(bot) {
            escape_water(bot).await;
        }

        match find_nearest_log(bot, &logs, mem) {
            Some((x, y, z)) => {
                sweeps = 0;
                let p = bot.entity.position;
                let d = (((x as f64 - p.x).powi(2) + (z as f64 - p.z).powi(2)) as f64).sqrt();

                // Too far to walk to directly (goto would time out long before it
                // arrives): close the gap with cheap hops toward it, then re-scan.
                // If we can't get meaningfully closer, the tree is unreachable.
                if d > 40.0 {
                    move_toward(bot, x, z).await;
                    let np = bot.entity.position;
                    let after = (((x as f64 - np.x).powi(2) + (z as f64 - np.z).powi(2)) as f64).sqrt();
                    if after > d - 4.0 {
                        mem.record(PoiKind::Log, (x, y, z), PoiStatus::Unreachable);
                        mem.log("gather_wood", "no_approach", &format!("{x},{y},{z} d={d:.0}"));
                    }
                    continue;
                }

                // Close enough — verify reachable (bounded), walk, chop.
                if !can_reach(bot, x, y, z, 3.0) {
                    mem.record(PoiKind::Log, (x, y, z), PoiStatus::Unreachable);
                    mem.log("gather_wood", "unreachable", &format!("{x},{y},{z}"));
                    continue;
                }
                println!("    wood: nearest log ({x},{y},{z}) d={d:.0} reachable — walking");
                mem.log("gather_wood", "tree", &format!("{x},{y},{z} d={d:.0}"));
                bot.goto_near(x, y, z, 3.0).await.ok();
                let gained = chop(bot).await;
                if gained > 0 {
                    println!("    wood: {} logs", count_logs(bot));
                    mem.log("gather_wood", "chopped", &format!("+{gained} logs total={}", count_logs(bot)));
                } else {
                    // Reached but couldn't fell it — remember it as unreachable too.
                    mem.record(PoiKind::Log, (x, y, z), PoiStatus::Unreachable);
                    mem.log("gather_wood", "unfelled", &format!("{x},{y},{z}"));
                }
            }
            None => {
                // No log in view → explore for fresh terrain: rotate direction
                // each sweep, bounded by a roam budget around home. Memory is NOT
                // cleared — known-unreachable trees stay remembered.
                sweeps += 1;
                if sweeps > 16 {
                    break; // explored a bounded area with nothing — give up this call
                }
                mem.log("gather_wood", "explore", &format!("sweep={sweeps}"));
                explore(bot, sweeps, home).await;
            }
        }
    }

    let n = count_logs(bot);
    if n >= target {
        success(format!("gathered {n}/{target} logs"))
    } else {
        failure(format!("gathered {n}/{target} logs"))
    }
}
