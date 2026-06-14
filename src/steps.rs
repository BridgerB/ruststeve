//! Speedrun step machine — ordered steps, each gated by `can_execute` and
//! satisfied by `is_complete`; `execute_step` dispatches by id. Port of steve's
//! `steps.ts` (early phases).

use rustcraft::bot::Bot;

use crate::memory::WorldMemory;
use crate::tasks;
use crate::types::{failure, GameState, Step, StepResult};

pub const STEPS: &[Step] = &[
    Step {
        id: "gather_wood",
        name: "Gather Wood",
        priority: 1,
        can_execute: |s| s.world.in_overworld() && s.alive,
        is_complete: |s| s.inventory.logs >= 5 || s.inventory.planks >= 12,
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
        is_complete: |s| s.inventory.coal >= 3,
    },
    Step {
        id: "mine_iron",
        name: "Mine Iron Ore",
        priority: 11,
        can_execute: |s| s.equipment.pickaxe_tier().rank() >= 2,
        // NETHER goal needs a lot of iron: pickaxe(3) + 2 buckets(6) + flint&steel(1)
        // = 10, +1 buffer. Mine it in one trip rather than re-descending repeatedly.
        is_complete: |s| s.inventory.iron_ore + s.inventory.iron_ingots >= 11,
    },
    Step {
        id: "smelt_iron",
        name: "Smelt Iron",
        priority: 12,
        can_execute: |s| s.equipment.has_furnace && s.inventory.coal >= 1 && s.inventory.iron_ore >= 1,
        is_complete: |s| s.inventory.iron_ingots >= 11,
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
    // Portal needs 2 buckets total: one stays WATER (to pour over the lava), one
    // stays EMPTY (the bot fills it with lava from the pool, refilling each cast).
    Step {
        id: "craft_bucket",
        name: "Craft Buckets",
        priority: 14,
        can_execute: |s| s.equipment.pickaxe_tier().rank() >= 3 && s.inventory.iron_ingots >= 3,
        is_complete: |s| s.inventory.buckets + s.inventory.water_buckets >= 2,
    },
    Step {
        id: "get_water_buckets",
        name: "Fill Water Buckets",
        priority: 15,
        // Fill ONE bucket with water — leave the other empty for lava.
        can_execute: |s| s.equipment.pickaxe_tier().rank() >= 3 && s.inventory.buckets >= 1,
        is_complete: |s| s.inventory.water_buckets >= 1 && s.inventory.buckets >= 1,
    },
    Step {
        id: "get_flint_and_steel",
        name: "Get Flint and Steel",
        priority: 16,
        can_execute: |s| s.equipment.pickaxe_tier().rank() >= 3 && s.inventory.iron_ingots >= 1,
        is_complete: |s| s.inventory.flint_and_steel >= 1,
    },
    // Casting 10 obsidian needs a big stack of throwaway scaffold/mould blocks
    // (cups, pillars). ~40 cobble gives margin over the ~30 the cast consumes.
    Step {
        id: "gather_build_blocks",
        name: "Gather Build Blocks",
        priority: 17,
        can_execute: |s| s.equipment.pickaxe_tier().rank() >= 3,
        is_complete: |s| s.inventory.cobblestone >= 40,
    },
    Step {
        id: "build_nether_portal",
        name: "Build Nether Portal",
        priority: 18,
        can_execute: |s| {
            s.world.in_overworld()
                && s.inventory.water_buckets >= 1
                && s.inventory.buckets >= 1
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
];

/// The FURTHEST-along step that can run and isn't complete. Picking the last
/// (not first) runnable step keeps the bot driving toward the goal: once it has
/// the materials to craft the pickaxe it does that, instead of re-running an
/// earlier gather/craft step that looks "incomplete" only because a later step
/// consumed its (consumable) output — which otherwise loops forever.
pub fn get_next_step(state: &GameState) -> Option<&'static Step> {
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
    STEPS.iter().filter(|s| (s.can_execute)(state) && !(s.is_complete)(state)).next_back()
}

/// How many steps are complete (progress reporting).
pub fn progress(state: &GameState) -> (usize, usize) {
    let done = STEPS.iter().filter(|s| (s.is_complete)(state)).count();
    (done, STEPS.len())
}

pub async fn execute_step(bot: &mut Bot<'_>, id: &str, mem: &mut WorldMemory) -> StepResult {
    match id {
        "gather_wood" => tasks::gather_wood::gather_wood(bot, 5, mem).await,
        "craft_planks" => tasks::craft::craft_planks(bot, mem).await,
        "craft_crafting_table" => tasks::craft::craft_crafting_table(bot).await,
        "craft_sticks" => tasks::craft::craft_sticks(bot, mem).await,
        "craft_wooden_pickaxe" => tasks::craft::craft_wooden_pickaxe(bot, mem).await,
        "mine_stone" => tasks::mining::mine_stone(bot, 16, mem).await,
        "craft_stone_pickaxe" => tasks::craft::craft_stone_pickaxe(bot, mem).await,
        "craft_stone_sword" => tasks::craft::craft_stone_sword(bot, mem).await,
        "craft_furnace" => tasks::craft::craft_furnace(bot, mem).await,
        "mine_coal" => tasks::mining::mine_ore(bot, "coal", 3, mem).await,
        "mine_iron" => tasks::mining::mine_ore(bot, "iron", 11, mem).await,
        "smelt_iron" => tasks::smelt::smelt_iron(bot, 11).await,
        "craft_iron_pickaxe" => tasks::craft::craft_iron_pickaxe(bot, mem).await,
        "craft_bucket" => tasks::craft::craft_buckets(bot, 2, mem).await,
        // Fill ONE water bucket (keep the second bucket empty for lava).
        "get_water_buckets" => tasks::bucket::fill_water_buckets(bot, 1, mem).await,
        "get_flint_and_steel" => tasks::craft::get_flint_and_steel(bot, mem).await,
        "gather_build_blocks" => tasks::mining::mine_stone(bot, 40, mem).await,
        "build_nether_portal" => tasks::portal::build_nether_portal(bot, mem).await,
        "enter_nether" => tasks::portal::enter_nether(bot).await,
        other => failure(format!("no executor for step {other}")),
    }
}
