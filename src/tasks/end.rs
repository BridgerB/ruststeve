//! Late-game: the End (cycle 4, Part 6 — reverse curriculum, dragon first).
//!
//! `dragon_beds` (v2) is the scripted attempt at skill 12 (crystals already gone).
//! v1 failure log (CHANGES, cycle 4 Phase 3): beds were placed on the lowest reachable floor, so each
//! explosion cratered the end stone and the next bed went off lower, 10–14 blocks under a perched
//! dragon's head. v2 builds its own geometry at the perch height:
//! 1. pillar up east of the fountain to stand at the pillar-top height (top + 2);
//! 2. bridge two blocks toward the pillar at top + 1 (each placed against the previous);
//! 3. on a real perch (low over the pillar, hd ≤ 3), place a bed on the bridge end and use it — beds
//!    explode in the End; rebuild the bridge after each blast.
//!
//! Below 10 hp: stop and eat (on the pillar; the dragon's breath is the hazard, not falling).
//! Every step is logged (`DRAGON …`). The harness checks the kill from server state.

use std::time::{Duration, Instant};

use crate::bot::{Bot, Face};
use crate::bot_utils::{count_items, select_item};
use crate::memory::WorldMemory;
use crate::tasks::portal::{cast_debug, eat_if_hurt, feet_y, name_at, place_cobble, solid_at};
use crate::types::{failure, success, StepResult};
use crate::vec3::vec3;

const BED: &str = "red_bed";

/// The exit fountain's bedrock pillar top at (0, y, 0), if generated and loaded.
fn fountain_top(bot: &Bot) -> Option<i32> {
    (40..100).rev().find(|&y| name_at(bot, 0, y, 0) == "bedrock")
}

/// The dragon as the client sees it (entity type name), with its position.
fn dragon(bot: &Bot) -> Option<crate::vec3::Vec3> {
    bot.entities
        .values()
        .find(|e| e.name.as_deref().map(|n| n.contains("ender_dragon")).unwrap_or(false))
        .map(|e| e.position)
}

/// Place a block at `c`: obsidian first (it survives the blasts), else the scaffold block.
async fn put(bot: &mut Bot<'_>, c: (i32, i32, i32)) -> bool {
    if solid_at(bot, c.0, c.1, c.2) {
        return true;
    }
    if count_items(bot, "obsidian") > 0 && select_item(bot, "obsidian").await.unwrap_or(false) {
        // Click the neighbour's face that points at `c`.
        for (dx, dy, dz, face) in [(1, 0, 0, Face::West), (-1, 0, 0, Face::East), (0, -1, 0, Face::Top), (0, 0, 1, Face::North), (0, 0, -1, Face::South)] {
            let r = (c.0 + dx, c.1 + dy, c.2 + dz);
            if solid_at(bot, r.0, r.1, r.2) {
                bot.look_at(vec3(c.0 as f64 + 0.5, c.1 as f64 + 0.5, c.2 as f64 + 0.5));
                bot.wait_ticks(2).await.ok();
                let _ = bot.place_block(r.0, r.1, r.2, face).await;
                bot.wait_ticks(3).await.ok();
                if solid_at(bot, c.0, c.1, c.2) {
                    return true;
                }
            }
        }
    }
    place_cobble(bot, c).await
}

