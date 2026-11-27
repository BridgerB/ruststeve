//! Issue 20e (agent E3, docs/problems/end-island.md), on hand-built End worlds: the void as an
//! edge, the look kept down, the pillar and the bridge from the platform with guards on both
//! sides, the wait for a flying dragon, the island reached, the bunker dug, the stock's spiral.

use std::sync::Arc;

use data::{BlockKind, EntityKind, ItemKind};
use plan::end::{self, BRIDGE_FLOOR, end_look, stock_dir, void_column, void_edge};
use plan::look::ENDERMAN_EYE;
use plan::{Aim, Plan, make};
use protocol::S2c;
use protocol::chunk::{Column, Paletted, Section};
use protocol::metadata::MetaValue;
use world::{Act, Event, World};

fn state(k: BlockKind) -> u32 {
    k.info().map(|i| i.default_state).unwrap_or(0)
}

fn section(k: BlockKind) -> Section {
    Section {
        block_count: if k == BlockKind::Air { 0 } else { 4096 },
        fluid_count: 0,
        blocks: Paletted::Single(state(k)),
        biomes: Paletted::Single(0),
    }
}

/// An End world, 16 sections a column (y 0 to 255): end stone in the lowest `solid` sections
/// (its top at y 16 x solid - 1), air over them, chunk columns x `cx` and z -2 to 2; the feet at
/// `pos`.
fn world(solid: usize, cx: std::ops::RangeInclusive<i32>, pos: [f64; 3]) -> World {
    let mut sections = vec![section(BlockKind::EndStone); solid];
    sections.extend(std::iter::repeat_n(section(BlockKind::Air), 16 - solid));
    let col = Arc::new(Column { min_y: 0, sections });
    let mut w = World::new();
    w.step(&Event::Packet(S2c::Login {
        entity_id: 1,
        view_distance: 8,
        simulation_distance: 3,
        dimension_type: 2,
        dimension: end::END.to_owned(),
        game_type: 0,
    }));
    for x in cx {
        for z in -2..=2 {
            w.step(&Event::Packet(S2c::LevelChunkWithLight {
                x,
                z,
                column: Arc::clone(&col),
            }));
        }
    }
    w.body = Some(physics::Body {
        pos,
        on_ground: true,
        ..physics::Body::default()
    });
    w.player.pos = Some(pos);
    w.player.health = Some(20.0);
    w.player.food = Some(20);
    // The bunker and the stock tunnels come after the crystals (the coordinator's decision after
    // frontier race 45978279345): these worlds are past them unless a test says otherwise.
    w.stage_reached = Some(world::stage::Stage::Crystals);
    w.inventory.slots.insert(36, (ItemKind::Cobblestone, 64));
    w.inventory.slots.insert(37, (ItemKind::Cobblestone, 64));
    w.inventory.slots.insert(38, (ItemKind::StonePickaxe, 1));
    w.inventory.slots.insert(39, (ItemKind::CookedBeef, 32));
    w.held = 0;
    w
}

fn set(w: &mut World, c: (i32, i32, i32), k: BlockKind) {
    w.step(&Event::Packet(S2c::BlockUpdate {
        pos: c,
        state: state(k),
    }));
}

/// The arrival platform over the void: 5x5 obsidian at y 48 (E1's measure of this seed).
fn platform(pos: [f64; 3]) -> World {
    let mut w = world(0, 4..=7, pos);
    for x in 98..=102 {
        for z in -2..=2 {
            set(&mut w, (x, 48, z), BlockKind::Obsidian);
        }
    }
    w
}

fn with_entity(w: &mut World, id: i32, kind: EntityKind, at: [f64; 3]) {
    w.step(&Event::Packet(S2c::AddEntity {
        id,
        uuid: [0; 16],
        kind: kind.id(),
        x: at[0],
        y: at[1],
        z: at[2],
        velocity: [0.0; 3],
    }));
}

fn table() -> Arc<make::Table> {
    let text = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../config/goals.toml"
    ))
    .unwrap_or_default();
    let goals: make::Goals = toml::from_str(&text).unwrap_or_else(|e| panic!("goals.toml: {e}"));
    Arc::new(make::Table::new(&goals).unwrap_or_else(|e| panic!("table: {e}")))
}

fn racer() -> Plan {
    Plan::with_table(Aim::Race, table())
}

fn noted(p: &Plan, what: &str) -> bool {
    p.events.iter().any(|e| e.what.starts_with(what))
}

fn notes(p: &Plan) -> Vec<String> {
    p.events.iter().map(|e| e.what.clone()).collect()
}

#[test]
fn the_void_beside_the_platform_is_an_edge_and_ground_is_not() {
    let w = platform([100.5, 49.0, 0.5]);
    // The platform's west edge: (97, 48, 0) and everything under it is air to y 0.
    assert!(void_column(&w, (97, 48, 0)));
    assert!(void_edge(&w, (98, 49, 0)));
    // Its middle has obsidian on every side.
    assert!(!void_edge(&w, (100, 49, 0)));
    // Ground anywhere under the cell beside: no void.
    let mut w = platform([100.5, 49.0, 0.5]);
    set(&mut w, (97, 3, 0), BlockKind::EndStone);
    assert!(!void_column(&w, (97, 48, 0)));
    // Not loaded under it: not a known void.
    let w = world(0, 6..=6, [100.5, 49.0, 0.5]);
    assert!(!void_column(&w, (200, 48, 0)));
}

#[test]
fn a_look_at_an_endermans_eyes_is_turned_down() {
    let mut w = world(4, 0..=1, [8.5, 64.0, 8.5]);
    let at = [18.5, 64.0, 8.5];
    with_entity(&mut w, 41, EntityKind::Enderman, at);
    let eye = [8.5, 64.0 + physics::EYE_HEIGHT, 8.5];
    let (yaw, pitch) = plan::walk::look_at(eye, [at[0], at[1] + ENDERMAN_EYE, at[2]]);
    #[allow(clippy::cast_possible_truncation)]
    let want = (yaw as f32, pitch);
    assert_eq!(end_look(&w, Some(want)), (want.0, 90.0));
    // The other way it is sent as asked.
    let away = (want.0 + 180.0, 0.0);
    assert_eq!(end_look(&w, Some(away)), away);
    // No look asked for: straight down.
    assert_eq!(end_look(&w, None).1, 90.0);
}

#[test]
fn in_the_end_a_walk_looks_down() {
    let w = platform([100.5, 49.0, 0.5]);
    let mut p = racer();
    let c = p.step(&w);
    assert_eq!(c.pitch, Some(90.0), "{:?}", notes(&p));
}

#[test]
fn on_the_platform_the_bot_pillars_up_to_the_bridges_height() {
    let w = platform([100.5, 49.0, 0.5]);
    let mut p = racer();
    p.step(&w);
    assert!(noted(&p, "end: arrived at (100, 49, 0)"), "{:?}", notes(&p));
    assert!(
        noted(&p, &format!("end: pillar up {} at", BRIDGE_FLOOR + 1 - 49)),
        "{:?}",
        notes(&p)
    );
    // Centred and holding a block, it jumps.
    let jumped = (0..4).any(|_| p.step(&w).jump);
    assert!(jumped, "{:?}", notes(&p));
}

/// The bot on its pillar at the bridge's height, the column under it of cobblestone.
fn on_the_pillar() -> World {
    let mut w = platform([100.5, f64::from(BRIDGE_FLOOR + 1), 0.5]);
    for y in 49..=BRIDGE_FLOOR {
        set(&mut w, (100, y, 0), BlockKind::Cobblestone);
    }
    w
}

