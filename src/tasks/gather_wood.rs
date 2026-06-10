//! Wood gathering — find the nearest reachable log, walk to it with the
//! pathfinder, chop the tree, collect the drops, repeat. Blacklists trees it
//! can't reach and explores for new terrain. Port of steve's `tasks/gather-wood`
//! (clean navigate → mine → repeat; no ad-hoc terrain hacks).

use std::collections::HashSet;

use rustcraft::bot::{Bot, DriveStep};
use rustcraft::vec3::vec3;

use crate::types::{failure, success, StepResult};

pub fn count_logs(bot: &Bot) -> i32 {
    bot.inventory.slots.iter().flatten().filter(|i| i.name.ends_with("_log")).map(|i| i.count).sum()
}

fn is_log_at(bot: &Bot, x: i32, y: i32, z: i32) -> bool {
    bot.block_at(x, y, z).map(|b| b.name.ends_with("_log")).unwrap_or(false)
}

/// Nearest log near the bot (raw chunk scan — sees trunks through their own
/// leaves), skipping blacklisted positions. Prefers near + at-or-above the bot.
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
                    // Nearest log (slight bias against logs far below to avoid pits).
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

/// Nearest log near the bot's level (raw scan — finds reachable trunk bases, not
/// just visible treetops). Wide horizontal radius, narrow vertical band.
fn find_log(bot: &Bot, radius: i32, blacklist: &HashSet<(i32, i32)>) -> Option<(i32, i32, i32)> {
    let p = bot.entity.position;
    let (bx, by, bz) = (p.x.floor() as i32, p.y.floor() as i32, p.z.floor() as i32);
    let mut best = None;
    let mut best_d = f64::MAX;
    for dx in -radius..=radius {
        for dz in -radius..=radius {
            if blacklist.contains(&(bx + dx, bz + dz)) {
                continue;
            }
            for dy in -4..=4 {
                let (x, y, z) = (bx + dx, by + dy, bz + dz);
                if is_log_at(bot, x, y, z) {
                    let d = (dx * dx + dz * dz) as f64 + 6.0 * (dy.abs() as f64);
                    if d < best_d {
                        best_d = d;
                        best = Some((x, y, z));
                    }
                    break;
                }
            }
        }
    }
    best
}

fn in_liquid(bot: &Bot) -> bool {
    let p = bot.entity.position;
    bot.block_at(p.x.floor() as i32, p.y.floor() as i32, p.z.floor() as i32)
        .map(|b| b.name.contains("water") || b.name.contains("lava"))
        .unwrap_or(false)
}

/// Any lava within `r` blocks (horizontally, ±1 vertically) of the bot — used to
/// stop the raw (non-pathfinder) walks from blindly stepping into a lava pool.
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

/// Walk to the nearest dropped item near the dug block (fast raw step) to pick it
/// up. Item entities are tracked from add_entity packets.
async fn collect_at(bot: &mut Bot<'_>, x: i32, z: i32) {
    let item_type = bot.registry.entities_by_name.get("item").map(|d| d.id);
    let logs0 = count_logs(bot);
    for _ in 0..20 {
        if count_logs(bot) > logs0 || lava_near(bot, 2) {
            break;
        }
        let bp = bot.entity.position;
        let (mut tx, mut tz) = (x as f64 + 0.5, z as f64 + 0.5);
        let mut best = 6.0;
        for e in bot.entities.values() {
            if item_type.is_some() && e.entity_type != item_type {
                continue;
            }
            let d = ((e.position.x - (x as f64 + 0.5)).powi(2) + (e.position.z - (z as f64 + 0.5)).powi(2)).sqrt();
            if d < best {
                best = d;
                tx = e.position.x;
                tz = e.position.z;
            }
        }
        bot.look_at(vec3(tx, bp.y - 0.5, tz));
        bot.set_control_state("forward", true);
        if bot.drive_tick().await.map(|s| matches!(s, DriveStep::Disconnected)).unwrap_or(true) {
            break;
        }
    }
    bot.clear_control_states();
    bot.wait_ticks(3).await.ok();
}

