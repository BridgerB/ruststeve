//! `lava_safe_move`, the movement primitive near lava (cycle 4, Part 7), and its gym drill.
//!
//! Cycle 5, decision 1: it stays in the crate but is NOT on the portal path. The race build (6b-safe)
//! uses 6b's portal code; this module is exercised by the `lava_safe_move` drill slug only.

use crate::bot::Bot;
use crate::tasks::portal::{cast_debug, dig_at, feet_y, is_air, is_lava, is_solid, name_at, solid_at};
use crate::types::{failure, success, StepResult};
use crate::vec3::vec3;

/// Why the bot's current cell is unsafe near lava, or None if it is safe. Read from chunk data:
/// - the floor under the body's centre is solid and not lava;
/// - no lava in any column the 0.6-wide body overlaps (floor, feet, head);
/// - no lava in the 3×3 ring at feet and head height;
/// - no open side cell that drops into lava within 4 blocks.
/// The last is cycle 4's "no missing floor in the four side cells" read as a lava drop, not any drop:
/// the 2-wide mold platform has open sides by design, and a survivable fall is not the hazard.
/// `allow_side`: one intended open side over lava (the sealed station's scoop side O) that the
/// side-drop check skips. Everything else is still checked.
pub(crate) fn lava_unsafe_here(bot: &Bot, allow_side: Option<(i32, i32)>) -> Option<String> {
    let p = bot.entity.position;
    let fy = feet_y(bot);
    let (cx, cz) = (p.x.floor() as i32, p.z.floor() as i32);
    // Footing: lava, or a non-solid floor (air/water) with lava within 4 below it. Water or air over
    // no lava is not this primitive's hazard (leave_water owns water); vetoing it stalled descents
    // over water in comparison 6c+lsm2.
    let floor = name_at(bot, cx, fy - 1, cz);
    if is_lava(&floor) || (!is_solid(&floor) && (2..=5).any(|d| is_lava(&name_at(bot, cx, fy - d, cz)))) {
        return Some(format!("footing {floor} under ({cx},{},{cz})", fy - 1));
    }
    for (ox, oz) in [(-0.299, -0.299), (-0.299, 0.299), (0.299, -0.299), (0.299, 0.299)] {
        let (x, z) = ((p.x + ox).floor() as i32, (p.z + oz).floor() as i32);
        for y in [fy - 1, fy, fy + 1] {
            if is_lava(&name_at(bot, x, y, z)) {
                return Some(format!("lava in a body column at ({x},{y},{z})"));
            }
        }
    }
    for dx in -1..=1 {
        for dz in -1..=1 {
            for y in [fy, fy + 1] {
                if is_lava(&name_at(bot, cx + dx, y, cz + dz)) {
                    return Some(format!("lava in the body ring at ({},{y},{})", cx + dx, cz + dz));
                }
            }
        }
    }
    for (dx, dz) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
        if Some((dx, dz)) == allow_side {
            continue;
        }
        let (x, z) = (cx + dx, cz + dz);
        if is_air(&name_at(bot, x, fy, z)) && !solid_at(bot, x, fy - 1, z) {
            if let Some(d) = (1..=4).find(|d| is_lava(&name_at(bot, x, fy - d, z))) {
                return Some(format!("side ({x},{z}) drops into lava {d} below"));
            }
        }
    }
    None
}

/// THE movement primitive near lava (cycle 4, Part 7). Every walk that can end within four blocks
/// of lava goes through here; nothing near lava improvises a raw walk.
/// 1. Farther than one block: only the lava-aware pathfinder (it refuses lava-adjacent cells and
///    lava-ringed drop landings), capped at `cap`. Never a raw jumping walk.
/// 2. The last block, for an arrival (reach ≤ 1): the sneaking, non-jumping settle.
/// 3. **Post-condition** (from chunk data, `lava_unsafe_here`): safe footing and no lava in the body
///    ring or a side drop.
/// 4. If it fails: step back one cell toward where the move started, write a `vetoed` primitive
///    row, and return false. The caller re-plans; it never improvises a stand.
/// Returns true only when the bot is within `reach` of the target and the post-condition holds.
pub(crate) async fn lava_safe_move(bot: &mut Bot<'_>, target: (i32, i32, i32), reach: f64, cap: std::time::Duration, why: &str) -> bool {
    lava_safe_move_ex(bot, target, reach, cap, why, None).await
}