#[test]
fn the_bridge_lays_a_guard_first_then_the_floor_ahead() {
    let w = on_the_pillar();
    let mut p = racer();
    p.step(&w);
    assert!(noted(&p, "end: bridge west from"), "{:?}", notes(&p));
    let mut acts = Vec::new();
    for _ in 0..10 {
        p.step(&w);
        acts.extend(p.take_acts());
    }
    assert!(noted(&p, "end: guard into"), "{:?}", notes(&p));
    assert!(
        acts.iter().any(|a| matches!(a, Act::UseOn { .. })),
        "{acts:?}"
    );
    // With both sides guarded, the floor ahead (99, 52, 0) is laid against the block stood on.
    let mut w = on_the_pillar();
    for z in [-1, 1] {
        set(&mut w, (100, BRIDGE_FLOOR, z), BlockKind::Cobblestone);
        set(&mut w, (100, BRIDGE_FLOOR + 1, z), BlockKind::Cobblestone);
    }
    let mut p = racer();
    let mut acts = Vec::new();
    for _ in 0..10 {
        p.step(&w);
        acts.extend(p.take_acts());
    }
    assert!(
        noted(
            &p,
            &format!("end: bridge block into (99, {BRIDGE_FLOOR}, 0)")
        ),
        "{:?}",
        notes(&p)
    );
    assert!(
        acts.contains(&Act::UseOn {
            pos: world::BlockPos::new(100, BRIDGE_FLOOR, 0),
            face: 4
        }),
        "{acts:?}"
    );
}

#[test]
fn a_flying_dragon_near_holds_the_bridge_crouched() {
    let mut w = on_the_pillar();
    with_entity(&mut w, 7, EntityKind::EnderDragon, [80.0, 70.0, 0.0]);
    if let Some(e) = w.entities.get_mut(&7) {
        // Phase 0, the holding pattern: flying.
        e.metadata.insert(16, MetaValue::Int(0));
    }
    let mut p = racer();
    p.step(&w);
    let c = p.step(&w);
    assert!(
        noted(&p, "end: bridge waits for the dragon"),
        "{:?}",
        notes(&p)
    );
    assert!(c.sneak && !c.forward, "{c:?}");
    // Perched (phase 6), the bridge goes on.
    if let Some(e) = w.entities.get_mut(&7) {
        e.metadata.insert(16, MetaValue::Int(6));
    }
    p.step(&w);
    assert!(noted(&p, "end: bridge goes on"), "{:?}", notes(&p));
}

#[test]
fn end_stone_under_the_sky_is_the_island_and_the_bot_walks_to_the_bunker_site() {
    // Flat end stone to y 63 from x 0 to 127; the feet at x 60.
    let w = world(4, 0..=7, [60.5, 64.0, 0.5]);
    let mut p = racer();
    p.step(&w);
    assert!(
        noted(&p, "end: on the island at (60, 64, 0)"),
        "{:?}",
        notes(&p)
    );
    assert!(
        noted(&p, "end: walk to the bunker site (12, 64, 0)"),
        "{:?}",
        notes(&p)
    );
}

#[test]
fn at_the_site_the_bunker_begins_with_its_post() {
    let mut w = world(4, 0..=7, [12.5, 64.0, 0.5]);
    // The block in hand (a hand-built world does not answer the hold's swap): the post first.
    w.held = 0;
    let mut p = racer();
    p.end.bridged = true;
    let mut acts = Vec::new();
    for _ in 0..5 {
        p.step(&w);
        acts.extend(p.take_acts());
    }
    assert!(
        noted(&p, "end: dig the bunker at (12, 64, 0)"),
        "{:?}",
        notes(&p)
    );
    // The post first: a block on the ground beside the shaft's top cell.
    assert!(
        acts.contains(&Act::UseOn {
            pos: world::BlockPos::new(13, 63, 0),
            face: 1
        }),
        "{acts:?}"
    );
}

#[test]
fn the_bunker_shaft_is_roofed_after_every_dig() {
    use plan::end::{Op, shaft_ops};
    let ops = shaft_ops((12, 64, 0), (13, 63, 0));
    // The post of two on the ground block beside the top cell.
    assert!(ops.contains(&Op::Place {
        cell: (13, 64, 0),
        against: (13, 63, 0),
        face: 1
    }));
    assert!(ops.contains(&Op::Place {
        cell: (13, 65, 0),
        against: (13, 64, 0),
        face: 1
    }));
    // Every dig under the feet is followed by the fall and the cell over the new head.
    let digs: Vec<usize> = ops
        .iter()
        .enumerate()
        .filter(|(_, o)| matches!(o, Op::Dig(_)))
        .map(|(i, _)| i)
        .collect();
    assert_eq!(digs.len(), 4);
    for i in digs {
        let Some(Op::Dig(c)) = ops.get(i) else {
            panic!("dig");
        };
        assert_eq!(ops.get(i + 1), Some(&Op::Settle));
        // The feet fall into c: the head is c + 1, the cell over it c + 2.
        assert_eq!(ops.get(i + 2), Some(&Op::Roof((c.0, c.1 + 2, c.2))));
    }
    // The bunker's feet four down at (12, 60, 0), the roof's top cells laid.
    assert!(ops.contains(&Op::Dig((12, 60, 0))));
    assert!(ops.contains(&Op::Roof((12, 62, 0))));
}

#[test]
fn a_flying_dragon_near_holds_the_bunker_shaft() {
    let mut w = world(4, 0..=7, [12.5, 64.0, 0.5]);
    with_entity(&mut w, 7, EntityKind::EnderDragon, [30.0, 80.0, 0.0]);
    if let Some(e) = w.entities.get_mut(&7) {
        e.metadata.insert(16, MetaValue::Int(1));
    }
    let mut p = racer();
    p.end.bridged = true;
    p.step(&w);
    assert!(
        noted(&p, "end: the bunker waits for the dragon"),
        "{:?}",
        notes(&p)
    );
    assert!(!noted(&p, "end: dig the bunker"), "{:?}", notes(&p));
}

#[test]
fn every_stock_run_begins_and_ends_at_the_bunker() {
    // The bunker at (12, 61, 0), sealed; the stock begins with the stairs down.
    let mut w = world(4, 0..=7, [12.5, 61.0, 0.5]);
    set(&mut w, (12, 61, 0), BlockKind::Air);
    set(&mut w, (12, 62, 0), BlockKind::Air);
    let mut p = racer();
    p.end.bridged = true;
    p.bunker = Some([12, 61, 0]);
    p.end.door = Some((1, 0));
    p.step(&w);
    assert!(noted(&p, "end: stock run 1 toward"), "{:?}", notes(&p));
    assert_eq!(p.end.stock_out, Some(plan::end::StockRun::Down));
    // The run over at its foot (12, 60, 1) (the stairs run along z, off the door), the way back
    // open: walked home.
    p.task = plan::Task::Idle;
    for c in [(12, 60, 1), (12, 61, 1), (12, 62, 1)] {
        set(&mut w, c, BlockKind::Air);
    }
    w.body = Some(physics::Body {
        pos: [12.5, 60.0, 1.5],
        on_ground: true,
        ..physics::Body::default()
    });
    p.step(&w);
    assert!(
        noted(&p, "end: stock run 1 over at (12, 60, 1)"),
        "{:?}",
        notes(&p)
    );
    assert!(
        noted(&p, "end: stock back to the bunker"),
        "{:?}",
        notes(&p)
    );
    // One under the bunker is no foot (the arms would run beside its stair): the stairs down are
    // begun again.
    assert_eq!(p.end.stock_foot, None);
    assert!(
        noted(&p, "end: the stairs down cut short at (12, 60, 1)"),
        "{:?}",
        notes(&p)
    );
    // Home again: counted, and the next run is walked out to from the bunker.
    p.task = plan::Task::Idle;
    w.body = Some(physics::Body {
        pos: [12.5, 61.0, 0.5],
        on_ground: true,
        ..physics::Body::default()
    });
    p.step(&w);
    assert!(
        noted(&p, "end: back at the bunker after stock run 1"),
        "{:?}",
        notes(&p)
    );
    assert_eq!(p.end.stock_back, 1);
    assert!(noted(&p, "end: stock run 2 toward"), "{:?}", notes(&p));
    assert_eq!(p.end.stock_out, Some(plan::end::StockRun::Down));
}

#[test]
fn in_the_sealed_bunker_short_of_stone_a_stock_tunnel_begins() {
    // The bunker at (12, 61, 0): its cells open, the roof at y 63 in place.
    let mut w = world(4, 0..=7, [12.5, 61.0, 0.5]);
    set(&mut w, (12, 61, 0), BlockKind::Air);
    set(&mut w, (12, 62, 0), BlockKind::Air);
    let mut p = racer();
    p.end.bridged = true;
    p.bunker = Some([12, 61, 0]);
    p.end.door = Some((1, 0));
    p.step(&w);
    assert!(noted(&p, "end: stock run 1 toward"), "{:?}", notes(&p));
    assert_eq!(p.task.name(), "stairs");
}