/// Would stepping one block in cardinal (fx,fz) drop the bot more than 1 block?
/// Used to stop the raw walks from striding off a cliff into a pit they can't
/// climb back out of (the pathfinder already avoids this; raw walking doesn't).
fn drop_ahead(bot: &Bot, fx: i32, fz: i32) -> bool {
    let p = bot.entity.position;
    let (px, py, pz) = (p.x.floor() as i32, p.y.floor() as i32, p.z.floor() as i32);
    // Solid at feet-1 (flat) or feet-2 (1-block step down) ahead = safe.
    let f1 = bot.block_state_at(px + fx, py - 1, pz + fz) != 0;
    let f2 = bot.block_state_at(px + fx, py - 2, pz + fz) != 0;
    !(f1 || f2)
}

fn is_soft(bot: &Bot, x: i32, y: i32, z: i32) -> bool {
    bot.block_at(x, y, z)
        .map(|b| {
            let n = &b.name;
            n.contains("dirt") || n.contains("grass") || n.contains("sand") || n.contains("gravel")
                || n.contains("leaves") || n.contains("snow") || n.contains("mud") || n.contains("clay")
                || n.contains("podzol") || n.contains("moss") || n.contains("rooted")
        })
        .unwrap_or(false)
}

/// Steve-style raw approach: when the pathfinder stalls short of a tree, walk
/// straight at it, carving STAIRS UP the slope (clear headroom ahead + above our
/// head, keep the feet-level block ahead as the step) and jumping to climb. This
/// brute-forces up a grassy mountainside to an elevated trunk. Stops when a log
/// is in chopping reach, near a stone wall it can't hand-dig, or near lava.
async fn approach_raw(bot: &mut Bot<'_>, tx: i32, ty: i32, tz: i32, max_ticks: u32) {
    let empty = HashSet::new();
    for _ in 0..max_ticks {
        if find_trunk_raw(bot, &empty).is_some() || lava_near(bot, 3) {
            break;
        }
        let p = bot.entity.position;
        let (px, py, pz) = (p.x.floor() as i32, p.y.floor() as i32, p.z.floor() as i32);
        let (dx, dz) = (tx as f64 - p.x, tz as f64 - p.z);
        if (dx * dx + dz * dz).sqrt() < 1.6 {
            break;
        }
        let (fx, fz) = if dx.abs() >= dz.abs() {
            (dx.signum() as i32, 0)
        } else {
            (0, dz.signum() as i32)
        };
        // Don't stride off a cliff into a pit we can't climb back out of.
        if drop_ahead(bot, fx, fz) {
            break;
        }
        // Clear headroom to climb (head-forward, above-forward, above our head),
        // but keep the feet-forward block as a step. Stone at body height = stop.
        let mut hard = false;
        for (ax, ay, az) in [(fx, 1, fz), (fx, 2, fz), (0, 2, 0)] {
            let (bx, by, bz) = (px + ax, py + ay, pz + az);
            if bot.block_state_at(bx, by, bz) != 0 {
                if is_soft(bot, bx, by, bz) {
                    if bot.dig(bx, by, bz).await.is_err() {
                        return;
                    }
                } else if ay <= 1 {
                    hard = true;
                }
            }
        }
        if hard {
            break;
        }
        bot.look_at(vec3(tx as f64 + 0.5, ty as f64 + 0.5, tz as f64 + 0.5));
        bot.set_control_state("forward", true);
        bot.set_control_state("sprint", true);
        bot.set_control_state("jump", true);
        if bot.drive_tick().await.map(|s| matches!(s, DriveStep::Disconnected)).unwrap_or(true) {
            break;
        }
    }
    bot.clear_control_states();
}

