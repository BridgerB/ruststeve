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

/// Wall 3 (lanes 1 and 10 of race 45204423376): the portal is entered with 24 hunger points,
/// not the whole reserve of 32; unset, the reserve stands.
#[test]
fn the_portal_food_check_is_proportionate() {
    let mut r = rules();
    assert_eq!(plan::portal_food(&r), 24);
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
