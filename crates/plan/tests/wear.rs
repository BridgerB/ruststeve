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
