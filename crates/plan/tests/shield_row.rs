//! The shields of the Nether kit (docs/problems/shield.md): after the kit's tools, the needs
//! planner on the real goal table makes the shields the held iron and wood allow, and never
//! leaves for more.

use data::{BlockKind, ItemKind};
use plan::make::{Choice, Counts, Goals, Table, Wear, make_with_wear};
use plan::shield::{kit_act, kit_row};

fn real_table() -> Table {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../config/goals.toml");
    let goals: Goals = toml::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    Table::new(&goals).unwrap()
}

fn counts(held: &[(ItemKind, i32)]) -> Counts {
    held.iter().copied().collect()
}

/// The row `kit_row` picks for `held` and its next act, a table and a furnace near: the act's
/// id and whether it is work where the bot stands.
fn next(held: &[(ItemKind, i32)]) -> Option<(&'static str, String, bool)> {
    let t = real_table();
    let c = counts(held);
    let row = kit_row(&c)?;
    let goal = t.until(row).unwrap();
    let near = |k: BlockKind| k == BlockKind::CraftingTable || k == BlockKind::Furnace;
    Some(
        match make_with_wear(&t, &c, &near, false, &Wear::default(), goal) {
            Choice::Act(r) => (row, r.id.clone(), kit_act(r)),
            Choice::Done => (row, "done".to_owned(), false),
            Choice::Stuck => (row, "stuck".to_owned(), false),
        },
    )
}

#[test]
fn the_row_is_what_the_held_iron_and_wood_make() {
    use ItemKind::{IronIngot, OakLog, OakPlanks, RawIron, Shield};
    assert_eq!(kit_row(&counts(&[])), None, "nothing held");
    assert_eq!(kit_row(&counts(&[(IronIngot, 2)])), None, "no wood");
    assert_eq!(kit_row(&counts(&[(OakLog, 3)])), None, "no iron");
    assert_eq!(
        kit_row(&counts(&[(IronIngot, 2), (OakPlanks, 12)])),
        Some("shields")
    );
    assert_eq!(
        kit_row(&counts(&[(RawIron, 8), (OakLog, 3)])),
        Some("shields")
    );
    assert_eq!(
        kit_row(&counts(&[(RawIron, 1), (OakLog, 3)])),
        Some("shield")
    );
    assert_eq!(
        kit_row(&counts(&[(IronIngot, 2), (OakPlanks, 11)])),
        Some("shield")
    );
    // One held: the second from one ingot and six planks; two held: no row.
    assert_eq!(
        kit_row(&counts(&[(Shield, 1), (IronIngot, 1), (OakPlanks, 6)])),
        Some("shields")
    );
    assert_eq!(kit_row(&counts(&[(Shield, 1), (OakPlanks, 6)])), None);
    assert_eq!(
        kit_row(&counts(&[(Shield, 2), (IronIngot, 4), (OakPlanks, 24)])),
        None
    );
}

#[test]
fn ingots_and_planks_at_a_table_are_crafted_into_shields() {
    let held = [(ItemKind::IronIngot, 2), (ItemKind::OakPlanks, 12)];
    assert_eq!(
        next(&held),
        Some(("shields", "craft:shield".to_owned(), true))
    );
    let one = [
        (ItemKind::Shield, 1),
        (ItemKind::IronIngot, 1),
        (ItemKind::OakPlanks, 6),
    ];
    assert_eq!(
        next(&one),
        Some(("shields", "craft:shield".to_owned(), true))
    );
}

/// What the clean `ready` snapshot bot holds beside its tools: raw iron and logs. The first step
/// is work where the bot stands, not a trip.
#[test]
fn raw_iron_and_logs_lead_to_the_shields_where_the_bot_stands() {
    let held = [
        (ItemKind::RawIron, 8),
        (ItemKind::OakLog, 6),
        (ItemKind::Coal, 2),
    ];
    let (row, act, here) = next(&held).unwrap();
    assert_eq!(row, "shields");
    assert!(here, "{act} is a trip");
}

/// Iron to smelt and nothing to burn but the shields' own planks: whatever the planner answers,
/// `kit_act` tells a trip from work, and the planner's call site skips a trip.
#[test]
fn a_trip_is_told_from_work() {
    let t = real_table();
    let gather = t.producers.iter().find(|r| r.id == "gather").unwrap();
    let mine = t.producers.iter().find(|r| r.id == "mine-iron").unwrap();
    let craft = t.producers.iter().find(|r| r.id == "craft:shield").unwrap();
    let place = t
        .producers
        .iter()
        .find(|r| r.id == "place-furnace")
        .unwrap();
    assert!(!kit_act(gather) && !kit_act(mine));
    assert!(kit_act(craft) && kit_act(place));
}
