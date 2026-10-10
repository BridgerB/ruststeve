//! Portal cast as a FIXED TEMPLATE ("mold") instead of ten independent per-cell constructions.
//!
//! Frame cells are `(bx+dx, by+dy, bz)` on the X/Y plane at z=bz; the bot works from +Z.
//! Layers bottom-up: y=by → (1,0),(2,0); by+1..by+3 → (0,k),(3,k); by+4 → (1,4),(2,4).
//!
//! Geometry (from the pour-ray math; the reason the old cast kept landing lava on its own feet):
//! every cell is poured from feet = cell.y+1, standing on the cell's own +Z cup wall — the
//! "platform" for that layer. From there the lava ray leaves the bot's column through its own
//! (air) feet cell, crosses the bowl (cell above the cup, air) and drops into the cup; the water
//! ray enters the bowl directly. From one block higher the ray clips the top of the stand block
//! and the lava lands at the feet. So the platform RISES one block per layer and nothing is ever
//! placed at cell.y+2 above a cell that still needs pouring.
//!
//! Template per cell (x,Y): floor (x,Y-1,bz) · cup walls N (x,Y,bz-1) E/W (x±1,Y,bz) · S = the
//! platform block (x,Y,bz+1) · bowl walls N/E/W at Y+1 · bowl S = the bot's own body. Walls are
//! ENSURED (natural rock counts), never dug, except: template cobble sitting IN a frame cell is dug
//! right before that cell is poured (the neighbour it walled is obsidian by then, no lava adjacent).
//! Water: one bucket. The bowl is enclosed (N/E/W walls, obsidian floor, bot body) so the source
//! stays put; it is scooped straight back after each conversion.

use std::time::{Duration, Instant};

use crate::bot::Bot;
use crate::bot_utils::{count_items, select_item};
use crate::memory::WorldMemory;
use crate::tasks::portal::{
    cast_debug, descend_to_y, dig_at, eat_if_hurt, ensure_solid, feet_y, fill_bucket, first_solid_on_ray, is_air,
    is_fluid_source, is_lava, is_solid, name_at, pillar_up, place_cobble, reliable_use, respawn_at_frame, solid_at,
    walk_to_xz, SAFE_SCOOP_STAND,
};
use crate::vec3::vec3;

/// The 10 frame cells as (dx, dy), grouped by layer (dy), bottom-up.
const LAYERS: [&[(i32, i32)]; 5] = [&[(1, 0), (2, 0)], &[(0, 1), (3, 1)], &[(0, 2), (3, 2)], &[(0, 3), (3, 3)], &[(1, 4), (2, 4)]];

fn frame_cells() -> Vec<(i32, i32)> {
    LAYERS.iter().flat_map(|l| l.iter().copied()).collect()
}

/// Cast all 10 cells of the frame anchored at `(bx,by,bz)` with the template. True when all 10
/// read as obsidian. `lava_pool` is the refill source found by prepare (SAFE_SCOOP_STAND is where
/// the bot last scooped safely).
/// A full cube (a real wall for a face-aimed pour) — excludes thin/partial blocks.
fn full_block(n: &str) -> bool {
    const PARTIAL: [&str; 24] = [
        "pointed_dripstone", "_slab", "_stairs", "_fence", "_wall", "_pane", "iron_bars", "chain",
        "lantern", "torch", "ladder", "snow", "carpet", "button", "sign", "cobweb", "cluster", "bud",
        "vein", "lichen", "vine", "dripleaf", "azalea", "rail",
    ];
    !PARTIAL.iter().any(|p| n.contains(p)) || n == "snow_block"
}

/// Lava cells inside the mold's footprint (pad floor + frame plane + platform rows) at an origin.
fn footprint_lava(bot: &Bot, (bx, by, bz): (i32, i32, i32)) -> usize {
    let mut n = 0;
    // Includes the rim ring (dx -2..6, dz -2..3) so the pad AND its rim sit on dry ground.
    for dx in -2..=6 {
        for dy in -1..=1 {
            for dz in -2..=3 {
                if is_lava(&name_at(bot, bx + dx, by + dy, bz + dz)) {
                    n += 1;
                }
            }
        }
    }
    n
}