#[test]
fn the_stock_spirals_out_from_the_door() {
    let door = (1, 0);
    let dirs: Vec<(i32, i32)> = (0..6).map(|n| stock_dir(door, n)).collect();
    // Legs of 1, 1, 2, 2 runs, a quarter turn each, never first toward the door.
    assert_eq!(
        dirs,
        vec![(0, 1), (-1, 0), (0, -1), (0, -1), (1, 0), (1, 0)]
    );
}

#[test]
fn in_a_roofed_tunnel_an_angry_enderman_seals_no_pocket() {
    // The bunker at (12, 61, 0) and a tunnel cell beside it, under the end stone's top at 63.
    let mut w = world(4, 0..=7, [12.5, 61.0, 0.5]);
    for c in [(12, 61, 0), (12, 62, 0), (12, 61, 1), (12, 62, 1)] {
        set(&mut w, c, BlockKind::Air);
    }
    with_entity(&mut w, 9, EntityKind::Enderman, [16.5, 64.0, 0.5]);
    if let Some(e) = w.entities.get_mut(&9) {
        e.metadata.insert(17, MetaValue::Bool(true));
    }
    assert!(end::covered(&w));
    let mut p = racer();
    p.end.bridged = true;
    p.bunker = Some([12, 61, 0]);
    p.end.door = Some((1, 0));
    p.step(&w);
    assert!(
        noted(&p, "end: covered from an angry enderman"),
        "{:?}",
        notes(&p)
    );
    assert!(!noted(&p, "provoked enderman"), "{:?}", notes(&p));
    // On the open surface it is the issue 19 pocket as before.
    let mut w = world(4, 0..=7, [12.5, 64.0, 0.5]);
    with_entity(&mut w, 9, EntityKind::Enderman, [16.5, 64.0, 0.5]);
    if let Some(e) = w.entities.get_mut(&9) {
        e.metadata.insert(17, MetaValue::Bool(true));
    }
    assert!(!end::covered(&w));
}

#[test]
fn up_a_pillar_with_no_crystal_left_the_crystals_task_brings_the_bot_down() {
    // E6's closed loop: the last crystal gone, the bot stayed 18 up on its pillar.
    let mut w = world(4, 0..=7, [20.5, 74.0, 0.5]);
    for y in 64..=73 {
        set(&mut w, (20, y, 0), BlockKind::EndStone);
    }
    for c in [(12, 61, 0), (12, 62, 0)] {
        set(&mut w, c, BlockKind::Air);
    }
    let mut p = racer();
    p.end.bridged = true;
    p.end.stock_over = true;
    p.bunker = Some([12, 61, 0]);
    p.end.door = Some((1, 0));
    p.spikes.pillars.push(plan::spikes::PillarRec {
        x: 20,
        z: 0,
        base: 63,
    });
    for _ in 0..3 {
        p.step(&w);
    }
    assert!(
        noted(&p, "end: up a pillar at (20, 74, 0)"),
        "{:?}",
        notes(&p)
    );
    assert!(!noted(&p, "end: no walk to the bunker"), "{:?}", notes(&p));
    assert!(!noted(&p, "end: back to the bunker"), "{:?}", notes(&p));
}

#[test]
fn hurt_the_walk_to_the_bunker_waits_while_a_cloud_lies_on_the_way() {
    // The bunker at (12, 61, 0), its shaft open to the surface; the bot 18 east at 10 health.
    let mut w = world(4, 0..=7, [30.5, 64.0, 0.5]);
    for y in 61..=63 {
        set(&mut w, (12, y, 0), BlockKind::Air);
    }
    w.player.health = Some(10.0);
    with_entity(&mut w, 11, EntityKind::AreaEffectCloud, [21.5, 64.0, 0.5]);
    if let Some(e) = w.entities.get_mut(&11) {
        e.metadata.insert(8, MetaValue::Float(3.0));
    }
    let mut p = racer();
    p.end.bridged = true;
    p.bunker = Some([12, 61, 0]);
    p.end.door = Some((1, 0));
    p.step(&w);
    assert!(noted(&p, "end: retreat to the bunker"), "{:?}", notes(&p));
    assert!(
        noted(&p, "end: the way to the bunker waits for a cloud"),
        "{:?}",
        notes(&p)
    );
    assert_eq!(p.task.name(), "idle");
    // The cloud gone, the walk begins.
    w.entities.remove(&11);
    p.retry_at = 0;
    p.step(&w);
    assert!(noted(&p, "end: back to the bunker"), "{:?}", notes(&p));
}

#[test]
fn deep_in_the_tunnels_with_no_way_back_the_bot_cuts_stairs_up_to_hand_over() {
    // The stock over at y 52, the bunker at (12, 59, 0) out of reach (its cells shut).
    let mut w = world(4, 0..=7, [30.5, 52.0, 0.5]);
    for c in [(30, 52, 0), (30, 53, 0), (29, 52, 0), (29, 53, 0)] {
        set(&mut w, c, BlockKind::Air);
    }
    with_entity(&mut w, 12, EntityKind::EndCrystal, [42.5, 83.0, 0.5]);
    let mut p = racer();
    p.end.bridged = true;
    p.end.stock_over = true;
    p.bunker = Some([12, 59, 0]);
    p.end.door = Some((1, 0));
    p.step(&w);
    assert!(
        noted(&p, "end: stairs up out of the tunnels from (30, 52, 0)"),
        "{:?}",
        notes(&p)
    );
    assert_eq!(p.task.name(), "stairs", "{:?}", notes(&p));
}

#[test]
fn on_a_laid_block_within_reach_of_the_fountain_the_bunker_is_dug_there() {
    // Race 45888713362 lane 0: 25 from the fountain on a cobblestone block, ten site walks done.
    let mut w = world(4, 0..=7, [23.5, 64.0, -7.5]);
    set(&mut w, (23, 63, -8), BlockKind::Cobblestone);
    let mut p = racer();
    p.end.bridged = true;
    p.end.site_walks = plan::end::SITE_WALKS;
    p.step(&w);
    assert!(
        noted(&p, "end: dig the bunker at (23, 64, -8)"),
        "{:?}",
        notes(&p)
    );
}

#[test]
fn the_bunker_opened_the_bot_walks_up_its_stair_onto_the_ground() {
    // Race 45890445738: both bots stood half on the stair's first step for 14,000 ticks.
    let mut w = world(4, 0..=7, [12.5, 60.0, 0.5]);
    for c in [
        (12, 60, 0),
        (12, 61, 0),
        (12, 62, 0),
        (11, 61, 0),
        (11, 62, 0),
        (11, 63, 0),
        (10, 62, 0),
        (10, 63, 0),
        (9, 63, 0),
    ] {
        set(&mut w, c, BlockKind::Air);
    }
    with_entity(&mut w, 12, EntityKind::EndCrystal, [42.5, 83.0, 0.5]);
    let mut p = racer();
    p.end.bridged = true;
    p.end.stock_over = true;
    p.bunker = Some([12, 60, 0]);
    p.end.door = Some((-1, 0));
    p.end.exit_walk = true;
    p.step(&w);
    assert!(
        noted(
            &p,
            "end: up the bunker's stair by [(11, 61, 0), (10, 62, 0), (9, 63, 0)]"
        ),
        "{:?}",
        notes(&p)
    );
    // Steered at the first step with a jump.
    let c = p.step(&w);
    assert!(
        c.forward && c.jump,
        "{c:?} {:?} {:?}",
        notes(&p),
        p.end.work
    );
    // On it (race 45898928805 lane 0 stood here, half over the bunker, with no route found): on
    // to the next step.
    w.body = Some(physics::Body {
        pos: [11.5, 61.0, 0.5],
        on_ground: true,
        ..physics::Body::default()
    });
    p.step(&w);
    let c = p.step(&w);
    assert!(c.forward && c.jump, "{c:?}");
}

#[test]
fn from_the_first_end_tick_every_look_is_straight_down() {
    // E5's race 45886819862 lane 1: the arrival's pitch 0 stared at an enderman at tick 60.
    let mut w = platform([100.5, 49.0, 0.5]);
    w.player.pitch = 0.0;
    with_entity(&mut w, 41, EntityKind::Enderman, [90.5, 49.0, 0.5]);
    let mut p = racer();
    // No body yet (the arrival's teleport not answered): down.
    let body = w.body.take();
    assert_eq!(p.step(&w).pitch, Some(90.0));
    w.body = body;
    assert_eq!(p.step(&w).pitch, Some(90.0));
}

