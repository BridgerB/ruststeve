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

/// Blaze wall 8 (race 45017249540): the portal goal holds a pickaxe. With its kit held but no
/// pickaxe, and the sticks, stone and a table for one, it asks for the craft (it answered Done
/// and cast by hand); with a pickaxe held it is Done.
#[test]
fn the_portal_goal_with_no_pickaxe_crafts_one() {
    let t = real_table();
    let goal = t.until("portal").unwrap();
    let mut counts: Counts = [
        (ItemKind::WaterBucket, 1),
        (ItemKind::Bucket, 1),
        (ItemKind::FlintAndSteel, 1),
        (ItemKind::Stick, 24),
        (ItemKind::CobbledDeepslate, 64),
    ]
    .into_iter()
    .collect();
    let table_near = |k: data::BlockKind| k == data::BlockKind::CraftingTable;
    let id = |c: Choice| match c {
        Choice::Act(r) => r.id.clone(),
        Choice::Done => "done".to_owned(),
        Choice::Stuck => "stuck".to_owned(),
    };
    let none = make_with_wear(&t, &counts, &table_near, false, &Wear::default(), goal);
    assert!(id(none).contains("pickaxe"), "no pickaxe asked for");
    counts.insert(ItemKind::StonePickaxe, 1);
    let held = make_with_wear(&t, &counts, &table_near, false, &Wear::default(), goal);
    assert_eq!(id(held), "done");
}
