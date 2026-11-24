//! Blaze wall 7 (race 45000585321): a worn pickaxe is no spare, and with digs short the spare's
//! gather is worth it from anywhere, so a bot underground goes for wood while its pickaxe lasts.

use data::ItemKind;
use plan::make::{Choice, Counts, Goals, Table, Wear, make_with_wear, without};

fn real_table() -> Table {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../config/goals.toml");
    let goals: Goals = toml::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    Table::new(&goals).unwrap()
}

/// Mine-iron holding one stone pickaxe, with the cobblestone for another but no wood, underground
/// (no log at the feet's height): what does the needs planner do next?
fn next(t: &Table, wear: &Wear) -> String {
    let goal = t
        .goals
        .iter()
        .map(|g| &g.2)
        .chain(&t.producers)
        .find(|g| g.id == "mine-iron")
        .unwrap();
    let counts: Counts = [(ItemKind::StonePickaxe, 1), (ItemKind::Cobblestone, 64)]
        .into_iter()
        .collect();
    let table_near = |k: data::BlockKind| k == data::BlockKind::CraftingTable;
    match make_with_wear(t, &counts, &table_near, false, wear, goal) {
        Choice::Act(r) => r.id.clone(),
        Choice::Done => "done".to_owned(),
        Choice::Stuck => "stuck".to_owned(),
    }
}

#[test]
fn with_digs_to_spare_the_bot_mines_on() {
    let t = real_table();
    assert_eq!(next(&t, &Wear::default()), "mine-iron");
}

#[test]
fn low_on_digs_underground_the_spares_gather_is_chosen() {
    let t = real_table();
    let wear = Wear {
        worn: Counts::new(),
        low_on_digs: true,
    };
    assert_eq!(next(&t, &wear), "gather");
}

#[test]
fn a_worn_tool_is_not_counted_as_a_spare() {
    let counts: Counts = [(ItemKind::StonePickaxe, 2)].into_iter().collect();
    let worn: Counts = [(ItemKind::StonePickaxe, 1)].into_iter().collect();
    assert_eq!(
        without(&counts, &worn).get(&ItemKind::StonePickaxe),
        Some(&1)
    );
}

/// Blaze wall 8 (race 45017249540): a pickaxe is makeable where the bot stands from held sticks
/// and stone with a table held or near, and not without the table or the wood.
#[test]
fn a_pickaxe_is_makeable_from_held_sticks_stone_and_a_table() {
    use protocol::S2c;
    use protocol::slot::Slot;
    let give = |w: &mut world::World, slot: i32, item: ItemKind, n: i32| {
        w.step(&world::Event::Packet(S2c::SetPlayerInventory {
            slot,
            item: Slot::item(item.id(), n),
        }));
    };
    let mut w = world::World::new();
    give(&mut w, 0, ItemKind::Stick, 24);
    give(&mut w, 1, ItemKind::CobbledDeepslate, 49);
    assert!(!plan::pickaxe_makeable(&w, false), "no table held or near");
    assert!(plan::pickaxe_makeable(&w, true));
    give(&mut w, 2, ItemKind::CraftingTable, 1);
    assert!(plan::pickaxe_makeable(&w, false), "a table held");
    give(&mut w, 0, ItemKind::Stick, 1);
    assert!(!plan::pickaxe_makeable(&w, true), "one stick is not two");
}

/// Blaze wall 17 (race 45128866973): a pickaxe left on the cursor by a swap is still a pickaxe
/// held, as the needs planner's counts have it, so a cast does not pause for it.
#[test]
fn a_carried_pickaxe_is_held() {
    let mut w = world::World::new();
    assert!(!plan::holds_pickaxe(&w));
    w.inventory.carried = Some((ItemKind::StonePickaxe, 1));
    assert!(plan::holds_pickaxe(&w));
    assert_eq!(plan::held(&w).get(&ItemKind::StonePickaxe), Some(&1));
}

/// Blaze wall 18 (race 45164371770, lane 4): before the portal the needs planner makes the Nether
/// kit from what the clean snapshot holds, and is done at four stone pickaxes and a table.
#[test]
fn the_nether_kit_makes_pickaxes_and_a_table() {
    let t = real_table();
    let goal = t.until("nether-kit").unwrap();
    let no_table = |_: data::BlockKind| false;
    let table_near = |k: data::BlockKind| k == data::BlockKind::CraftingTable;
    let act = |counts: &Counts, near: &dyn Fn(data::BlockKind) -> bool| match make_with_wear(
        &t,
        counts,
        near,
        false,
        &Wear::default(),
        goal,
    ) {
        Choice::Act(r) => r.id.clone(),
        Choice::Done => "done".to_owned(),
        Choice::Stuck => "stuck".to_owned(),
    };
    let held: Counts = [
        (ItemKind::Cobblestone, 64),
        (ItemKind::Stick, 23),
        (ItemKind::OakLog, 4),
        (ItemKind::CraftingTable, 1),
    ]
    .into_iter()
    .collect();
    assert_eq!(act(&held, &no_table), "place-table");
    assert_eq!(act(&held, &table_near), "craft:stone_pickaxe");
    // Two shields are the kit's last items (docs/problems/shield.md).
    let made: Counts = [
        (ItemKind::StonePickaxe, 4),
        (ItemKind::CraftingTable, 1),
        (ItemKind::Shield, 2),
    ]
    .into_iter()
    .collect();
    assert_eq!(act(&made, &no_table), "done");
    // No wood, no sticks: nothing to make them from where the bot stands.
    let bare: Counts = [(ItemKind::Cobblestone, 64)].into_iter().collect();
    assert_ne!(act(&bare, &no_table), "done");
}

/// Blaze wall 19: a bot that left its table still makes a pickaxe from a log and sticks, or from
/// two logs; sticks alone, or three planks, make no table.
#[test]
fn held_wood_makes_the_table_and_the_sticks() {
    assert!(plan::wood_makes(23, 0, 1));
    assert!(plan::wood_makes(0, 1, 1));
    assert!(plan::wood_makes(0, 0, 2));
    assert!(!plan::wood_makes(0, 0, 1), "a table and no sticks");
    assert!(!plan::wood_makes(24, 0, 0), "sticks make no table");
    assert!(!plan::wood_makes(24, 3, 0));
}

/// Blaze wall 19: two landings looked at in turn each keep their own stalls, so both are refused
/// after three looks that came no nearer; a look that comes nearer starts the count again.
#[test]
fn a_landing_keeps_its_stalls_across_a_flip() {
    let (a, b) = ((124, -53, -102), (129, -52, -105));
    let mut kept = Vec::new();
    assert_eq!(plan::landing_stalls(&mut kept, a, 6.0), (6.0, 0));
    assert_eq!(plan::landing_stalls(&mut kept, b, 6.5), (6.5, 0));
    assert_eq!(plan::landing_stalls(&mut kept, a, 6.2), (6.0, 1));
    assert_eq!(plan::landing_stalls(&mut kept, b, 6.4), (6.5, 1));
    assert_eq!(plan::landing_stalls(&mut kept, a, 5.8), (6.0, 2));
    assert_eq!(plan::landing_stalls(&mut kept, a, 6.0), (6.0, 3));
    assert_eq!(plan::landing_stalls(&mut kept, b, 5.0), (5.0, 0));
    assert_eq!(kept.len(), 2);
}