pub async fn dragon_beds(bot: &mut Bot<'_>, _mem: &mut WorldMemory) -> StepResult {
    if count_items(bot, BED) < 1 {
        // Wait before failing: the gym re-calls the step at once (an instant return spammed the log).
        bot.wait_ticks(100).await.ok();
        return failure("DRAGON: no beds left".to_string());
    }
    let t0 = Instant::now();
    let budget = Duration::from_secs(crate::learn::budget_s("dragon").min(900.0) as u64);
    let Some(top) = fountain_top(bot) else {
        return failure("DRAGON: no exit fountain loaded at (0, y, 0)".to_string());
    };
    // 1. Pillar at x = 6, z = 0 up to the perch height.
    let (px, pz) = (6, 0);
    let stand_y = top + 2;
    let _ = tokio::time::timeout(Duration::from_secs(30), bot.goto_near(px, feet_y(bot), pz, 0.6)).await;
    bot.clear_control_states();
    crate::tasks::portal_mold::settle_xz(bot, px as f64 + 0.5, pz as f64 + 0.5, 0.25, 80).await;
    bot.set_control_state("sneak", false);
    // pillar_up's last height check can land mid-jump (cycle 4: "on pillar at 67.1" with the column
    // top at 65, feet really 66), so let the bot land and re-check before trusting the height.
    for _ in 0..3 {
        bot.wait_ticks(15).await.ok();
        if feet_y(bot) >= stand_y && bot.entity.on_ground {
            break;
        }
        pillar_up_with(bot, stand_y, Some("obsidian")).await;
    }
    bot.wait_ticks(15).await.ok();
    let p = bot.entity.position;
    cast_debug(&format!(
        "DRAGON v2: fountain top y={top}; on pillar at ({:.1},{:.1},{:.1}) want feet {stand_y}; beds={} obsidian={} hp={:.0}",
        p.x, p.y, p.z, count_items(bot, BED), count_items(bot, "obsidian"), bot.health
    ));
    if feet_y(bot) < stand_y {
        bot.wait_ticks(60).await.ok();
        return failure(format!("DRAGON v2: pillar reached feet {} < {stand_y}", feet_y(bot)));
    }
    let (sx, sz) = (bot.entity.position.x.floor() as i32, bot.entity.position.z.floor() as i32);
    // 2. Bridge toward the pillar level with the block we stand on (the first cell is placed against
    //    it, the second against the first); the bed goes on its far end.
    // The bridge always ends at x = 4, whatever x the pillar went up at: the goto to x 6 sometimes ends
    // at x 7–8 (late5-v2c trial 1 pillared at 8, the bed sat at x 6: 0 detonations in 600 s; trial 2's
    // bed at x 5 was 5.6 from a dragon perched dead over the fountain). Each cell is placed against
    // the previous one.
    // At the fountain-top level, so the bed sits at top + 1, UNDER the sitting dragon's head: a bed at
    // top + 2 was placed and then destroyed with the dragon landed (late12-v2j, 2 beds in 4 s), the
    // head/neck sweeping that cell. The first cell rests against the pillar block at the same height.
    let by0 = top.min(feet_y(bot) - 1);
    let bridge: Vec<(i32, i32, i32)> = (4.min(sx - 1)..sx).rev().map(|x| (x, by0, sz)).collect();
    let bed_cell = *bridge.last().expect("bridge has at least one cell");
    // Side arms from the far end to z ± 2, beside the line rather than on it: the sitting dragon's head
    // points at the bot, i.e. along this very line, and destroyed every on-line bed in 3 of 4 perches
    // (late14-v2l perch 3, late15-v2m perch 1). Each arm cell rests on the previous one.
    // A second pair of arms one cell nearer the pillar: a dragon sitting nearer the bridge (x ≈ 1.2,
    // re-summoned in late18-v2p) had its parts over all four first candidates (server-rejected 770×).
    let near_x = (bed_cell.0 + 1).min(sx - 1);
    let arms: Vec<(i32, i32, i32)> = [1, 2, -1, -2]
        .iter()
        .map(|&dz| (bed_cell.0, by0, sz + dz))
        .chain([1, 2, -1, -2].iter().map(|&dz| (near_x, by0, sz + dz)))
        .collect();
    // Bed candidates, in order: on the line (far end first), then the arm tips, far pair first.
    let candidates: Vec<(i32, i32, i32)> = bridge
        .iter()
        .rev()
        .copied()
        .chain([(bed_cell.0, by0, sz + 2), (bed_cell.0, by0, sz - 2), (near_x, by0, sz + 2), (near_x, by0, sz - 2)])
        .collect();
    let mut last_bridge_log = Instant::now() - Duration::from_secs(30);
    let mut detonations = 0;
    // The client's inventory did not drop after each placed bed (late4-v2b: 22 "detonations" with 6
    // beds; server: 0 beds left, no bed at the cell). Spend at most the beds held at the start, and
    // count a detonation only when the placed bed is seen and then gone.
    let beds0 = count_items(bot, BED);
    let mut used = 0;
    let mut sitting_until: Option<Instant> = None;
    let mut gone_streak = 0;
    // Candidate cells that lost a bed during the current perch (cleared when the dragon leaves it).
    let mut eaten: Vec<(i32, i32, i32)> = Vec::new();
    let mut seen = false;
    let mut last_seen = Instant::now();
    let mut last_log = Instant::now() - Duration::from_secs(30);
    while t0.elapsed() < budget {
        if bot.health <= 0.0 {
            let _ = bot.respawn().await;
            return failure(format!("DRAGON v2 died after {detonations} detonations"));
        }
        if bot.health < 10.0 {
            cast_debug(&format!("DRAGON v2 hp {:.0} < 10 — eating on the pillar", bot.health));
            for _ in 0..6 {
                if bot.health >= 16.0 {
                    break;
                }
                eat_if_hurt(bot).await;
                bot.wait_ticks(20).await.ok();
            }
            continue;
        }
        // Knocked off the pillar (a blast or the dragon): climb back before anything else. late7-v2e
        // trial 1: blown down to feet 64 after bed 1, then 52 "could not rebuild bridge" in 594 s.
        if feet_y(bot) < by0 + 1 {
            cast_debug(&format!("DRAGON v2 off the pillar (feet {} < {}) — re-pillaring at ({sx},{sz})", feet_y(bot), by0 + 1));
            let _ = tokio::time::timeout(Duration::from_secs(20), bot.goto_near(sx, feet_y(bot), sz, 0.6)).await;
            bot.clear_control_states();
            crate::tasks::portal_mold::settle_xz(bot, sx as f64 + 0.5, sz as f64 + 0.5, 0.25, 80).await;
            bot.set_control_state("sneak", false);
            pillar_up_with(bot, by0 + 1, Some("obsidian")).await;
            bot.wait_ticks(15).await.ok();
            bot.set_control_state("sneak", false);
            continue;
        }
        let mut bridge_ok = true;
        for &c in &bridge {
            if !solid_at(bot, c.0, c.1, c.2) && !put(bot, c).await {
                bridge_ok = false;
                if last_bridge_log.elapsed() > Duration::from_secs(10) {
                    last_bridge_log = Instant::now();
                    cast_debug(&format!("DRAGON v2 could not rebuild bridge cell {c:?} (feet {}, stand {stand_y})", feet_y(bot)));
                }
                break;
            }
        }
        if !bridge_ok {
            bot.wait_ticks(20).await.ok();
            continue;
        }
        // Arms are best-effort: a missing arm only removes its tip from the candidates.
        for &c in &arms {
            if !solid_at(bot, c.0, c.1, c.2) && !put(bot, c).await {
                break;
            }
        }
        let Some(dp) = dragon(bot) else {
            if seen && last_seen.elapsed() > Duration::from_secs(15) {
                return success(format!("DRAGON v2 not seen for 15 s after {detonations} detonations (harness checks the kill)"));
            }
            bot.wait_ticks(10).await.ok();
            continue;
        };
        seen = true;
        last_seen = Instant::now();
        let hd = (dp.x.powi(2) + dp.z.powi(2)).sqrt();
        // Near the bed, not just over the fountain: late4-v2b bed #2 went off with the dragon perched
        // on the far (west) side, ~7.6 from the bed.
        let bed_d = ((dp.x - (bed_cell.0 as f64 + 0.5)).powi(2) + (dp.z - (bed_cell.2 as f64 + 0.5)).powi(2)).sqrt();
        // ≤ 6 from the bed: with the bed at x 4 that covers the fountain perch (hd ≤ 3 → bed_d ≤ ~5.6)
        // on the near half, and excludes the far side (late4-v2b bed #2 at 7.6 from the bed).
        // LANDED, not landing: every real detonation had the dragon at y 65.8–66.0 over the y-65 fountain
        // top. During the slow landing descent it still destroys blocks it touches (sniffed, late11-v2i:
        // the server placed the bed, then removed it within 10 ticks, unused; 16 beds drained that way).
        // + 2.6, not + 1.3: a dragon re-summoned after a kill (no fight manager) sits at y 67.4 over the
        // y-65 top (late17-v2o trial 3: perched at hd 1.6, gate never opened). The landing-vs-sitting
        // distinction is the movement check below (< 0.3 in 10 ticks), not this height.
        let perched = hd <= 3.0 && bed_d <= 6.0 && dp.y <= top as f64 + 2.6;
        if last_log.elapsed() > Duration::from_secs(10) {
            last_log = Instant::now();
            cast_debug(&format!("DRAGON v2 at ({:.1},{:.1},{:.1}) hd={hd:.1} perched={perched} hp={:.0} beds={}", dp.x, dp.y, dp.z, bot.health, count_items(bot, BED)));
        }
        if !perched {
            if hd > 6.0 {
                eaten.clear(); // a new perch can lie differently
            }
            bot.wait_ticks(4).await.ok();
            continue;
        }
        if count_items(bot, BED) < 1 || used >= beds0 {
            bot.wait_ticks(100).await.ok();
            return failure(format!("DRAGON v2 out of beds after {detonations} detonations ({used}/{beds0} placed)"));
        }
        // Only on a SITTING dragon (moved < 0.3 over 10 ticks). During the landing approach its body
        // destroys non-immune blocks it touches: late6-v2d trial 1 placed 10 beds for 4 detonations,
        // with clusters of "did not place … air" before each one (RCON: 10 beds at the trial start).
        // Confirm sitting once per perch, not per bed: a perch lasts ~5–10 s and late10-v2h got 4 beds
        // into its one perch in 600 s (~1.7 s per bed, 0.5 s of it this check).
        if sitting_until.is_none_or(|t| Instant::now() > t) {
            bot.wait_ticks(10).await.ok();
            let Some(dp2) = dragon(bot) else { continue };
            if dp2.distance(dp) > 0.3 {
                continue;
            }
            sitting_until = Some(Instant::now() + Duration::from_secs(4));
        }
        // 3. Place a bed and use it IN THE SAME CLIENT TICK, candidates in order (on the line far end
        // first, then the arm tips), skipping cells that lost a bed this perch. The server handles both
        // packets before the dragon's entity tick, so the bed goes off before a head or neck lying over it
        // destroys it (late16-v2n: the on-line cells lost every bed when placed, then used 10 ticks later).
        // A detonation is counted from the server's `explode` packet, not from the block view.
        let mut fired = false;
        let mut vanished = false;
        for &(cx, cy, cz) in &candidates {
            if !solid_at(bot, cx, cy, cz) {
                continue; // an arm that didn't place
            }
            if eaten.contains(&(cx, cy, cz)) {
                continue;
            }
            // Per candidate: a bed consumed by the previous try leaves the hand empty (`held=None`).
            if !select_item(bot, BED).await.unwrap_or(false) {
                break;
            }
            let beds_before = count_items(bot, BED);
            let ex0 = bot.explosions;
            let hp0 = bot.health;
            let (bx, by, bz) = (cx, cy + 1, cz);
            bot.look_at(vec3(bx as f64 + 0.5, by as f64 + 0.3, bz as f64 + 0.5));
            bot.wait_ticks(1).await.ok();
            let _ = bot.place_block(cx, cy, cz, Face::Top).await;
            let _ = bot.place_block(bx, by, bz, Face::Top).await;
            bot.wait_ticks(10).await.ok();
            used += (count_items(bot, BED) < beds_before) as i32;
            if bot.explosions > ex0 {
                detonations += 1;
                fired = true;
                cast_debug(&format!(
                    "DRAGON v2 detonation #{detonations}: bed ({bx},{by},{bz}) {used}/{beds0}, dragon was ({:.1},{:.1},{:.1}) hd {hd:.1}, now {:?}; bot hp {hp0:.0} → {:.0}",
                    dp.x, dp.y, dp.z, dragon(bot).map(|p| (p.x.round(), p.y.round(), p.z.round())), bot.health
                ));
                break;
            }
            if count_items(bot, BED) < beds_before {
                vanished = true;
                eaten.push((cx, cy, cz));
                cast_debug(&format!("DRAGON v2 bed placed then gone at ({bx},{by},{bz}), no explosion — a dragon part overlaps it ({used}/{beds0})"));
                continue;
            }
            cast_debug(&format!("DRAGON v2 bed rejected at ({bx},{by},{bz}) — {} (beds {})", name_at(bot, bx, by, bz), count_items(bot, BED)));
        }
        if fired {
            gone_streak = 0;
            continue;
        }
        bot.wait_ticks(if vanished { 40 } else { 20 }).await.ok();
        sitting_until = None;
        // Every candidate destroyed in one attempt: this perch eats them (late12-v2j drained 10 in 33 s
        // at full dragon health). Hold until the dragon LEAVES (a 20 s cap resumed while it still sat,
        // late14-v2l); 120 s is only a safety cap.
        if vanished {
            gone_streak += 1;
        }
        if gone_streak >= 1 {
            cast_debug(&format!("DRAGON v2 every bed cell destroyed ({used}/{beds0}) — holding until the dragon leaves this perch"));
            let t_hold = Instant::now();
            while t_hold.elapsed() < Duration::from_secs(120) {
                bot.wait_ticks(10).await.ok();
                if dragon(bot).is_none_or(|p| (p.x.powi(2) + p.z.powi(2)).sqrt() > 6.0) {
                    break;
                }
            }
            gone_streak = 0;
        }
    }
    failure(format!("DRAGON v2 budget spent: {detonations} detonations, dragon seen={seen}"))
}

