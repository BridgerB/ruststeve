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
    // Ten stems held: no wood step.
    let mut wood = w;
    give(&mut wood, 21, ItemKind::WarpedStem, 10, None);
    assert!(skip_why(Step::Wood, &wood, &b, false).is_some());
}

/// A hand-built forest: two stems 4 tall at the feet's level, a lone stem cell up in a cap, a stem
/// 30 off, one 60 off, one far above, nylium all round. Each column comes once, from its lowest
/// seen cell; the one beyond 48 and the one 19 over the feet never; a refused column is left out.
#[test]
fn stems_are_dug_from_each_columns_lowest_cell_within_48() {
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
    seen(&mut w, (60, 41, 0), BlockKind::WarpedStem);
    seen(&mut w, (6, 60, 0), BlockKind::WarpedStem);
    seen(&mut w, (2, 41, 2), BlockKind::WarpedWartBlock);
    let t = stem_targets(&w, &b, |_| false);
    assert_eq!(t, vec![(1, 50, 1), (3, 41, 0), (-5, 41, 2), (30, 41, 0)]);
    let t = stem_targets(&w, &b, |c| c.0 == 1);
    assert_eq!(t.first(), Some(&(3, 41, 0)));
}

/// A hand-built warped forest cavern of the model, loaded through the packets a server sends:
/// nylium on the floor at y 40, one huge fungus at the origin (a stem to y 50, a wart cap at y 51
/// from -3 to 3), the bot standing on the cap at `spawn`.
fn cap_world(spawn: (i32, i32, i32)) -> World {
    use protocol::S2c;
    use sim::nether::{NETHER, Stage, nether_type};
    use world::Event;
    let mut stage = Stage::new(2, 40, 70);
    stage
        .fill((-32, 47), (40, 40), (-32, 47), BlockKind::WarpedNylium)
        .fill((0, 0), (41, 50), (0, 0), BlockKind::WarpedStem)
        .fill((-3, 3), (51, 51), (-3, 3), BlockKind::WarpedWartBlock)
        .spawn(spawn);
    let model = stage.build().unwrap();
    let mut w = World::new();
    w.step(&Event::Packet(S2c::Login {
        entity_id: 1,
        view_distance: 8,
        simulation_distance: 3,
        dimension_type: nether_type(),
        dimension: NETHER.to_owned(),
        game_type: 0,
    }));
    for ((x, z), c) in &model.columns {
        w.step(&Event::Packet(S2c::LevelChunkWithLight {
            x: *x,
            z: *z,
            column: c.column.clone(),
        }));
    }
    let [x, y, z] = model.spawn;
    w.step(&Event::Packet(S2c::PlayerPosition {
        teleport_id: 1,
        x,
        y,
        z,
        yaw: 0.0,
        pitch: 0.0,
        flags: 0,
    }));
    w.body.get_or_insert(physics::Body {
        pos: [x, y, z],
        ..physics::Body::default()
    });
    w
}

/// Race 45537373222 lanes 0 and 2: a bot on a fungus cap is up in the cap. The floor to walk to
/// is seen nylium 8 to 24 off with room over it; over the stem the cap block underfoot is dug
/// (the stem under it is a floor), and off the stem, over the cap's hollow (a fall of 10), never.
#[test]
fn a_bot_on_a_fungus_cap_goes_down_to_the_floor() {
    let b = Base::default();
    let w = cap_world((0, 52, 0));
    assert!(
        base::on_cap(&w),
        "on the wart: {:?}",
        w.block_under_player()
    );
    let floor = base::floor_targets(&w, &b, |_| false);
    assert!(!floor.is_empty());
    for c in &floor {
        assert_eq!(c.1, 41, "over the nylium: {c:?}");
        let d2 = c.0 * c.0 + c.2 * c.2;
        assert!((64..=576).contains(&d2), "8 to 24 off: {c:?}");
    }
    assert_eq!(
        base::dig_down(&w),
        Some((0, 51, 0)),
        "the stem under it holds the bot"
    );
    let hollow = cap_world((2, 52, 2));
    assert!(base::on_cap(&hollow));
    assert_eq!(
        base::dig_down(&hollow),
        None,
        "a fall of 10 is never dug into"
    );
    let floor = cap_world((10, 41, 10));
    assert!(
        !base::on_cap(&floor),
        "on nylium: {:?}",
        floor.block_under_player()
    );
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
    let picks = [(ItemKind::WarpedPlanks, 3), (ItemKind::Stick, 2)];
    assert_eq!(
        next("base-wood-pickaxe", &picks, true),
        "craft:wooden_pickaxe"
    );
}