#[test]
fn a_pocket_is_built_from_end_stone_when_that_is_all_that_is_held() {
    // E5's race 45886819862 lane 1: "no block held for a pocket" with 192 end stone. Since E5's
    // wall 15 no pocket is sealed in the End at all (`dragon::Plan::end_enderman_step`).
    let mut w = world(4, 0..=7, [12.5, 64.0, 0.5]);
    w.inventory.slots.clear();
    w.inventory.slots.insert(36, (ItemKind::EndStone, 64));
    w.held = 0;
    with_entity(&mut w, 9, EntityKind::Enderman, [16.5, 64.0, 0.5]);
    if let Some(e) = w.entities.get_mut(&9) {
        e.metadata.insert(17, MetaValue::Bool(true));
    }
    let mut p = racer();
    p.end.bridged = true;
    for _ in 0..3 {
        p.step(&w);
    }
    assert!(!noted(&p, "pocket block 1"), "{:?}", notes(&p));
    assert!(!noted(&p, "no block held"), "{:?}", notes(&p));
    assert!(
        noted(&p, "end: no pocket for enderman 9"),
        "{:?}",
        notes(&p)
    );
}

#[test]
fn on_the_bunkers_stair_an_angry_enderman_seals_no_pocket() {
    // Race 45898928805: a pocket laid in the stair shut the bunker's own door.
    let mut w = world(4, 0..=7, [11.5, 61.0, 0.5]);
    for c in [
        (12, 60, 0),
        (12, 61, 0),
        (12, 62, 0),
        (11, 61, 0),
        (11, 62, 0),
        (11, 63, 0),
    ] {
        set(&mut w, c, BlockKind::Air);
    }
    with_entity(&mut w, 9, EntityKind::Enderman, [16.5, 64.0, 0.5]);
    if let Some(e) = w.entities.get_mut(&9) {
        e.metadata.insert(17, MetaValue::Bool(true));
    }
    let mut p = racer();
    p.end.bridged = true;
    p.bunker = Some([12, 60, 0]);
    p.end.door = Some((-1, 0));
    assert!(p.at_bunker(&w));
    p.step(&w);
    assert!(!noted(&p, "provoked enderman"), "{:?}", notes(&p));
    assert!(
        noted(&p, "end: covered from an angry enderman"),
        "{:?}",
        notes(&p)
    );
}

#[test]
fn a_shut_step_of_the_bunkers_stair_is_dug_out_on_the_way_up() {
    // The bunker opened, its stair's second step (10, 62, 0) shut by a block laid there.
    let mut w = world(4, 0..=7, [12.5, 60.0, 0.5]);
    for c in [
        (12, 60, 0),
        (12, 61, 0),
        (12, 62, 0),
        (11, 61, 0),
        (11, 62, 0),
        (11, 63, 0),
        (10, 63, 0),
        (9, 63, 0),
    ] {
        set(&mut w, c, BlockKind::Air);
    }
    with_entity(&mut w, 12, EntityKind::EndCrystal, [42.5, 83.0, 0.5]);
    w.held = 2;
    let mut p = racer();
    p.end.bridged = true;
    p.end.stock_over = true;
    p.bunker = Some([12, 60, 0]);
    p.end.door = Some((-1, 0));
    p.end.exit_walk = true;
    p.step(&w);
    // The way up: each step's cells dug before it is stepped onto, the shut one among them.
    let ops = p
        .end
        .work
        .as_ref()
        .map(|wk| wk.ops.clone())
        .unwrap_or_default();
    let dig = ops
        .iter()
        .position(|o| *o == plan::end::Op::Dig((10, 62, 0)))
        .unwrap_or_else(|| panic!("no dig of the shut step: {ops:?}"));
    // Dug before any step onto it: by the stair dug again (wall 20) or by the steered exit.
    if let Some(step) = ops
        .iter()
        .position(|o| *o == plan::end::Op::Step((10, 62, 0)))
    {
        assert!(dig < step, "{ops:?}");
    }
    // And the room over the head of the step below.
    assert!(ops.contains(&plan::end::Op::Dig((11, 63, 0))), "{ops:?}");
}

#[test]
fn the_bunker_is_not_left_while_a_fireball_or_cloud_lies_by_its_stair() {
    // Race 45910659261 lane 1: a fireball on the stair, its cloud killed the bot in 4 s.
    let mut w = world(4, 0..=7, [12.5, 60.0, 0.5]);
    for c in [(12, 60, 0), (12, 61, 0)] {
        set(&mut w, c, BlockKind::Air);
    }
    with_entity(&mut w, 12, EntityKind::EndCrystal, [42.5, 83.0, 0.5]);
    // The stair's top is (9, 64, 0) (door west): a cloud 4 off it.
    with_entity(&mut w, 13, EntityKind::AreaEffectCloud, [6.5, 64.0, 0.5]);
    let mut p = racer();
    p.end.bridged = true;
    p.end.stock_over = true;
    p.bunker = Some([12, 60, 0]);
    p.end.door = Some((-1, 0));
    p.step(&w);
    assert!(
        noted(
            &p,
            "end: the bunker stays shut for a AreaEffectCloud near its stair"
        ),
        "{:?}",
        notes(&p)
    );
    assert!(!noted(&p, "end: open the bunker"), "{:?}", notes(&p));
    // The cloud gone: opened.
    w.entities.remove(&13);
    p.retry_at = 0;
    p.step(&w);
    assert!(noted(&p, "end: open the bunker"), "{:?}", notes(&p));
}

#[test]
fn down_a_notch_under_the_bunker_the_bot_climbs_back_into_it() {
    // Race 45918666403 lane 2: a breath notch dug to (14, 56, 0) under the bunker at (14, 58, 0).
    let mut w = world(4, 0..=7, [12.5, 58.0, 0.5]);
    for y in 58..=61 {
        set(&mut w, (12, y, 0), BlockKind::Air);
    }
    let mut p = racer();
    p.end.bridged = true;
    p.end.stock_over = true;
    p.bunker = Some([12, 60, 0]);
    p.end.door = Some((-1, 0));
    p.step(&w);
    assert!(
        noted(
            &p,
            "end: up out of the notch to the bunker from (12, 58, 0)"
        ),
        "{:?}",
        notes(&p)
    );
}

#[test]
fn an_open_with_fire_coming_by_the_stair_is_cut_and_the_bunker_sealed() {
    let mut w = world(4, 0..=7, [12.5, 60.0, 0.5]);
    for c in [(12, 60, 0), (12, 61, 0), (12, 62, 0)] {
        set(&mut w, c, BlockKind::Air);
    }
    with_entity(&mut w, 13, EntityKind::AreaEffectCloud, [8.5, 64.0, 0.5]);
    let mut p = racer();
    p.end.bridged = true;
    p.end.stock_over = true;
    p.bunker = Some([12, 60, 0]);
    p.end.door = Some((-1, 0));
    p.end.work = Some(plan::end::Work::new(
        plan::end::WorkKind::Open,
        vec![plan::end::Op::Dig((11, 61, 0))],
        0,
    ));
    p.step(&w);
    // The fireball in flight seals the bunker (wall 14) before the fire cut (wall 13) is needed:
    // either way the open is cut and the bunker sealed.
    assert!(
        noted(&p, "end: the open cut for fire by the stair")
            || noted(&p, "end: the bunker sealed for a DragonFireball"),
        "{:?}",
        notes(&p)
    );
    assert!(noted(&p, "end: seal the bunker"), "{:?}", notes(&p));
    assert!(
        p.end
            .work
            .as_ref()
            .is_none_or(|wk| wk.kind != plan::end::WorkKind::Open),
        "{:?}",
        p.end.work
    );
}