/// End crystals the client tracks: (entity id, position of the crystal's base).
fn crystals(bot: &Bot) -> Vec<(i32, crate::vec3::Vec3)> {
    bot.entities
        .values()
        .filter(|e| e.name.as_deref().map(|n| n.contains("end_crystal")).unwrap_or(false))
        .map(|e| (e.id, e.position))
        .collect()
}

/// Pitch (radians, up positive) for a full-power arrow from `eye` to pass through `target`, by
/// simulating vanilla arrow flight per tick (pos += v; v *= 0.99; v.y −= 0.05; launch speed 3.0).
/// The flattest pitch whose path is within 0.35 of the target height at its horizontal distance.
fn arrow_pitch(bot: &Bot, eye: crate::vec3::Vec3, target: crate::vec3::Vec3) -> Option<f64> {
    let d = ((target.x - eye.x).powi(2) + (target.z - eye.z).powi(2)).sqrt();
    let (ux, uz) = ((target.x - eye.x) / d, (target.z - eye.z) / d);
    let dy = target.y - eye.y;
    // A solid block on the path before the target blocks the shot (late20-cry1: a 69° arc from
    // 10.6 out crossed the tower's own side ~10 under its top; 8 misses into obsidian).
    let blocked = |h: f64, y: f64| {
        let (x, yy, z) = (eye.x + ux * h, eye.y + y, eye.z + uz * h);
        (target.x - x).powi(2) + (target.y - yy).powi(2) + (target.z - z).powi(2) > 1.2 * 1.2
            && solid_at(bot, x.floor() as i32, yy.floor() as i32, z.floor() as i32)
    };
    let mut best: Option<(f64, f64)> = None;
    let mut deg = -30.0_f64;
    while deg <= 80.0 {
        let p = deg.to_radians();
        let (mut h, mut y, mut vh, mut vy) = (0.0_f64, 0.0_f64, 3.0 * p.cos(), 3.0 * p.sin());
        for _ in 0..200 {
            let (h0, y0) = (h, y);
            h += vh;
            y += vy;
            vh *= 0.99;
            vy = vy * 0.99 - 0.05;
            let reach = h >= d;
            let h_end = if reach { d } else { h };
            let y_end = if reach { y0 + (y - y0) * (d - h0) / (h - h0) } else { y };
            // Sample the segment for blocks (4 points per tick).
            if (1..=4).any(|k| {
                let f = k as f64 / 4.0;
                blocked(h0 + (h_end - h0) * f, y0 + (y_end - y0) * f)
            }) {
                break;
            }
            if reach {
                let err = (y_end - dy).abs();
                if err < 0.35 && best.is_none_or(|(_, e)| err < e) {
                    best = Some((p, err));
                }
                break;
            }
        }
        // The flattest clear arc first: stop at the first band that hits.
        if best.is_some() && deg > best.unwrap().0.to_degrees() + 1.0 {
            break;
        }
        deg += 0.1;
    }
    best.map(|(p, _)| p)
}

