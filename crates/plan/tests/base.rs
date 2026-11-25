//! Issue 18d, the forest base (docs/problems/pearls-base.md): its numbers, what it counts, the
//! stems it digs and the goal rows it makes, on hand-built worlds and the real goal table.

use data::{BlockKind, ItemKind};
use plan::base::{
    self, Base, STEPS, Step, bare_hand, drop_slot, kit_held, skip_why, stem_targets, weapon_kills,
};
use plan::make::{Choice, Counts, Goals, Table, Wear, make_with_wear};
use world::World;

const NETHER: &str = "minecraft:the_nether";
const FEET: [f64; 3] = [0.5, 41.0, 0.5];

fn goals() -> Goals {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../config/goals.toml");
    toml::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

fn table() -> Table {
    Table::new(&goals()).unwrap()
}

fn bare() -> World {
    let mut w = World::new();
    w.body = Some(physics::Body {
        pos: FEET,
        ..physics::Body::default()
    });
    w.dimension = Some(world::Dimension {
        type_id: 3,
        name: NETHER.to_owned(),
        min_y: Some(0),
        height: Some(256),
    });
    w
}

fn state(k: BlockKind) -> u32 {
    k.info().unwrap().default_state
}

/// A block seen exposed (air on its side) in memory, as the world records one.
fn seen(w: &mut World, pos: (i32, i32, i32), k: BlockKind) {
    let air = Some(state(BlockKind::Air));
    let s =
        memory::Seen::check(NETHER, pos, state(k), [None, None, air, None, None, None]).unwrap();
    w.memory.record_sign(&s);
    w.memory.record(s, 0);
}

/// `item` in window slot `slot`, with `uses` left when given.
fn give(w: &mut World, slot: i16, item: ItemKind, n: i32, uses: Option<i32>) {
    w.inventory.slots.insert(slot, (item, n));
    if let Some(u) = uses {
        let max = data::generated::item_props::max_damage(item);
        w.inventory.wear.insert(
            slot,
            protocol::slot::Wear {
                damage: Some(max - u),
                max_damage: None,
            },
        );
    }
}

/// The frontier bot's tools (race-45481633896 lane 3): an iron sword of 161 uses, stone
/// pickaxes of 13 and 131.
fn frontier() -> World {
    let mut w = bare();
    give(&mut w, 36, ItemKind::IronSword, 1, Some(161));
    give(&mut w, 37, ItemKind::StonePickaxe, 1, Some(13));
    give(&mut w, 38, ItemKind::StonePickaxe, 1, Some(131));
    give(&mut w, 9, ItemKind::RawIron, 7, None);
    give(&mut w, 10, ItemKind::Coal, 1, None);
    give(&mut w, 11, ItemKind::Stick, 3, None);
    give(&mut w, 12, ItemKind::Cobblestone, 64, None);
    give(&mut w, 13, ItemKind::Furnace, 1, None);
    give(&mut w, 14, ItemKind::BlazeRod, 7, None);
    w
}

#[test]
fn the_configs_base_is_the_default_and_its_names_parse() {
    let g = goals();
    assert_eq!(g.base, Base::default(), "base_done reads the default");
    for (n, h) in &g.base.hits_per_kill {
        assert!(ItemKind::from_name(n).is_some(), "{n}");
        assert!(*h > 0);
    }
    for n in &g.base.drop {
        let k = ItemKind::from_name(n).unwrap();
        assert!(!base::NEVER_DROP.contains(&k), "{n}");
    }
    let t = table();
    for id in [
        "base-table",
        "base-iron-sword",
        "base-stone-sword",
        "base-iron-pickaxe",
        "base-stone-pickaxe",
        "base-shield",
    ] {
        assert!(t.until(id).is_some(), "{id}");
    }
    // Warped stems are logs, their planks are planks, and warped planks do not burn.
    let logs = data::generated::tags::item_tag("logs").unwrap();
    assert!(logs.contains(&ItemKind::WarpedStem));
    let planks = data::generated::tags::item_tag("planks").unwrap();
    assert!(planks.contains(&ItemKind::WarpedPlanks));
    assert_eq!(
        data::generated::fuel::fuel_ticks(ItemKind::WarpedPlanks),
        None
    );
}

/// The frontier's sword is 23 kills (161 / 7), a stone sword 16 (131 / 8); a sword under 10 uses
/// counts for nothing. A fresh iron sword (250 / 7 = 35) and a shield make the kit with 144
/// pickaxe uses.
#[test]
fn swords_are_counted_in_kills_and_the_kit_is_judged_on_them() {
    let b = Base::default();
    let mut w = frontier();
    assert_eq!(weapon_kills(&w, &b), 23);
    assert!(!kit_held(&w, &b));
    give(&mut w, 39, ItemKind::StoneSword, 1, Some(131));
    give(&mut w, 40, ItemKind::IronSword, 1, Some(9));
    assert_eq!(weapon_kills(&w, &b), 39);
    w.inventory.slots.remove(&39);
    w.inventory.wear.remove(&39);
    give(&mut w, 41, ItemKind::IronSword, 1, None);
    give(&mut w, 45, ItemKind::Shield, 1, None);
    assert_eq!(weapon_kills(&w, &b), 23 + 35);
    assert!(kit_held(&w, &b));
    assert!(base::base_done(&w));
}

/// What the frontier bot skips: nothing before the shield but the pickaxe (144 uses held); with
/// no coal (lane 2) the smelt and so the iron sword.
#[test]
fn the_frontier_bot_skips_the_pickaxe_and_without_coal_the_smelt() {
    let b = Base::default();
    let w = frontier();
    let skipped = |w: &World| -> Vec<Step> {
        STEPS
            .iter()
            .copied()
            .filter(|s| skip_why(*s, w, &b, false).is_some())
            .collect()
    };
    // The iron sword waits on ingots, which the smelt makes.
    assert_eq!(
        skipped(&w),
        vec![Step::IronSword, Step::Pickaxe, Step::Shield]
    );
    let mut ingots = w.clone();
    give(&mut ingots, 20, ItemKind::IronIngot, 7, None);
    ingots.inventory.slots.remove(&9);
    assert_eq!(skipped(&ingots), vec![Step::Smelt, Step::Pickaxe]);
    let mut lane2 = w.clone();
    lane2.inventory.slots.remove(&10);
    let why = skip_why(Step::Smelt, &lane2, &b, false).unwrap();
    assert!(why.contains("no coal"), "{why}");
    // Eight stems held: no wood step.
    let mut wood = w;
    give(&mut wood, 21, ItemKind::WarpedStem, 8, None);
    assert!(skip_why(Step::Wood, &wood, &b, false).is_some());
}

/// A hand-built forest: stems of three fungi at the feet's level, one stem high in a cap, one
/// beyond the radius, nylium all round. The nearest low stems come first; a refused one and the
/// high and far ones never.
#[test]
fn stems_are_the_nearest_seen_at_the_feet_level_within_the_radius() {
    let b = Base::default();
    let mut w = bare();
    for x in -6..=6 {
        for z in -6..=6 {
            seen(&mut w, (x, 40, z), BlockKind::WarpedNylium);
        }
    }
    for y in 41..=44 {
        seen(&mut w, (3, y, 0), BlockKind::WarpedStem);
        seen(&mut w, (-5, y, 2), BlockKind::WarpedStem);
    }
    seen(&mut w, (1, 50, 1), BlockKind::WarpedStem);
    seen(&mut w, (30, 41, 0), BlockKind::WarpedStem);
    seen(&mut w, (2, 41, 2), BlockKind::WarpedWartBlock);
    let t = stem_targets(&w, &b, |_| false);
    assert_eq!(
        t,
        vec![
            (3, 41, 0),
            (3, 42, 0),
            (3, 43, 0),
            (3, 44, 0),
            (-5, 41, 2),
            (-5, 42, 2),
            (-5, 43, 2),
            (-5, 44, 2)
        ]
    );
    let t = stem_targets(&w, &b, |c| c.0 == 3);
    assert_eq!(t.first(), Some(&(-5, 41, 2)));
}

/// Fewer than four main slots free: granite goes before dirt; pearls and rods never.
#[test]
fn a_full_pack_drops_its_filler_never_pearls_or_rods() {
    let b = Base::default();
    let mut w = bare();
    for s in 9i16..=44 {
        give(&mut w, s, ItemKind::Netherrack, 64, None);
    }
    give(&mut w, 20, ItemKind::EnderPearl, 3, None);
    give(&mut w, 21, ItemKind::BlazeRod, 7, None);
    give(&mut w, 22, ItemKind::Dirt, 30, None);
    give(&mut w, 30, ItemKind::Granite, 12, None);
    assert_eq!(drop_slot(&w, &b), Some((30, ItemKind::Granite)));
    w.inventory.slots.remove(&30);
    assert_eq!(drop_slot(&w, &b), Some((22, ItemKind::Dirt)));
    for s in [9, 10, 11, 12] {
        w.inventory.slots.remove(&s);
    }
    assert_eq!(drop_slot(&w, &b), None, "four free");
}

/// A stem is dug by hand: a sword in hand is swapped for a slot with no wearing tool.
#[test]
fn a_sword_in_hand_is_put_away_before_a_stem() {
    let mut w = frontier();
    w.held = 0;
    assert_eq!(bare_hand(&w), Some(world::Act::Hold { slot: 3 }));
    w.held = 3;
    assert_eq!(bare_hand(&w), None);
}

fn next(id: &str, held: &[(ItemKind, i32)], table_near: bool) -> String {
    let t = table();
    let c: Counts = held.iter().copied().collect();
    let near = move |k: BlockKind| table_near && k == BlockKind::CraftingTable;
    match make_with_wear(&t, &c, &near, true, &Wear::default(), t.until(id).unwrap()) {
        Choice::Act(r) => r.id.clone(),
        Choice::Done => "done".to_owned(),
        Choice::Stuck => "stuck".to_owned(),
    }
}

/// The goal table makes the base from warped wood: planks, then sticks, then the table; a sword,
/// a pickaxe and the shield at the table.
#[test]
fn the_base_rows_make_the_kit_from_warped_wood() {
    let stems = [(ItemKind::WarpedStem, 8), (ItemKind::Stick, 3)];
    assert_eq!(
        next("base-table", &stems, false),
        "craft:warped_planks",
        "planks first"
    );
    let planks = [(ItemKind::WarpedPlanks, 4), (ItemKind::Stick, 3)];
    assert_eq!(next("base-table", &planks, false), "craft:stick");
    let sticks = [(ItemKind::WarpedPlanks, 6), (ItemKind::Stick, 7)];
    assert_eq!(next("base-table", &sticks, false), "craft:crafting_table");
    let placed = [(ItemKind::CraftingTable, 1), (ItemKind::Stick, 7)];
    assert_eq!(next("base-table", &placed, false), "place-table");
    assert_eq!(next("base-table", &placed, true), "done");
    let iron = [(ItemKind::IronIngot, 7), (ItemKind::Stick, 4)];
    assert_eq!(next("base-iron-sword", &iron, true), "craft:iron_sword");
    let stone = [(ItemKind::Cobblestone, 64), (ItemKind::Stick, 4)];
    assert_eq!(next("base-stone-sword", &stone, true), "craft:stone_sword");
    assert_eq!(
        next("base-stone-pickaxe", &stone, true),
        "craft:stone_pickaxe"
    );
    let shield = [(ItemKind::IronIngot, 1), (ItemKind::WarpedPlanks, 6)];
    assert_eq!(next("base-shield", &shield, true), "craft:shield");
}