pub async fn cast_frame_mold(
    bot: &mut Bot<'_>,
    (bx0, by, bz0): (i32, i32, i32),
    lava_pool: Option<(i32, i32, i32)>,
    mem: &mut WorldMemory,
) -> bool {
    let budget_secs: u64 = std::env::var("MOLD_BUDGET_SECS").ok().and_then(|s| s.parse().ok()).unwrap_or(2400);
    let deadline = Instant::now() + Duration::from_secs(budget_secs);
    // KEEP THE FOOTPRINT OFF THE POOL. The pad floor is placed with ensure_solid, and a block
    // placed into a lava source DESTROYS it — on the gym's 5×5 pool the pad ate the sources and
    // the third refill found "0 sources, 36 lava blocks seen" (all flowing). Shift the origin
    // away from the lava (try the four directions, up to 6 blocks) until the footprint is dry,
    // and persist the shifted anchor so resumes agree. Only shift a frame with no obsidian yet.
    let (mut bx, mut bz) = (bx0, bz0);
    {
        let at0 = |dx: i32, dy: i32| (bx0 + dx, by + dy, bz0);
        let has_obsidian = frame_cells().iter().any(|&(dx, dy)| name_at(bot, at0(dx, dy).0, at0(dx, dy).1, at0(dx, dy).2) == "obsidian");
        if !has_obsidian && footprint_lava(bot, (bx0, by, bz0)) > 0 {
            let mut best = (footprint_lava(bot, (bx0, by, bz0)), bx0, bz0);
            'shift: for d in 1..=6 {
                for (sx, sz) in [(0, d), (0, -d), (-d, 0), (d, 0)] {
                    let n = footprint_lava(bot, (bx0 + sx, by, bz0 + sz));
                    if n < best.0 {
                        best = (n, bx0 + sx, bz0 + sz);
                    }
                    if n == 0 {
                        break 'shift;
                    }
                }
            }
            (bx, bz) = (best.1, best.2);
            if (bx, bz) != (bx0, bz0) {
                crate::tasks::portal::set_frame_anchor((bx, by, bz));
                cast_debug(&format!("MOLD origin shifted off the pool: ({bx0},{by},{bz0}) → ({bx},{by},{bz}) lava_in_footprint={}", best.0));
            }
        }
    }
    let at = |dx: i32, dy: i32| (bx + dx, by + dy, bz);
    let is_obs = |bot: &Bot, c: (i32, i32, i32)| name_at(bot, c.0, c.1, c.2) == "obsidian";
    let done = |bot: &Bot| frame_cells().iter().filter(|&&(dx, dy)| is_obs(bot, at(dx, dy))).count();
    cast_debug(&format!("MOLD start anchor=({bx},{by},{bz}) done={}/10 cobble={} lava_b={} water_b={} bucket={}",
        done(bot), crate::tasks::portal::scaffold_count(bot), count_items(bot, "lava_bucket"),
        count_items(bot, "water_bucket"), count_items(bot, "bucket")));
    mem.log("mold", "start", &format!("{bx},{by},{bz} done={}", done(bot)));

    fill_pad(bot, (bx, by, bz)).await;
    cast_debug("MOLD pad done");
    // RIM: one cobble ring at pad level (y=by) around the pad. Bowl water that escapes past the
    // bot's body runs onto the pad, and the pad floor is the pool's own surface level — on the
    // gym arena it reached the pool and turned all 25 sources to obsidian (refill: "0 lava
    // blocks seen"). The rim keeps spilled water on the pad where it gets scooped.
    for dz in -2..=3 {
        for dx in -2..=6 {
            let on_edge = dz == -2 || dz == 3 || dx == -2 || dx == 6;
            if !on_edge {
                continue;
            }
            let r = (bx + dx, by, bz + dz);
            if is_air(&name_at(bot, r.0, r.1, r.2)) {
                // Stand inside the pad (one block toward the centre) to place the rim block.
                let inward = (bx + dx.clamp(-1, 5), by, bz + dz.clamp(-1, 2));
                let _ = bot.goto_near(inward.0, by, inward.2, 1.5).await;
                ensure_solid(bot, r, 0).await;
            }
        }
    }
    cast_debug("MOLD rim done");
    // Respawn on the pad's stair base (solid floor, off the frame plane, never a cup).
    let _ = bot.run_command(&format!("spawnpoint @s {} {} {}", bx + 5, by, bz + 2)).await;

    for (li, layer) in LAYERS.iter().enumerate() {
        let y = by + li as i32;
        if layer.iter().all(|&(dx, dy)| is_obs(bot, at(dx, dy))) {
            continue;
        }
        if Instant::now() > deadline {
            break;
        }
        // Lava for this layer (2 pours) — refill BEFORE climbing, from the pad level.
        // Up to 3 tries: a roofed-lake refill often returns empty after only repositioning, and the
        // next try scoops (rust-gym-003 `MOLD end 8/10`, rust-gym-002 `MOLD end 4/10`, 11:1x — both
        // then refilled on prepare's first attempt, ~1 min later).
        for _ in 0..3 {
            if count_items(bot, "lava_bucket") >= 2 || count_items(bot, "bucket") < 1 {
                break;
            }
            refill_lava(bot, lava_pool, (bx, by, bz)).await;
        }
        // A roofed-source refill digs the roof it scoops through — often a PAD cell (the pool is
        // under the frame). rust-gym-002 (12:27 binary): refill stand `(893,-56,1216) INVALID
        // floor=air`, then died 9 s after `MOLD layer 3: platform done` (server: `tried to swim in
        // lava`). Re-lay the pad after every refill.
        fill_pad(bot, (bx, by, bz)).await;
        if count_items(bot, "lava_bucket") < 1 {
            cast_debug("MOLD no lava bucket after refill — abort layer");
            break;
        }
        // Stair up the east side so the pad/pool stays reachable from every layer's platform.
        ensure_stair(bot, bx, by, bz, li as i32).await;
        cast_debug(&format!("MOLD layer {li}: stair done"));
        // PLATFORM for this layer: blocks at y (feet y+1) along z=bz+1 (the cup walls' S side)
        // and z=bz+2, dx -1..=4; headroom above cleared.
        ensure_platform(bot, bx, y, bz).await;
        cast_debug(&format!("MOLD layer {li}: platform done at ({:.1},{:.1},{:.1}) hp={:.0}", bot.entity.position.x, bot.entity.position.y, bot.entity.position.z, bot.health));

        for &(dx, dy) in layer.iter() {
            let cell = at(dx, dy);
            if is_obs(bot, cell) {
                continue;
            }
            if Instant::now() > deadline {
                break;
            }
            eat_if_hurt(bot).await;
            let mut ok = false;
            for attempt in 0..3 {
                if bot.health <= 0.0 {
                    cast_debug("MOLD dead — respawn");
                    // respawn_at_frame, not a bare respawn: the spawnpoint on the pad is rejected as
                    // unsafe, so a bare respawn landed on the SURFACE and descend_to_y dug back down
                    // at ~45 s per block (rust-gym-003: `MOLD dead` at 636 s, still at y 48 at 1403 s).
                    crate::tasks::portal::respawn_at_frame(bot).await;
                    let _ = bot.goto_near(bx + dx, y + 1, bz + 2, 1.5).await;
                }
                if count_items(bot, "lava_bucket") < 1 && !name_at(bot, cell.0, cell.1, cell.2).contains("lava") {
                    refill_lava(bot, lava_pool, (bx, by, bz)).await;
                    if count_items(bot, "lava_bucket") < 1 {
                        break;
                    }
                }
                if cast_cell(bot, (bx, by, bz), (dx, dy), attempt).await {
                    ok = true;
                    break;
                }
            }
            let n = done(bot);
            mem.log("mold", "cell", &format!("{dx},{dy} ok={ok} done={n}/10"));
            cast_debug(&format!("MOLD cell ({dx},{dy}) ok={ok} → {n}/10"));
            if !ok {
                // A cell that won't cast after 3 stances: fall out so the caller re-derives (the
                // frame persists; the next call resumes here).
                return false;
            }
        }
    }
    let n = done(bot);
    if n == 10 {
        open_front(bot, bx, by, bz).await;
    }
    cast_debug(&format!("MOLD end {n}/10"));
    n == 10
}

/// Remove the platform tower in front of the frame (dx 0..=3, dz 1..=2, dy 0..=5) so the lighter
/// can stand at (x, by, bz+2) and walk in. Without this the frame read 10/10 but the flint&steel
/// never reached the bottom row (`frame_check 10/10`, no `portal_lit`) — the stand was cobble.
async fn open_front(bot: &mut Bot<'_>, bx: i32, by: i32, bz: i32) {
    // 1. Down the east stair to the pad (feet by at the stair base). Digging our own column down
    //    through the tower stalled (dug=1, no drop) and every unreachable goto after that cost
    //    18 s × 48 cells — gym-003 sat 6+ min in here.
    timed_move(bot, bx + 5, by, bz + 2, 1.0, "front").await;
    if feet_y(bot) > by + 1 {
        descend_to_y(bot, by).await; // fallback: dig down where we stand
    }
    // 2. From the rim row in front of each column (z=bz+3, feet by+1) dig the tower top-down:
    //    every cell is within ~3.6 blocks of the eye from there. One goto per column.
    cast_debug("MOLD front: at the stair base");
    for dx in 0..=3 {
        timed_move(bot, bx + dx, by + 1, bz + 3, 1.0, "front").await;
        let mut dug = 0;
        let mut still = 0;
        for dz in [2, 1] {
            for dy in (0..=5).rev() {
                let c = (bx + dx, by + dy, bz + dz);
                let n = name_at(bot, c.0, c.1, c.2);
                if is_solid(&n) && n != "obsidian" {
                    dig_at(bot, c.0, c.1, c.2).await;
                    dug += 1;
                    if !is_air(&name_at(bot, c.0, c.1, c.2)) {
                        still += 1;
                    }
                }
            }
        }
        // Timing: open_front took 551 s on gym-001 with one slow move logged; this names the column.
        cast_debug(&format!("MOLD front column dx={dx}: dug {dug}, still solid {still}"));
    }
    // The lighter stands at (bx+1..2, by, bz+2): those two cells and the head cells above them
    // MUST be air. The pass above missed the dy=0 blocks once (gym-003: both stand cells still
    // cobble → goto to the stand failed → "would not light"). Re-check from the pad side.
    for dx in 1..=2 {
        for dy in 0..=1 {
            let c = (bx + dx, by + dy, bz + 2);
            let n = name_at(bot, c.0, c.1, c.2);
            if is_solid(&n) && n != "obsidian" {
                timed_move(bot, bx + 4, by, bz + 2, 1.5, "front").await;
                dig_at(bot, c.0, c.1, c.2).await;
            }
        }
    }
    cast_debug(&format!(
        "MOLD front opened at ({bx},{by},{bz}) — feet={} stand=({},{})",
        feet_y(bot),
        name_at(bot, bx + 1, by, bz + 2),
        name_at(bot, bx + 2, by, bz + 2)
    ));
}

