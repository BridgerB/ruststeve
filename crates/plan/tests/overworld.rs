//! The overworld leg (docs/problems/overworld-leg.md): spare pickaxes ahead of need (wall 2) and
//! a proportionate food check at the portal (wall 3).

use data::ItemKind;
use plan::make::{Goals, Table};
use protocol::S2c;
use protocol::slot::{Slot, Wear};

fn rules() -> plan::make::SurvivalRules {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../config/goals.toml");
    let goals: Goals = toml::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    Table::new(&goals).unwrap().survival
}

fn give(w: &mut world::World, slot: i32, item: ItemKind, n: i32, damage: Option<i32>) {
    w.step(&world::Event::Packet(S2c::SetPlayerInventory {
        slot,
        item: Slot::Item {
            item: item.id(),
            count: n,
            wear: Wear {
                damage,
                max_damage: None,
            },
        },
    }));
}

/// Wall 2 (lane 5 of race 45194253932): a stone pickaxe with 11 uses left and a wooden one are
/// no spare; with sticks, stone and a table in the pack more are made now. Two fresh ones held,
/// or no wood, and nothing is.
#[test]
fn spare_pickaxes_are_made_before_the_last_one_breaks() {
    let r = rules();
    assert_eq!(r.spare_pickaxes, 2);
    let mut w = world::World::new();
    give(&mut w, 0, ItemKind::StonePickaxe, 1, Some(120));
    give(&mut w, 1, ItemKind::WoodenPickaxe, 1, None);
    give(&mut w, 2, ItemKind::Stick, 25, None);
    give(&mut w, 3, ItemKind::Cobblestone, 64, None);
    give(&mut w, 4, ItemKind::CraftingTable, 1, None);
    assert_eq!(plan::fresh_pickaxes(&w, r.spare_uses), 0);
    assert!(plan::spares_due(&w, &r, false));
    give(&mut w, 5, ItemKind::StonePickaxe, 1, None);
    assert_eq!(plan::fresh_pickaxes(&w, r.spare_uses), 1);
    assert!(
        plan::spares_due(&w, &r, false),
        "one fresh pickaxe is no spare"
    );
    give(&mut w, 6, ItemKind::StonePickaxe, 1, Some(10));
    assert_eq!(plan::fresh_pickaxes(&w, r.spare_uses), 2);
    assert!(!plan::spares_due(&w, &r, false), "two fresh ones held");
    // No wood: never a trip for a spare.
    let mut bare = world::World::new();
    give(&mut bare, 0, ItemKind::StonePickaxe, 1, Some(120));
    give(&mut bare, 3, ItemKind::Cobblestone, 64, None);
    assert!(!plan::spares_due(&bare, &r, true));
}

/// Wall 3 (lanes 1 and 10 of race 45204423376): the portal is entered with 36 hunger points,
/// not the whole reserve of 48 (wall 4); unset, the reserve stands.
#[test]
fn the_portal_food_check_is_proportionate() {
    let mut r = rules();
    assert_eq!(r.food_reserve, 48);
    assert_eq!(plan::portal_food(&r), 36);
    assert!(plan::portal_food(&r) < r.food_reserve);
    r.portal_food = 0;
    assert_eq!(plan::portal_food(&r), r.food_reserve);
    r.portal_food = 99;
    assert_eq!(
        plan::portal_food(&r),
        r.food_reserve,
        "never over the reserve"
    );
}

/// Wall 1: the lava hunt's own stairs head for a known pool of ten along the longer axis until
/// it is within a leg, then (and with no pool) turn a quarter a leg. No terrain is read.
#[test]
fn own_stairs_head_for_the_pool_then_spiral() {
    use plan::{SPIRAL, own_heading};
    let feet = (55, 70, -133);
    assert_eq!(own_heading(feet, Some((118, -45, -99)), 0), (1, 0));
    assert_eq!(own_heading(feet, Some((60, -45, -99)), 3), (0, 1));
    assert_eq!(own_heading(feet, Some((58, -45, -130)), 2), SPIRAL[2]);
    let legs: Vec<_> = (0..5).map(|n| own_heading(feet, None, n)).collect();
    assert_eq!(legs, [(1, 0), (0, 1), (-1, 0), (0, -1), (1, 0)]);
}