/// A clear full-power arrow arc from `eye` to the crystal at `cpos`: its centre first, then its top
/// (base + 1.8). A crystal sits ~1 above a wide obsidian top (or an iron-bar cage roof), so an arc to the
/// centre from below clips the rim; the top clears more of it (late25-cry6: 16/16 stands had floors,
/// 0 a clear arc to the centre). Returns (pitch, aim point).
fn aim(bot: &Bot, eye: crate::vec3::Vec3, cpos: crate::vec3::Vec3) -> Option<(f64, crate::vec3::Vec3)> {
    [1.0, 1.8].iter().find_map(|&up| {
        let t = vec3(cpos.x, cpos.y + up, cpos.z);
        arrow_pitch(bot, eye, t).map(|p| (p, t))
    })
}

/// Crystals gym (cycle 4 Part 9, skill 11): destroy every end crystal with a bow from the ground.
/// Any projectile destroys an uncaged crystal; walk to within ~14 of a tower, solve the arrow's pitch
/// from flight physics, draw 22 ticks (full power), release, and confirm by the entity disappearing.
/// v1 gives a crystal up after 8 misses (caged towers block arrows; the climb is the next version).
/// Crystals given up or missed, by entity id, for the current trial (the gym resets it).
pub static CRYSTAL_MISSES: std::sync::Mutex<Option<std::collections::HashMap<i32, u32>>> = std::sync::Mutex::new(None);