/// Cast one cell from the fixed stance. True when the cell reads as obsidian.
async fn cast_cell(bot: &mut Bot<'_>, (bx, by, bz): (i32, i32, i32), (dx, dy): (i32, i32), attempt: u32) -> bool {
    let (x, y) = (bx + dx, by + dy);
    let cup = |bot: &Bot| name_at(bot, x, y, bz);
    let tag = format!("MOLD ({dx},{dy}) a{attempt}");
    // Stand on the platform in front of the cell first (all wall placements are within reach).
    // Position + hp around the stance moves: batch 10 had a death within 9 s of `platform done`, before
    // any cell line, so one of these two moves kills and the log could not say which.
    let at = |bot: &Bot| {
        let p = bot.entity.position;
        format!("({:.1},{:.1},{:.1}) hp={:.0}", p.x, p.y, p.z, bot.health)
    };
    // Never start a pour hurt: a miss costs ~4–11 hp (lava in the stance cell, then scooped back),
    // and rust-gym-005 entered its third attempt at hp 5 and died. Back off the stand line and eat.
    if bot.health < 14.0 {
        cast_debug(&format!("{tag}: hp {:.0} < 14 — stepping back to heal before the pour", bot.health));
        bot.set_control_state("sneak", false);
        timed_move(bot, x, y + 1, bz + 3, 0.6, "heal-retreat").await;
        for _ in 0..8 {
            if bot.health <= 0.0 || bot.health >= 16.0 {
                break;
            }
            eat_if_hurt(bot).await;
            bot.wait_ticks(20).await.ok();
        }
        return false;
    }
    cast_debug(&format!("{tag}: stance goto from {}", at(bot)));
    let _ = bot.goto_near(x, y + 1, bz + 1, 1.5).await;
    cast_debug(&format!("{tag}: after goto {}", at(bot)));
    // settle_xz, not walk_to_xz: walk_to_xz jumps when stalled and hops off the 2-wide platform.
    // It was harmless only while its tick count was really a packet count (it barely moved). Since
    // the real-tick fix it walks for real, and batches 6–8 died right after `platform done` (lava
    // deaths per batch: 0 in batch 5, then 4, 6, 9).
    settle_xz(bot, x as f64 + 0.5, bz as f64 + 1.5, 0.3, 40).await;
    cast_debug(&format!("{tag}: after settle {}", at(bot)));
    if feet_y(bot) < y + 1 {
        pillar_up(bot, y + 1).await;
    }
    // Walls: floor, cup N/E/W, bowl N/E/W. Bowl E/W is left OPEN toward the sibling cell of the
    // same layer (bottom pair / top pair share their bowls' side; both bowls are contained by the
    // outer walls and the other cell's cobble floor).
    let sibling = |ox: i32| LAYERS.iter().any(|l| l.contains(&(dx, dy)) && l.contains(&(ox - bx, dy)));
    let mut walls = vec![(x, y - 1, bz), (x, y, bz - 1), (x + 1, y, bz), (x - 1, y, bz), (x, y + 1, bz - 1)];
    if !sibling(x + 1) {
        walls.push((x + 1, y + 1, bz));
    }
    if !sibling(x - 1) {
        walls.push((x - 1, y + 1, bz));
    }
    for w in walls {
        // A PARTIAL block is not a wall: the pours aim at wall faces, and a thin hitbox (pointed
        // dripstone at a bowl/cup north wall) sent three lava pours nowhere (rust-gym-002:
        // `lava MISSED cup=air lava_at=[] … bowlN=pointed_dripstone` ×3). Dig it, place cobble.
        let wn = name_at(bot, w.0, w.1, w.2);
        if is_solid(&wn) && !full_block(&wn) && wn != "obsidian" {
            dig_at(bot, w.0, w.1, w.2).await;
        }
        if !solid_at(bot, w.0, w.1, w.2) && !ensure_solid(bot, w, 0).await {
            cast_debug(&format!("{tag}: wall FAIL {w:?}"));
            return false;
        }
    }
    // The cell itself: template cobble (a neighbour's wall) → dig; lava (earlier try) → keep.
    // Loop: gravel/sand above refills a dug cup (rust-gym-001, 13:4x: `lava MISSED cup=gravel` ×3
    // at (3,3) — the pour hit falling gravel). Dig until it stays clear.
    for _ in 0..8 {
        let n = cup(bot);
        if !is_solid(&n) || n == "obsidian" {
            break;
        }
        dig_at(bot, x, y, bz).await;
        bot.wait_ticks(if n.contains("gravel") || n.contains("sand") { 12 } else { 4 }).await.ok();
    }
    // Bowl must be air (never cobble/water): dig cobble; water is scooped below.
    let bowl = (x, y + 1, bz);
    let bn = name_at(bot, bowl.0, bowl.1, bowl.2);
    if is_solid(&bn) && bn != "obsidian" {
        dig_at(bot, bowl.0, bowl.1, bowl.2).await;
    }
    scoop_water_near(bot, (bx, by, bz)).await;
    // Water still in the cup (a flowing remnant, or a source the scoop couldn't reach): a
    // block placed into water replaces it; dig it back out → air. Never leave water in a cup.
    if cup(bot).contains("water") {
        ensure_solid(bot, (x, y, bz), 0).await;
        dig_at(bot, x, y, bz).await;
        bot.wait_ticks(4).await.ok();
    }
    if !is_air(&cup(bot)) && !cup(bot).contains("lava") {
        cast_debug(&format!("{tag}: cell still not clear ({}) — skip", cup(bot)));
        return false;
    }

    // STANCE ASSERTION: feet exactly y+1, centred on (x+0.5, bz+1.5), on the ground. Never pour
    // from anywhere else — a wrong stance is a retry, not a hunt.
    if !assert_stance(bot, x, y + 1, bz + 1).await {
        let p = bot.entity.position;
        cast_debug(&format!("{tag}: stance FAIL feet={} bot=({:.2},{:.2}) want feet {} at ({:.1},{:.1})", feet_y(bot), p.x, p.z, y + 1, x as f64 + 0.5, bz as f64 + 1.5));
        return false;
    }
    bot.set_control_state("sneak", true);

    // LAVA into the cup.
    if !cup(bot).contains("lava") {
        if !select_item(bot, "lava_bucket").await.unwrap_or(false) {
            bot.set_control_state("sneak", false);
            return false;
        }
        // Aim at the cup's NORTH WALL face (low), not the cup floor. The floor aim was a steep
        // ray that cleared the stand's top edge by ~0.2 blocks from the SNEAKING eye (1.27, not
        // 1.62) — any z drift ≥0.1 put the lava in the bot's own feet cell (hp 20→6, bucket lost).
        // The wall aim leaves the stand column ~0.6 above the stand top and still enters the cup.
        let aim = vec3(x as f64 + 0.5, y as f64 + 0.3, bz as f64 - 0.02);
        bot.look_at(aim);
        bot.wait_ticks(3).await.ok();
        bot.look_at(aim);
        let _ = bot.sync_look().await;
        bot.activate_item().await.ok();
        bot.wait_ticks(8).await.ok();
        if !cup(bot).contains("lava") {
            bot.wait_ticks(10).await.ok(); // block updates can lag a busy server; re-read before judging
        }
        if !cup(bot).contains("lava") {
            let strays = lava_near(bot, (bx, by, bz));
            let p = bot.entity.position;
            cast_debug(&format!(
                "{tag}: lava MISSED cup={} lava_at={strays:?} eye=({:.2},{:.2},{:.2}) bowlN={} stand={}",
                cup(bot), p.x, p.y + 1.62, p.z,
                name_at(bot, x, y + 1, bz - 1), name_at(bot, x, y, bz + 1)
            ));
            // Recover the source (it is within the mold) so the retry has its bucket back.
            let in_bowl = strays.contains(&(x, y + 1, bz));
            // Lava in our OWN stance cell means the ray stopped on the BOWL's south face: the bowl is
            // solid on the server while we see air (ghost air). rust-gym-005 (batch 1) missed (0,2)
            // this way three times, scooping lava back out of its own feet each time, hp 20 → 16 → 5
            // → dead. Re-sync the bowl the same way as the cup below.
            let in_stance = strays.contains(&(x, y + 1, bz + 1));
            if select_item(bot, "bucket").await.unwrap_or(false) {
                for l in strays {
                    reliable_use(bot, vec3(l.0 as f64 + 0.5, l.1 as f64 + 0.5, l.2 as f64 + 0.5)).await;
                }
            }
            // Lava in the BOWL means the ray stopped on the cup's top face: the cup is SOLID on the
            // server while our local view says air (a dig the server didn't complete — the mirror
            // image of the ghost-block bug). Place-then-dig re-syncs both sides: the place fails
            // server-side but makes the local cell solid, and the dig then runs for cobble's real
            // break time so the server breaks it too.
            // The stand is level with the bowl, so a bowl landing flows into our own cell. Lava
            // still at our feet (flowing, unscoopable) → step back down the stand line first.
            // rust-gym-002 (11:34 binary) stayed to re-dig after its 2nd bowl landing with lava at
            // (875,-59,1202..1204) — its own cell was 1203 — and died 7 s later (`MOLD dead`).
            let fy = feet_y(bot);
            let (fx, fz) = (bot.entity.position.x.floor() as i32, bot.entity.position.z.floor() as i32);
            let hot = [(0, 0), (1, 0), (-1, 0), (0, 1), (0, -1)]
                .iter()
                .any(|&(dx, dz)| name_at(bot, fx + dx, fy, fz + dz).contains("lava"));
            if hot {
                bot.set_control_state("sneak", false);
                cast_debug(&format!("{tag}: lava at our feet after the miss — stepping back"));
                timed_move(bot, x, y + 1, bz + 3, 0.6, "miss-retreat").await;
                return false;
            }
            if in_stance {
                ensure_solid(bot, (x, y + 1, bz), 0).await;
                dig_at(bot, x, y + 1, bz).await;
                bot.wait_ticks(4).await.ok();
                cast_debug(&format!("{tag}: lava landed in our stance cell — bowl re-synced → bowl={}", name_at(bot, x, y + 1, bz)));
                bot.set_control_state("sneak", false);
                return false;
            }
            if in_bowl {
                ensure_solid(bot, (x, y, bz), 0).await;
                dig_at(bot, x, y, bz).await;
                bot.wait_ticks(4).await.ok();
                cast_debug(&format!("{tag}: re-dug cup after bowl landing → cup={}", cup(bot)));
            }
            bot.set_control_state("sneak", false);
            return false;
        }
    }
    // WATER onto it (into the bowl).
    if count_items(bot, "water_bucket") < 1 {
        // The water is in the world — usually spilled inside the mold (a cup read `water`). Get it
        // back before giving up: rust-gym-002 (05:47 binary) restarted the mold with `water_b=0`
        // after two `cell still not clear (water)` attempts and could convert nothing.
        bot.set_control_state("sneak", false);
        scoop_water_near(bot, (bx, by, bz)).await;
        if count_items(bot, "water_bucket") < 1 {
            fill_bucket(bot, "water").await;
        }
        cast_debug(&format!("{tag}: no water bucket → recovered water_b={}", count_items(bot, "water_bucket")));
        return false;
    }
    select_item(bot, "water_bucket").await.ok();
    // Aim at the bowl's NORTH WALL inner face, not the bowl floor: a filled bucket's raycast
    // ignores fluids, so a floor aim went straight through the lava source and REPLACED it with
    // water (`cup=water`, no obsidian). Hitting the wall face places the water in the bowl cell.
    let waim = vec3(x as f64 + 0.5, y as f64 + 1.5, bz as f64 - 0.02);
    bot.look_at(waim);
    bot.wait_ticks(3).await.ok();
    bot.look_at(waim);
    let _ = bot.sync_look().await;
    bot.activate_item().await.ok();
    // Conversion is a neighbour update — 1-2 ticks. Poll per tick and scoop the source THE
    // MOMENT the cup reads obsidian: water spreads one block per 5 ticks, and a 60-tick wait let
    // it run off the pad into the pool (25 sources → obsidian). The scoop below removes every
    // flowing cell with the source.
    let mut made = false;
    for _ in 0..60 {
        bot.wait_ticks(1).await.ok();
        if cup(bot) == "obsidian" {
            made = true;
            break;
        }
    }
    let water = water_near(bot, (bx, by, bz));
    cast_debug(&format!("{tag}: after water cup={} water_at={water:?} made={made}", cup(bot)));
    // Reclaim the water (bowl first) — immediately, before it spreads further.
    if select_item(bot, "bucket").await.unwrap_or(false) {
        let baim = vec3(bowl.0 as f64 + 0.5, bowl.1 as f64 + 0.5, bowl.2 as f64 + 0.5);
        bot.look_at(baim);
        bot.wait_ticks(1).await.ok();
        bot.look_at(baim);
        bot.activate_item().await.ok();
        bot.wait_ticks(4).await.ok();
        if count_items(bot, "water_bucket") < 1 {
            scoop_water_near(bot, (bx, by, bz)).await;
        }
    }
    bot.set_control_state("sneak", false);
    if !made && cup(bot).contains("lava") {
        cast_debug(&format!("{tag}: water did not convert — cup still lava, retry water"));
    }
    made || cup(bot) == "obsidian"
}

