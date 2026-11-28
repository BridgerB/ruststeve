//! The iron pickaxe made at the cook site (docs/problems/lava-portal.md wall 2): from the iron
//! held beyond the kit's ingots, after the shields, never a trip for iron; then dug with first.

use data::{BlockKind, ItemKind};
use plan::make::{Choice, Counts, Goals, Table, Wear, make_with_wear};
use protocol::S2c;
use protocol::slot::{Slot, Wear as SlotWear};

fn table() -> Table {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../config/goals.toml");
    let goals: Goals = toml::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    Table::new(&goals).unwrap()
}

/// The pack as counts, a kind named twice summed.
fn counts(held: &[(ItemKind, i32)]) -> Counts {
    held.iter().fold(Counts::new(), |mut c, (k, n)| {
        *c.entry(*k).or_default() += n;
        c
    })
}

/// The cook goal's id and count for a pack, with `fresh` iron pickaxes held with uses to spare.
fn goal(held: &[(ItemKind, i32)], fresh: i32) -> Option<(String, i32)> {
    let t = table();
    plan::cook_goal_with(&counts(held), &t.survival, fresh).map(|r| (r.id, r.provides[0].1))
}

/// The cook's next step through the needs planner, a table and a furnace where the bot stands.
fn step(held: &[(ItemKind, i32)], fresh: i32) -> String {
    let t = table();
    let c = counts(held);
    let Some(goal) = plan::cook_goal_with(&c, &t.survival, fresh) else {
        return "nothing".to_owned();
    };
    let near = |k: BlockKind| matches!(k, BlockKind::CraftingTable | BlockKind::Furnace);
    match make_with_wear(&t, &c, &near, true, &Wear::default(), &goal) {
        Choice::Act(r) => r.id.clone(),
        Choice::Done => "done".to_owned(),
        Choice::Stuck => "stuck".to_owned(),
    }
}

fn give(w: &mut world::World, slot: i32, item: ItemKind, n: i32, damage: Option<i32>) {
    w.step(&world::Event::Packet(S2c::SetPlayerInventory {
        slot,
        item: Slot::Item {
            item: item.id(),
            count: n,
            wear: SlotWear {
                damage,
                max_damage: None,
            },
        },
    }));
}

/// The numbers the rule rests on, from the generated data (the 26.1.2 jar): the recipe is three
/// ingots and two sticks; iron cuts stone in 8 ticks against stone's 12 and deepslate in 15
/// against 23, and lasts 250 uses against 131; the rule is on in the real goal table.
#[test]
fn the_rule_rests_on_the_generated_data() {
    let t = table();
    assert_eq!(t.survival.kit_iron_pickaxe, 1);
    let recipe = data::generated::recipes::RECIPES
        .iter()
        .find(|r| r.id == "iron_pickaxe")
        .unwrap();
    let data::types::Shape::Shaped { cells, .. } = recipe.shape else {
        panic!("iron_pickaxe is shaped");
    };
    let of = |k: ItemKind| cells.iter().flatten().filter(|c| c.contains(&k)).count();
    assert_eq!(of(ItemKind::IronIngot), plan::IRON_PICKAXE_INGOTS as usize);
    assert_eq!(of(ItemKind::Stick), 2);
    let ticks = |b, k| data::dig::break_ticks(b, Some(k), false, true);
    assert_eq!(ticks(BlockKind::Stone, ItemKind::IronPickaxe), Some(8));
    assert_eq!(ticks(BlockKind::Stone, ItemKind::StonePickaxe), Some(12));
    assert_eq!(ticks(BlockKind::Deepslate, ItemKind::IronPickaxe), Some(15));
    assert_eq!(
        ticks(BlockKind::Deepslate, ItemKind::StonePickaxe),
        Some(23)
    );
    assert_eq!(
        data::generated::item_props::max_damage(ItemKind::IronPickaxe),
        250
    );
    assert_eq!(
        data::generated::item_props::max_damage(ItemKind::StonePickaxe),
        131
    );
}