pub async fn crystals_bow(bot: &mut Bot<'_>, _mem: &mut WorldMemory) -> StepResult {
    if count_items(bot, "bow") < 1 || count_items(bot, "arrow") < 1 {
        bot.wait_ticks(100).await.ok();
        return failure("CRYSTALS: no bow or arrows".to_string());
    }
    let t0 = Instant::now();
    let budget = Duration::from_secs(crate::learn::budget_s("crystals").min(1800.0) as u64);
    // Persisted across step calls (reset per trial by the gym): a local map reset whenever the step
    // re-entered, so an unreachable crystal was re-tried 171 times in one trial (late26-cry7 t2).
    let mut misses: std::collections::HashMap<i32, u32> = CRYSTAL_MISSES.lock().unwrap().clone().unwrap_or_default();
    let (mut shots, mut kills) = (0, 0);
    let mut empty_since: Option<Instant> = None;
    while t0.elapsed() < budget {
        *CRYSTAL_MISSES.lock().unwrap() = Some(misses.clone());
        if bot.health <= 0.0 {
            let _ = bot.respawn().await;
            return failure(format!("CRYSTALS died after {kills} kills, {shots} shots"));
        }
        if bot.health < 10.0 {
            eat_if_hurt(bot).await;
        }
        let cs = crystals(bot);
        if cs.is_empty() {
            let since = *empty_since.get_or_insert_with(Instant::now);
            if since.elapsed() > Duration::from_secs(10) {
                return success(format!("CRYSTALS none visible for 10 s: {kills} kills in {shots} shots (harness checks the server)"));
            }
            bot.wait_ticks(20).await.ok();
            continue;
        }
        empty_since = None;
        let p = bot.entity.position;
        let Some(&(id, cpos)) = cs
            .iter()
            .filter(|(id, _)| misses.get(id).copied().unwrap_or(0) < 8)
            .min_by(|a, b| a.1.distance_xz(p).total_cmp(&b.1.distance_xz(p)))
        else {
            *CRYSTAL_MISSES.lock().unwrap() = Some(misses.clone());
            bot.wait_ticks(200).await.ok(); // nothing left to try: don't let the gym re-call it at once
            return failure(format!("CRYSTALS {} left, each missed 8× (caged?): {kills} kills in {shots} shots", cs.len()));
        };
        let target = vec3(cpos.x, cpos.y + 1.0, cpos.z);
        if target.distance_xz(p) > 30.0 {
            // Close in to ~24 out, not to the foot: from 10 out a steep arc crosses the tower's own
            // side (late20-cry1: 16 misses at 69–74° into obsidian). Shallower arcs clear the top edge.
            let _ = tokio::time::timeout(
                Duration::from_secs(40),
                bot.goto_near(cpos.x.floor() as i32, feet_y(bot), cpos.z.floor() as i32, 24.0),
            )
            .await;
            bot.clear_control_states();
            if target.distance_xz(bot.entity.position) > 32.0 {
                *misses.entry(id).or_insert(0) += 2;
                cast_debug(&format!("CRYSTALS could not close on crystal {id} at ({:.0},{:.0},{:.0})", cpos.x, cpos.y, cpos.z));
            }
            continue;
        }
        let eye = vec3(p.x, p.y + 1.52, p.z); // arrows spawn 0.1 under the eye
        let Some((pitch0, target)) = aim(bot, eye, cpos) else {
            // No clear arc from here: choose a stand around the tower (radius 20 and 26, 8 directions)
            // that has a floor and a clear arc, nearest to the bot. Backing straight out (cry2) walked
            // toward the fountain and found no arc (trial 1 stalled 4 crystals short).
            let floor_y = |x: i32, z: i32| (40..=95).rev().find(|&y| solid_at(bot, x, y - 1, z) && !solid_at(bot, x, y, z) && !solid_at(bot, x, y + 1, z));
            let mut stands: Vec<(f64, (i32, i32, i32))> = Vec::new();
            let mut floored = 0;
            for r in [14.0_f64, 20.0, 26.0, 32.0, 40.0] {
                for k in 0..16 {
                    let a = k as f64 * std::f64::consts::PI / 8.0;
                    let (sx, sz) = ((cpos.x + r * a.cos()).floor() as i32, (cpos.z + r * a.sin()).floor() as i32);
                    let Some(fy) = floor_y(sx, sz) else { continue };
                    floored += 1;
                    let e = vec3(sx as f64 + 0.5, fy as f64 + 1.52, sz as f64 + 0.5);
                    if aim(bot, e, cpos).is_some() {
                        let d = (p.x - sx as f64).powi(2) + (p.z - sz as f64).powi(2);
                        stands.push((d, (sx, fy, sz)));
                    }
                }
            }
            stands.sort_by(|a, b| a.0.total_cmp(&b.0));
            let Some(&(_, (sx, sy, sz))) = stands.first() else {
                // Far away, the stands may sit in chunks the client hasn't loaded (late23-cry4 gave up
                // three crystals at 0 s from the spawn): walk toward the tower first, give up only
                // when it's close and still has no stand.
                if target.distance_xz(p) > 30.0 {
                    cast_debug(&format!("CRYSTALS no stand known yet for ({:.0},{:.0},{:.0}) — walking closer", target.x, target.y, target.z));
                    let _ = tokio::time::timeout(
                        Duration::from_secs(40),
                        bot.goto_near(cpos.x.floor() as i32, feet_y(bot), cpos.z.floor() as i32, 24.0),
                    )
                    .await;
                    bot.clear_control_states();
                    *misses.entry(id).or_insert(0) += 1;
                    continue;
                }
                *misses.entry(id).or_insert(0) += 8;
                cast_debug(&format!(
                    "CRYSTALS no stand with a clear arc to ({:.0},{:.0},{:.0}) — giving it up (bot at ({:.1},{:.1},{:.1}), hd {:.1}; {floored}/80 stands have a floor, 0 a clear arc)",
                    target.x, target.y, target.z, p.x, p.y, p.z, target.distance_xz(p)
                ));
                continue;
            };
            *misses.entry(id).or_insert(0) += 1;
            cast_debug(&format!("CRYSTALS no clear arc from ({:.0},{:.0},{:.0}) — walking to stand ({sx},{sy},{sz}) for ({:.0},{:.0},{:.0})", p.x, p.y, p.z, target.x, target.y, target.z));
            let _ = tokio::time::timeout(Duration::from_secs(40), bot.goto_near(sx, sy, sz, 1.0)).await;
            bot.clear_control_states();
            continue;
        };
        // Jitter repeated shots by ±0.4° (identical misses teach nothing).
        let m = misses.get(&id).copied().unwrap_or(0);
        let pitch = pitch0 + [0.0_f64, 0.4, -0.4][(m % 3) as usize].to_radians();
        let yaw = (-(target.x - p.x)).atan2(-(target.z - p.z));
        if !select_item(bot, "bow").await.unwrap_or(false) {
            continue;
        }
        bot.clear_control_states();
        bot.look(yaw, pitch);
        bot.wait_ticks(2).await.ok();
        let _ = bot.activate_item().await;
        bot.wait_ticks(22).await.ok();
        bot.look(yaw, pitch);
        bot.wait_ticks(1).await.ok();
        let _ = bot.release_use_item().await;
        shots += 1;
        bot.wait_ticks(30).await.ok();
        if !bot.entities.contains_key(&id) {
            kills += 1;
            cast_debug(&format!("CRYSTALS kill #{kills}: crystal {id} at ({:.0},{:.0},{:.0}), pitch {:.1}°, shot {shots}", cpos.x, cpos.y, cpos.z, pitch.to_degrees()));
        } else {
            let m = misses.entry(id).or_insert(0);
            *m += 1;
            cast_debug(&format!("CRYSTALS miss {m} on crystal {id} at ({:.0},{:.0},{:.0}), pitch {:.1}°, from ({:.1},{:.1},{:.1})", cpos.x, cpos.y, cpos.z, pitch.to_degrees(), p.x, p.y, p.z));
        }
    }
    failure(format!("CRYSTALS budget spent: {kills} kills in {shots} shots, {} left", crystals(bot).len()))
}

