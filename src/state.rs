//! Derive [`GameState`] from a live bot — count inventory items, detect
//! equipment tiers, read world/vitals. Port of steve's `state.ts`.

use rustcraft::bot::Bot;

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
    // A lit portal exists if a nether_portal block is nearby (the cast-and-light step
    // succeeded). Cheap line-of-sight-bounded search.
    let portal_built = bot.find_block("nether_portal", 16).is_some();
    GameState {
        inventory: inv,
        equipment,
        world: WorldState {
            dimension: bot.game.dimension.clone(),
            dragon_dead: false,
            portal_built,
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