/// Exact stance: feet == `feet`, |x-(cx+0.5)|<0.25, |z-(cz+0.5)|<0.25, on ground. Re-walks up to 3×.
async fn assert_stance(bot: &mut Bot<'_>, cx: i32, feet: i32, cz: i32) -> bool {
    let (tx, tz) = (cx as f64 + 0.5, cz as f64 + 0.5);
    // Sneak while settling: the stand block's north face is the frame plane (the cup is air at
    // foot level), so an un-sneaked overshoot hangs the hitbox over the cup and never settles.
    bot.set_control_state("sneak", true);
    for attempt in 0..3 {
        // The stand block and its two platform neighbours (behind, and the diagonal the bot
        // usually approaches over) must be solid — a platform cell that failed to place reads
        // air and strands the bot a block away (probe: (4833,-45,2479) air on a stance fail).
        for c in [(cx, feet - 1, cz), (cx, feet - 1, cz + 1), (cx + 1, feet - 1, cz + 1), (cx - 1, feet - 1, cz + 1)] {
            if !solid_at(bot, c.0, c.1, c.2) {
                ensure_solid(bot, c, 0).await;
            }
        }
        if attempt > 0 {
            // Back off one block south along the platform, then come in straight.
            settle_xz(bot, tx, tz + 1.0, 0.3, 60).await;
        }
        settle_xz(bot, tx, tz, 0.15, 160).await;
        if feet_y(bot) > feet {
            descend_to_y(bot, feet).await;
        } else if feet_y(bot) < feet {
            pillar_up(bot, feet).await;
        }
        bot.set_control_state("sneak", true);
        bot.wait_ticks(4).await.ok();
        let p = bot.entity.position;
        // 0.4: the wall-face aims are insensitive to this much drift (the cup/bowl faces are a
        // full block wide and the ray enters them near their centre from ±0.4).
        if feet_y(bot) == feet && (p.x - tx).abs() < 0.4 && (p.z - tz).abs() < 0.4 && bot.entity.on_ground {
            return true;
        }
    }
    false
}