#[test]
fn in_an_open_bunker_neither_a_strafe_nor_a_fireball_in_flight_seals_it() {
    // Frontier race 45931225605 lane 1: the fireball aimed at the bot came into its open bunker.
    let open_bunker = || {
        let mut w = world(4, 0..=7, [12.5, 60.0, 0.5]);
        for c in [
            (12, 60, 0),
            (12, 61, 0),
            (12, 62, 0),
            (11, 61, 0),
            (11, 62, 0),
        ] {
            set(&mut w, c, BlockKind::Air);
        }
        with_entity(&mut w, 7, EntityKind::EnderDragon, [40.0, 80.0, 0.0]);
        if let Some(e) = w.entities.get_mut(&7) {
            e.metadata.insert(16, MetaValue::Int(1));
        }
        w
    };
    let racer_in = || {
        let mut p = racer();
        p.end.bridged = true;
        p.end.stock_over = true;
        p.bunker = Some([12, 60, 0]);
        p.end.door = Some((-1, 0));
        p
    };
    // The strafe alone: no seal (a bot never seen holds the strafe for good).
    let w = open_bunker();
    let mut p = racer_in();
    p.step(&w);
    assert!(!noted(&p, "end: the bunker sealed for"), "{:?}", notes(&p));
    // A fireball in flight within 64: no seal either (its cloud goes to the nearest living thing
    // within 4 of its hit, walls or not: E5's dodge moves the bot instead, wall 15).
    let mut w = open_bunker();
    with_entity(&mut w, 8, EntityKind::DragonFireball, [30.0, 75.0, 0.0]);
    let mut p = racer_in();
    p.step(&w);
    assert!(!noted(&p, "end: the bunker sealed for"), "{:?}", notes(&p));
    assert!(
        !noted(&p, "end: the bunker stays shut for a DragonFireball"),
        "{:?}",
        notes(&p)
    );
}

#[test]
fn the_seal_never_lays_a_block_in_the_body_and_stops_after_failing() {
    // E5's dev race 45940964839 lane 0: "Seal work failed ((13, 59, 0) is in the body's box)"
    // 1,067 times. The bunker at (12, 60, 0), its door cell (11, 61, 0) open, the roof shut.
    let at = |x: f64| {
        let mut w = world(4, 0..=7, [x, 60.0, 0.5]);
        for c in [(12, 60, 0), (12, 61, 0), (11, 61, 0)] {
            set(&mut w, c, BlockKind::Air);
        }
        w
    };
    let racer_in = || {
        let mut p = racer();
        p.end.bridged = true;
        p.bunker = Some([12, 60, 0]);
        p.end.door = Some((-1, 0));
        p
    };
    // The body over the door cell's edge: no seal (only that cell to lay), the stock goes on.
    let w = at(12.2);
    let mut p = racer_in();
    p.step(&w);
    assert!(!noted(&p, "end: seal the bunker"), "{:?}", notes(&p));
    assert!(noted(&p, "end: stock run 1"), "{:?}", notes(&p));
    // Three seals failed in a row: no fourth.
    let w = at(12.5);
    let mut p = racer_in();
    p.end.seal_fails = plan::end::SEAL_TRIES;
    p.step(&w);
    assert!(!noted(&p, "end: seal the bunker"), "{:?}", notes(&p));
    assert!(noted(&p, "end: stock run 1"), "{:?}", notes(&p));
}

#[test]
fn a_hold_for_a_cloud_alone_under_a_strafe_is_capped() {
    // The bunker shut, a cloud by its stair's top (9, 64, 0), the dragon strafing, no fireball.
    let mut w = world(4, 0..=7, [12.5, 60.0, 0.5]);
    for c in [(12, 60, 0), (12, 61, 0)] {
        set(&mut w, c, BlockKind::Air);
    }
    with_entity(&mut w, 12, EntityKind::EndCrystal, [42.5, 83.0, 0.5]);
    with_entity(&mut w, 13, EntityKind::AreaEffectCloud, [6.5, 64.0, 0.5]);
    with_entity(&mut w, 7, EntityKind::EnderDragon, [-40.0, 80.0, 0.0]);
    if let Some(e) = w.entities.get_mut(&7) {
        e.metadata.insert(16, MetaValue::Int(1));
    }
    let mut p = racer();
    p.end.bridged = true;
    p.end.stock_over = true;
    p.bunker = Some([12, 60, 0]);
    p.end.door = Some((-1, 0));
    p.step(&w);
    assert!(
        noted(&p, "end: the bunker stays shut for a"),
        "{:?}",
        notes(&p)
    );
    // Past the cap: opened to be seen.
    p.end.fire_since = Some(0);
    w.ticks = 400;
    p.retry_at = 0;
    p.step(&w);
    assert!(
        noted(&p, "end: the bunker opens though a cloud is by its stair"),
        "{:?}",
        notes(&p)
    );
}

#[test]
fn a_strafe_calls_the_bot_out_of_the_stock() {
    // Frontier race 45947478869: three bots about 10,000 ticks in the stock's tunnels, unseen, the
    // strafe held for good. The bunker at (12, 61, 0), sealed; the dragon strafing.
    let strafe = |stone: i32| {
        let mut w = world(4, 0..=7, [12.5, 61.0, 0.5]);
        set(&mut w, (12, 61, 0), BlockKind::Air);
        set(&mut w, (12, 62, 0), BlockKind::Air);
        w.inventory.slots.insert(40, (ItemKind::EndStone, stone));
        with_entity(&mut w, 12, EntityKind::EndCrystal, [42.5, 83.0, 0.5]);
        with_entity(&mut w, 7, EntityKind::EnderDragon, [-40.0, 80.0, 0.0]);
        if let Some(e) = w.entities.get_mut(&7) {
            e.metadata.insert(16, MetaValue::Int(1));
        }
        w
    };
    let racer_in = || {
        let mut p = racer();
        p.end.bridged = true;
        p.bunker = Some([12, 61, 0]);
        p.end.door = Some((1, 0));
        p
    };
    // Short of what the pillars need: paused, no stock run, out to be seen.
    let w = strafe(20);
    let mut p = racer_in();
    p.step(&w);
    assert!(
        noted(&p, "end: stock paused for a strafe"),
        "{:?}",
        notes(&p)
    );
    assert!(!noted(&p, "end: stock run 1"), "{:?}", notes(&p));
    assert!(noted(&p, "end: open the bunker"), "{:?}", notes(&p));
    // With enough held: the stock is over.
    let w = strafe(plan::end::STOCK_ENOUGH);
    let mut p = racer_in();
    p.step(&w);
    assert!(noted(&p, "end: stock over for a strafe"), "{:?}", notes(&p));
    assert!(p.end.stock_over);
}

fn dragon_at(w: &mut World, at: [f64; 3], phase: i32) {
    with_entity(w, 7, EntityKind::EnderDragon, at);
    if let Some(e) = w.entities.get_mut(&7) {
        e.metadata.insert(16, MetaValue::Int(phase));
    }
}

#[test]
fn under_a_strafe_near_no_retreat_takes_the_bot_into_the_bunker() {
    // Wall 17: a bot out of sight holds the strafe for good. Hurt on the surface, the bunker at
    // (12, 61, 0) open by its shaft, the dragon strafing 30 off.
    let mut w = world(4, 0..=7, [30.5, 64.0, 0.5]);
    for y in 61..=63 {
        set(&mut w, (12, y, 0), BlockKind::Air);
    }
    w.player.health = Some(10.0);
    dragon_at(&mut w, [50.0, 70.0, 0.0], 1);
    let mut p = racer();
    p.end.bridged = true;
    p.end.stock_over = true;
    p.bunker = Some([12, 61, 0]);
    p.end.door = Some((1, 0));
    let _ = p.end_task(&w, 0);
    assert!(!noted(&p, "end: back to the bunker"), "{:?}", notes(&p));
    // In a lull (holding, 100 off) the retreat goes.
    let mut w2 = world(4, 0..=7, [30.5, 64.0, 0.5]);
    for y in 61..=63 {
        set(&mut w2, (12, y, 0), BlockKind::Air);
    }
    w2.player.health = Some(10.0);
    dragon_at(&mut w2, [130.0, 70.0, 0.0], 0);
    let _ = p.end_task(&w2, 0);
    assert!(noted(&p, "end: back to the bunker"), "{:?}", notes(&p));
}