fn table() -> Table {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../config/goals.toml");
    let goals: Goals = toml::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    Table::new(&goals).unwrap()
}

/// The cook phase's next step through the needs planner on the real goal table, with a table
/// and a furnace near or not.
fn cook_step(held: &[(ItemKind, i32)], table_near: bool, furnace_near: bool) -> String {
    use plan::make::{Choice, Counts, Wear, make_with_wear};
    let t = table();
    let counts: Counts = held.iter().copied().collect();
    let Some(goal) = plan::cook_goal(&counts, &t.survival) else {
        return "nothing raw".to_owned();
    };
    let near = move |k: data::BlockKind| match k {
        data::BlockKind::CraftingTable => table_near,
        data::BlockKind::Furnace => furnace_near,
        _ => false,
    };
    match make_with_wear(&t, &counts, &near, true, &Wear::default(), &goal) {
        Choice::Act(r) => r.id.clone(),
        Choice::Done => "done".to_owned(),
        Choice::Stuck => "stuck".to_owned(),
    }
}

/// Wall 4: a dozen raw porkchops and four raw chickens with the snapshot bot's pack. First the
/// planks that burn for all sixteen (11, from logs), then a furnace made and placed where the
/// bot stands, then the smelt; cooked food and nothing raw is nothing to cook.
#[test]
fn raw_meat_is_cooked_through_the_needs_planner() {
    use ItemKind::{
        BirchLog, Chicken, Cobblestone, CookedPorkchop, CraftingTable, Furnace, OakPlanks, Porkchop,
    };
    let r = table().survival;
    assert_eq!(r.meat_stock, 30);
    let pack = [
        (Porkchop, 12),
        (Chicken, 4),
        (BirchLog, 4),
        (Cobblestone, 64),
        (CraftingTable, 1),
    ];
    let counts: plan::make::Counts = pack.iter().copied().collect();
    assert_eq!(
        plan::eat::meat_pieces(&counts, &r),
        16,
        "raw chicken counts"
    );
    let fuel = plan::cook_goal(&counts, &r).unwrap();
    assert_eq!(fuel.id, "cook-fuel");
    assert_eq!(
        fuel.provides[0].1, 11,
        "16 pieces at 200 ticks, a plank burns 300"
    );
    let step = cook_step(&pack, false, false);
    assert!(
        step.starts_with("craft:") && step.ends_with("planks"),
        "{step}"
    );
    // The planks held: the furnace is made at a placed table, placed, and the meat smelted.
    let fuelled = [
        (Porkchop, 12),
        (Chicken, 4),
        (OakPlanks, 12),
        (Cobblestone, 64),
        (CraftingTable, 1),
    ];
    assert_eq!(
        plan::cook_goal(&fuelled.iter().copied().collect(), &r)
            .unwrap()
            .id,
        "cook"
    );
    assert_eq!(cook_step(&fuelled, false, false), "place-table");
    assert_eq!(cook_step(&fuelled, true, false), "craft:furnace");
    let mut with_furnace = fuelled.to_vec();
    with_furnace.push((Furnace, 1));
    assert_eq!(cook_step(&with_furnace, true, false), "place-furnace");
    let smelt = cook_step(&fuelled, true, true);
    assert!(smelt.starts_with("smelt:cooked_"), "{smelt}");
    assert_eq!(
        cook_step(&[(CookedPorkchop, 12), (OakPlanks, 4)], true, true),
        "nothing raw"
    );
}

/// A chicken is game now (its meat is cooked before it is eaten), and raw chicken is still
/// never the food to eat.
#[test]
fn a_chicken_is_hunted_and_never_eaten_raw() {
    let r = rules();
    assert!(plan::hunt::game(data::EntityKind::Chicken, &r));
    assert!(r.unsafe_foods.contains(&ItemKind::Chicken));
    assert!(plan::eat::cooks(ItemKind::Chicken).is_some());
    assert!(plan::eat::cooks(ItemKind::CookedBeef).is_none());
}