/// Precise, NON-JUMPING centring (walk_to_xz jumps whenever progress stalls, which on the
/// 2-wide platform hops the bot onto a neighbouring block or off the edge — "feet one too
/// high", never settling). Sneak on, look at the target, pulse forward in short bursts, stop
/// inside `tol`. Never leaves the current block level.
pub(crate) async fn settle_xz(bot: &mut Bot<'_>, tx: f64, tz: f64, tol: f64, max_ticks: u32) {
    bot.set_control_state("sneak", true);
    bot.set_control_state("jump", false);
    let mut t = 0;
    while t < max_ticks {
        let p = bot.entity.position;
        let d = ((tx - p.x).powi(2) + (tz - p.z).powi(2)).sqrt();
        if d < tol {
            break;
        }
        // Axis-aligned legs, Z first then X: a sneaking bot refuses to step over an edge, so a
        // diagonal toward the stand that skirts an air cell (a platform block that failed to
        // place) never moves at all (3× `stance FAIL` at 1.2 blocks off). Straight legs stay on
        // the row the bot is already standing on.
        let (lx, lz) = if (tz - p.z).abs() > tol * 0.7 { (p.x, tz) } else { (tx, p.z) };
        bot.look_at(vec3(lx, p.y + 1.62, lz));
        // Far: 3-tick bursts (sneak speed needs a few ticks to build). Near: a single tick of
        // input then two ticks coasting — 3-tick bursts overshot a 0.2 target by ~0.4 every time
        // and oscillated until the tick budget ran out (all three bots, first cell).
        bot.set_control_state("forward", true);
        let burst = if d >= 0.6 { 3 } else { 2 };
        for _ in 0..burst {
            let _ = bot.drive_tick().await;
            t += 1;
        }
        if d < 0.6 {
            bot.set_control_state("forward", false);
            let _ = bot.drive_tick().await;
            t += 1;
        }
    }
    bot.set_control_state("forward", false);
}

/// Platform for a layer at height `y`: solid at (dx -1..=4, y, bz+1..=bz+2), air for 3 above.
/// goto_near that logs when it takes > 3 s (ensure_platform spent 325 s on layer 0 in solid rock
/// with no log lines — this names the slow targets).
async fn timed_goto(bot: &mut Bot<'_>, x: i32, y: i32, z: i32, r: f64, why: &str) {
    // These gotos only put the bot in REACH of a dig/place. Already within r+1 horizontally and a
    // block vertically → skip: the pathfinder spent 20 s on a ~1-block move that ended 1.6 from a
    // r=1.5 goal (`slow goto platform → (1347,-54,422) took 20376 ms`) and retried the near-miss.
    // Measured (04:47–05:25): 22 slow platform gotos, most `ok=false` after ~20 s with the bot left
    // 3–4 blocks from the target and ≤1 block off vertically — already within dig/place reach. So
    // skip when the target cell is within 4.0 of the EYE (reach is ~4.5), not just within r+1.
    let p = bot.entity.position;
    let hd = ((x as f64 + 0.5 - p.x).powi(2) + (z as f64 + 0.5 - p.z).powi(2)).sqrt();
    let eye_d = (hd.powi(2) + (y as f64 + 0.5 - (p.y + 1.62)).powi(2)).sqrt();
    if (hd <= r + 1.0 && (feet_y(bot) - y).abs() <= 1) || eye_d <= 4.0 {
        return;
    }
    // Reach, not arrival: the targets are platform cells the bot can't stand in, so a tight r had
    // the pathfinder walk to ~2 blocks and then retry for 5–9 s (gym-001 06:30: eight such gotos
    // ending 1.8–2.2 from an r=1.5/2 goal). r ≥ 3 stops as soon as the cell is comfortably in reach.
    let r = r.max(3.0);
    let t = Instant::now();
    let ok = capped_goto(bot, x, y, z, r).await;
    let ms = t.elapsed().as_millis();
    if ms > 3000 {
        let p = bot.entity.position;
        cast_debug(&format!("MOLD slow goto {why} → ({x},{y},{z}) r={r} took {ms} ms ok={ok} now=({:.1},{:.1},{:.1})", p.x, p.y, p.z));
    }
}

/// timed goto WITHOUT the in-reach skip — for gotos whose point is to MOVE the bot (down the stair,
/// onto a row, to a dig stand for a tall tower), not just to bring one cell into reach.
/// PAD: a solid floor under the frame plane and the two platform rows, dx -1..=5 (5 = stair
/// base). Lava cells are skipped — never place into the pool; the origin shift avoids it.
async fn fill_pad(bot: &mut Bot<'_>, (bx, by, bz): (i32, i32, i32)) {
    for dz in -1..=2 {
        for dx in -1..=5 {
            let f = (bx + dx, by - 1, bz + dz);
            if solid_at(bot, f.0, f.1, f.2) || is_lava(&name_at(bot, f.0, f.1, f.2)) {
                continue;
            }
            let _ = bot.goto_near(f.0, by, f.2 + 1, 2.5).await;
            ensure_solid(bot, f, 0).await;
        }
    }
}

/// `goto_near` capped at 8 s. A mold goto that hasn't arrived by then ends `ok=false` at the
/// pathfinder's ~20 s limit anyway, with the bot already near (batch 2: platform gotos of 21 s and
/// 28 s, all `ok=false`). Cancelling can leave movement keys held, so clear them.
async fn capped_goto(bot: &mut Bot<'_>, x: i32, y: i32, z: i32, r: f64) -> bool {
    let hp0 = bot.health;
    let ok = match tokio::time::timeout(Duration::from_secs(8), bot.goto_near(x, y, z, r)).await {
        Ok(r) => r.unwrap_or(false),
        Err(_) => {
            bot.clear_control_states();
            false
        }
    };
    // Hurt during a mold move: name the move. Batch 5 rust-gym-005 died between `layer 3: stair done`
    // and `platform done` (the return from a −54 station to the platform) with no line in between.
    if bot.health < hp0 {
        let p = bot.entity.position;
        cast_debug(&format!(
            "MOLD goto → ({x},{y},{z}) HURT hp {hp0:.0}→{:.0} now ({:.1},{:.1},{:.1}) below={} feet={}",
            bot.health, p.x, p.y, p.z,
            name_at(bot, p.x.floor() as i32, p.y.floor() as i32 - 1, p.z.floor() as i32),
            name_at(bot, p.x.floor() as i32, p.y.floor() as i32, p.z.floor() as i32)
        ));
    }
    ok
}

async fn timed_move(bot: &mut Bot<'_>, x: i32, y: i32, z: i32, r: f64, why: &str) {
    let t = Instant::now();
    let ok = capped_goto(bot, x, y, z, r).await;
    let ms = t.elapsed().as_millis();
    if ms > 3000 {
        let p = bot.entity.position;
        cast_debug(&format!("MOLD slow move {why} → ({x},{y},{z}) r={r} took {ms} ms ok={ok} now=({:.1},{:.1},{:.1})", p.x, p.y, p.z));
    }
}