#[test]
fn the_paused_stock_resumes_only_with_the_dragon_holding_far_off() {
    // The bunker at (12, 61, 0), sealed; the stock paused by a strafe.
    let at = |d: [f64; 3], phase: i32| {
        let mut w = world(4, 0..=7, [12.5, 61.0, 0.5]);
        set(&mut w, (12, 61, 0), BlockKind::Air);
        set(&mut w, (12, 62, 0), BlockKind::Air);
        dragon_at(&mut w, d, phase);
        w
    };
    let mut p = racer();
    p.end.bridged = true;
    p.bunker = Some([12, 61, 0]);
    p.end.door = Some((1, 0));
    p.end.strafe_paused = true;
    // Holding but 40 off: still paused.
    let _ = p.end_task(&at([50.0, 70.0, 0.0], 0), 0);
    assert!(!noted(&p, "end: stock resumed"), "{:?}", notes(&p));
    assert!(!noted(&p, "end: stock run 1"), "{:?}", notes(&p));
    // Holding 100 off: resumed.
    let _ = p.end_task(&at([110.0, 70.0, 0.0], 0), 0);
    assert!(noted(&p, "end: stock resumed in a lull"), "{:?}", notes(&p));
    assert!(noted(&p, "end: stock run 1"), "{:?}", notes(&p));
}

#[test]
fn the_first_stock_run_waits_for_a_lull_and_a_fireball_pauses_it() {
    let at = |d: Option<([f64; 3], i32)>| {
        let mut w = world(4, 0..=7, [12.5, 61.0, 0.5]);
        set(&mut w, (12, 61, 0), BlockKind::Air);
        set(&mut w, (12, 62, 0), BlockKind::Air);
        if let Some((pos, phase)) = d {
            dragon_at(&mut w, pos, phase);
        }
        w
    };
    let racer_in = || {
        let mut p = racer();
        p.end.bridged = true;
        p.bunker = Some([12, 61, 0]);
        p.end.door = Some((1, 0));
        p
    };
    // The dragon holding 40 off: the first run waits.
    let mut p = racer_in();
    let _ = p.end_task(&at(Some(([50.0, 70.0, 0.0], 0))), 0);
    assert!(noted(&p, "end: stock waits for a lull"), "{:?}", notes(&p));
    assert!(!noted(&p, "end: stock run 1"), "{:?}", notes(&p));
    // Holding 100 off: it begins.
    let _ = p.end_task(&at(Some(([110.0, 70.0, 0.0], 0))), 0);
    assert!(noted(&p, "end: stock run 1"), "{:?}", notes(&p));
    // A fireball in flight 20 off (no strafe phase sent): the stock pauses.
    let mut w = at(None);
    with_entity(&mut w, 8, EntityKind::DragonFireball, [30.0, 70.0, 0.0]);
    let mut p = racer_in();
    let _ = p.end_task(&w, 0);
    assert!(
        noted(&p, "end: stock paused for a strafe"),
        "{:?}",
        notes(&p)
    );
    assert!(!noted(&p, "end: stock run 1"), "{:?}", notes(&p));
}

#[test]
fn the_crystals_handing_back_for_blocks_starts_the_stock_again_in_a_lull() {
    // Frontier race 45959448248 lane 3: paused for a strafe at 3,840, about 0 end stone to the cap.
    let at = |d: ([f64; 3], i32)| {
        let mut w = world(4, 0..=7, [12.5, 61.0, 0.5]);
        set(&mut w, (12, 61, 0), BlockKind::Air);
        set(&mut w, (12, 62, 0), BlockKind::Air);
        with_entity(&mut w, 12, EntityKind::EndCrystal, [42.5, 83.0, 0.5]);
        dragon_at(&mut w, d.0, d.1);
        w
    };
    for (over, paused) in [(true, false), (false, true)] {
        let mut p = racer();
        p.end.bridged = true;
        p.bunker = Some([12, 61, 0]);
        p.end.door = Some((1, 0));
        p.end.stock_over = over;
        p.end.strafe_paused = paused;
        p.end.stock_runs = 13;
        p.end.handed_back = true;
        // A strafe 30 off: not yet.
        let _ = p.end_task(&at(([40.0, 70.0, 0.0], 1)), 0);
        assert!(!noted(&p, "end: stock again"), "{:?}", notes(&p));
        // The dragon holding 100 off: the stock again.
        let _ = p.end_task(&at(([110.0, 70.0, 0.0], 0)), 0);
        assert!(
            noted(&p, "end: stock again: the crystals handed back for blocks"),
            "over {over} paused {paused}: {:?}",
            notes(&p)
        );
        assert!(!p.end.stock_over && !p.end.strafe_paused);
        assert!(noted(&p, "end: stock run 1"), "{:?}", notes(&p));
    }
}

#[test]
fn short_of_end_stone_the_bot_mines_the_islands_surface_where_it_stands() {
    // Frontier race 45969992660 lane 1: handed back for 6 of 27 at 1,820, then 2,800 ticks for a
    // lull and 3,020 idle at the spike's foot. Flat end stone to y 63, the bot at (50, 64, 0).
    let mut w = world(4, 0..=7, [50.5, 64.0, 0.5]);
    surface_kit(&mut w);
    w.inventory.slots.insert(40, (ItemKind::EndStone, 6));
    with_entity(&mut w, 12, EntityKind::EndCrystal, [42.5, 83.0, 0.5]);
    dragon_at(&mut w, [40.0, 75.0, 0.0], 1);
    let mut p = racer();
    p.end.bridged = true;
    p.end.stock_over = true;
    p.bunker = Some([12, 60, 0]);
    p.end.door = Some((1, 0));
    p.end.handed_back = true;
    let t = p.end_task(&w, 0);
    assert_eq!(
        t.as_ref().map(plan::Task::name),
        Some("dig"),
        "{:?}",
        notes(&p)
    );
    assert!(noted(&p, "end: surface end stone at"), "{:?}", notes(&p));
    // The cell dug: beside the feet, one under an open cell, never under the feet.
    let c = plan::end::surface_cell(&w, (50, 64, 0)).unwrap_or_else(|| panic!("no cell"));
    assert_ne!(c, (50, 63, 0));
    assert_eq!(c.1, 63);
    // Within 3 of a void edge: none. A void column at (24, *, 0): every cell under y 64 gone.
    let mut w = world(4, 0..=7, [50.5, 64.0, 0.5]);
    for y in 0..=63 {
        for x in 51..=54 {
            for z in -4..=4 {
                set(&mut w, (x, y, z), BlockKind::Air);
            }
        }
        for x in 46..=50 {
            for z in [-4, 4] {
                set(&mut w, (x, y, z), BlockKind::Air);
            }
        }
    }
    assert_eq!(plan::end::surface_cell(&w, (50, 64, 0)), None);
}

#[test]
fn home_again_the_walks_back_count_again() {
    // The same lane spent its 24 walks to a bunker with no way in, then stood idle.
    let mut w = world(4, 0..=7, [12.5, 61.0, 0.5]);
    set(&mut w, (12, 61, 0), BlockKind::Air);
    set(&mut w, (12, 62, 0), BlockKind::Air);
    let mut p = racer();
    p.end.bridged = true;
    p.end.stock_over = true;
    p.bunker = Some([12, 61, 0]);
    p.end.door = Some((1, 0));
    p.end.home_walks = plan::end::HOME_WALKS;
    let _ = p.end_task(&w, 0);
    assert_eq!(p.end.home_walks, 0);
}

#[test]
fn an_open_bunker_with_a_shut_stair_step_digs_it_again() {
    // Race 45969992660 lane 2: the step (12, 60, 0) shut with the shaft open. The bunker at
    // (12, 60, 0), door west: its roof (12, 62, 0) and first step (11, 61, 0) dug, the second step
    // (10, 62, 0) shut.
    let mut w = world(4, 0..=7, [12.5, 60.0, 0.5]);
    for c in [
        (12, 60, 0),
        (12, 61, 0),
        (12, 62, 0),
        (11, 61, 0),
        (11, 62, 0),
        (11, 63, 0),
        (10, 63, 0),
        (9, 63, 0),
    ] {
        set(&mut w, c, BlockKind::Air);
    }
    let mut p = racer();
    p.end.bridged = true;
    p.end.stock_over = true;
    p.bunker = Some([12, 60, 0]);
    p.end.door = Some((-1, 0));
    let _ = p.end_task(&w, 0);
    assert!(
        noted(&p, "end: the bunker's stair dug again"),
        "{:?}",
        notes(&p)
    );
    let ops = p
        .end
        .work
        .as_ref()
        .map(|wk| wk.ops.clone())
        .unwrap_or_default();
    assert!(ops.contains(&plan::end::Op::Dig((10, 62, 0))), "{ops:?}");
}

