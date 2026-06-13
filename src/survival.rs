//! Survival priority hierarchy — reflexes that run BEFORE any goal/task each
//! cycle, highest-danger first. This is the layer above the speedrun steps: no
//! matter what task we'd like to do, staying alive comes first.
//!
//! `handle_survival` returns true if a reflex fired, so the caller skips its
//! normal task this cycle and re-evaluates (the world just changed).
//!
//! Order = priority. Add new reflexes in the right slot; the first one whose
//! condition holds wins and the rest don't run this cycle.

use rustcraft::bot::{Bot, DriveStep};

use crate::bot_utils::{head_in_water, leave_water};
use crate::memory::WorldMemory;

/// Is the bot standing in / submerged in lava?
fn in_lava(bot: &Bot) -> bool {
    let p = bot.entity.position;
    let (x, z) = (p.x.floor() as i32, p.z.floor() as i32);
    let feet = p.y.floor() as i32;
    let head = (p.y + 1.0).floor() as i32;
    [feet, head]
        .iter()
        .any(|&y| bot.block_at(x, y, z).map(|b| b.name.contains("lava")).unwrap_or(false))
}

/// Thrash up and out of lava (swim up + drive forward) — best-effort, every
/// tick counts when you're burning.
async fn escape_lava(bot: &mut Bot<'_>, ticks: u32) {
    for _ in 0..ticks {
        if !in_lava(bot) && bot.entity.on_ground {
            break;
        }
        bot.set_control_state("jump", true); // rise
        bot.set_control_state("forward", true); // drift to an edge
        bot.set_control_state("sprint", true);
        if bot.drive_tick().await.map(|s| matches!(s, DriveStep::Disconnected)).unwrap_or(true) {
            break;
        }
    }
    bot.clear_control_states();
}

/// Run the survival reflexes in priority order. Returns true if one fired.
pub async fn handle_survival(bot: &mut Bot<'_>, mem: &mut WorldMemory) -> bool {
    // 1. In lava — get out NOW. Most immediately lethal.
    if in_lava(bot) {
        println!("    !! in lava — escaping");
        mem.log("survival", "lava", "escaping");
        escape_lava(bot, 60).await;
        return true;
    }

    // 2. Head underwater — surface before we drown.
    if head_in_water(bot) {
        println!("    !! underwater — surfacing");
        mem.log("survival", "water", "surfacing");
        leave_water(bot, 80).await;
        return true;
    }

    false
}