/// `lava_safe_move` with one intended open side over lava (the station's scoop side).
pub(crate) async fn lava_safe_move_ex(bot: &mut Bot<'_>, target: (i32, i32, i32), reach: f64, cap: std::time::Duration, why: &str, allow_side: Option<(i32, i32)>) -> bool {
    let t0 = std::time::Instant::now();
    let start = bot.entity.position;
    let start_cell = (start.x.floor() as i32, feet_y(bot), start.z.floor() as i32);
    let hd = |b: &Bot| ((target.0 as f64 + 0.5 - b.entity.position.x).powi(2) + (target.2 as f64 + 0.5 - b.entity.position.z).powi(2)).sqrt();
    let adjacent = hd(bot) <= 1.5 && (feet_y(bot) - target.1).abs() == 0;
    if !adjacent {
        // Never let the pathfinder walk all the way onto an arrival target (reach ≤ 1): stop ≥ 1.6 short and
        // finish with the sneaking settle, which will not walk off an edge. Walked or dropped all the way,
        // the bot's momentum carried it past a station stand into the open lava side (cycle-4 comparison,
        // rust-gym-002, dead within 1 s of `STATION pick`).
        // 6c+lsm5 drill: the 1.6 stop-short left every last block to the sneaking settle, which can't dig
        // and stalls at pool corners — one clean 10/10 in 24 runs. The momentum death it was added for
        // is covered by the pathfinder refusing drop landings beside an open lava hole (same build), so
        // path all the way again (6c+lsm: 10/10 twice) and keep the settle as the finisher.
        let path_reach = reach;
        let r = tokio::time::timeout(cap, bot.goto_near(target.0, target.1, target.2, path_reach)).await;
        if r.is_err() {
            bot.clear_control_states();
        }
    }
    if reach <= 1.0 && feet_y(bot) == target.1 && hd(bot) <= 2.5 {
        // One cell diagonal to the target with a pool corner between: the straight sneaking settle stops at
        // the edge and never leaves its cell (drill on 6c+lsm3: moves 6/8/9/10 vetoed for the target's own
        // open side, evaluated from the old cell). Go in an L through an orthogonal neighbour that is a
        // safe stand (solid non-lava floor, open body cells).
        // Any move that changes both x and z: a straight settle cuts the corner (next-lsm-L drill move 9
        // ended with its centre over a flush pool corner, 2 cells off in x). Take the L whose two legs
        // cross only safe stands.
        let (cx, cz) = (bot.entity.position.x.floor() as i32, bot.entity.position.z.floor() as i32);
        if target.0 != cx && target.2 != cz {
            let stand = |b: &Bot, x: i32, z: i32| {
                let f = name_at(b, x, target.1 - 1, z);
                is_solid(&f) && !is_lava(&f) && is_air(&name_at(b, x, target.1, z)) && is_air(&name_at(b, x, target.1 + 1, z))
            };
            let span = |a: i32, b: i32| if a <= b { a..=b } else { b..=a };
            let leg_x = |b: &Bot, z: i32| span(cx, target.0).all(|x| stand(b, x, z));
            let leg_z = |b: &Bot, x: i32| span(cz, target.2).all(|z| stand(b, x, z));
            // Via (tx, cz): x first along z = cz, then z along x = tx. Via (cx, tz): the other way round.
            let mut via = if leg_x(bot, cz) && leg_z(bot, target.0) {
                Some((target.0, cz))
            } else if leg_z(bot, cx) && leg_x(bot, target.2) {
                Some((cx, target.2))
            } else {
                None
            };
            // No open L: the corner is usually rock (the drill arena's rim corners are uncarved, which the
            // old full-pathfinder move dug through; 6c+lsm4 drill 6/10). Dig a corner waypoint's body cells
            // when it has solid non-lava footing and no lava touches it or the cell above.
            if via.is_none() && (target.0 - cx).abs() == 1 && (target.2 - cz).abs() == 1 {
                for (wx, wz) in [(cx, target.2), (target.0, cz)] {
                    let f = name_at(bot, wx, target.1 - 1, wz);
                    let lava_near = (target.1..=target.1 + 2).any(|y| {
                        [(0, 0), (1, 0), (-1, 0), (0, 1), (0, -1)].iter().any(|&(ox, oz)| is_lava(&name_at(bot, wx + ox, y, wz + oz)))
                    });
                    if !is_solid(&f) || is_lava(&f) || lava_near {
                        continue;
                    }
                    for y in [target.1 + 1, target.1] {
                        if !is_air(&name_at(bot, wx, y, wz)) {
                            dig_at(bot, wx, y, wz).await;
                        }
                    }
                    bot.wait_ticks(5).await.ok();
                    if stand(bot, wx, wz) {
                        via = Some((wx, wz));
                        break;
                    }
                }
            }
            if let Some((wx, wz)) = via {
                crate::tasks::portal_mold::settle_xz(bot, wx as f64 + 0.5, wz as f64 + 0.5, 0.2, 80).await;
            }
        }
        crate::tasks::portal_mold::settle_xz(bot, target.0 as f64 + 0.5, target.2 as f64 + 0.5, 0.2, 120).await;
        bot.set_control_state("sneak", false);
    }
    let arrived = {
        let p = bot.entity.position;
        let d3 = ((target.0 as f64 + 0.5 - p.x).powi(2) + (target.1 as f64 - p.y).powi(2) + (target.2 as f64 + 0.5 - p.z).powi(2)).sqrt();
        d3 <= reach.max(0.5) + 0.75
    };
    let unsafe_why = lava_unsafe_here(bot, allow_side);
    let row_ctx = |b: &Bot, extra: &str| {
        let p = b.entity.position;
        serde_json::json!({ "why": why, "target": [target.0, target.1, target.2], "reach": reach,
            "start": [start_cell.0, start_cell.1, start_cell.2], "end": [p.x.floor() as i64, p.y.floor() as i64, p.z.floor() as i64],
            "health": b.health, "detail": extra })
    };
    if let Some(u) = unsafe_why {
        cast_debug(&format!("lava_safe_move {why} → {target:?}: VETO ({u}) — stepping back toward {start_cell:?}"));
        bot.clear_control_states();
        // Already IN lava (the path's last drop or a flow the chunk data didn't show): escape first. The
        // step-back goto can't path out of lava, and the caller's own step keeps running with the
        // survival reflex blocked until it returns (6c+lsm3 rust-gym-001: veto at hp 8, dead in the pool).
        if crate::survival::in_lava(bot) {
            // escape_lava drives forward along the current yaw: face where the move started (known ground).
            bot.look_at(vec3(start_cell.0 as f64 + 0.5, start_cell.1 as f64 + 1.6, start_cell.2 as f64 + 0.5));
            crate::survival::escape_lava(bot, 60).await;
        }
        let p = bot.entity.position;
        let back = ((start_cell.0 as f64 + 0.5 - p.x).powi(2) + (start_cell.2 as f64 + 0.5 - p.z).powi(2)).sqrt();
        if back <= 1.5 && feet_y(bot) == start_cell.1 {
            crate::tasks::portal_mold::settle_xz(bot, start_cell.0 as f64 + 0.5, start_cell.2 as f64 + 0.5, 0.2, 80).await;
            bot.set_control_state("sneak", false);
        } else {
            let _ = tokio::time::timeout(std::time::Duration::from_secs(8), bot.goto_near(start_cell.0, start_cell.1, start_cell.2, 0.5)).await;
            bot.clear_control_states();
        }
        let ctx = row_ctx(bot, &u);
        crate::learn::primitive_row(bot, "lava_safe_move", "build_nether_portal", t0, "vetoed", &u, ctx);
        return false;
    }
    let ctx = row_ctx(bot, "");
    crate::learn::primitive_row(bot, "lava_safe_move", "build_nether_portal", t0, if arrived { "ok" } else { "failed" }, if arrived { "" } else { "not within reach" }, ctx);
    arrived
}