#[test]
fn the_stock_arms_never_run_toward_the_bunkers_stair() {
    use plan::end::stock_dir;
    let door = (-1, 0);
    let mut w = world(4, 0..=7, [12.5, 61.0, 0.5]);
    set(&mut w, (12, 61, 0), BlockKind::Air);
    set(&mut w, (12, 62, 0), BlockKind::Air);
    let mut p = racer();
    p.end.bridged = true;
    p.bunker = Some([12, 61, 0]);
    p.end.door = Some(door);
    p.end.stock_foot = Some((12, 55, -6));
    p.end.stock_runs = 1;
    let _ = p.end_task(&w, 0);
    assert!(!p.end.arms.is_empty(), "{:?}", notes(&p));
    assert!(
        p.end.arms.iter().all(|(d, _)| *d != door),
        "{:?}",
        p.end.arms
    );
    assert!(p.end.arms.iter().any(|(d, _)| *d == stock_dir(door, 0)));
}

#[test]
fn a_cloud_on_the_feet_holds_no_work_of_e3s_at_the_bunkers_stair() {
    // Frontier race 45978279345 lane 2: the stair dug again and the open cut, in turn, 43 times
    // each, with the bot standing in the cloud. The bunker (12, 60, 0) open, a step shut, the bot
    // on its first step (11, 61, 0) with a cloud on it.
    let mut w = world(4, 0..=7, [11.5, 61.0, 0.5]);
    for c in [
        (12, 60, 0),
        (12, 61, 0),
        (12, 62, 0),
        (11, 61, 0),
        (11, 62, 0),
        (11, 63, 0),
    ] {
        set(&mut w, c, BlockKind::Air);
    }
    with_entity(&mut w, 13, EntityKind::AreaEffectCloud, [11.5, 61.0, 0.5]);
    if let Some(e) = w.entities.get_mut(&13) {
        e.metadata.insert(8, MetaValue::Float(3.0));
    }
    let mut p = racer();
    p.end.bridged = true;
    p.end.stock_over = true;
    p.bunker = Some([12, 60, 0]);
    p.end.door = Some((-1, 0));
    p.end.work = Some(plan::end::Work::new(
        plan::end::WorkKind::Open,
        vec![plan::end::Op::Dig((10, 62, 0))],
        0,
    ));
    for _ in 0..4 {
        p.step(&w);
    }
    assert!(p.end.work.is_none(), "{:?}", p.end.work);
    assert!(
        !noted(&p, "end: the bunker's stair dug again"),
        "{:?}",
        notes(&p)
    );
    assert!(!noted(&p, "end: the open cut"), "{:?}", notes(&p));
}

#[test]
fn from_arrival_to_the_crystals_no_bunker_or_tunnel_only_surface_mining() {
    // The coordinator's decision after frontier race 45978279345: before the crystals are done
    // the bot stays on the surface in the open and mines its end stone there.
    let mut w = world(4, 0..=7, [50.5, 64.0, 0.5]);
    surface_kit(&mut w);
    w.stage_reached = Some(world::stage::Stage::Island);
    with_entity(&mut w, 12, EntityKind::EndCrystal, [42.5, 83.0, 0.5]);
    let mut p = racer();
    p.end.bridged = true;
    let t = p.end_task(&w, 0);
    assert_eq!(
        t.as_ref().map(plan::Task::name),
        Some("dig"),
        "{:?}",
        notes(&p)
    );
    assert!(noted(&p, "end: surface end stone at"), "{:?}", notes(&p));
    for not in [
        "end: dig the bunker",
        "end: walk to the bunker site",
        "end: stock run",
    ] {
        assert!(!noted(&p, not), "{not}: {:?}", notes(&p));
    }
    assert!(p.bunker.is_none());
    // A bunker already dug (an old snapshot): still no stock tunnel, the surface instead.
    let mut p = racer();
    p.end.bridged = true;
    p.bunker = Some([12, 60, 0]);
    p.end.door = Some((1, 0));
    let t = p.end_task(&w, 0);
    assert_eq!(
        t.as_ref().map(plan::Task::name),
        Some("dig"),
        "{:?}",
        notes(&p)
    );
    assert!(!noted(&p, "end: stock run"), "{:?}", notes(&p));
    // Under the ground: stairs up to the open.
    let mut w = world(4, 0..=7, [12.5, 52.0, 0.5]);
    w.stage_reached = Some(world::stage::Stage::Island);
    set(&mut w, (12, 52, 0), BlockKind::Air);
    set(&mut w, (12, 53, 0), BlockKind::Air);
    let mut p = racer();
    p.end.bridged = true;
    let t = p.end_task(&w, 0);
    assert_eq!(
        t.as_ref().map(plan::Task::name),
        Some("stairs"),
        "{:?}",
        notes(&p)
    );
    assert!(
        noted(&p, "end: stairs up to the open from"),
        "{:?}",
        notes(&p)
    );
}

#[test]
fn the_first_surface_stock_stops_at_enough_for_the_tallest_pillar() {
    // The coordinator: exposure on the ground before the first climb is what kills; 64 is the
    // tallest pillar and its walls, the restock on a hand-back covers the rest.
    let mut w = world(4, 0..=7, [50.5, 64.0, 0.5]);
    surface_kit(&mut w);
    w.stage_reached = Some(world::stage::Stage::Island);
    with_entity(&mut w, 12, EntityKind::EndCrystal, [42.5, 83.0, 0.5]);
    w.inventory
        .slots
        .insert(40, (ItemKind::EndStone, plan::end::STOCK_ENOUGH - 1));
    let mut p = racer();
    p.end.bridged = true;
    let t = p.end_task(&w, 0);
    assert_eq!(
        t.as_ref().map(plan::Task::name),
        Some("dig"),
        "{:?}",
        notes(&p)
    );
    // One more held: the surface stock is over and the crystals are asked.
    w.inventory
        .slots
        .insert(40, (ItemKind::EndStone, plan::end::STOCK_ENOUGH));
    let mut p = racer();
    p.end.bridged = true;
    let _ = p.end_task(&w, 0);
    assert!(noted(&p, "end: surface stock over"), "{:?}", notes(&p));
    assert!(!noted(&p, "end: surface end stone at"), "{:?}", notes(&p));
}

#[test]
fn surface_mining_notes_every_dig_and_keeps_its_pit_one_deep() {
    // Race 45988967677: 117 surface digs began with no reason; lanes 1 and 5 stalled in pits.
    let mut w = world(4, 0..=7, [50.5, 64.0, 0.5]);
    surface_kit(&mut w);
    w.stage_reached = Some(world::stage::Stage::Island);
    with_entity(&mut w, 12, EntityKind::EndCrystal, [42.5, 83.0, 0.5]);
    let mut p = racer();
    p.end.bridged = true;
    for _ in 0..3 {
        p.task = plan::Task::Idle;
        let _ = p.end_task(&w, 0);
    }
    let digs = p
        .events
        .iter()
        .filter(|e| e.what.starts_with("end: surface end stone at"))
        .count();
    assert_eq!(digs, 3, "{:?}", notes(&p));
    // Down one in the pit (the mining began at 64): only the cells beside the feet at their height.
    assert_eq!(
        plan::end::surface_cell_at(&w, (50, 63, 0), false).map(|c| c.1),
        Some(63)
    );
    let mut w2 = world(4, 0..=7, [20.5, 63.0, 0.5]);
    for c in [
        (50, 63, 0),
        (51, 63, 0),
        (49, 63, 0),
        (50, 63, 1),
        (50, 63, -1),
    ] {
        set(&mut w2, c, BlockKind::Air);
    }
    // Its walls at 63 gone, the floor at 62 beside is never dug below the level.
    assert_eq!(plan::end::surface_cell_at(&w2, (50, 63, 0), false), None);
}

/// No cobblestone (it counts as a pillar block now), the stone pickaxe in hand.
fn surface_kit(w: &mut World) {
    w.inventory.slots.remove(&36);
    w.inventory.slots.remove(&37);
    w.held = 2;
}

