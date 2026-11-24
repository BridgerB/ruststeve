//! The shields of the Nether kit (docs/problems/shield.md): after the kit's tools, the needs
//! planner on the real goal table makes the shields the held iron and wood allow, and never
//! leaves for more.

use data::{BlockKind, ItemKind};
use plan::make::{Act, Choice, Counts, Goals, Table, Wear, make_with_wear};
use plan::shield::{cook_choice, kit_act, kit_row};

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

/// What the real Nether snapshot bot holds (race 45194253932, lane 5, its `snap.toml`): raw
/// iron, two logs and a plank, a table, cobblestone, sticks, raw meat, no shield and no furnace.
fn nether_snapshot() -> Counts {
    use ItemKind::*;
    counts(&[
        (RawIron, 8),
        (BirchLog, 1),
        (OakLog, 1),
        (BirchPlanks, 1),
        (CraftingTable, 1),
        (StonePickaxe, 4),
        (Cobblestone, 256),
        (Beef, 6),
        (Porkchop, 5),
        (Stick, 13),
        (FlintAndSteel, 1),
        (Bucket, 1),
        (WaterBucket, 2),
        (IronHelmet, 1),
        (IronChestplate, 1),
        (IronLeggings, 1),
        (IronBoots, 1),
        (IronSword, 1),
    ])
}

fn take(c: &mut Counts, set: &[ItemKind], n: i32) {
    let mut n = n;
    for k in set {
        let have = c.get(k).copied().unwrap_or(0);
        let t = have.min(n);
        if t > 0 {
            c.insert(*k, have - t);
            n -= t;
        }
    }
    assert_eq!(n, 0, "short of {set:?}");
}

/// The smelt's fuel as `smelt::fuels_held` burns it: planks, then sticks over the reserve.
fn burn(c: &mut Counts, ticks: i32) {
    let planks = data::generated::tags::item_tag("planks").unwrap();
    let mut left = ticks;
    while left > 0 {
        if let Some(k) = planks.iter().find(|k| c.get(k).copied().unwrap_or(0) > 0) {
            *c.get_mut(k).unwrap() -= 1;
            left -= 300;
        } else {
            let s = c.get_mut(&ItemKind::Stick).unwrap();
            assert!(*s > plan::make::STICK_RESERVE, "out of fuel");
            *s -= 1;
            left -= 100;
        }
    }
}

/// Shield wall 3: the real Nether snapshot bot makes its one shield where it stands. The needs
/// planner, asked for the row `kit_row` picks, puts the table down, makes and puts down a
/// furnace, smelts one ingot (its smelt cooks what the shield is short of, no more), crafts the
/// planks and the shield: every step work where the bot stands, within the call site's bound of
/// 12 tasks. Then the fuel left cooks the raw beef.
#[test]
fn the_nether_snapshot_bot_makes_one_shield_where_it_stands() {
    let t = real_table();
    let mut c = nether_snapshot();
    let (mut table, mut furnace) = (false, false);
    let mut steps = Vec::new();
    while let Some(row) = kit_row(&c) {
        assert_eq!(row, "shield", "one shield's iron and wood");
        assert!(steps.len() < 12, "no shield in 12 steps: {steps:?}");
        let goal = t.until(row).unwrap();
        let near = |k: BlockKind| {
            (k == BlockKind::CraftingTable && table) || (k == BlockKind::Furnace && furnace)
        };
        let r = match make_with_wear(&t, &c, &near, false, &Wear::default(), goal) {
            Choice::Act(r) => r,
            other => panic!("after {steps:?}: {other:?}"),
        };
        assert!(kit_act(r), "{} is a trip, after {steps:?}", r.id);
        steps.push(r.id.clone());
        match r.act.unwrap() {
            Act::Place(BlockKind::CraftingTable) => {
                take(&mut c, &[ItemKind::CraftingTable], 1);
                table = true;
            }
            Act::Place(BlockKind::Furnace) => {
                take(&mut c, &[ItemKind::Furnace], 1);
                furnace = true;
            }
            Act::Craft(rec) => {
                for (_, set) in plan::craft::layout(rec, 3) {
                    take(&mut c, set, 1);
                }
                *c.entry(rec.result).or_default() += i32::from(rec.count);
            }
            Act::Smelt(s) => {
                let n = plan::shield::smelt_count(row, &c).unwrap();
                assert_eq!(n, 1, "one ingot for one shield");
                take(&mut c, s.input, n);
                burn(&mut c, i32::try_from(s.ticks).unwrap() * n);
                *c.entry(s.result).or_default() += n * i32::from(s.count);
            }
            other => panic!("{other:?} after {steps:?}"),
        }
    }
    assert_eq!(c.get(&ItemKind::Shield), Some(&1), "{steps:?}");
    assert!(table && furnace, "{steps:?}");
    assert!(steps.len() <= 8, "{steps:?}");
    // The fuel left: two planks and eleven sticks over the reserve, 1,700 ticks: the six beef.
    let (r, n) = cook_choice(&c).unwrap();
    assert_eq!((r.result, n), (ItemKind::CookedBeef, 6), "{c:?}");
}

#[test]
fn nothing_is_cooked_without_raw_meat_or_fuel() {
    assert!(cook_choice(&counts(&[(ItemKind::Beef, 4)])).is_none());
    assert!(cook_choice(&counts(&[(ItemKind::OakPlanks, 4)])).is_none());
    assert!(cook_choice(&counts(&[(ItemKind::Beef, 4), (ItemKind::Stick, 2)])).is_none());
    // A plank burns 300 ticks, a cook takes 200: one plank cooks one.
    let (r, n) = cook_choice(&counts(&[
        (ItemKind::Porkchop, 3),
        (ItemKind::OakPlanks, 1),
    ]))
    .unwrap();
    assert_eq!((r.result, n), (ItemKind::CookedPorkchop, 1));
}

/// A shield made at the surface (the overworld flow's cook site) is never made again in the
/// Nether: with one held and the makings of another, the Nether row is none.
#[test]
fn a_shield_held_from_the_surface_ends_the_nether_row() {
    let mut c = nether_snapshot();
    assert_eq!(plan::shield::nether_row(&c), Some("shield"));
    c.insert(ItemKind::Shield, 1);
    assert_eq!(plan::shield::nether_row(&c), None);
    c.insert(ItemKind::Shield, 2);
    assert_eq!(plan::shield::nether_row(&c), None);
}