/// `pillar_up` with a chosen block, for the End only (the dragon destroys any block outside its immune
/// tag, cobble included, so the dragon pillar is obsidian). Kept here, not in portal.rs, so the portal
/// module stays 6b plus the cycle-5 safety fixes.
pub(crate) async fn pillar_up_with(bot: &mut Bot<'_>, target_y: i32, block: Option<&'static str>) -> bool {
    bot.set_control_state("sneak", true);
    let cell_x = bot.entity.position.x.floor() as i32;
    let cell_z = bot.entity.position.z.floor() as i32;
    for _ in 0..24 {
        if feet_y(bot) >= target_y {
            break;
        }
        crate::tasks::portal::walk_to_xz(bot, cell_x as f64 + 0.5, cell_z as f64 + 0.5, 0.1, 24).await;
        bot.set_control_state("sneak", true);
        let f = feet_y(bot);
        // Clear the climb path two/three blocks up so a stray block doesn't block the jump.
        for dy in [2, 3] {
            let n = name_at(bot, cell_x, f + dy, cell_z);
            if crate::tasks::portal::is_solid(&n) && n != "obsidian" {
                crate::tasks::portal::dig_at(bot, cell_x, f + dy, cell_z).await;
            }
        }
        let b = block.filter(|b| count_items(bot, b) > 0).unwrap_or_else(|| crate::tasks::portal::build_block(bot));
        if !select_item(bot, b).await.unwrap_or(false) {
            break;
        }
        bot.look_at(vec3(cell_x as f64 + 0.5, (f - 2) as f64, cell_z as f64 + 0.5));
        bot.wait_ticks(3).await.ok();
        bot.set_control_state("jump", true);
        bot.wait_ticks(7).await.ok();
        // Place on top of the block one below our feet (the pillar we stand on).
        if solid_at(bot, cell_x, f - 1, cell_z) {
            let _ = bot.place_block(cell_x, f - 1, cell_z, Face::Top).await;
        }
        bot.wait_ticks(5).await.ok();
        bot.set_control_state("jump", false);
        bot.wait_ticks(8).await.ok();
    }
    feet_y(bot) >= target_y
    // Leave sneak ON — caller clears it once the block is poured.
}