#[test]
fn the_pickaxe_budget_keeps_a_reserve_and_wears_the_most_worn_first() {
    // Race 45994686575 lane 5: three stone pickaxes worn out by 49,600, "no pickaxe" 15,240 ticks.
    let mut w = world(4, 0..=7, [50.5, 64.0, 0.5]);
    surface_kit(&mut w);
    w.stage_reached = Some(world::stage::Stage::Island);
    with_entity(&mut w, 12, EntityKind::EndCrystal, [42.5, 83.0, 0.5]);
    // A second pickaxe in slot 3 (hotbar 3), more worn than the one in hand.
    w.inventory.slots.insert(39, (ItemKind::StonePickaxe, 1));
    w.inventory.wear.insert(
        39,
        protocol::slot::Wear {
            damage: Some(100),
            max_damage: Some(131),
        },
    );
    assert_eq!(plan::end::most_worn_pick(&w), Some(3));
    let mut p = racer();
    p.end.bridged = true;
    let _ = p.end_task(&w, 0);
    assert!(
        p.take_acts().contains(&Act::Hold { slot: 3 }),
        "{:?}",
        notes(&p)
    );
    // Both near the reserve: no surface dig, and the reserve says so.
    w.inventory.wear.insert(
        38,
        protocol::slot::Wear {
            damage: Some(120),
            max_damage: Some(131),
        },
    );
    w.inventory.wear.insert(
        39,
        protocol::slot::Wear {
            damage: Some(110),
            max_damage: Some(131),
        },
    );
    w.held = 3;
    let mut p = racer();
    p.end.bridged = true;
    let _ = p.end_task(&w, 0);
    assert!(
        noted(&p, "end: surface dig held for the pickaxe reserve"),
        "{:?}",
        notes(&p)
    );
    assert!(!noted(&p, "end: surface end stone at"), "{:?}", notes(&p));
}

#[test]
fn carried_blocks_count_and_a_dig_drops_where_the_bot_can_take_it() {
    // Race 46038776204: lane 8 spent 8,160 ticks on 77 surface digs (6 "Uncollected"); the
    // frontier bot carries about 92 full blocks.
    let mut w = world(4, 0..=7, [50.5, 64.0, 0.5]);
    surface_kit(&mut w);
    w.stage_reached = Some(world::stage::Stage::Island);
    with_entity(&mut w, 12, EntityKind::EndCrystal, [42.5, 83.0, 0.5]);
    w.inventory.slots.insert(9, (ItemKind::Netherrack, 64));
    w.inventory
        .slots
        .insert(10, (ItemKind::CobbledDeepslate, 28));
    assert_eq!(plan::end::pillar_blocks(&w), 92);
    let mut p = racer();
    p.end.bridged = true;
    let _ = p.end_task(&w, 0);
    assert!(!noted(&p, "end: surface end stone at"), "{:?}", notes(&p));
    // A block beside the feet over a full block: dug; with air under it (its drop would fall
    // away): not.
    let mut w = world(4, 0..=7, [50.5, 64.0, 0.5]);
    set(&mut w, (51, 64, 0), BlockKind::EndStone);
    assert_eq!(
        plan::end::surface_cell_at(&w, (50, 64, 0), false),
        Some((51, 64, 0))
    );
    set(&mut w, (51, 63, 0), BlockKind::Air);
    assert_eq!(plan::end::surface_cell_at(&w, (50, 64, 0), false), None);
}

#[test]
fn surface_mining_keeps_off_the_perch_and_holds_for_a_landing_dragon() {
    // Race 46038776204 lanes 4 and 8: caught in their pits with no dodge cell; lane 4 mined at 24
    // from the fountain while the dragon landed and sat.
    let mut w = world(4, 0..=7, [24.5, 64.0, 0.5]);
    surface_kit(&mut w);
    w.stage_reached = Some(world::stage::Stage::Island);
    with_entity(&mut w, 12, EntityKind::EndCrystal, [42.5, 83.0, 0.5]);
    let mut p = racer();
    p.end.bridged = true;
    let _ = p.end_task(&w, 0);
    assert!(noted(&p, "end: out to mine at"), "{:?}", notes(&p));
    assert!(!noted(&p, "end: surface end stone at"), "{:?}", notes(&p));
    // Out at 50 with the dragon sitting on its perch: held.
    let mut w = world(4, 0..=7, [50.5, 64.0, 0.5]);
    surface_kit(&mut w);
    w.stage_reached = Some(world::stage::Stage::Island);
    with_entity(&mut w, 12, EntityKind::EndCrystal, [42.5, 83.0, 0.5]);
    dragon_at(&mut w, [0.5, 66.0, 0.5], 6);
    let mut p = racer();
    p.end.bridged = true;
    let _ = p.end_task(&w, 0);
    assert!(
        noted(&p, "end: surface mining held for the dragon"),
        "{:?}",
        notes(&p)
    );
    assert!(!noted(&p, "end: surface end stone at"), "{:?}", notes(&p));
}

#[test]
fn in_a_two_deep_water_pit_with_air_running_out_the_head_comes_out() {
    // Race 46045482138 lane 5: 8 drown hits sitting idle in water in a pit, dead at 34,900.
    // A pit two deep at (50, 62 to 63, 0) full of water, the bot at its bottom, the eye in water.
    let mut w = world(4, 0..=7, [50.5, 62.0, 0.5]);
    for y in 62..=63 {
        set(&mut w, (50, y, 0), BlockKind::Water);
    }
    w.player.air = Some(200);
    let mut p = racer();
    p.end.bridged = true;
    let c = p.step(&w);
    assert!(c.jump, "{c:?}");
    assert!(c.forward, "pressing into a side whose top is free: {c:?}");
    assert!(
        noted(&p, "end: head out of the water at (50, 63, 0)"),
        "{:?}",
        notes(&p)
    );
    // Roofed over the head: dug up.
    let mut w = world(4, 0..=7, [50.5, 62.0, 0.5]);
    for y in 62..=63 {
        set(&mut w, (50, y, 0), BlockKind::Water);
    }
    set(&mut w, (50, 64, 0), BlockKind::EndStone);
    w.player.air = Some(200);
    w.held = 2;
    let mut p = racer();
    p.end.bridged = true;
    let mut acts = Vec::new();
    for _ in 0..3 {
        p.step(&w);
        acts.extend(p.take_acts());
    }
    assert!(
        noted(&p, "end: dig up out of the water at (50, 64, 0)"),
        "{:?}",
        notes(&p)
    );
    assert!(
        acts.iter()
            .any(|a| matches!(a, Act::Dig { pos, .. } if *pos == world::BlockPos::new(50, 64, 0))),
        "{acts:?}"
    );
    // Full air, no hit: nothing.
    let mut w = world(4, 0..=7, [50.5, 62.0, 0.5]);
    for y in 62..=63 {
        set(&mut w, (50, y, 0), BlockKind::Water);
    }
    w.player.air = Some(300);
    let mut p = racer();
    p.end.bridged = true;
    p.step(&w);
    assert!(!noted(&p, "end: head out of the water"), "{:?}", notes(&p));
}

#[test]
fn a_pillar_wanting_more_than_the_stock_held_reopens_it() {
    // Frontier race 46055289196: "surface stock over (29 … (29 wanted))", then E4's "too few
    // blocks (29 blocks held, 37 wanted …)" 60 times, no progress to 5,700 ticks.
    assert_eq!(
        plan::end::wanted_in("29 blocks held, 37 wanted for the pillar at (33, 58, -21)"),
        Some(37)
    );
    let mut w = world(4, 0..=7, [50.5, 64.0, 0.5]);
    surface_kit(&mut w);
    w.stage_reached = Some(world::stage::Stage::Island);
    with_entity(&mut w, 12, EntityKind::EndCrystal, [42.5, 83.0, 0.5]);
    w.inventory.slots.insert(40, (ItemKind::EndStone, 29));
    let mut p = racer();
    p.end.bridged = true;
    p.end.open_stock_over = true;
    p.events.push(plan::PlanEvent {
        tick: 0,
        what: "spikes: no walk to the foot: too few blocks".to_owned(),
        why: "29 blocks held, 37 wanted for the pillar at (33, 58, -21)".to_owned(),
    });
    assert!(p.blocks_wanted(&w, 0) >= 37);
    let t = p.end_task(&w, 0);
    assert!(noted(&p, "end: surface stock again"), "{:?}", notes(&p));
    assert_eq!(
        t.as_ref().map(plan::Task::name),
        Some("dig"),
        "{:?}",
        notes(&p)
    );
}