async fn ensure_platform(bot: &mut Bot<'_>, bx: i32, y: i32, bz: i32) {
    // Two passes, never placing into the bot's own cell (no pillar_up: a placement the server
    // rejects because the player is inside it left a locally-predicted GHOST block the bot then
    // collided with — every first stance failed until the bot happened to move a row).
    //  1. From the previous level's back row (feet y, z=bz+2) place the FRONT row (z=bz+1) at y.
    //  2. Step up onto the new front row (feet y+1) and place the BACK row (z=bz+2) at y.
    let clear_head = |c: (i32, i32, i32)| -> Vec<(i32, i32, i32)> { (1..=3).map(|h| (c.0, c.1 + h, c.2)).collect() };
    timed_goto(bot, bx + 4, y, bz + 2, 1.5, "platform").await;
    for dx in (-1..=4).rev() {
        let c = (bx + dx, y, bz + 1);
        for hc in clear_head(c) {
            let n = name_at(bot, hc.0, hc.1, hc.2);
            if is_solid(&n) && n != "obsidian" {
                timed_goto(bot, c.0, y, bz + 2, 2.0, "platform").await;
                dig_at(bot, hc.0, hc.1, hc.2).await;
            }
        }
        if !solid_at(bot, c.0, c.1, c.2) {
            timed_goto(bot, c.0, y, bz + 2, 1.5, "platform").await;
            ensure_solid(bot, c, 0).await;
        }
    }
    // Step up onto the front row.
    timed_move(bot, bx + 4, y + 1, bz + 1, 1.0, "platform-stepup").await;
    // No jump-step. It used to be a 6-`drive_tick` forward+jump, which lasted milliseconds (packets,
    // not ticks) and so never moved the bot; the step-up above did the work. Made a real 6-tick jump
    // this cycle, it hopped the 2-wide platform's edge into the pool beside it (batch A rust-gym-002:
    // dead inside `ensure_platform`, `eat: health 0->0` right after `layer 4: platform done`). Tuned
    // behaviour that relied on the old timing is kept as it was: no jump.
    for dx in (-1..=4).rev() {
        let c = (bx + dx, y, bz + 2);
        for hc in clear_head(c) {
            let n = name_at(bot, hc.0, hc.1, hc.2);
            if is_solid(&n) && n != "obsidian" {
                timed_goto(bot, c.0, y + 1, bz + 1, 2.0, "platform").await;
                dig_at(bot, hc.0, hc.1, hc.2).await;
            }
        }
        if !solid_at(bot, c.0, c.1, c.2) {
            timed_goto(bot, c.0, y + 1, bz + 1, 1.5, "platform").await;
            ensure_solid(bot, c, 0).await;
        }
    }
}

/// East-side stair from the pad up to the current platform level: steps at (bx+5, by-1+k, bz+2+?)
/// — a straight 1-wide column of blocks at x=bx+5 rising with z fixed can't be climbed, so run the
/// steps along +X: step k at (bx+5+k, by-1+k, bz+2) with headroom.
async fn ensure_stair(bot: &mut Bot<'_>, bx: i32, by: i32, bz: i32, layers: i32) {
    for k in 0..=layers {
        let s = (bx + 5 + k, by - 1 + k, bz + 2);
        if !solid_at(bot, s.0, s.1, s.2) {
            let _ = bot.goto_near(s.0, s.1 + 1, s.2 + 1, 2.5).await;
            ensure_solid(bot, s, 0).await;
        }
        for h in 1..=2 {
            let n = name_at(bot, s.0, s.1 + h, s.2);
            if is_solid(&n) && n != "obsidian" {
                dig_at(bot, s.0, s.1 + h, s.2).await;
            }
        }
    }
}

// ── sealed refill station (docs/refill-design.md, revision 2026-10-02) ─────────────────────────

/// A station: stand feet cell S and the open direction d toward the pool.
type Station = ((i32, i32, i32), (i32, i32));

/// Per-frame station state, keyed by the frame anchor so a new trial/frame starts clean:
/// (anchor, current station, exhausted stands).
static STATION: std::sync::Mutex<Option<((i32, i32, i32), Option<Station>, Vec<(i32, i32, i32)>)>> = std::sync::Mutex::new(None);

const DIRS4: [(i32, i32); 4] = [(1, 0), (-1, 0), (0, 1), (0, -1)];

/// Lava sources a bot standing centred at S can scoop through the open cell O = S + d with nothing
/// dug: O's column at fy−1..fy−3, and the next column out (O + d) at fy−1. Only sources whose eye
/// ray is clear and within bucket reach (4.6).
fn station_targets(bot: &Bot, (s, (dx, dz)): Station) -> Vec<(i32, i32, i32)> {
    let (sx, fy, sz) = s;
    let (ox, oz) = (sx + dx, sz + dz);
    let eye = vec3(sx as f64 + 0.5, fy as f64 + 1.62, sz as f64 + 0.5);
    let mut cand: Vec<(i32, i32, i32)> = (1..=3).map(|k| (ox, fy - k, oz)).collect();
    cand.push((ox + dx, fy - 1, oz + dz));
    cand.into_iter()
        .filter(|&t| is_fluid_source(bot, t.0, t.1, t.2, "lava"))
        .filter(|&t| {
            let aim = vec3(t.0 as f64 + 0.5, t.1 as f64 + 0.6, t.2 as f64 + 0.5);
            let reach = ((aim.x - eye.x).powi(2) + (aim.y - eye.y).powi(2) + (aim.z - eye.z).powi(2)).sqrt();
            reach <= 4.6 && first_solid_on_ray(bot, eye, aim, t).is_none()
        })
        .collect()
}

/// Is S still a valid stand: solid non-lava floor that isn't gravel/sand, no lava under the floor,
/// air body, no lava in the body or touching it at feet/head height, and O open (air feet + head).
fn station_ok(bot: &Bot, (s, (dx, dz)): Station) -> bool {
    let (sx, fy, sz) = s;
    let floor = name_at(bot, sx, fy - 1, sz);
    let falls = floor.contains("gravel") || floor.contains("sand");
    let body_air = is_air(&name_at(bot, sx, fy, sz)) && is_air(&name_at(bot, sx, fy + 1, sz));
    let lava_touch = DIRS4.iter().chain([(0, 0)].iter()).any(|&(ex, ez)| {
        is_lava(&name_at(bot, sx + ex, fy, sz + ez)) || is_lava(&name_at(bot, sx + ex, fy + 1, sz + ez))
    });
    // O needs air at FEET height only: the eye ray to O's own column crosses O at ~fy+0.61. O's head
    // cell matters only for the O+d target, and `station_targets`' ray check covers that. Requiring both
    // rejected every station over a lake in a low cave (batch 5 rust-gym-002: 28 stands ok, 12 open
    // sources, `STATION none`, `MOLD end 0/10`).
    let open = is_air(&name_at(bot, sx + dx, fy, sz + dz));
    // Lava UNDER the floor is allowed: in a deep lake the lake runs under the rim, and requiring
    // rock below rejected every station (rust-gym-003, batch 1: the one open stand at (151163,−54,4378)
    // had a −56 source under its −55 floor). The station never digs, and gravel/sand floors are out.
    is_solid(&floor) && !is_lava(&floor) && !falls && body_air && !lava_touch && open
}

