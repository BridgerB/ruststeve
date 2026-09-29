//! Derive [`GameState`] from a live bot — count inventory items, detect
//! equipment tiers, read world/vitals. Port of steve's `state.ts`.

use crate::bot::Bot;

use crate::types::{Equipment, GameState, Inventory, Tier, WorldState};

/// Tier of a tool's MATERIAL prefix (the part before `_pickaxe`/`_sword`). Pickaxes
/// and swords share this exact mapping, so both tier fns just strip their suffix.
fn material_tier(material: &str) -> Tier {
    match material {
        "wooden" => Tier::Wood,
        "stone" => Tier::Stone,
        "iron" | "golden" => Tier::Iron,
        "diamond" | "netherite" => Tier::Diamond,
        _ => Tier::None,
    }
}

/// Pickaxe tier from an item name.
pub fn pickaxe_tier(name: &str) -> Tier {
    name.strip_suffix("_pickaxe").map_or(Tier::None, material_tier)
}

fn sword_tier(name: &str) -> Tier {
    name.strip_suffix("_sword").map_or(Tier::None, material_tier)
}

const FOODS: &[&str] = &[
    "bread", "apple", "cooked_beef", "cooked_porkchop", "cooked_chicken", "cooked_mutton",
    "cooked_cod", "cooked_salmon", "baked_potato", "carrot", "melon_slice", "golden_apple",
    "golden_carrot", "beef", "porkchop", "chicken", "mutton",
];

pub fn sync_from_bot(bot: &Bot) -> GameState {
    let mut inv = Inventory::default();
    let mut pickaxe = Tier::None;
    let mut sword = Tier::None;

    for item in bot.inventory.slots.iter().flatten() {
        if item.count <= 0 {
            continue;
        }
        let n = item.name.as_str();
        let c = item.count;
        if n.ends_with("_log") {
            inv.logs += c;
        } else if n.ends_with("_planks") {
            inv.planks += c;
        } else if n == "stick" {
            inv.sticks += c;
        } else if n == "cobblestone" || n == "cobbled_deepslate" {
            inv.cobblestone += c;
        } else if n == "dirt" {
            inv.dirt += c;
        } else if n == "coal" || n == "charcoal" {
            inv.coal += c;
        } else if n == "raw_iron" || n == "iron_ore" || n == "deepslate_iron_ore" {
            inv.iron_ore += c;
        } else if n == "iron_ingot" {
            inv.iron_ingots += c;
        } else if n == "diamond" {
            inv.diamonds += c;
        } else if n == "crafting_table" {
            inv.crafting_tables += c;
        } else if n == "bucket" {
            inv.buckets += c;
        } else if n == "water_bucket" {
            inv.water_buckets += c;
        } else if n == "flint" {
            inv.flint += c;
        } else if n == "flint_and_steel" {
            inv.flint_and_steel += c;
        } else if n == "blaze_rod" {
            inv.blaze_rods += c;
        }
        if FOODS.contains(&n) {
            inv.food += c;
        }
        let pt = pickaxe_tier(n);
        if pt.rank() > pickaxe.rank() {
            pickaxe = pt;
        }
        let st = sword_tier(n);
        if st.rank() > sword.rank() {
            sword = st;
        }
    }

    let equipment = Equipment {
        pickaxe: Some(pickaxe),
        sword: Some(sword),
        has_crafting_table: inv.crafting_tables > 0,
        has_furnace: bot.inventory.slots.iter().flatten().any(|i| i.name == "furnace"),
    };

    let p = bot.entity.position;
    // A lit portal exists if a nether_portal block is CLOSE (the cast-and-light step just ran —
    // the bot stands adjacent to the frame it lit). Radius 8, not 32: a 32-block radius detected
    // OTHER projects' portals (steve races here too) and made a fresh bot think it was already
    // done → it skipped the whole pipeline to enter_nether, which then couldn't reach that distant
    // portal → the bot looped the final step forever. 8 blocks ≈ only the bot's own just-lit portal.
    let portal_built = !bot.find_exposed_blocks("nether_portal", 8, 1).is_empty();
    // Fortress reached = nether brick visible nearby (a fortress is almost entirely nether
    // brick). Only scan in the nether — no bricks exist overworld, and the 64-radius scan
    // isn't free. Stays true while the bot fights at the fortress; flips off only if it
    // wanders far away (fine — kill_blaze keeps it there).
    // ARENA_TEST: validate the blaze-combat pipeline in the OVERWORLD (a nether_brick arena),
    // where world chunks load reliably — the cross-dimension command-tp into the nether streams
    // chunks too slowly (bot's world stays empty ~20s+), which is a separate SDK issue.
    let in_nether = bot.game.dimension.contains("nether") || std::env::var("ARENA_TEST").is_ok();
    // Fortress reached = nether brick nearby. Use a direct block_at scan, NOT find_block —
    // find_block needs line-of-sight to an EXPOSED block (anti-X-ray) and returns None for the
    // floor the bot stands on (confirmed live). A small radius each tick is plenty + cheap.
    let fortress_found = in_nether && {
        let p = bot.entity.position;
        let (ox, oy, oz) = (p.x.floor() as i32, p.y.floor() as i32, p.z.floor() as i32);
        let mut near = false;
        'scan: for dx in -5..=5 {
            for dy in -3..=3 {
                for dz in -5..=5 {
                    if bot.block_at(ox + dx, oy + dy, oz + dz).map(|b| b.name.contains("nether_brick")).unwrap_or(false) {
                        near = true;
                        break 'scan;
                    }
                }
            }
        }
        near
    };
    GameState {
        inventory: inv,
        equipment,
        world: WorldState {
            dimension: bot.game.dimension.clone(),
            dragon_dead: false,
            portal_built,
            fortress_found,
        },
        health: bot.health,
        food: bot.food,
        position: (p.x, p.y, p.z),
        alive: bot.health > 0.0,
    }
}

pub fn is_dragon_dead(state: &GameState) -> bool {
    state.world.dragon_dead
}
