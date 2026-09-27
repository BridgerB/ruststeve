//! Speedrun step machine — ordered steps, each gated by `can_execute` and
//! satisfied by `is_complete`; `execute_step` dispatches by id. Port of steve's
//! `steps.ts` (early phases).

use crate::bot::Bot;

use crate::memory::WorldMemory;
use crate::tasks;
use crate::types::{failure, GameState, Step, StepResult};

pub const STEPS: &[Step] = &[
    Step {
        id: "gather_wood",
        name: "Gather Wood",
        priority: 1,
        can_execute: |s| s.world.in_overworld() && s.alive,
        // Keep a WOOD RESERVE. A full spawn→nether run spends ~11 planks (table + wooden
        // pick + all the sticks); with only a 12-plank target a container desync that eats
        // planks (or a re-placed table) dropped the bot to 0 wood mid-run and the recovery
        // above sent it back to Gather Wood (~15 min lost, seen in the race). A slightly
        // bigger buffer avoids that round trip; ~1 extra log up front is far cheaper.
        is_complete: |s| s.inventory.logs >= 6 || s.inventory.planks >= 16,
    },
    Step {
        id: "craft_planks",
        name: "Craft Planks",
        priority: 2,
        can_execute: |s| s.inventory.logs >= 2,
        is_complete: |s| s.inventory.planks >= 8,
    },
    Step {
        id: "craft_crafting_table",
        name: "Craft Crafting Table",
        priority: 3,
        can_execute: |s| s.inventory.planks >= 4,
        is_complete: |s| s.equipment.has_crafting_table,
    },
    Step {
        id: "craft_sticks",
        name: "Craft Sticks",
        priority: 4,
        can_execute: |s| s.inventory.planks >= 2,
        is_complete: |s| s.inventory.sticks >= 4,
    },
    Step {
        id: "craft_wooden_pickaxe",
        name: "Craft Wooden Pickaxe",
        priority: 5,
        can_execute: |s| s.inventory.planks >= 3 && s.inventory.sticks >= 2,
        is_complete: |s| s.equipment.pickaxe_tier().rank() >= 1,
    },
    Step {
        id: "mine_stone",
        name: "Mine Cobblestone",
        priority: 6,
        can_execute: |s| s.equipment.pickaxe_tier().rank() >= 1,
        is_complete: |s| s.inventory.cobblestone >= 16,
    },
    Step {
        id: "craft_stone_pickaxe",
        name: "Craft Stone Pickaxe",
        priority: 7,
        can_execute: |s| s.inventory.cobblestone >= 3 && s.inventory.sticks >= 2,
        is_complete: |s| s.equipment.pickaxe_tier().rank() >= 2,
    },
    // === STONE PHASE ===
    Step {
        id: "craft_stone_sword",
        name: "Craft Stone Sword",
        priority: 8,
        // Stone-phase steps gate on already having the stone pickaxe so the
        // furthest-along-step picker can't jump to them (or the iron phase)
        // before the cobblestone→stone-pickaxe chain is finished.
        can_execute: |s| s.equipment.pickaxe_tier().rank() >= 2 && s.inventory.cobblestone >= 2 && s.inventory.sticks >= 1,
        is_complete: |s| s.equipment.sword.map(|t| t.rank()).unwrap_or(0) >= 1,
    },
    Step {
        id: "craft_furnace",
        name: "Craft Furnace",
        priority: 9,
        can_execute: |s| s.equipment.pickaxe_tier().rank() >= 2 && s.inventory.cobblestone >= 8,
        is_complete: |s| s.equipment.has_furnace,
    },
    // === IRON PHASE ===
    Step {
        id: "mine_coal",
        name: "Mine Coal",
        priority: 10,
        can_execute: |s| s.equipment.pickaxe_tier().rank() >= 2,
        // The race goal is the IRON PICKAXE (3 ingots). Smelting 4 iron needs <1 coal
        // as fuel, so only a small amount is needed — keep it low so the bot doesn't
        // spend long underground (death/pickaxe-break risk) mining coal it won't use.
        // 2, not 3: 1 coal smelts 8 items, so 2 coal already smelts all ~13 raw iron
        // (buckets + iron pick + flint&steel). Requiring a 3rd stalled a race bot 30+ min
        // strip-searching coal-poor terrain for a coal it never needed.
        is_complete: |s| s.inventory.coal >= 2,
    },
    Step {
        id: "mine_iron",
        name: "Mine Iron Ore",
        priority: 11,
        can_execute: |s| s.equipment.pickaxe_tier().rank() >= 2,
        // NETHER goal needs a lot of iron: pickaxe(3) + 3 buckets(9) + flint&steel(1)
        // = 13, +2 buffer. The 3rd bucket is load-bearing: on natural terrain the bot
        // deep-descends to a lava lake and scoops early (empty→lava), so it must reach
        // the cast still holding 1 EMPTY + 1 WATER bucket — 2 total (the old target) left
        // it with 0 empty after the scoop and the cast bailed "need a water/empty bucket".
        is_complete: |s| s.inventory.iron_ore + s.inventory.iron_ingots >= 15,
    },
    Step {
        id: "smelt_iron",
        name: "Smelt Iron",
        priority: 12,
        can_execute: |s| s.equipment.has_furnace && s.inventory.coal >= 1 && s.inventory.iron_ore >= 1,
        is_complete: |s| s.inventory.iron_ingots >= 14,
    },
    Step {
        id: "craft_iron_pickaxe",
        name: "Craft Iron Pickaxe",
        priority: 13,
        can_execute: |s| s.inventory.iron_ingots >= 3 && s.inventory.sticks >= 2,
        is_complete: |s| s.equipment.pickaxe_tier().rank() >= 3,
    },
    // === NETHER PREP === (all gated behind the iron pickaxe so the bot finishes
    // the iron-pickaxe chain FIRST — otherwise the furthest-step picker jumps to
    // these and never crafts the pickaxe it already has the iron for).
    // Portal needs 3 buckets total (matches the passing gym prereq): one stays WATER
    // (poured over the lava cup), and TWO stay EMPTY — the natural-terrain cast scoops
    // lava early during the deep descent (empty→lava), so it must still hold 1 empty +
    // 1 water when it reaches the frame. 2 buckets left it with 0 empty after the scoop.
    Step {
        id: "craft_bucket",
        name: "Craft Buckets",
        priority: 14,
        can_execute: |s| s.equipment.pickaxe_tier().rank() >= 3 && s.inventory.iron_ingots >= 3,
        is_complete: |s| s.inventory.buckets + s.inventory.water_buckets >= 3,
    },
    Step {
        id: "get_water_buckets",
        name: "Fill Water Buckets",
        priority: 15,
        // Fill ONE bucket with water. Completion is ONLY water>=1 — the fill task consumes
        // an empty bucket, so requiring buckets>=2 here deadlocked (filling water can never
        // RAISE the empty count, so a bot with 1 empty+1 water re-ran this forever). The
        // 2-empty requirement lives in craft_bucket (sum>=3) + the portal gate (buckets>=2).
        can_execute: |s| s.equipment.pickaxe_tier().rank() >= 3 && s.inventory.buckets >= 1,
        is_complete: |s| s.inventory.water_buckets >= 1,
    },
    Step {
        id: "get_flint_and_steel",
        name: "Get Flint and Steel",
        priority: 16,
        can_execute: |s| s.equipment.pickaxe_tier().rank() >= 3 && s.inventory.iron_ingots >= 1,
        is_complete: |s| s.inventory.flint_and_steel >= 1,
    },
    // Casting 10 obsidian needs a big stack of throwaway scaffold/mould blocks
    // (cups, pillars). ~40 gives margin over the ~30 the cast consumes. Count dirt
    // alongside cobblestone (which already folds in cobbled_deepslate): the cast's
    // build_block/scaffold_count accept cobble/deepslate/dirt interchangeably, so
    // demanding 40 *cobblestone* specifically wedged the bot in stone-poor terrain
    // (gravel/diorite/dripstone) — it mined forever, stuck at 12/40 cobble while
    // holding 36 dirt (48 usable scaffold, more than the cast needs).
    Step {
        id: "gather_build_blocks",
        name: "Gather Build Blocks",
        priority: 17,
        can_execute: |s| s.equipment.pickaxe_tier().rank() >= 3,
        is_complete: |s| s.inventory.cobblestone + s.inventory.dirt >= 40,
    },
    Step {
        id: "build_nether_portal",
        name: "Build Nether Portal",
        priority: 18,
        can_execute: |s| {
            s.world.in_overworld()
                && s.inventory.water_buckets >= 1
                && s.inventory.buckets >= 2
                && s.inventory.flint_and_steel >= 1
                && s.inventory.cobblestone >= 30
        },
        is_complete: |s| s.world.portal_built,
    },
    Step {
        id: "enter_nether",
        name: "Enter Nether",
        priority: 19,
        can_execute: |s| s.world.portal_built && s.world.in_overworld(),
        is_complete: |s| s.world.in_nether(),
    },
    // ── post-nether: fortress → blaze spawner → blaze rod ──────────────────────
    Step {
        id: "find_fortress",
        name: "Find Nether Fortress",
        priority: 20,
        can_execute: |s| s.world.in_nether() || std::env::var("ARENA_TEST").is_ok(),
        is_complete: |s| s.world.fortress_found,
    },
    Step {
        id: "kill_blaze",
        name: "Kill a Blaze",
        priority: 21,
        // Need a real sword (equipment.sword is always Some(..); gate on the tier rank).
        can_execute: |s| s.world.fortress_found && s.equipment.sword_tier().rank() >= 1,
        is_complete: |s| s.inventory.blaze_rods >= 1,
    },
];