/// Choose a station around the frame: a valid stand outside the mold footprint (pad + rim dx −2..=6,
/// dz −2..=3, stair x up to bx+10) with the most targets, ties to the nearest to `near`.
/// The window is the frame's own: ±16 around the anchor, feet by−3..=by+2. Batch 1 intersected a
/// ±2 band around the old scoop stand with the frame band and excluded the one usable feet level
/// (rust-gym-005: stand y −54, anchor −57, lava surface −58 → `STATION none` ×3).
///
/// RANKING (build 5), measured from the BOT, lexicographic:
///  1. |station feet − bot feet|: the pathfinder will not make even a 2-down approach into a lake-rim
///     cell (batch 4: 6/6 walks `ok=false` from feet −52 to stations at −54). A station at the bot's
///     own level works whenever O's column is open down to the lava (targets reach fy−3).
///  2. whether the straight bot→station line crosses lava at the station's floor level: a station
///     across an open lake is "near" but unreachable (batch 3: 5× `ok=false` across a −55 lake).
///  3. horizontal distance, then 4. more targets.
/// `near` is unused for ranking now. It was the lake's own level during the early scoop, which
/// pulled every pick down to the rim.
fn pick_station(bot: &Bot, near: (i32, i32, i32), (bx, by, bz): (i32, i32, i32), exhausted: &[(i32, i32, i32)], footprint: bool) -> Option<Station> {
    let _ = near;
    let in_footprint = |x: i32, z: i32| footprint && (bx - 2..=bx + 10).contains(&x) && (bz - 2..=bz + 3).contains(&z);
    let bp = bot.entity.position;
    let (bpx, bfy, bpz) = (bp.x.floor() as i32, feet_y(bot), bp.z.floor() as i32);
    let crosses_lava = |(x, fy, z): (i32, i32, i32)| {
        (1..8).any(|k| {
            let f = k as f64 / 8.0;
            let (sx, sz) = ((bpx as f64 + (x - bpx) as f64 * f).floor() as i32, (bpz as f64 + (z - bpz) as f64 * f).floor() as i32);
            is_lava(&name_at(bot, sx, fy - 1, sz))
        })
    };
    let mut best: Option<((i32, i32, i32, i32), Station)> = None;
    for fy in by - 3..=by + 4 {
        for x in bx - 16..=bx + 16 {
            for z in bz - 16..=bz + 16 {
                if in_footprint(x, z) || exhausted.contains(&(x, fy, z)) {
                    continue;
                }
                for &d in DIRS4.iter() {
                    let st = ((x, fy, z), d);
                    if !station_ok(bot, st) {
                        continue;
                    }
                    let n = station_targets(bot, st).len();
                    if n == 0 {
                        continue;
                    }
                    let key = ((fy - bfy).abs(), crosses_lava((x, fy, z)) as i32, (x - bpx).abs() + (z - bpz).abs(), -(n as i32));
                    if best.as_ref().is_none_or(|(bk, _)| key < *bk) {
                        best = Some((key, st));
                    }
                }
            }
        }
    }
    best.map(|b| b.1)
}

/// Make the station safe with placements only (never a dig): cap lava beside S or O at feet/head
/// height, and fill any side cell of S that has no floor (a drop), except O itself.
async fn seal_station(bot: &mut Bot<'_>, (s, (dx, dz)): Station) {
    let (sx, fy, sz) = s;
    let (ox, oz) = (sx + dx, sz + dz);
    let mut cells: Vec<(i32, i32, i32)> = Vec::new();
    for &(ex, ez) in DIRS4.iter() {
        for y in [fy, fy + 1] {
            for (cx, cz) in [(sx + ex, sz + ez), (ox + ex, oz + ez)] {
                if (cx, cz) == (sx, sz) || (cx, cz) == (ox, oz) {
                    continue;
                }
                if is_lava(&name_at(bot, cx, y, cz)) {
                    cells.push((cx, y, cz));
                }
            }
        }
        let (cx, cz) = (sx + ex, sz + ez);
        if (cx, cz) != (ox, oz) && is_air(&name_at(bot, cx, fy, cz)) && !solid_at(bot, cx, fy - 1, cz) {
            cells.push((cx, fy - 1, cz));
        }
    }
    for c in cells {
        let ok = place_cobble(bot, c).await;
        cast_debug(&format!("STATION seal {c:?} → {ok}"));
    }
}

/// Bot centred on S? (feet cell matches and within 0.25 of the centre)
fn at_station(bot: &Bot, (sx, fy, sz): (i32, i32, i32)) -> bool {
    let p = bot.entity.position;
    feet_y(bot) == fy && (p.x - (sx as f64 + 0.5)).abs() <= 0.25 && (p.z - (sz as f64 + 0.5)).abs() <= 0.25
}