fn noted(p: &plan::Plan, what: &str) -> usize {
    p.events.iter().filter(|e| e.what.contains(what)).count()
}

/// Race 45542558792: walled in with no stem in view, every bot waited 2,400 ticks a step and no
/// hunt began. Now the wait pauses the base after 200 ticks (`None`: the hunt runs); after the
/// pause the wood looks again, and a second long wait gives it up, which skips at once every step
/// the wood was to supply, with one note, and ends the base.
#[test]
fn no_wood_pauses_the_base_then_skips_what_wood_makes() {
    let mut p = plan::Plan::with_table(plan::Aim::Race, std::sync::Arc::new(table()));
    let mut w = frontier();
    w.held = 3;
    assert_eq!(p.base_task(&w, 0), Some(plan::Task::Idle), "the wood waits");
    assert_eq!(p.base_task(&w, 100), Some(plan::Task::Idle));
    assert_eq!(p.base_task(&w, 200), None, "paused: the hunt runs");
    assert_eq!(
        noted(&p, "base paused: the hunt goes on with what is held"),
        1
    );
    assert_eq!(p.base_task(&w, 3000), None, "still paused");
    assert_eq!(
        p.base_task(&w, 6200),
        Some(plan::Task::Idle),
        "the wood looks again"
    );
    assert_eq!(noted(&p, "base resumed"), 1);
    assert_eq!(p.base_task(&w, 6400), None);
    assert!(p.base.over, "{:?}", p.events);
    assert_eq!(noted(&p, "base: wood gave up"), 1);
    assert_eq!(
        noted(&p, "base: no wood: the hunt goes on with what is held"),
        1
    );
    assert_eq!(p.base.ended.len(), STEPS.len(), "{:?}", p.base.ended);
    assert_eq!(noted(&p, "base: table begun"), 0, "never waited on");
    assert_eq!(p.base_task(&w, 6420), None);
}

/// A kill resumes a paused base at once (`base_resume`, the hunt's call).
#[test]
fn a_kill_resumes_a_paused_base() {
    let mut p = plan::Plan::with_table(plan::Aim::Race, std::sync::Arc::new(table()));
    let mut w = frontier();
    w.held = 3;
    p.base_task(&w, 0);
    assert_eq!(p.base_task(&w, 200), None);
    p.base_resume();
    assert_eq!(p.base_task(&w, 260), Some(plan::Task::Idle));
    assert!(!p.base.over);
}

/// The hunt's filler pickaxe (P3): a wooden one, the most worn first, never a stone one; wooden
/// uses count apart from the stone reserve, and three wooden pickaxes end the wood picks step.
#[test]
fn the_spare_pick_is_a_wooden_one_and_never_the_reserve() {
    let b = Base::default();
    let mut w = frontier();
    assert_eq!(base::spare_pick(&w), None, "stone pickaxes are the reserve");
    assert!(skip_why(Step::WoodPicks, &w, &b, true).is_none());
    give(&mut w, 15, ItemKind::WoodenPickaxe, 1, Some(59));
    give(&mut w, 16, ItemKind::WoodenPickaxe, 1, Some(20));
    assert_eq!(base::spare_pick(&w), Some(16), "the most worn first");
    assert_eq!(base::wood_pick_uses(&w), 79);
    assert_eq!(
        base::reserve_pick_uses(&w),
        144,
        "wood does not count in the reserve"
    );
    give(&mut w, 17, ItemKind::WoodenPickaxe, 1, Some(59));
    give(&mut w, 18, ItemKind::WoodenPickaxe, 1, Some(59));
    assert!(
        skip_why(Step::WoodPicks, &w, &b, true).is_some(),
        "197 uses held"
    );
}