/// The FURTHEST-along step that can run and isn't complete. Picking the last
/// (not first) runnable step keeps the bot driving toward the goal: once it has
/// the materials to craft the pickaxe it does that, instead of re-running an
/// earlier gather/craft step that looks "incomplete" only because a later step
/// consumed its (consumable) output — which otherwise loops forever.
pub fn get_next_step(state: &GameState) -> Option<&'static Step> {
    // The pickaxe/wood recovery guards below are OVERWORLD tool-phase logic — a bot in the
    // nether (post-nether combat phase) has a combat kit with no pickaxe/wood on purpose, and
    // must NOT be dragged back to "gather wood"/"craft pickaxe" (there are no trees in the
    // nether — it'd wander forever). Skip both recoveries once in the nether; the normal
    // furthest-runnable picker then advances find_fortress → kill_blaze.
    if state.world.in_overworld() && std::env::var("ARENA_TEST").is_err() {
    // PICKAXE RECOVERY (highest priority): if we have no pickaxe but the materials
    // to make one, go back UP the chain and re-craft it before anything else.
    // Never proceed to mine by hand — only WOOD is gathered by hand. A bot whose
    // pickaxe just broke re-arms (best tier it can afford) and continues.
    if state.equipment.pickaxe_tier().rank() == 0 {
        let sticks = state.inventory.sticks;
        let recover = if state.inventory.iron_ingots >= 3 && sticks >= 2 {
            Some("craft_iron_pickaxe")
        } else if state.inventory.cobblestone >= 3 && sticks >= 2 {
            Some("craft_stone_pickaxe")
        } else if state.inventory.planks >= 3 && sticks >= 2 {
            Some("craft_wooden_pickaxe")
        } else {
            None // lack materials — fall through (gather wood/planks/sticks first)
        };
        if let Some(id) = recover {
            if let Some(step) = STEPS.iter().find(|st| st.id == id) {
                return Some(step);
            }
        }
    }
    // WOOD RECOVERY: completely out of wood (no logs, no planks, no table item) while
    // still in the tool phase (no iron pickaxe yet) means the bot can't craft the table
    // its next step needs — the furthest-step picker would otherwise stick on that
    // table-craft and loop "need a crafting table" forever (a lost bot per race). Go
    // gather wood first; harmless if a placed table is nearby (the bot needs wood for
    // sticks anyway), and it self-clears the moment logs come in.
    if state.inventory.logs == 0
        && state.inventory.planks == 0
        && state.inventory.crafting_tables == 0
        && state.equipment.pickaxe_tier().rank() < 3
    {
        if let Some(step) = STEPS.iter().find(|st| st.id == "gather_wood") {
            return Some(step);
        }
    }
    } // end overworld-only recovery guards
    STEPS.iter().filter(|s| (s.can_execute)(state) && !(s.is_complete)(state)).next_back()
}