/// Refill from the sealed station until 2 lava buckets (or no empty bucket). Returns true when at
/// least one bucket was filled. Never improvises a stand beside the pool: with no valid station it
/// fails and the mold aborts the layer.
/// `want`: stop at this many lava buckets. `reserve`: always keep this many empty buckets.
/// `footprint`: exclude the mold's footprint around `anchor` (false for prepare's early scoop, before any frame exists).
pub(crate) async fn station_refill(bot: &mut Bot<'_>, anchor: (i32, i32, i32), lava_pool: Option<(i32, i32, i32)>, want: i32, reserve: i32, footprint: bool) -> bool {
    let before = count_items(bot, "lava_bucket");
    {
        let mut g = STATION.lock().unwrap();
        if g.as_ref().is_none_or(|(a, _, _)| *a != anchor) {
            *g = Some((anchor, None, Vec::new()));
        }
    }
    for resite in 0..3 {
        if count_items(bot, "lava_bucket") >= want || count_items(bot, "bucket") <= reserve {
            break;
        }
        // Current station, re-validated; otherwise pick one near the old scoop stand / pool / bot.
        let (cur, exhausted) = {
            let g = STATION.lock().unwrap();
            let (_, cur, ex) = g.as_ref().unwrap();
            (*cur, ex.clone())
        };
        let st = match cur.filter(|&st| station_ok(bot, st) && !station_targets(bot, st).is_empty()) {
            Some(st) => st,
            None => {
                if let Some((s, _)) = cur {
                    STATION.lock().unwrap().as_mut().unwrap().2.push(s);
                }
                let near = SAFE_SCOOP_STAND
                    .lock()
                    .unwrap()
                    .map(|p| (p.0.floor() as i32, p.1.floor() as i32, p.2.floor() as i32))
                    .or(lava_pool.map(|l| (l.0, l.1 + 1, l.2)))
                    .unwrap_or((bot.entity.position.x.floor() as i32, feet_y(bot), bot.entity.position.z.floor() as i32));
                let mut ex = exhausted.clone();
                if let Some((s, _)) = cur {
                    ex.push(s);
                }
                let Some(st) = pick_station(bot, near, anchor, &ex, footprint) else {
                    // Why none: how many cells pass each filter in the window (batch 1: a stand that
                    // looked valid on an RCON probe beside open −55 sources was not picked).
                    let (bx, by, bz) = anchor;
                    let (mut ok, mut lava_src, mut sample) = (0, 0, None);
                    for fy in by - 3..=by + 2 {
                        for x in bx - 16..=bx + 16 {
                            for z in bz - 16..=bz + 16 {
                                if is_fluid_source(bot, x, fy - 1, z, "lava") && is_air(&name_at(bot, x, fy, z)) {
                                    lava_src += 1;
                                }
                                for &d in DIRS4.iter() {
                                    if station_ok(bot, ((x, fy, z), d)) {
                                        ok += 1;
                                        if sample.is_none() && is_lava(&name_at(bot, x + d.0, fy - 1, z + d.1)) {
                                            sample = Some(((x, fy, z), d, name_at(bot, x + d.0, fy - 1, z + d.1)));
                                        }
                                    }
                                }
                            }
                        }
                    }
                    cast_debug(&format!(
                        "STATION none near {near:?} (exhausted {}) — refill fails, no improvised stand; window: {ok} stand/dir pairs pass station_ok, {lava_src} open sources one below an air cell, sample beside lava: {sample:?}",
                        ex.len()
                    ));
                    return count_items(bot, "lava_bucket") > before;
                };
                cast_debug(&format!("STATION pick {:?} open {:?} targets={} (resite {resite})", st.0, st.1, station_targets(bot, st).len()));
                STATION.lock().unwrap().as_mut().unwrap().1 = Some(st);
                st
            }
        };
        let (s, d) = st;
        // Walk in from the entrance side (S − d): pathfind next to it, then a short sneaking settle.
        if !at_station(bot, s) {
            // 30 s, digging allowed: a station across rock needs a dug route (batch 2: a station
            // ~9 rock rows away never got reached under the 8 s cap; the dig line has rock under it).
            bot.movement.blocks_cant_break.clear();
            let t0 = Instant::now();
            let reached = tokio::time::timeout(Duration::from_secs(30), bot.goto_near(s.0, s.1, s.2, 0.5)).await;
            // Ground truth for batch 2's `could not reach` at 2 blocks, same y: what did the
            // pathfinder actually return?
            let outcome = match &reached {
                Err(_) => "timeout 30 s".to_string(),
                Ok(Ok(b)) => format!("ok={b}"),
                Ok(Err(e)) => format!("err={e}"),
            };
            if reached.is_err() {
                bot.clear_control_states();
            }
            let p = bot.entity.position;
            cast_debug(&format!("STATION walk → {s:?}: {outcome} in {} ms, now ({:.1},{:.1},{:.1})", t0.elapsed().as_millis(), p.x, p.y, p.z));
            let p = bot.entity.position;
            let off = ((p.x - (s.0 as f64 + 0.5)).powi(2) + (p.z - (s.2 as f64 + 0.5)).powi(2)).sqrt();
            if feet_y(bot) == s.1 && off <= 1.2 {
                // 0.15 / 120: batch 1's 0.2 / 30 left a bot 0.5 off centre on the block edge.
                settle_xz(bot, s.0 as f64 + 0.5, s.2 as f64 + 0.5, 0.15, 120).await;
            }
            bot.set_control_state("sneak", false);
        }
        if !at_station(bot, s) {
            let p = bot.entity.position;
            cast_debug(&format!("STATION could not reach {s:?} (bot at {:.1},{:.1},{:.1}) — next station", p.x, p.y, p.z));
            STATION.lock().unwrap().as_mut().unwrap().1 = None;
            // Exclude the unreachable stand's whole 5×5 neighbourhood at its level, not just the cell.
            // Batch 5's 1655 s pass spent ~4.5 min on 12 walks of ~20 s, each to the next cell of one
            // unreachable rim patch (x 172149–172151, z 3870–3872).
            for ex in -2..=2 {
                for ez in -2..=2 {
                    STATION.lock().unwrap().as_mut().unwrap().2.push((s.0 + ex, s.1, s.2 + ez));
                }
            }
            continue;
        }
        seal_station(bot, st).await;
        // Scoop sneaking (no edge walk-off into O), targets nearest-first, never digging.
        bot.set_control_state("sneak", true);
        for t in station_targets(bot, st) {
            if count_items(bot, "lava_bucket") >= want || count_items(bot, "bucket") <= reserve {
                break;
            }
            if bot.health <= 0.0 {
                bot.set_control_state("sneak", false);
                respawn_at_frame(bot).await;
                return count_items(bot, "lava_bucket") > before;
            }
            let p = bot.entity.position;
            let fy = feet_y(bot);
            let footing_lava = is_lava(&name_at(bot, p.x.floor() as i32, fy - 1, p.z.floor() as i32))
                || is_lava(&name_at(bot, p.x.floor() as i32, fy, p.z.floor() as i32));
            if bot.health < 14.0 || footing_lava || !at_station(bot, s) {
                cast_debug(&format!("STATION abort hp={:.0} footing_lava={footing_lava} at ({:.1},{:.1},{:.1}) — stepping back out", bot.health, p.x, p.y, p.z));
                bot.set_control_state("sneak", false);
                walk_to_xz(bot, (s.0 - d.0) as f64 + 0.5, (s.2 - d.1) as f64 + 0.5, 0.3, 20).await;
                eat_if_hurt(bot).await;
                return count_items(bot, "lava_bucket") > before;
            }
            let n0 = count_items(bot, "lava_bucket");
            for dy in [0.6_f64, 0.9] {
                if !select_item(bot, "bucket").await.unwrap_or(false) {
                    break;
                }
                let look = vec3(t.0 as f64 + 0.5, t.1 as f64 + dy, t.2 as f64 + 0.5);
                bot.look_at(look);
                bot.wait_ticks(7).await.ok();
                bot.look_at(look);
                bot.activate_item().await.ok();
                bot.wait_ticks(10).await.ok();
                if count_items(bot, "lava_bucket") > n0 {
                    break;
                }
                // The server took the source but the inventory update lagged: the target is gone
                // locally right after the use. Mirror it, as fill_bucket does (batch 1: 4 such scoops
                // read `lava_b=0` and two good stations were written off as exhausted).
                if !is_fluid_source(bot, t.0, t.1, t.2, "lava") && count_items(bot, "bucket") > 0 {
                    bot.ensure_item("lava_bucket", 1);
                    if let Some(b) = bot.inventory.slots.iter_mut().flatten().find(|i| i.name == "bucket") {
                        b.count -= 1;
                    }
                    cast_debug(&format!("STATION scoop {t:?}: source vanished — predicted lava bucket"));
                    break;
                }
            }
            cast_debug(&format!("STATION scoop {t:?} → lava_b={} (was {n0})", count_items(bot, "lava_bucket")));
        }
        bot.set_control_state("sneak", false);
        if station_targets(bot, st).is_empty() {
            cast_debug(&format!("STATION {s:?} exhausted"));
            STATION.lock().unwrap().as_mut().unwrap().1 = None;
            STATION.lock().unwrap().as_mut().unwrap().2.push(s);
        }
    }
    count_items(bot, "lava_bucket") > before
}

/// Refill lava buckets at the pool station (the last safe scoop stand), then come back to the pad.
async fn refill_lava(bot: &mut Bot<'_>, lava_pool: Option<(i32, i32, i32)>, anchor: (i32, i32, i32)) {
    // Sealed station (cycle 3): every refill death of cycle 3 happened re-planning a stand beside a pool the
    // scoops were reshaping. The legacy stand/fill_bucket path is deleted (cycle 6, decision 9).
    let ok = station_refill(bot, anchor, lava_pool, 2, 0, true).await;
    cast_debug(&format!("MOLD refill (station) ok={ok} → lava_b={} bucket={}", count_items(bot, "lava_bucket"), count_items(bot, "bucket")));
}

fn lava_near(bot: &Bot, (bx, by, bz): (i32, i32, i32)) -> Vec<(i32, i32, i32)> {
    let mut v = Vec::new();
    for dx in -1..=5 {
        for dy in -1..=7 {
            for dz in -1..=2 {
                if is_lava(&name_at(bot, bx + dx, by + dy, bz + dz)) {
                    v.push((bx + dx, by + dy, bz + dz));
                }
            }
        }
    }
    v
}

fn water_near(bot: &Bot, (bx, by, bz): (i32, i32, i32)) -> Vec<(i32, i32, i32)> {
    let mut v = Vec::new();
    for dx in -1..=5 {
        for dy in -1..=7 {
            for dz in -1..=2 {
                if name_at(bot, bx + dx, by + dy, bz + dz).contains("water") {
                    v.push((bx + dx, by + dy, bz + dz));
                }
            }
        }
    }
    v
}

/// Scoop every water block in the mold volume until none remains or the bucket is full.
async fn scoop_water_near(bot: &mut Bot<'_>, origin: (i32, i32, i32)) {
    for _ in 0..3 {
        let ws = water_near(bot, origin);
        if ws.is_empty() || count_items(bot, "bucket") < 1 {
            break;
        }
        if !select_item(bot, "bucket").await.unwrap_or(false) {
            break;
        }
        for w in ws {
            reliable_use(bot, vec3(w.0 as f64 + 0.5, w.1 as f64 + 0.5, w.2 as f64 + 0.5)).await;
        }
        bot.wait_ticks(6).await.ok();
    }
}