/// Walk to and pick up every dropped item within range (after felling a tree).
async fn collect_all(bot: &mut Bot<'_>) {
    let item_type = bot.registry.entities_by_name.get("item").map(|d| d.id);
    for _ in 0..40 {
        let bp = bot.entity.position;
        // nearest item drop within 10 blocks
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

/// Fell the tree the bot is standing at: break EVERY reachable trunk log (block
/// prediction turns each to air, so `find_trunk_raw` walks up the trunk), without
/// wandering off to collect between cuts, then collect all the drops at the end.
/// Returns how many logs were gained.
async fn chop(bot: &mut Bot<'_>, target: i32) -> i32 {
    let start = count_logs(bot);
    let mut trunk_bl: HashSet<(i32, i32, i32)> = HashSet::new();
    let mut idle = 0;
    for _ in 0..60 {
        let Some((tx, ty, tz)) = find_trunk_raw(bot, &trunk_bl) else { break };
        let center = vec3(tx as f64 + 0.5, ty as f64 + 0.5, tz as f64 + 0.5);
        let dist_before = bot.entity.position.distance(center);
        let mut progress = false;
        match bot.dig_toward(tx, ty, tz).await {
            Ok(true) => progress = true, // broke the log (now air via prediction)
            Ok(false) => {
                // cleared an occluder / out of reach — step toward the log.
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
                trunk_bl.insert((tx, ty, tz)); // can't reach this log
                idle = 0;
            }
        }
    }
    // Pick up everything the felled tree dropped.
    let _ = target;
    collect_all(bot).await;
    count_logs(bot) - start
}

/// March a long way to leave a dead-end (stone-hill) area and reach fresh,
/// tree-accessible terrain. Uses the pathfinder (which routes AROUND unbreakable
/// stone) to a far waypoint in a rotating direction — a raw walk just wedges on
/// the stone walls here. Re-scans for reachable logs along the way.
async fn explore(bot: &mut Bot<'_>, attempt: i32, home: (i32, i32), blacklist: &HashSet<(i32, i32)>) {
    // Commit to a heading (rotating only SLOWLY across calls) and travel far via
    // short ~16-block hops chained FROM THE CURRENT POSITION — short hops path
    // reliably even over hills, and chaining them covers real ground instead of
    // timing out on one distant waypoint and bouncing in place.
    let angle = attempt as f64 * 0.5;
    let p = bot.entity.position;
    println!("    wood: exploring from ({:.0},{:.0}) dir={angle:.2}", p.x, p.z);
    let mut blocked_hops = 0;
    for hop in 1..=8 {
        let p = bot.entity.position;
        // Fan the heading slightly per hop so a single obstacle doesn't fully stop us.
        let a = angle + (hop as f64 * 0.15);
        let tx = p.x.floor() as i32 + (a.cos() * 16.0) as i32;
        let tz = p.z.floor() as i32 + (a.sin() * 16.0) as i32;
        let reached = bot.goto_xz(tx, tz, 4.0).await.unwrap_or(false);
        if in_liquid(bot) {
            escape_water(bot).await;
        }
        if find_log(bot, 28, blacklist).is_some() {
            let np = bot.entity.position;
            println!("    wood: found trees near ({:.0},{:.0})", np.x, np.z);
            return;
        }
        if !reached {
            blocked_hops += 1;
            if blocked_hops >= 3 {
                let np = bot.entity.position;
                let liq = in_liquid(bot);
                let below = bot.block_at(np.x.floor() as i32, np.y.floor() as i32 - 1, np.z.floor() as i32).map(|b| b.name.clone()).unwrap_or_default();
                println!(
                    "    wood: wedged at ({:.0},{:.0},{:.0}) liquid={liq} below={below} — retreating home",
                    np.x, np.y, np.z
                );
                // Stuck (e.g. walked into a stone bowl we can't climb out of without
                // a pickaxe). Retreat toward home — the path we arrived by is, by
                // definition, traversable — then the next call rotates the heading.
                let _ = bot.goto_xz(home.0, home.1, 6.0).await;
                return;
            }
        } else {
            blocked_hops = 0;
        }
    }
}

pub async fn gather_wood(bot: &mut Bot<'_>, target: i32) -> StepResult {
    bot.wait_ticks(4).await.ok();
    // Remember where we started — a reachable anchor to retreat to when a basin
    // chase dead-ends.
    let home = {
        let p = bot.entity.position;
        (p.x.floor() as i32, p.z.floor() as i32)
    };
    let mut blacklist: HashSet<(i32, i32)> = HashSet::new();
    let mut attempts = 0;
    let mut failed_trees = 0; // consecutive trees we couldn't get a log from
    let mut anchor = home; // last spot we made real progress (moved/chopped) from
    let mut stuck = 0; // attempts with no real movement

    while count_logs(bot) < target && attempts < 60 {
        attempts += 1;
        if in_liquid(bot) {
            escape_water(bot).await;
        }

        // Find the nearest reachable log.
        let mut found = None;
        for r in [12, 24, 40] {
            found = find_log(bot, r, &blacklist);
            if found.is_some() {
                break;
            }
        }

        // Nothing nearby, struck out on several trees, or wedged in place (reaching
        // trees we can't actually chop) → leave and find accessible terrain.
        if found.is_none() || failed_trees >= 4 || stuck >= 5 {
            println!("    wood: leaving this area — exploring for accessible trees");
            // Do NOT clear the blacklist — keeping the unreachable trees (e.g. on a
            // steep hill) blacklisted is what stops us re-targeting the same one
            // forever. Old far-away entries are harmless (out of find_log range).
            explore(bot, attempts, home, &blacklist).await;
            failed_trees = 0;
            stuck = 0;
            anchor = {
                let p = bot.entity.position;
                (p.x.floor() as i32, p.z.floor() as i32)
            };
            continue;
        }

        let (x, y, z) = found.unwrap();
        let p0 = bot.entity.position;
        let d = ((x as f64 - p0.x).powi(2) + (z as f64 - p0.z).powi(2)).sqrt();
        println!("    wood: tree ({x},{y},{z}) d={d:.0} — walking");
        let arrived = bot.goto_near(x, y, z, 3.0).await.unwrap_or(false);
        // If the pathfinder stalled short of the tree (no trunk in chopping
        // reach yet), brute-force toward it — carve up the slope like steve's
        // raw-walk fallback — then try once more if still short.
        if find_trunk_raw(bot, &HashSet::new()).is_none() {
            approach_raw(bot, x, y, z, 60).await;
            if find_trunk_raw(bot, &HashSet::new()).is_none() {
                let _ = bot.goto_near(x, y, z, 3.0).await;
                approach_raw(bot, x, y, z, 40).await;
            }
        }
        let tr = find_trunk_raw(bot, &HashSet::new());
        let reached = tr.is_some();
        {
            let p = bot.entity.position;
            println!("    wood: at ({:.0},{:.0},{:.0}) reached={reached} trunk={tr:?}", p.x, p.y, p.z);
        }
        let gained = chop(bot, target).await;
        // Track real movement so we notice being wedged even when "reached".
        let now = {
            let p = bot.entity.position;
            (p.x.floor() as i32, p.z.floor() as i32)
        };
        let moved_far = ((now.0 - anchor.0).pow(2) + (now.1 - anchor.1).pow(2)) >= 36;
        if gained > 0 {
            println!("    wood: {} logs", count_logs(bot));
            failed_trees = 0;
            stuck = 0;
            anchor = now;
        } else {
            // Couldn't get a log here — skip this tree AND the trunk we reached.
            blacklist.insert((x, z));
            if let Some((tx, _, tz)) = tr {
                blacklist.insert((tx, tz));
            }
            failed_trees += 1;
            let _ = (arrived, reached);
            if moved_far {
                stuck = 0;
                anchor = now;
            } else {
                stuck += 1;
            }
        }
    }

    let logs = count_logs(bot);
    if logs >= target {
        success(format!("gathered {logs}/{target} logs"))
    } else {
        failure(format!("gathered {logs}/{target} logs"))
    }
}