/// How many steps are complete (progress reporting).
pub fn progress(state: &GameState) -> (usize, usize) {
    let done = STEPS.iter().filter(|s| (s.is_complete)(state)).count();
    (done, STEPS.len())
}

pub async fn execute_step(bot: &mut Bot<'_>, id: &str, mem: &mut WorldMemory) -> StepResult {
    match id {
        "gather_wood" => tasks::gather_wood::gather_wood(bot, 6, mem).await,
        "craft_planks" => tasks::craft::craft_planks(bot, mem).await,
        "craft_crafting_table" => tasks::craft::craft_crafting_table(bot, mem).await,
        "craft_sticks" => tasks::craft::craft_sticks(bot, mem).await,
        "craft_wooden_pickaxe" => tasks::craft::craft_wooden_pickaxe(bot, mem).await,
        "mine_stone" => tasks::mining::mine_stone(bot, 16, mem).await,
        "craft_stone_pickaxe" => tasks::craft::craft_stone_pickaxe(bot, mem).await,
        "craft_stone_sword" => tasks::craft::craft_stone_sword(bot, mem).await,
        "craft_furnace" => tasks::craft::craft_furnace(bot, mem).await,
        "mine_coal" => tasks::mining::mine_ore(bot, "coal", 2, mem).await,
        // Keep this in lock-step with mine_iron's is_complete target (15): if the task
        // mines fewer than is_complete wants, the step never completes and re-runs forever.
        "mine_iron" => tasks::mining::mine_ore(bot, "iron", 15, mem).await,
        // Target 14 must match smelt_iron's is_complete (14) — a lower task target would
        // "succeed" below the step threshold and re-run forever.
        "smelt_iron" => tasks::smelt::smelt_iron(bot, 14, mem).await,
        "craft_iron_pickaxe" => tasks::craft::craft_iron_pickaxe(bot, mem).await,
        // Count 3 must match craft_bucket's is_complete (buckets+water>=3) — a lower count
        // stops the task below the step threshold and re-runs "have N buckets" forever.
        "craft_bucket" => tasks::craft::craft_buckets(bot, 3, mem).await,
        // Fill ONE water bucket (keep the second bucket empty for lava).
        "get_water_buckets" => tasks::bucket::fill_water_buckets(bot, 1, mem).await,
        "get_flint_and_steel" => tasks::craft::get_flint_and_steel(bot, mem).await,
        "gather_build_blocks" => tasks::mining::mine_stone(bot, 40, mem).await,
        "build_nether_portal" => tasks::portal::build_nether_portal(bot, mem).await,
        "enter_nether" => tasks::portal::enter_nether(bot).await,
        "find_fortress" => tasks::nether::find_fortress(bot, mem).await,
        "kill_blaze" => tasks::nether::kill_blaze(bot, mem, 1).await,
        other => failure(format!("no executor for step {other}")),
    }
}