/// Race 46139640077's pack at the cook (8 raw iron, the meat cooked): six ingots smelted (the
/// kit's three and the pickaxe's three), then the two shields, then the pickaxe crafted at the
/// table (sticks first when none are held); once held, nothing more.
#[test]
fn the_cook_site_makes_an_iron_pickaxe_after_the_shields() {
    use ItemKind::{CookedPorkchop, IronIngot, IronPickaxe, OakPlanks, RawIron, Shield, Stick};
    let ore = [(CookedPorkchop, 30), (OakPlanks, 20), (RawIron, 8)];
    assert_eq!(goal(&ore, 0), Some(("cook-iron".to_owned(), 6)));
    assert!(
        step(&ore, 0).starts_with("smelt:iron_ingot"),
        "{}",
        step(&ore, 0)
    );
    let smelted = [
        (CookedPorkchop, 30),
        (OakPlanks, 20),
        (IronIngot, 6),
        (RawIron, 2),
    ];
    assert_eq!(goal(&smelted, 0), Some(("cook-shield".to_owned(), 2)));
    let shielded = [
        (CookedPorkchop, 30),
        (OakPlanks, 8),
        (IronIngot, 4),
        (RawIron, 2),
        (Shield, 2),
        (Stick, 4),
    ];
    assert_eq!(goal(&shielded, 0), Some(("cook-pickaxe".to_owned(), 1)));
    assert_eq!(step(&shielded, 0), "craft:iron_pickaxe");
    let no_sticks = [
        (CookedPorkchop, 30),
        (OakPlanks, 8),
        (IronIngot, 4),
        (Shield, 2),
    ];
    assert!(
        step(&no_sticks, 0).contains("stick"),
        "{}",
        step(&no_sticks, 0)
    );
    let made = [
        (CookedPorkchop, 30),
        (IronIngot, 1),
        (RawIron, 2),
        (Shield, 2),
        (IronPickaxe, 1),
    ];
    assert_eq!(goal(&made, 1), None);
    assert_eq!(step(&made, 1), "nothing");
}

/// Never a trip for iron: with no more iron than the kit's ingots, or under three beyond them, no
/// pickaxe is asked for; with one held worn (none with uses to spare), one more is.
#[test]
fn only_the_iron_held_beyond_the_kit_makes_one() {
    use ItemKind::{CookedPorkchop, IronIngot, IronPickaxe, OakPlanks, RawIron, Shield};
    let t = table();
    let r = &t.survival;
    let kit = [(CookedPorkchop, 30), (IronIngot, 1), (Shield, 2)];
    assert_eq!(plan::iron_pickaxes_due(&counts(&kit), r, 0), 0);
    assert_eq!(goal(&kit, 0), None);
    let two_over = [
        (CookedPorkchop, 30),
        (IronIngot, 1),
        (RawIron, 2),
        (Shield, 2),
    ];
    assert_eq!(plan::iron_pickaxes_due(&counts(&two_over), r, 0), 0);
    assert_eq!(goal(&two_over, 0), None);
    let three_over = [
        (CookedPorkchop, 30),
        (OakPlanks, 8),
        (IronIngot, 1),
        (RawIron, 3),
        (Shield, 2),
    ];
    assert_eq!(plan::iron_pickaxes_due(&counts(&three_over), r, 0), 1);
    assert_eq!(goal(&three_over, 0), Some(("cook-iron".to_owned(), 4)));
    let worn = [
        (CookedPorkchop, 30),
        (OakPlanks, 4),
        (IronIngot, 4),
        (Shield, 2),
        (IronPickaxe, 1),
    ];
    assert_eq!(goal(&worn, 0), Some(("cook-pickaxe".to_owned(), 2)));
    assert_eq!(goal(&worn, 1), None);
    // The old entry point takes the kit's pickaxe as held: the shields' cook as before.
    assert_eq!(
        plan::cook_goal(
            &counts(&[(CookedPorkchop, 30), (OakPlanks, 20), (RawIron, 8)]),
            r
        )
        .map(|g| (g.id, g.provides[0].1)),
        Some(("cook-iron".to_owned(), 3))
    );
}

/// An iron pickaxe with 10 uses left has none to spare; a fresh one has.
#[test]
fn a_worn_iron_pickaxe_is_not_fresh() {
    let t = table();
    let mut w = world::World::new();
    give(&mut w, 0, ItemKind::IronPickaxe, 1, Some(240));
    assert_eq!(plan::fresh_iron_pickaxes(&w, t.survival.spare_uses), 0);
    give(&mut w, 1, ItemKind::IronPickaxe, 1, None);
    assert_eq!(plan::fresh_iron_pickaxes(&w, t.survival.spare_uses), 1);
}

/// Dug with first: every dig takes the fastest held tool that harvests the block
/// (`data::dig::best_tool`, the choice `plan::dig::Dig` makes for stairs, tunnels and casts).
#[test]
fn every_dig_takes_the_iron_pickaxe_over_a_stone_one() {
    use ItemKind::{IronPickaxe, StonePickaxe, WoodenPickaxe};
    for b in [
        BlockKind::Stone,
        BlockKind::Deepslate,
        BlockKind::Cobblestone,
        BlockKind::Andesite,
        BlockKind::Tuff,
    ] {
        assert_eq!(
            data::dig::best_tool(b, [StonePickaxe, IronPickaxe, WoodenPickaxe]),
            Some(IronPickaxe),
            "{b:?}"
        );
    }
}