/// `lava_safe_move` gym drill (cycle 4 Part 7, SLUG=lava_safe_move). The LavaPool arena puts the bot at
/// (px, py, pz) beside a 5×5 flush pool at x px+4..px+8, z pz−2..pz+2, y py−1. Ten moves alternate
/// between the four rims, so each one paths around the pool; each rim stand's pool side is its
/// intended open side. Pass = 10 arrivals with the post-condition true (deaths are counted by the gym).
/// Set by the last `lsm_drill` run that went 10/10; the gym slug's pass reads it. The drill is not in
/// the step list, so the gym's default pass (the step's `is_complete`) never held and it re-ran the
/// drill from wherever the bot stood (targets no longer rim stands).
pub static DRILL_OK: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// The arena start of this trial's drill (reset per trial with DRILL_OK). A re-run after a failed pass
/// used the bot's position wherever it stood, so its targets drifted off the pool (6c+lsm3, next-lsm-L).
pub static DRILL_ANCHOR: std::sync::Mutex<Option<(i32, i32, i32)>> = std::sync::Mutex::new(None);


pub async fn lsm_drill(bot: &mut Bot<'_>) -> StepResult {
    DRILL_OK.store(false, std::sync::atomic::Ordering::Relaxed);
    let here = (bot.entity.position.x.floor() as i32, feet_y(bot), bot.entity.position.z.floor() as i32);
    let (px, py, pz) = *DRILL_ANCHOR.lock().unwrap().get_or_insert(here);
    if here != (px, py, pz) {
        lava_safe_move(bot, (px, py, pz), 0.3, std::time::Duration::from_secs(30), "drill_home").await;
    }
    let w = (1, 0); // the pool is +X of the west rim
    let e = (-1, 0);
    let n = (0, 1);
    let s = (0, -1);
    let targets = [
        ((px + 3, py, pz), w), ((px + 6, py, pz - 3), n), ((px + 9, py, pz), e), ((px + 6, py, pz + 3), s),
        ((px + 3, py, pz - 2), w), ((px + 4, py, pz - 3), n), ((px + 9, py, pz + 2), e), ((px + 8, py, pz + 3), s),
        ((px + 3, py, pz + 2), w), ((px + 8, py, pz - 3), n),
    ];
    let mut ok = 0;
    for (i, (t, side)) in targets.iter().enumerate() {
        let arrived = lava_safe_move_ex(bot, *t, 0.3, std::time::Duration::from_secs(30), "drill", Some(*side)).await;
        let unsafe_now = lava_unsafe_here(bot, Some(*side));
        cast_debug(&format!("DRILL {}/10 → {t:?}: arrived={arrived} post={} hp={:.0}", i + 1, unsafe_now.clone().unwrap_or_else(|| "ok".into()), bot.health));
        if arrived && unsafe_now.is_none() {
            ok += 1;
        }
    }
    if ok == targets.len() {
        DRILL_OK.store(true, std::sync::atomic::Ordering::Relaxed);
        success(format!("lava_safe_move drill {ok}/10"))
    } else {
        failure(format!("lava_safe_move drill {ok}/10"))
    }
}

