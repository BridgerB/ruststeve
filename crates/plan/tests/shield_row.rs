//! The shields in the Nether kit (docs/problems/shield.md): the needs planner on the real goal
//! table makes two shields from what a geared bot holds before the portal, last of the kit, and
//! the kit is met with two held.

use data::{BlockKind, ItemKind};
use plan::make::{Choice, Counts, Goals, Table, Wear, make_with_wear};

fn real_table() -> Table {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../config/goals.toml");
    let goals: Goals = toml::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    Table::new(&goals).unwrap()
}

/// The kit row's next act holding `held`, with a table and a furnace near or not.
fn next(held: &[(ItemKind, i32)], stations: bool) -> String {
    let t = real_table();
    let goal = t.until("nether-kit").unwrap();
    let counts: Counts = held.iter().copied().collect();
    let near =
        move |k: BlockKind| stations && (k == BlockKind::CraftingTable || k == BlockKind::Furnace);
    match make_with_wear(&t, &counts, &near, false, &Wear::default(), goal) {
        Choice::Act(r) => r.id.clone(),
        Choice::Done => "done".to_owned(),
        Choice::Stuck => "stuck".to_owned(),
    }
}

const TOOLS: [(ItemKind, i32); 2] = [(ItemKind::StonePickaxe, 4), (ItemKind::CraftingTable, 1)];

#[test]
fn an_ingot_and_six_planks_at_a_table_are_crafted_into_a_shield() {
    let mut held = TOOLS.to_vec();
    held.extend([(ItemKind::IronIngot, 1), (ItemKind::OakPlanks, 6)]);
    assert_eq!(next(&held, true), "craft:shield");
}

#[test]
fn raw_iron_and_logs_lead_to_the_shield() {
    // What the clean `ready` snapshot bot holds beside its tools: raw iron and logs. Every step
    // is an act (planks, a smelt, the stations), never stuck.
    let mut held = TOOLS.to_vec();
    held.extend([(ItemKind::RawIron, 8), (ItemKind::OakLog, 6)]);
    let first = next(&held, true);
    assert!(first != "done" && first != "stuck", "{first}");
    // The ingot smelted and the planks made: the craft.
    held.extend([(ItemKind::IronIngot, 1), (ItemKind::OakPlanks, 8)]);
    assert_eq!(next(&held, true), "craft:shield");
}

#[test]
fn the_kit_is_met_with_two_shields_held_and_the_tools_come_first() {
    let mut held = TOOLS.to_vec();
    held.push((ItemKind::Shield, 2));
    assert_eq!(next(&held, true), "done");
    // One held (in the off hand or not): the second is made.
    let mut one = TOOLS.to_vec();
    one.extend([
        (ItemKind::Shield, 1),
        (ItemKind::IronIngot, 1),
        (ItemKind::OakPlanks, 6),
    ]);
    assert_eq!(next(&one, true), "craft:shield");
    // Without the tools the shield's makings are not touched first.
    let short = [
        (ItemKind::StonePickaxe, 3),
        (ItemKind::CraftingTable, 1),
        (ItemKind::IronIngot, 1),
        (ItemKind::OakPlanks, 6),
        (ItemKind::Cobblestone, 3),
        (ItemKind::Stick, 2),
    ];
    assert_eq!(next(&short, true), "craft:stone_pickaxe");
}

/// With no wood and no iron held the row goes for them, as it does for the pickaxes' wood: a
/// gather first. `Plan::nether_task` bounds the kit at `KIT_TASKS` tasks, so the flow before
/// it should hold the makings (two ingots or raw iron with fuel, twelve planks or three logs).
#[test]
fn nothing_to_make_it_from_is_a_gather() {
    assert_eq!(next(&TOOLS, true), "gather");
}