/// The pickaxe in the hand is noted once at each change of its kind.
#[test]
fn the_pickaxe_in_hand_is_noted_at_each_change() {
    let mut w = world::World::new();
    w.body = Some(physics::Body {
        pos: [0.5, 64.0, 0.5],
        ..physics::Body::default()
    });
    give(&mut w, 0, ItemKind::StonePickaxe, 1, None);
    give(&mut w, 1, ItemKind::IronPickaxe, 1, None);
    let mut p = plan::Plan::new(plan::Aim::Hold);
    let notes = |p: &plan::Plan| {
        p.events
            .iter()
            .filter(|e| e.what.starts_with("pickaxe in hand: "))
            .map(|e| e.what.clone())
            .collect::<Vec<_>>()
    };
    let _ = p.step(&w);
    let _ = p.step(&w);
    assert_eq!(notes(&p), ["pickaxe in hand: stone_pickaxe"]);
    w.held = 1;
    let _ = p.step(&w);
    let _ = p.step(&w);
    assert_eq!(
        notes(&p),
        [
            "pickaxe in hand: stone_pickaxe",
            "pickaxe in hand: iron_pickaxe"
        ]
    );
    let why = p
        .events
        .iter()
        .rev()
        .find(|e| e.what == "pickaxe in hand: iron_pickaxe")
        .map(|e| e.why.clone())
        .unwrap();
    assert!(
        why.contains("250 uses left; was stone_pickaxe; other pickaxes held: stone_pickaxe"),
        "{why}"
    );
}

/// The cook's next step through the needs planner with the old entry point (`cook_goal`, no
/// iron pickaxe).
fn old_step(held: &[(ItemKind, i32)]) -> String {
    let t = table();
    let c = counts(held);
    let Some(goal) = plan::cook_goal(&c, &t.survival) else {
        return "nothing".to_owned();
    };
    let near = |k: BlockKind| matches!(k, BlockKind::CraftingTable | BlockKind::Furnace);
    match make_with_wear(&t, &c, &near, true, &Wear::default(), &goal) {
        Choice::Act(r) => r.id.clone(),
        Choice::Done => "done".to_owned(),
        Choice::Stuck => "stuck".to_owned(),
    }
}

/// Race 46148925674 (typecraft `ready`, tick 20): 3 birch and 1 oak log, an oak plank, 25
/// sticks, 8 raw iron, 6 beef and 6 porkchop, and the chickens hunted after. The four logs
/// cover the meat, the kit's ingots and the shields' planks (7,400 fuel ticks held or made
/// against 6,600) but not the pickaxe's smelts and sticks too (7,500): it is skipped, and the
/// cook's goal and every step are the old cook's, with no gather for its fuel. With planks to
/// spare it is made.
#[test]
fn short_of_fuel_the_cook_runs_as_without_the_pickaxe() {
    use ItemKind::{
        Beef, BirchLog, BirchPlanks, Chicken, Cobblestone, OakLog, OakPlanks, Porkchop, RawIron,
        Stick,
    };
    let t = table();
    let r = &t.survival;
    for chickens in 0..=6 {
        let mut pack = vec![
            (BirchLog, 3),
            (OakLog, 1),
            (OakPlanks, 1),
            (Stick, 25),
            (RawIron, 8),
            (Beef, 6),
            (Porkchop, 6),
            (Cobblestone, 64),
        ];
        if chickens > 0 {
            pack.push((Chicken, chickens));
        }
        // The cook's road: the planks crafted from the logs, then each kind cooked.
        let planked = |p: &[(ItemKind, i32)]| {
            p.iter()
                .map(|(k, n)| match k {
                    BirchLog | OakLog => (BirchPlanks, 4 * n),
                    _ => (*k, *n),
                })
                .collect::<Vec<_>>()
        };
        for held in [pack.clone(), planked(&pack)] {
            let c = counts(&held);
            assert_eq!(plan::iron_pickaxes_due(&c, r, 0), 1, "{held:?}");
            assert_eq!(plan::iron_pickaxes_fuelled(&c, r, 0), 0, "{held:?}");
            assert_eq!(
                plan::cook_goal_with(&c, r, 0),
                plan::cook_goal(&c, r),
                "{held:?}"
            );
            assert_eq!(step(&held, 0), old_step(&held), "{held:?}");
            assert_ne!(step(&held, 0), "gather", "{held:?}");
        }
    }
    // Four planks over the cook's need (8,600 held against 7,500): the pickaxe is made.
    let spare = [
        (BirchLog, 3),
        (OakLog, 1),
        (OakPlanks, 5),
        (Stick, 25),
        (RawIron, 8),
        (Beef, 6),
        (Porkchop, 6),
    ];
    assert_eq!(plan::iron_pickaxes_fuelled(&counts(&spare), r, 0), 1);
    assert_eq!(
        plan::cook_goal_with(&counts(&spare), r, 0)
            .map(|g| g.id)
            .as_deref(),
        Some("cook-fuel")
    );
}
